use crate::{DeserializeError, ScalarType};
use std::io::Read;
use std::marker::PhantomData;

use byteorder::ByteOrder;
use byteorder::ReadBytesExt;

pub(crate) struct BinValReader<E: ByteOrder> {
    _endian: PhantomData<E>,
}

pub(crate) struct AsciiValReader {}

pub(crate) trait ScalarReader {
    fn read_i8(reader: impl Read) -> Result<i8, std::io::Error>;
    fn read_u8(reader: impl Read) -> Result<u8, std::io::Error>;
    fn read_i16(reader: impl Read) -> Result<i16, std::io::Error>;
    fn read_u16(reader: impl Read) -> Result<u16, std::io::Error>;
    fn read_i32(reader: impl Read) -> Result<i32, std::io::Error>;
    fn read_u32(reader: impl Read) -> Result<u32, std::io::Error>;
    fn read_f32(data: impl Read) -> Result<f32, std::io::Error>;
    fn read_f64(reader: impl Read) -> Result<f64, std::io::Error>;

    /// Fixed byte size of a scalar type in the ply file, if known.
    fn scalar_byte_size(_t: ScalarType) -> Option<usize> {
        None
    }

    fn read_count(reader: impl Read, t: ScalarType) -> Result<usize, DeserializeError> {
        let count: i64 = match t {
            ScalarType::I8 => Self::read_i8(reader)? as i64,
            ScalarType::U8 => Self::read_u8(reader)? as i64,
            ScalarType::I16 => Self::read_i16(reader)? as i64,
            ScalarType::U16 => Self::read_u16(reader)? as i64,
            ScalarType::I32 => Self::read_i32(reader)? as i64,
            ScalarType::U32 => Self::read_u32(reader)? as i64,
            _ => {
                return Err(DeserializeError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "List count cannot be a float",
                )))
            }
        };
        if count < 0 {
            return Err(DeserializeError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Negative list count",
            )));
        }
        Ok(count as usize)
    }
}

impl<E: ByteOrder> ScalarReader for BinValReader<E> {
    fn scalar_byte_size(t: ScalarType) -> Option<usize> {
        Some(match t {
            ScalarType::I8 | ScalarType::U8 => 1,
            ScalarType::I16 | ScalarType::U16 => 2,
            ScalarType::I32 | ScalarType::U32 | ScalarType::F32 => 4,
            ScalarType::F64 => 8,
        })
    }

    fn read_i8(mut reader: impl Read) -> Result<i8, std::io::Error> {
        reader.read_i8()
    }

    fn read_u8(mut reader: impl Read) -> Result<u8, std::io::Error> {
        reader.read_u8()
    }

    fn read_i16(mut reader: impl Read) -> Result<i16, std::io::Error> {
        reader.read_i16::<E>()
    }

    fn read_u16(mut reader: impl Read) -> Result<u16, std::io::Error> {
        reader.read_u16::<E>()
    }

    fn read_i32(mut reader: impl Read) -> Result<i32, std::io::Error> {
        reader.read_i32::<E>()
    }

    fn read_u32(mut reader: impl Read) -> Result<u32, std::io::Error> {
        reader.read_u32::<E>()
    }

    fn read_f32(mut reader: impl Read) -> Result<f32, std::io::Error> {
        reader.read_f32::<E>()
    }

    fn read_f64(mut reader: impl Read) -> Result<f64, std::io::Error> {
        reader.read_f64::<E>()
    }
}

impl ScalarReader for AsciiValReader {
    fn read_i8(reader: impl Read) -> Result<i8, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<i8>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse i8 from ASCII",
            )
        })
    }

    fn read_u8(reader: impl Read) -> Result<u8, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<u8>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse u8 from ASCII",
            )
        })
    }

    fn read_i16(reader: impl Read) -> Result<i16, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<i16>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse i16 from ASCII",
            )
        })
    }

    fn read_u16(reader: impl Read) -> Result<u16, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<u16>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse u16 from ASCII",
            )
        })
    }

    fn read_i32(reader: impl Read) -> Result<i32, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<i32>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse i32 from ASCII",
            )
        })
    }

    fn read_u32(reader: impl Read) -> Result<u32, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<u32>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse u32 from ASCII",
            )
        })
    }

    fn read_f32(reader: impl Read) -> Result<f32, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<f32>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse f32 from ASCII",
            )
        })
    }

    fn read_f64(reader: impl Read) -> Result<f64, std::io::Error> {
        Self::read_ascii_token(reader)?.parse::<f64>().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Failed to parse f64 from ASCII",
            )
        })
    }
}

impl AsciiValReader {
    fn read_ascii_token(mut reader: impl Read) -> Result<String, std::io::Error> {
        let mut token = String::new();

        loop {
            let mut byte = [0u8; 1];
            reader.read_exact(&mut byte)?;
            if byte[0].is_ascii_whitespace() {
                if !token.is_empty() {
                    break;
                }
            } else {
                token.push(byte[0] as char);
            }
        }

        if token.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No valid ASCII token found",
            ));
        }

        Ok(token)
    }
}
