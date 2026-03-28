use crate::{
    de::val_reader::ScalarReader, DeserializeError, PlyProperty, PropertyType, ScalarType,
};
use serde::{
    de::{value::BytesDeserializer, DeserializeSeed, Error, MapAccess, SeqAccess, Visitor},
    Deserializer,
};
use std::{collections::HashMap, io::Read, marker::PhantomData};

/// Pre-computed plan for seq-based deserialization of all-scalar rows.
/// For each struct field (in declaration order), stores where to find its
/// value in the bulk-read row buffer.
struct SeqPlan {
    row_byte_size: usize,
    /// One entry per struct field. `Some((offset, type))` if the field has a
    /// matching PLY property.
    field_offsets: Vec<Option<(usize, ScalarType)>>,
}

/// Deserialization strategy, computed once on the first `deserialize_struct` call.
enum Strategy {
    /// Not yet determined.
    Unknown,
    /// All properties are fixed-size scalars and no aliases detected.
    /// Bulk-read the row and serve fields via `visit_seq` — no key matching.
    Seq(SeqPlan),
    /// Fall back to string-based `visit_map` matching (for lists, aliases, ASCII).
    StringFallback,
}

pub(crate) struct RowDeserializer<'a, R: Read, S: ScalarReader> {
    pub reader: &'a mut R,
    properties: &'a [PlyProperty],
    strategy: Strategy,
    /// Reusable buffer for bulk row reads in the Seq path.
    row_buf: Vec<u8>,
    _marker: PhantomData<S>,
}

impl<'a, R: Read, S: ScalarReader> RowDeserializer<'a, R, S> {
    pub fn new(reader: &'a mut R, properties: &'a [PlyProperty]) -> Self {
        Self {
            reader,
            properties,
            strategy: Strategy::Unknown,
            row_buf: Vec::new(),
            _marker: PhantomData,
        }
    }
}

/// Try to build a SeqPlan for all-scalar rows.
///
/// Returns `None` if any property is a list, byte sizes are unknown (ASCII),
/// or `#[serde(alias)]` breaks the dense field index invariant.
fn build_seq_plan<S: ScalarReader>(properties: &[PlyProperty], fields: &[&str]) -> Option<SeqPlan> {
    // Compute byte offset for each PLY property, building a name → (offset, type) map.
    let mut prop_lookup: HashMap<&str, (usize, ScalarType)> =
        HashMap::with_capacity(properties.len());
    let mut offset = 0usize;
    for prop in properties {
        match prop.property_type {
            PropertyType::Scalar(t) => {
                let size = S::scalar_byte_size(t)?;
                prop_lookup.insert(&prop.name, (offset, t));
                offset += size;
            }
            PropertyType::List { .. } => return None,
        }
    }

    // Detect aliases: for each PLY property, find its position in the `fields`
    // slice. Matched positions must be dense (0..N) — aliases insert extra
    // entries that break density, so this reliably detects them.
    let fields_lookup: HashMap<&str, usize> =
        fields.iter().enumerate().map(|(i, &f)| (f, i)).collect();

    let mut num_matched = 0usize;
    let mut max_index = 0usize;
    for prop in properties {
        if let Some(&idx) = fields_lookup.get(prop.name.as_str()) {
            num_matched += 1;
            max_index = max_index.max(idx);
        }
    }

    if num_matched == 0 || max_index >= num_matched {
        return None;
    }

    // Build the seq plan: for each struct field (in `fields` order),
    // look up the matching PLY property's byte offset and type.
    let field_offsets: Vec<Option<(usize, ScalarType)>> = fields
        .iter()
        .map(|&name| prop_lookup.get(name).copied())
        .collect();

    Some(SeqPlan {
        row_byte_size: offset,
        field_offsets,
    })
}

impl<'de, R: Read, S: ScalarReader> Deserializer<'de> for &mut RowDeserializer<'_, R, S> {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(DeserializeError::custom(
            "Rows must be deserialized as maps or structs.",
        ))
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if matches!(self.strategy, Strategy::Unknown) {
            self.strategy = match build_seq_plan::<S>(self.properties, fields) {
                Some(plan) => {
                    self.row_buf.resize(plan.row_byte_size, 0);
                    Strategy::Seq(plan)
                }
                None => Strategy::StringFallback,
            };
        }

        match &self.strategy {
            Strategy::Seq(plan) => {
                self.reader
                    .read_exact(&mut self.row_buf)
                    .map_err(DeserializeError)?;
                visitor.visit_seq(IndexedSeqAccess {
                    row_buf: &self.row_buf,
                    plan,
                    current_field: 0,
                    _marker: PhantomData::<S>,
                })
            }
            _ => self.deserialize_map(visitor),
        }
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_map(StringMapAccess {
            reader: self.reader,
            properties: self.properties,
            current_property: 0,
            _marker: PhantomData::<S>,
        })
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 u8 i16 u16 i32 u32 i64 u64 f32 f64 char str string
        bytes byte_buf option unit unit_struct seq tuple
        tuple_struct enum identifier ignored_any
    }
}

/// Serves struct fields in declaration order from a pre-read row buffer.
/// Each field reads from a computed byte offset.
struct IndexedSeqAccess<'a, S: ScalarReader> {
    row_buf: &'a [u8],
    plan: &'a SeqPlan,
    current_field: usize,
    _marker: PhantomData<S>,
}

impl<'de, S: ScalarReader> SeqAccess<'de> for IndexedSeqAccess<'_, S> {
    type Error = DeserializeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if self.current_field >= self.plan.field_offsets.len() {
            return Ok(None);
        }
        let entry = self.plan.field_offsets[self.current_field];
        self.current_field += 1;
        match entry {
            Some((offset, data_type)) => {
                let mut slice = &self.row_buf[offset..];
                seed.deserialize(ScalarDeserializer::<&[u8], S> {
                    reader: &mut slice,
                    data_type,
                    _marker: PhantomData,
                })
                .map(Some)
            }
            None => Ok(None),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.plan.field_offsets.len() - self.current_field)
    }
}

/// String-based field matching via `visit_bytes` on property names.
struct StringMapAccess<'a, R: Read, S: ScalarReader> {
    reader: &'a mut R,
    properties: &'a [PlyProperty],
    current_property: usize,
    _marker: PhantomData<S>,
}

impl<'de, R: Read, S: ScalarReader> MapAccess<'de> for StringMapAccess<'_, R, S> {
    type Error = DeserializeError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        let Some(prop) = self.properties.get(self.current_property) else {
            return Ok(None);
        };
        seed.deserialize(BytesDeserializer::new(prop.name.as_bytes()))
            .map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        let prop = &self.properties[self.current_property];
        self.current_property += 1;
        match prop.property_type {
            PropertyType::Scalar(data_type) => seed.deserialize(ScalarDeserializer {
                reader: &mut self.reader,
                data_type,
                _marker: PhantomData::<S>,
            }),
            PropertyType::List {
                count_type,
                data_type,
            } => seed.deserialize(ListDeserializer {
                reader: &mut self.reader,
                count_type,
                data_type,
                _marker: PhantomData::<S>,
            }),
        }
    }
}

struct ScalarDeserializer<'a, R: Read, S: ScalarReader> {
    reader: &'a mut R,
    data_type: ScalarType,
    _marker: PhantomData<S>,
}

impl<'de, R: Read, S: ScalarReader> Deserializer<'de> for ScalarDeserializer<'_, R, S> {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self.data_type {
            ScalarType::I8 => visitor.visit_i8(S::read_i8(self.reader)?),
            ScalarType::U8 => visitor.visit_u8(S::read_u8(self.reader)?),
            ScalarType::I16 => visitor.visit_i16(S::read_i16(self.reader)?),
            ScalarType::U16 => visitor.visit_u16(S::read_u16(self.reader)?),
            ScalarType::I32 => visitor.visit_i32(S::read_i32(self.reader)?),
            ScalarType::U32 => visitor.visit_u32(S::read_u32(self.reader)?),
            ScalarType::F32 => visitor.visit_f32(S::read_f32(self.reader)?),
            ScalarType::F64 => visitor.visit_f64(S::read_f64(self.reader)?),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 u8 i16 u16 i32 u32 f32 f64 i128 i64 u128 u64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct
        map struct enum identifier ignored_any
    }
}

struct ListDeserializer<R: Read, S: ScalarReader> {
    reader: R,
    count_type: ScalarType,
    data_type: ScalarType,
    _marker: PhantomData<S>,
}

impl<'de, R: Read, S: ScalarReader> Deserializer<'de> for ListDeserializer<R, S> {
    type Error = DeserializeError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_some(self)
    }

    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let count = S::read_count(&mut self.reader, self.count_type)?;
        visitor.visit_seq(ListSeqAccess {
            reader: &mut self.reader,
            remaining: count,
            data_type: self.data_type,
            _marker: PhantomData::<S>,
        })
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    serde::forward_to_deserialize_any! {
        bool i8 u8 i16 u16 i32 u32 f32 f64 i128 i64 u128 u64 char str string
        bytes byte_buf unit unit_struct tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct ListSeqAccess<R: Read, S> {
    reader: R,
    data_type: ScalarType,
    remaining: usize,
    _marker: PhantomData<S>,
}

impl<'de, R: Read, S: ScalarReader> SeqAccess<'de> for ListSeqAccess<R, S> {
    type Error = DeserializeError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        seed.deserialize(ScalarDeserializer {
            reader: &mut self.reader,
            data_type: self.data_type,
            _marker: PhantomData::<S>,
        })
        .map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.remaining)
    }
}
