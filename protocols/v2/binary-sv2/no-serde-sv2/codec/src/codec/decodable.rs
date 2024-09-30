use crate::{
    codec::{GetSize, SizeHint},
    datatypes::{
        ShortTxId, Signature, Sv2DataType, U32AsRef, B016M, B0255, B032, B064K, U24, U256,
    },
    Error,
};
use alloc::vec::Vec;
use std::convert::TryFrom;
#[cfg(not(feature = "no_std"))]
use std::io::{Cursor, Read};

/// Implmented by all the decodable structure, it can be derived for any structure composed only
/// of primitives or other Decodable types. It defines methods to parse the structure from raw
/// data and reconstruct it from decoded fields.
pub trait Decodable<'a>: Sized {
    fn get_structure(data: &[u8]) -> Result<Vec<FieldMarker>, Error>;

    fn from_decoded_fields(data: Vec<DecodableField<'a>>) -> Result<Self, Error>;

    /// Parses a structure from raw bytes by iterating through its fields and decoding them.
    /// Splits the data based on field size and decodes each segment.
    fn from_bytes(data: &'a mut [u8]) -> Result<Self, Error> {
        let structure = Self::get_structure(data)?;
        let mut fields = Vec::new();
        let mut tail = data;

        for field in structure {
            let field_size = field.size_hint_(tail, 0)?;
            if field_size > tail.len() {
                return Err(Error::DecodableConversionError);
            }
            let (head, t) = tail.split_at_mut(field_size);
            tail = t;
            fields.push(field.decode(head)?);
        }
        Self::from_decoded_fields(fields)
    }

    /// Reads a structure from a reader stream, Reads all available data into a buffer,
    /// determines the structure, and then decodes each field from the buffer.
    #[cfg(not(feature = "no_std"))]
    fn from_reader(reader: &mut impl Read) -> Result<Self, Error> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;

        let structure = Self::get_structure(&data[..])?;

        let mut fields = Vec::new();
        let mut reader = Cursor::new(data);

        for field in structure {
            fields.push(field.from_reader(&mut reader)?);
        }
        Self::from_decoded_fields(fields)
    }
}

/// Enum representing different types of primitive markers.
/// Used to define the structure of primitive data types for decoding.
#[derive(Debug, Clone, Copy)]
pub enum PrimitiveMarker {
    U8,
    U16,
    Bool,
    U24,
    U256,
    ShortTxId,
    Signature,
    U32,
    U32AsRef,
    F32,
    U64,
    B032,
    B0255,
    B064K,
    B016M,
}

/// Enum representing field markers, which define the structure of a data field.
/// Fields can be primitives or nested structures.
#[derive(Debug, Clone)]
pub enum FieldMarker {
    Primitive(PrimitiveMarker),
    Struct(Vec<FieldMarker>),
}

/// A trait that provides a mechanism to retrieve the marker associated with a data field.
/// This marker is used to help the decoder to identify and interpret the field type.
pub trait GetMarker {
    fn get_marker() -> FieldMarker;
}

/// Represents a decoded primitive data type, used by the decoder to construct messages,
/// Includes various types like integers, floats and binary data.
#[derive(Debug)]
pub enum DecodablePrimitive<'a> {
    U8(u8),
    U16(u16),
    Bool(bool),
    U24(U24),
    U256(U256<'a>),
    ShortTxId(ShortTxId<'a>),
    Signature(Signature<'a>),
    U32(u32),
    U32AsRef(U32AsRef<'a>),
    F32(f32),
    U64(u64),
    B032(B032<'a>),
    B0255(B0255<'a>),
    B064K(B064K<'a>),
    B016M(B016M<'a>),
}

/// Represents a decoded field, which may either be a primitive or a nested structure.
/// The decoder uses this to build the final decoded data.
#[derive(Debug)]
pub enum DecodableField<'a> {
    Primitive(DecodablePrimitive<'a>),
    Struct(Vec<DecodableField<'a>>),
}

/// Provide a size hint for each primitive marker.
/// This method helps estimate the size of the data field represented by the marker.
impl SizeHint for PrimitiveMarker {
    // PrimitiveMarker need introspection to return a size hint. This method is not implementeable
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        unimplemented!()
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        match self {
            Self::U8 => u8::size_hint(data, offset),
            Self::U16 => u16::size_hint(data, offset),
            Self::Bool => bool::size_hint(data, offset),
            Self::U24 => U24::size_hint(data, offset),
            Self::U256 => U256::size_hint(data, offset),
            Self::ShortTxId => ShortTxId::size_hint(data, offset),
            Self::Signature => Signature::size_hint(data, offset),
            Self::U32 => u32::size_hint(data, offset),
            Self::U32AsRef => U32AsRef::size_hint(data, offset),
            Self::F32 => f32::size_hint(data, offset),
            Self::U64 => u64::size_hint(data, offset),
            Self::B032 => B032::size_hint(data, offset),
            Self::B0255 => B0255::size_hint(data, offset),
            Self::B064K => B064K::size_hint(data, offset),
            Self::B016M => B016M::size_hint(data, offset),
        }
    }
}

/// Provides a size hint for each field marker, which may be a primitive or a nested structure.
/// Used to estimate the total size of data associated with a field.
impl SizeHint for FieldMarker {
    // FieldMarker need introspection to return a size hint. This method is not implementeable
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        unimplemented!()
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        match self {
            Self::Primitive(p) => p.size_hint_(data, offset),
            Self::Struct(ps) => {
                let mut size = 0;
                for p in ps {
                    size += p.size_hint_(data, offset + size)?;
                }
                Ok(size)
            }
        }
    }
}

/// Implements size hinting for a vector of field markers, summing the size of individual marker.
impl SizeHint for Vec<FieldMarker> {
    // FieldMarker need introspection to return a size hint. This method is not implementeable
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        unimplemented!()
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        let mut size = 0;
        for field in self {
            let field_size = field.size_hint_(data, offset + size)?;
            size += field_size;
        }
        Ok(size)
    }
}

/// Converts a `PrimitiveMarker` into a `FieldMarker`
impl From<PrimitiveMarker> for FieldMarker {
    fn from(v: PrimitiveMarker) -> Self {
        FieldMarker::Primitive(v)
    }
}

/// Attempts to convert a vector of field markers into a single field marker, representing a structure.
/// Returns an error if the vector is empty.
impl TryFrom<Vec<FieldMarker>> for FieldMarker {
    type Error = crate::Error;

    fn try_from(mut v: Vec<FieldMarker>) -> Result<Self, crate::Error> {
        match v.len() {
            // It shouldn't be possible to call this function with a void Vec but for safety
            // reasons it is implemented with TryFrom and not From if needed should be possible
            // to use From and just panic
            0 => Err(crate::Error::VoidFieldMarker),
            // This is always safe: if v.len is 1 pop can not fail
            1 => Ok(v.pop().unwrap()),
            _ => Ok(FieldMarker::Struct(v)),
        }
    }
}

impl<'a> From<DecodableField<'a>> for Vec<DecodableField<'a>> {
    fn from(v: DecodableField<'a>) -> Self {
        match v {
            DecodableField::Primitive(p) => vec![DecodableField::Primitive(p)],
            DecodableField::Struct(ps) => ps,
        }
    }
}

/// Defines the decoding process for a primitive marker, which parses a segment of data
/// and returns the corresponding `DecodablePrimitive`.
impl PrimitiveMarker {
    fn decode<'a>(&self, data: &'a mut [u8], offset: usize) -> DecodablePrimitive<'a> {
        match self {
            Self::U8 => DecodablePrimitive::U8(u8::from_bytes_unchecked(&mut data[offset..])),
            Self::U16 => DecodablePrimitive::U16(u16::from_bytes_unchecked(&mut data[offset..])),
            Self::Bool => DecodablePrimitive::Bool(bool::from_bytes_unchecked(&mut data[offset..])),
            Self::U24 => DecodablePrimitive::U24(U24::from_bytes_unchecked(&mut data[offset..])),
            Self::U256 => DecodablePrimitive::U256(U256::from_bytes_unchecked(&mut data[offset..])),
            Self::ShortTxId => {
                DecodablePrimitive::ShortTxId(ShortTxId::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::Signature => {
                DecodablePrimitive::Signature(Signature::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::U32 => DecodablePrimitive::U32(u32::from_bytes_unchecked(&mut data[offset..])),
            Self::U32AsRef => {
                DecodablePrimitive::U32AsRef(U32AsRef::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::F32 => DecodablePrimitive::F32(f32::from_bytes_unchecked(&mut data[offset..])),
            Self::U64 => DecodablePrimitive::U64(u64::from_bytes_unchecked(&mut data[offset..])),
            Self::B032 => DecodablePrimitive::B032(B032::from_bytes_unchecked(&mut data[offset..])),
            Self::B0255 => {
                DecodablePrimitive::B0255(B0255::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::B064K => {
                DecodablePrimitive::B064K(B064K::from_bytes_unchecked(&mut data[offset..]))
            }
            Self::B016M => {
                DecodablePrimitive::B016M(B016M::from_bytes_unchecked(&mut data[offset..]))
            }
        }
    }

    #[allow(clippy::wrong_self_convention)]
    #[cfg(not(feature = "no_std"))]
    #[allow(clippy::wrong_self_convention)]
    fn from_reader<'a>(&self, reader: &mut impl Read) -> Result<DecodablePrimitive<'a>, Error> {
        match self {
            Self::U8 => Ok(DecodablePrimitive::U8(u8::from_reader_(reader)?)),
            Self::U16 => Ok(DecodablePrimitive::U16(u16::from_reader_(reader)?)),
            Self::Bool => Ok(DecodablePrimitive::Bool(bool::from_reader_(reader)?)),
            Self::U24 => Ok(DecodablePrimitive::U24(U24::from_reader_(reader)?)),
            Self::U256 => Ok(DecodablePrimitive::U256(U256::from_reader_(reader)?)),
            Self::ShortTxId => Ok(DecodablePrimitive::ShortTxId(ShortTxId::from_reader_(
                reader,
            )?)),
            Self::Signature => Ok(DecodablePrimitive::Signature(Signature::from_reader_(
                reader,
            )?)),
            Self::U32 => Ok(DecodablePrimitive::U32(u32::from_reader_(reader)?)),
            Self::U32AsRef => Ok(DecodablePrimitive::U32AsRef(U32AsRef::from_reader_(
                reader,
            )?)),
            Self::F32 => Ok(DecodablePrimitive::F32(f32::from_reader_(reader)?)),
            Self::U64 => Ok(DecodablePrimitive::U64(u64::from_reader_(reader)?)),
            Self::B032 => Ok(DecodablePrimitive::B032(B032::from_reader_(reader)?)),
            Self::B0255 => Ok(DecodablePrimitive::B0255(B0255::from_reader_(reader)?)),
            Self::B064K => Ok(DecodablePrimitive::B064K(B064K::from_reader_(reader)?)),
            Self::B016M => Ok(DecodablePrimitive::B016M(B016M::from_reader_(reader)?)),
        }
    }
}

impl<'a> GetSize for DecodablePrimitive<'a> {
    fn get_size(&self) -> usize {
        match self {
            DecodablePrimitive::U8(v) => v.get_size(),
            DecodablePrimitive::U16(v) => v.get_size(),
            DecodablePrimitive::Bool(v) => v.get_size(),
            DecodablePrimitive::U24(v) => v.get_size(),
            DecodablePrimitive::U256(v) => v.get_size(),
            DecodablePrimitive::ShortTxId(v) => v.get_size(),
            DecodablePrimitive::Signature(v) => v.get_size(),
            DecodablePrimitive::U32(v) => v.get_size(),
            DecodablePrimitive::U32AsRef(v) => v.get_size(),
            DecodablePrimitive::F32(v) => v.get_size(),
            DecodablePrimitive::U64(v) => v.get_size(),
            DecodablePrimitive::B032(v) => v.get_size(),
            DecodablePrimitive::B0255(v) => v.get_size(),
            DecodablePrimitive::B064K(v) => v.get_size(),
            DecodablePrimitive::B016M(v) => v.get_size(),
        }
    }
}

/// Provides decoding functionality for a field marker by using the marker type to decode the
/// corresponding data and return a DecodableFiel
impl FieldMarker {
    pub(crate) fn decode<'a>(&self, data: &'a mut [u8]) -> Result<DecodableField<'a>, Error> {
        match self {
            Self::Primitive(p) => Ok(DecodableField::Primitive(p.decode(data, 0))),
            Self::Struct(ps) => {
                let mut decodeds = Vec::new();
                let mut tail = data;
                for p in ps {
                    let field_size = p.size_hint_(tail, 0)?;
                    let (head, t) = tail.split_at_mut(field_size);
                    tail = t;
                    decodeds.push(p.decode(head)?);
                }
                Ok(DecodableField::Struct(decodeds))
            }
        }
    }

    #[allow(clippy::wrong_self_convention)]
    #[cfg(not(feature = "no_std"))]
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_reader<'a>(
        &self,
        reader: &mut impl Read,
    ) -> Result<DecodableField<'a>, Error> {
        match self {
            Self::Primitive(p) => Ok(DecodableField::Primitive(p.from_reader(reader)?)),
            Self::Struct(ps) => {
                let mut decodeds = Vec::new();
                for p in ps {
                    decodeds.push(p.from_reader(reader)?);
                }
                Ok(DecodableField::Struct(decodeds))
            }
        }
    }
}
