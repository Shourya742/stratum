// At lower level I generally prefer to work with slices as more efficient than Read/Write streams
// eg: Read for & [u8] is implemented with memcpy but here is more than enough to just get a
// pointer to the original data

// Although enum decode would be easy to implement it is not done since any message can
// be derived from its number!
use crate::Error;
pub mod decodable;
pub mod encodable;
mod impls;
#[cfg(feature = "with_buffer_pool")]
use buffer_sv2::Slice;

/// The `SizeHint` trait provides a mechanism to return the encoded bytes size of a decodable type.
///
/// It defines two methods for retrieving the size of an encoded message:
///
/// - `size_hint` is a static method that takes the raw data and an offset and returns the total
///     size of the encoded message. This is particularly useful for types where the encoded size
///     may vary based on the contents of the data, and we need to calculate how much space is
///     required for decoding.
/// - `size_hint_` is an instance method that performs the same function but allows the size to be
///     be determined with respect to the current instance of the type.
///
/// These methods are crucial in decoding scenarios where the full size of the message
/// is not immediately known, helping to determine how many bytes need to be read.
pub trait SizeHint {
    fn size_hint(data: &[u8], offset: usize) -> Result<usize, Error>;
    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error>;
}

/// The `GetSize` trait returns the total size in bytes of an encodable type.
///
/// It provides a single method, `get_size`, which returns the total size of the type
/// in bytes.
pub trait GetSize {
    fn get_size(&self) -> usize;
}

#[cfg(feature = "with_buffer_pool")]
impl GetSize for Slice {
    /// Provides the total size of a `Slice` by returning its length.
    ///
    /// This implementation for the `Slice` type returns the number of bytes in the
    /// slice, which represents its total size when encoded.
    fn get_size(&self) -> usize {
        self.len()
    }
}
/// The `Fixed` trait is implemented by all primitives with a constant, fixed size.
///
/// Types implementing this trait must define the constant `SIZE`, representing the
/// fixed number of bytes needed to encode or decode the type. This trait is used for
/// types that have a know size at compile time , such as integers, fixed-size arrays, etc.
pub trait Fixed {
    const SIZE: usize;
}

/// The `Variable` trait is designed for types that have variable size when  encoded.
///
/// Types implementing this trait provide the following:
///
/// - `HEADER_SIZE`: The size of the header in bytes. This header often contains metadata like
///     the length of the variables-sized data.
/// - `MAX_SIZE`: The maximum allowed size for this type. This value is essential in
///     ensuring that dynamically sized data does not exceed defined limits.
///
/// The trait also includes methods to calculate the inner size of the data (`inner_size`) and
/// to return the header as a byte vector (`get_header`). These methods are essential for
/// managing dynamically sized types in scenarios like serialization and encoding.
pub trait Variable {
    const HEADER_SIZE: usize;
    //const ELEMENT_SIZE: usize;
    const MAX_SIZE: usize;

    fn inner_size(&self) -> usize;

    /// Retrieves the header as a byte vector. This header typically contains information
    /// about the size or type of the data that follows.
    fn get_header(&self) -> Vec<u8>;
}

impl<T: Fixed> SizeHint for T {
    /// Provides the size hind for a fixed-size type
    ///
    /// Since the type implementing `Fixed` has a constant size, the `size_hint` method
    /// simply returns the fixed size, making it easy to predict the size of the encoded data.
    fn size_hint(_data: &[u8], _offset: usize) -> Result<usize, Error> {
        Ok(Self::SIZE)
    }

    ///Instance-based size hint for a fixed-size type.
    ///
    /// Similar to the static `size_hint_`, this method returns the constant size for
    /// the specific instance of the type.
    fn size_hint_(&self, _: &[u8], _offset: usize) -> Result<usize, Error> {
        Ok(Self::SIZE)
    }
}

impl<T: Fixed> GetSize for T {
    /// Returns the size of the fixed-size type.
    ///
    /// As the type implements `Fixed`, this method directly returns the constant `SIZE`
    /// associated with the type. This is useful when encoding or decoding to know exactly
    /// how much space the type occupies in memory or when serialized.
    fn get_size(&self) -> usize {
        Self::SIZE
    }
}
