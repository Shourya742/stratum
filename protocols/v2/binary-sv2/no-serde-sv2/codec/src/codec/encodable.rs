use crate::{
    codec::GetSize,
    datatypes::{
        ShortTxId, Signature, Sv2DataType, U32AsRef, B016M, B0255, B032, B064K, U24, U256,
    },
    Error,
};
use alloc::vec::Vec;
#[cfg(not(feature = "no_std"))]
use std::io::{Error as E, Write};

/// The `Encodable` trait defines the interface for encoding a type into bytes.
///
/// The trait provides methods for serializing an instance of a type into a byte
/// array or writing it directly into an output writer. The trait is flexible,
/// allowing various types, including primitives, structures, and collections,
/// to implement custom serialization logic.
///
/// The trait offers two key methods for encoding:
///
/// - The first, `to_bytes`, takes a mutable byte slice as a destination buffer.
///   This method encodes the object directly into the provided buffer, returning
///   the number of bytes written or an error if the encoding process fails.
/// - The second, `to_writer`, (only available when not compiling for `no-std`)
///   accepts a writer as a destination for the encoded bytes, allowing the
///   serialized data to be written to any implementor of the `Write` trait.
///
/// Implementing types can define custom encoding logic, and this trait is
/// especially useful when dealing with different data structures that need
/// to be serialized for transmission.
pub trait Encodable {
    /// Encodes the object into the provided byte slice.
    ///
    /// The method uses the destination buffer `dst` to write the serialized
    /// bytes. It returns the number of bytes written on success or an `Error`
    /// if encoding fails.
    #[allow(clippy::wrong_self_convention)]
    fn to_bytes(self, dst: &mut [u8]) -> Result<usize, Error>;

    /// Write the encoded object into the provided writer.
    ///
    /// This method serializes the object and writes it directly
    /// to the `dst` writer. It is only available in environments
    /// where `std` is available. If the encoding fails, error is
    /// returned.
    #[cfg(not(feature = "no_std"))]
    #[allow(clippy::wrong_self_convention)]
    fn to_writer(self, dst: &mut impl Write) -> Result<(), E>;
}

//
impl<'a, T: Into<EncodableField<'a>>> Encodable for T {
    #[allow(clippy::wrong_self_convention)]
    fn to_bytes(self, dst: &mut [u8]) -> Result<usize, Error> {
        let encoded_field = self.into();
        encoded_field.encode(dst, 0)
    }

    #[cfg(not(feature = "no_std"))]
    #[allow(clippy::wrong_self_convention, unconditional_recursion)]
    fn to_writer(self, dst: &mut impl Write) -> Result<(), E> {
        let encoded_field = self.into();
        encoded_field.to_writer(dst)
    }
}

/// The `EncodablePrimitive` enum defines primitive types  that can be encoded.
///
/// The enum represents various data types, such a integers, bool, and byte array
/// that can be encoded into a byte representation. Each variant holds a specific
/// type, and encoding logic is provided through the `encode` method.
#[derive(Debug)]
pub enum EncodablePrimitive<'a> {
    U8(u8),
    OwnedU8(u8),
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

impl<'a> EncodablePrimitive<'a> {
    /// Provides the encoding logic for each primitive type.
    ///
    /// The `encode` method takes the `EncodablePrimitive` variant and serializes it
    /// into the destination buffer `dst`. The method returns the number of bytes written
    /// . If the buffer is too small or encoding fails, it returns an error.
    fn encode(&self, dst: &mut [u8]) -> Result<usize, Error> {
        match self {
            Self::U8(v) => v.to_slice(dst),
            Self::OwnedU8(v) => v.to_slice(dst),
            Self::U16(v) => v.to_slice(dst),
            Self::Bool(v) => v.to_slice(dst),
            Self::U24(v) => v.to_slice(dst),
            Self::U256(v) => v.to_slice(dst),
            Self::ShortTxId(v) => v.to_slice(dst),
            Self::Signature(v) => v.to_slice(dst),
            Self::U32(v) => v.to_slice(dst),
            Self::U32AsRef(v) => v.to_slice(dst),
            Self::F32(v) => v.to_slice(dst),
            Self::U64(v) => v.to_slice(dst),
            Self::B032(v) => v.to_slice(dst),
            Self::B0255(v) => v.to_slice(dst),
            Self::B064K(v) => v.to_slice(dst),
            Self::B016M(v) => v.to_slice(dst),
        }
    }

    /// Write the encoded object into the provided writer.
    ///
    /// This method serializes the object and writes it directly to the
    /// provided writer. It is only available in environments where `std`
    /// is available.
    #[cfg(not(feature = "no_std"))]
    pub fn write(&self, writer: &mut impl Write) -> Result<(), E> {
        match self {
            Self::U8(v) => v.to_writer_(writer),
            Self::OwnedU8(v) => v.to_writer_(writer),
            Self::U16(v) => v.to_writer_(writer),
            Self::Bool(v) => v.to_writer_(writer),
            Self::U24(v) => v.to_writer_(writer),
            Self::U256(v) => v.to_writer_(writer),
            Self::ShortTxId(v) => v.to_writer_(writer),
            Self::Signature(v) => v.to_writer_(writer),
            Self::U32(v) => v.to_writer_(writer),
            Self::U32AsRef(v) => v.to_writer_(writer),
            Self::F32(v) => v.to_writer_(writer),
            Self::U64(v) => v.to_writer_(writer),
            Self::B032(v) => v.to_writer_(writer),
            Self::B0255(v) => v.to_writer_(writer),
            Self::B064K(v) => v.to_writer_(writer),
            Self::B016M(v) => v.to_writer_(writer),
        }
    }
}

impl<'a> GetSize for EncodablePrimitive<'a> {
    fn get_size(&self) -> usize {
        match self {
            Self::U8(v) => v.get_size(),
            Self::OwnedU8(v) => v.get_size(),
            Self::U16(v) => v.get_size(),
            Self::Bool(v) => v.get_size(),
            Self::U24(v) => v.get_size(),
            Self::U256(v) => v.get_size(),
            Self::ShortTxId(v) => v.get_size(),
            Self::Signature(v) => v.get_size(),
            Self::U32(v) => v.get_size(),
            Self::U32AsRef(v) => v.get_size(),
            Self::F32(v) => v.get_size(),
            Self::U64(v) => v.get_size(),
            Self::B032(v) => v.get_size(),
            Self::B0255(v) => v.get_size(),
            Self::B064K(v) => v.get_size(),
            Self::B016M(v) => v.get_size(),
        }
    }
}

/// The `EncodableField` enum defines encodable fields, which may either be primitive
/// values or structured collections.
///
/// Each `EncodableField` represents either a primitive value or a collection of values
/// (a structure). The encoding process for `EncodableField` supports nesting, allowing
/// for complex hierarchical data structures to be serialized.
#[derive(Debug)]
pub enum EncodableField<'a> {
    Primitive(EncodablePrimitive<'a>),
    Struct(Vec<EncodableField<'a>>),
}

/// Provides the encoding logic for fields
///
/// The `encode` method serializes a field into the destination buffer `dst`, starting
/// at the provided `offset`. If the field is a structure, it recursively encodes
/// each contained field. If the buffer is too small or encoding fails, the method
/// returns an error.
impl<'a> EncodableField<'a> {
    pub fn encode(&self, dst: &mut [u8], mut offset: usize) -> Result<usize, Error> {
        match (self, dst.len() >= offset) {
            (Self::Primitive(p), true) => p.encode(&mut dst[offset..]),
            (Self::Struct(ps), true) => {
                let mut result = 0;
                for p in ps {
                    let encoded_bytes = p.encode(dst, offset)?;
                    offset += encoded_bytes;
                    result += encoded_bytes;
                }
                Ok(result)
            }
            (_, false) => Err(Error::WriteError(offset, dst.len())),
        }
    }

    #[cfg(not(feature = "no_std"))]
    pub fn to_writer(&self, writer: &mut impl Write) -> Result<(), E> {
        match self {
            Self::Primitive(p) => p.write(writer),
            Self::Struct(ps) => {
                for p in ps {
                    p.to_writer(writer)?;
                }
                Ok(())
            }
        }
    }
}

/// Provides the logic for calculating the size of the encodable field.
///
/// The `get_size` method returns the size in bytes required to encode the field.
/// For structucred fields, it calculates the total size of all contained fields.
impl<'a> GetSize for EncodableField<'a> {
    fn get_size(&self) -> usize {
        match self {
            Self::Primitive(p) => p.get_size(),
            Self::Struct(ps) => {
                let mut size = 0;
                for p in ps {
                    size += p.get_size();
                }
                size
            }
        }
    }
}
