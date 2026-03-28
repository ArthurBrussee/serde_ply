use serde::{Deserialize, Deserializer};
use std::io::{BufReader, Cursor};

// ============================================================================
// Shared helpers and types
// ============================================================================

/// Build a binary LE PLY file from a header and raw f32 row data.
fn make_binary_ply(header: &str, rows: &[&[f32]]) -> Vec<u8> {
    let mut data = header.as_bytes().to_vec();
    for row in rows {
        for v in *row {
            data.extend_from_slice(&v.to_le_bytes());
        }
    }
    data
}

#[derive(Deserialize, Default, Debug, PartialEq)]
struct NewFloat(f32);

#[derive(Deserialize, Debug, PartialEq)]
struct FlexibleVertex {
    #[serde(rename = "x")]
    position_x: f32,
    #[serde(alias = "y", alias = "pos_y")]
    position_y: f32,
    z: f32,
    #[serde(deserialize_with = "u8_to_normalized")]
    red: f32,
    #[serde(deserialize_with = "u8_to_normalized")]
    green: f32,
    #[serde(deserialize_with = "u8_to_normalized")]
    blue: f32,
    #[serde(default)]
    confidence: f32,
    normal_x: Option<f32>,
    #[serde(skip)]
    computed: String,
    #[serde(default)]
    new_typed: NewFloat,
}

impl Default for FlexibleVertex {
    fn default() -> Self {
        Self {
            position_x: 0.0,
            position_y: 0.0,
            z: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            confidence: 1.0,
            normal_x: None,
            computed: "computed".to_string(),
            new_typed: NewFloat(4.0),
        }
    }
}

fn u8_to_normalized<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let val: u8 = u8::deserialize(deserializer)?;
    Ok(val as f32 / 255.0)
}

// ============================================================================
// 1. rename
// ============================================================================

#[test]
fn test_field_renaming() {
    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property float x
property float y
property float z
property uchar red
property uchar green
property uchar blue
end_header
1.0 2.0 3.0 255 128 64
"#;

    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<FlexibleVertex> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0].position_x, 1.0);
    assert_eq!(vertices[0].position_y, 2.0);
    assert!((vertices[0].red - 1.0).abs() < 0.001);
    assert!((vertices[0].green - 0.502).abs() < 0.001);
}

#[test]
fn test_seq_with_rename_binary() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct Renamed {
        #[serde(rename = "pos_x")]
        x: f32,
        #[serde(rename = "pos_y")]
        y: f32,
    }

    #[derive(Deserialize)]
    struct Ply {
        vertex: Vec<Renamed>,
    }

    let data = make_binary_ply(
        "ply\nformat binary_little_endian 1.0\nelement vertex 1\nproperty float pos_x\nproperty float pos_y\nend_header\n",
        &[&[10.0, 20.0]],
    );

    let ply: Ply = serde_ply::from_bytes(&data).unwrap();
    assert_eq!(ply.vertex[0], Renamed { x: 10.0, y: 20.0 });
}

// ============================================================================
// 2. alias
// ============================================================================

#[test]
fn test_field_aliases() {
    // Test aliased name "pos_y" instead of "y".
    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property float x
property float pos_y
property float z
property uchar red
property uchar green
property uchar blue
end_header
1.0 2.0 3.0 255 128 64
"#;

    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<FlexibleVertex> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0].position_x, 1.0);
    assert_eq!(vertices[0].position_y, 2.0);
}

#[test]
fn test_alias_via_from_reader() {
    // Test alias using from_reader (goes through deserialize_struct -> u64 path).
    #[derive(Deserialize, Debug, PartialEq)]
    struct Face {
        #[serde(alias = "vertex_index")]
        vertex_indices: Vec<u32>,
    }

    #[derive(Deserialize, Debug)]
    struct Ply {
        face: Vec<Face>,
    }

    // PLY uses the alias name "vertex_index"
    let ply_data = r#"ply
format ascii 1.0
element face 2
property list uchar uint vertex_index
end_header
3 0 1 2
4 0 1 2 3
"#;

    let ply: Ply = serde_ply::from_reader(Cursor::new(ply_data)).unwrap();
    assert_eq!(ply.face.len(), 2);
    assert_eq!(ply.face[0].vertex_indices, vec![0, 1, 2]);
    assert_eq!(ply.face[1].vertex_indices, vec![0, 1, 2, 3]);

    // Also test with the primary name
    let ply_data2 = r#"ply
format ascii 1.0
element face 1
property list uchar uint vertex_indices
end_header
3 0 1 2
"#;

    let ply2: Ply = serde_ply::from_reader(Cursor::new(ply_data2)).unwrap();
    assert_eq!(ply2.face[0].vertex_indices, vec![0, 1, 2]);
}

#[test]
fn test_skip_plus_alias_via_from_reader() {
    // Edge case: skip + alias on same struct.
    #[derive(Deserialize, Debug, PartialEq)]
    struct Vertex {
        x: f32,
        #[serde(skip_deserializing)]
        _metadata: f32,
        #[serde(alias = "pos_y")]
        y: f32,
        z: f32,
    }

    #[derive(Deserialize, Debug)]
    struct Ply {
        vertex: Vec<Vertex>,
    }

    // Use the alias name
    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property float x
property float pos_y
property float z
end_header
1.0 2.0 3.0
"#;

    let ply: Ply = serde_ply::from_reader(Cursor::new(ply_data)).unwrap();
    assert_eq!(
        ply.vertex[0],
        Vertex {
            x: 1.0,
            _metadata: 0.0,
            y: 2.0,
            z: 3.0
        }
    );

    // Use the primary name
    let ply_data2 = r#"ply
format ascii 1.0
element vertex 1
property float x
property float y
property float z
end_header
1.0 2.0 3.0
"#;

    let ply2: Ply = serde_ply::from_reader(Cursor::new(ply_data2)).unwrap();
    assert_eq!(
        ply2.vertex[0],
        Vertex {
            x: 1.0,
            _metadata: 0.0,
            y: 2.0,
            z: 3.0
        }
    );
}

// ============================================================================
// 3. default
// ============================================================================

#[test]
fn test_default_fields() {
    // Fields not present in PLY get default values.
    #[derive(Deserialize, Debug)]
    #[allow(unused)]
    struct SimpleVertex {
        x: f32,
        y: f32,
        z: f32,
        #[serde(default)]
        confidence: f32,
    }

    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property float x
property float y
property float z
end_header
1.0 2.0 3.0
"#;

    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<SimpleVertex> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0].confidence, 0.0);
}

// ============================================================================
// 4. skip
// ============================================================================

#[test]
fn test_skip_deserializing() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct VertexWithSkip {
        x: f32,
        #[serde(skip_deserializing)]
        skipped: f32,
        y: f32,
    }

    #[derive(Deserialize, Debug)]
    struct Ply {
        vertex: Vec<VertexWithSkip>,
    }

    let ply_data = r#"ply
format ascii 1.0
element vertex 2
property float x
property float y
end_header
1.0 2.0
3.0 4.0
"#;

    let ply: Ply = serde_ply::from_reader(Cursor::new(ply_data)).unwrap();
    assert_eq!(ply.vertex.len(), 2);
    assert_eq!(
        ply.vertex[0],
        VertexWithSkip {
            x: 1.0,
            skipped: 0.0,
            y: 2.0
        }
    );
    assert_eq!(
        ply.vertex[1],
        VertexWithSkip {
            x: 3.0,
            skipped: 0.0,
            y: 4.0
        }
    );
}

// ============================================================================
// 5. Option
// ============================================================================

#[test]
fn test_optional_fields() {
    let ply_data = r#"ply
format ascii 1.0
element vertex 2
property float x
property float y
property float z
property uchar red
property uchar green
property uchar blue
property float normal_x
end_header
1.0 2.0 3.0 255 128 64 0.707
4.0 5.0 6.0 200 100 50 0.0
"#;
    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<FlexibleVertex> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 2);
    assert_eq!(vertices[0].normal_x, Some(0.707));
    assert_eq!(vertices[1].normal_x, Some(0.0));
}

#[test]
fn test_optional_fields_binary() {
    let ply_data = r#"ply
format binary_little_endian 1.0
element vertex 2
property float x
property float y
property float z
property uchar red
property uchar green
property uchar blue
property float normal_x
end_header
"#;

    let mut binary_data = Vec::new();
    binary_data.extend_from_slice(ply_data.as_bytes());

    // First vertex: 1.0, 2.0, 3.0, 255, 128, 64, 0.707
    binary_data.extend_from_slice(&1.0f32.to_le_bytes());
    binary_data.extend_from_slice(&2.0f32.to_le_bytes());
    binary_data.extend_from_slice(&3.0f32.to_le_bytes());
    binary_data.push(255u8);
    binary_data.push(128u8);
    binary_data.push(64u8);
    binary_data.extend_from_slice(&0.707f32.to_le_bytes());

    // Second vertex: 4.0, 5.0, 6.0, 200, 100, 50, 0.0
    binary_data.extend_from_slice(&4.0f32.to_le_bytes());
    binary_data.extend_from_slice(&5.0f32.to_le_bytes());
    binary_data.extend_from_slice(&6.0f32.to_le_bytes());
    binary_data.push(200u8);
    binary_data.push(100u8);
    binary_data.push(50u8);
    binary_data.extend_from_slice(&0.0f32.to_le_bytes());

    let cursor = Cursor::new(binary_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<FlexibleVertex> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 2);
    assert_eq!(vertices[0].normal_x, Some(0.707));
    assert_eq!(vertices[1].normal_x, Some(0.0));
}

#[test]
fn test_optional_list_elements_ascii() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct VertexWithOptionalList {
        x: f32,
        y: f32,
        z: f32,
        normals: Vec<Option<f32>>,
    }

    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property float x
property float y
property float z
property list uchar float normals
end_header
1.0 2.0 3.0 3 0.1 0.2 0.3
"#;

    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<VertexWithOptionalList> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0].normals, vec![Some(0.1), Some(0.2), Some(0.3)]);
}

#[test]
fn test_optional_list_elements_binary() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct VertexWithOptionalList {
        x: f32,
        y: f32,
        z: f32,
        normals: Vec<Option<f32>>,
    }

    let ply_data = r#"ply
format binary_little_endian 1.0
element vertex 1
property float x
property float y
property float z
property list uchar float normals
end_header
"#;

    let mut binary_data = Vec::new();
    binary_data.extend_from_slice(ply_data.as_bytes());

    binary_data.extend_from_slice(&1.0f32.to_le_bytes());
    binary_data.extend_from_slice(&2.0f32.to_le_bytes());
    binary_data.extend_from_slice(&3.0f32.to_le_bytes());
    binary_data.push(3u8); // list count
    binary_data.extend_from_slice(&0.1f32.to_le_bytes());
    binary_data.extend_from_slice(&0.2f32.to_le_bytes());
    binary_data.extend_from_slice(&0.3f32.to_le_bytes());

    let cursor = Cursor::new(binary_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<VertexWithOptionalList> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0].normals, vec![Some(0.1), Some(0.2), Some(0.3)]);
}

// ============================================================================
// 6. Wrappers (newtype, transparent, deserialize_with)
// ============================================================================

#[test]
fn test_newtype_field() {
    let ply_data = r#"ply
format ascii 1.0
element vertex 2
property float x
property float y
property float z
property uchar red
property uchar green
property uchar blue
property float normal_x
property float new_typed
end_header
1.0 2.0 3.0 255 128 64 0.707 1.0
4.0 5.0 6.0 200 100 50 0.0 2.0
"#;
    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<FlexibleVertex> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 2);
    assert_eq!(vertices[0].new_typed.0, 1.0);
    assert_eq!(vertices[1].new_typed.0, 2.0);
}

#[test]
fn test_transparent_wrappers() {
    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(transparent)]
    struct VertexId(u32);

    #[derive(Deserialize, Debug)]
    #[allow(unused)]
    struct IndexedVertex {
        id: VertexId,
        x: f32,
        y: f32,
        z: f32,
    }

    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property uint id
property float x
property float y
property float z
end_header
42 1.0 2.0 3.0
"#;

    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<IndexedVertex> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0].id, VertexId(42));
}

#[test]
fn test_custom_list_conversion() {
    #[derive(Deserialize, Debug)]
    #[allow(unused)]
    struct VertexWithNormalizedIndices {
        x: f32,
        y: f32,
        z: f32,
        #[serde(deserialize_with = "indices_to_normalized")]
        indices: Vec<f32>,
    }

    fn indices_to_normalized<'de, D>(deserializer: D) -> Result<Vec<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let indices: Vec<u32> = Vec::deserialize(deserializer)?;
        Ok(indices.into_iter().map(|i| i as f32 / 100.0).collect())
    }

    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property float x
property float y
property float z
property list uchar uint indices
end_header
1.0 2.0 3.0 3 100 200 300
"#;

    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<VertexWithNormalizedIndices> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    assert_eq!(vertices[0].indices, vec![1.0, 2.0, 3.0]);
}

// ============================================================================
// 7. flatten
// ============================================================================

#[test]
fn test_serde_flatten_support() {
    use std::collections::HashMap;

    #[derive(Deserialize, Debug, PartialEq)]
    struct VertexWithFlatten {
        x: f32,
        y: f32,
        z: f32,
        #[serde(flatten)]
        extra: HashMap<String, f32>,
    }

    let ply_data = r#"ply
format ascii 1.0
element vertex 1
property float x
property float y
property float z
property float val_0
property float val_1
property float confidence
end_header
1.0 2.0 3.0 10.0 20.0 0.95
"#;

    let cursor = Cursor::new(ply_data);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<VertexWithFlatten> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    let vertex = &vertices[0];
    assert_eq!(vertex.x, 1.0);
    assert_eq!(vertex.y, 2.0);
    assert_eq!(vertex.z, 3.0);

    assert_eq!(vertex.extra.get("val_0"), Some(&10.0));
    assert_eq!(vertex.extra.get("val_1"), Some(&20.0));
    assert_eq!(vertex.extra.get("confidence"), Some(&0.95));
    assert_eq!(vertex.extra.len(), 3);
}

#[test]
fn test_serde_flatten_support_binary() {
    use std::collections::HashMap;

    #[derive(Deserialize, Debug, PartialEq)]
    struct VertexWithFlatten {
        x: f32,
        y: f32,
        z: f32,
        #[serde(flatten)]
        extra: HashMap<String, f32>,
    }

    let ply_data = r#"ply
format binary_little_endian 1.0
element vertex 1
property float x
property float y
property float z
property float val_0
property float val_1
property float confidence
end_header
"#;

    let mut binary_data = Vec::new();
    binary_data.extend_from_slice(&1.0f32.to_le_bytes());
    binary_data.extend_from_slice(&2.0f32.to_le_bytes());
    binary_data.extend_from_slice(&3.0f32.to_le_bytes());
    binary_data.extend_from_slice(&10.0f32.to_le_bytes());
    binary_data.extend_from_slice(&20.0f32.to_le_bytes());
    binary_data.extend_from_slice(&0.95f32.to_le_bytes());

    let mut ply_with_binary = ply_data.as_bytes().to_vec();
    ply_with_binary.extend_from_slice(&binary_data);

    let cursor = Cursor::new(ply_with_binary);
    let mut file = serde_ply::PlyReader::from_reader(BufReader::new(cursor)).unwrap();
    let vertices: Vec<VertexWithFlatten> = file.next_element().unwrap();

    assert_eq!(vertices.len(), 1);
    let vertex = &vertices[0];
    assert_eq!(vertex.x, 1.0);
    assert_eq!(vertex.y, 2.0);
    assert_eq!(vertex.z, 3.0);

    assert_eq!(vertex.extra.get("val_0"), Some(&10.0));
    assert_eq!(vertex.extra.get("val_1"), Some(&20.0));
    assert_eq!(vertex.extra.get("confidence"), Some(&0.95));
    assert_eq!(vertex.extra.len(), 3);
}

// ============================================================================
// 8. Seq path edge cases (binary all-scalar rows)
// ============================================================================

#[test]
fn test_seq_mixed_scalar_types() {
    // Different scalar types have different byte sizes.
    // The seq path must compute offsets correctly across type boundaries.
    #[derive(Deserialize, Debug, PartialEq)]
    struct Mixed {
        a: u8,
        b: f32,
        c: u8,
        d: f64,
    }

    #[derive(Deserialize)]
    struct Ply {
        vertex: Vec<Mixed>,
    }

    let header = b"ply\nformat binary_little_endian 1.0\nelement vertex 1\nproperty uchar a\nproperty float b\nproperty uchar c\nproperty double d\nend_header\n";
    let mut data = header.to_vec();
    data.push(42u8);
    data.extend_from_slice(&3.125f32.to_le_bytes());
    data.push(99u8);
    data.extend_from_slice(&2.719f64.to_le_bytes());

    let ply: Ply = serde_ply::from_bytes(&data).unwrap();
    assert_eq!(ply.vertex[0].a, 42);
    assert!((ply.vertex[0].b - 3.125).abs() < 1e-6);
    assert_eq!(ply.vertex[0].c, 99);
    assert!((ply.vertex[0].d - 2.719).abs() < 1e-10);
}

#[test]
fn test_seq_many_rows_buffer_reuse() {
    // Verify the row buffer is correctly reused across many rows
    // without data leaking between them.
    #[derive(Deserialize, Debug, PartialEq)]
    struct V {
        a: f32,
        b: f32,
    }

    #[derive(Deserialize)]
    struct Ply {
        vertex: Vec<V>,
    }

    let rows: Vec<[f32; 2]> = (0..1000).map(|i| [i as f32, (i * 10) as f32]).collect();
    let row_refs: Vec<&[f32]> = rows.iter().map(|r| r.as_slice()).collect();

    let data = make_binary_ply(
        "ply\nformat binary_little_endian 1.0\nelement vertex 1000\nproperty float a\nproperty float b\nend_header\n",
        &row_refs,
    );

    let ply: Ply = serde_ply::from_bytes(&data).unwrap();
    assert_eq!(ply.vertex.len(), 1000);
    for i in 0..1000 {
        assert_eq!(
            ply.vertex[i],
            V {
                a: i as f32,
                b: (i * 10) as f32
            },
            "row {i}"
        );
    }
}

#[test]
fn test_seq_single_field() {
    #[derive(Deserialize, Debug, PartialEq)]
    struct Single {
        x: f32,
    }

    #[derive(Deserialize)]
    struct Ply {
        vertex: Vec<Single>,
    }

    let data = make_binary_ply(
        "ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nend_header\n",
        &[&[1.0], &[2.0], &[3.0]],
    );

    let ply: Ply = serde_ply::from_bytes(&data).unwrap();
    assert_eq!(
        ply.vertex,
        vec![Single { x: 1.0 }, Single { x: 2.0 }, Single { x: 3.0 }]
    );
}

#[test]
fn test_seq_all_defaults_no_match() {
    // Struct has all default fields, none matching any PLY property.
    // Falls back to string path (no matched fields -> seq plan returns None).
    #[derive(Deserialize, Debug, PartialEq)]
    struct AllDefaults {
        #[serde(default)]
        missing_a: f32,
        #[serde(default)]
        missing_b: f32,
    }

    #[derive(Deserialize)]
    struct Ply {
        vertex: Vec<AllDefaults>,
    }

    let data = make_binary_ply(
        "ply\nformat binary_little_endian 1.0\nelement vertex 1\nproperty float x\nproperty float y\nend_header\n",
        &[&[1.0, 2.0]],
    );

    let ply: Ply = serde_ply::from_bytes(&data).unwrap();
    assert_eq!(
        ply.vertex[0],
        AllDefaults {
            missing_a: 0.0,
            missing_b: 0.0
        }
    );
}
