//! Hand-written wire primitives shared by every generated packet.
//!
//! The layout follows `Protocol/src/main/java/com/hypixel/hytale/protocol/io/`:
//! fixed fields are little-endian, UUIDs are two big-endian `i64`s, varints are
//! non-negative 7-bit little-endian groups (at most five bytes), fixed strings
//! are zero-padded, and variable strings/arrays are varint-prefixed and bounded.
//!
//! Every read is bounds-checked. Invalid input returns [`ProtocolError`]; it
//! never panics.

use std::fmt;

/// Errors produced by the wire layer.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// Not enough bytes remained for the requested value.
    #[error("unexpected end of input: needed {needed} bytes, {remaining} remain")]
    UnexpectedEof {
        /// Bytes required.
        needed: usize,
        /// Bytes available.
        remaining: usize,
    },
    /// A varint exceeded the five-byte protocol limit or was truncated.
    #[error("invalid varint: {0}")]
    InvalidVarInt(&'static str),
    /// A varint was negative (encoding only).
    #[error("varint cannot encode a negative value: {0}")]
    NegativeVarInt(i64),
    /// A length or count exceeded its field bound.
    #[error("{field} length {length} exceeds maximum {max}")]
    LengthTooLarge {
        /// Field name.
        field: &'static str,
        /// Observed length.
        length: usize,
        /// Allowed maximum.
        max: usize,
    },
    /// A fixed string did not fit its slot.
    #[error("{field} exceeds fixed length {length}: {} bytes", value_len)]
    FixedStringTooLong {
        /// Field name.
        field: &'static str,
        /// Fixed slot length.
        length: usize,
        /// Encoded byte length.
        value_len: usize,
    },
    /// A string declared as ASCII contained a non-ASCII byte.
    #[error("{field} contains a non-ASCII byte")]
    InvalidAscii {
        /// Field name.
        field: &'static str,
    },
    /// A variable UTF-8 string was malformed.
    #[error("{field} is not well-formed UTF-8")]
    InvalidUtf8 {
        /// Field name.
        field: &'static str,
    },
    /// A nullable-bitset index was outside the bitfield.
    #[error("nullable bit index {index} is outside {bytes} bytes")]
    NullableIndex {
        /// Bit index.
        index: usize,
        /// Bitfield size in bytes.
        bytes: usize,
    },
    /// A frame length was negative or larger than the protocol maximum.
    #[error("invalid frame payload length {length} (max {max})")]
    InvalidFrameLength {
        /// Declared payload length.
        length: u32,
        /// Allowed maximum.
        max: u32,
    },
    /// A frame payload exceeded the per-packet maximum.
    #[error("packet {packet_id} payload length {length} exceeds max {max}")]
    PayloadTooLarge {
        /// Packet id.
        packet_id: u32,
        /// Declared payload length.
        length: u32,
        /// Allowed maximum.
        max: u32,
    },
    /// zstd compression failed.
    #[error("zstd compression failed: {0}")]
    Compression(String),
    /// zstd decompression failed.
    #[error("zstd decompression failed: {0}")]
    Decompression(String),
    /// A compressed frame did not declare its uncompressed size.
    #[error("zstd frame does not declare a content size")]
    MissingContentSize,
    /// A decompressed payload exceeded the caller's bound.
    #[error("decompressed payload {length} exceeds maximum {max}")]
    DecompressedTooLarge {
        /// Actual length.
        length: u64,
        /// Allowed maximum.
        max: usize,
    },
    /// An enum field carried a value with no declared variant.
    #[error("invalid {type_name} enum value {value}")]
    InvalidEnum {
        /// Enum type name.
        type_name: &'static str,
        /// Wire value.
        value: i64,
    },
    /// A variable offset slot disagreed with the canonical field-order position.
    #[error("{field} offset slot is {found}, expected {expected}")]
    NonCanonicalOffset {
        /// Field name.
        field: &'static str,
        /// Stored slot.
        found: i32,
        /// Position reached by the field-order walk.
        expected: i32,
    },
    /// A map contained the same key twice.
    #[error("{field} contains duplicate key {key:?}")]
    DuplicateMapKey {
        /// Field name.
        field: &'static str,
        /// Repeated key.
        key: String,
    },
    /// A generated fixed-block write exceeded the fixed block.
    #[error("fixed block write at {offset} for {length} bytes exceeds {size}")]
    FixedBlockOverflow {
        /// Destination offset.
        offset: usize,
        /// Bytes to write.
        length: usize,
        /// Fixed block size.
        size: usize,
    },
}

/// Result alias for the wire layer.
pub type Result<T> = std::result::Result<T, ProtocolError>;

/// A 128-bit UUID in Java's two-`i64` representation.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Uuid {
    /// Most significant 64 bits.
    pub most_significant: i64,
    /// Least significant 64 bits.
    pub least_significant: i64,
}

impl Uuid {
    /// Construct a UUID from Java's most/least significant longs.
    pub const fn new(most_significant: i64, least_significant: i64) -> Self {
        Self {
            most_significant,
            least_significant,
        }
    }

    /// The 16 wire bytes in RFC 4122 order (two big-endian longs).
    pub fn to_bytes(self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&self.most_significant.to_be_bytes());
        bytes[8..].copy_from_slice(&self.least_significant.to_be_bytes());
        bytes
    }

    /// Read a UUID from 16 wire bytes.
    pub fn from_bytes(bytes: &[u8; 16]) -> Self {
        let mut most = [0u8; 8];
        most.copy_from_slice(&bytes[..8]);
        let mut least = [0u8; 8];
        least.copy_from_slice(&bytes[8..]);
        Self {
            most_significant: i64::from_be_bytes(most),
            least_significant: i64::from_be_bytes(least),
        }
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.to_bytes();
        write!(
            formatter,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        )
    }
}

/// Bounds-checked reader over a byte slice.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    /// Create a reader at offset zero.
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    /// Current byte offset.
    pub const fn position(&self) -> usize {
        self.position
    }

    /// Bytes left.
    pub const fn remaining(&self) -> usize {
        self.data.len() - self.position
    }

    /// True when all bytes have been consumed.
    pub const fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Move to an absolute byte offset inside the input.
    pub fn seek(&mut self, position: usize) -> Result<()> {
        if position > self.data.len() {
            return Err(ProtocolError::UnexpectedEof {
                needed: position,
                remaining: self.data.len(),
            });
        }
        self.position = position;
        Ok(())
    }

    /// Advance without reading.
    fn skip(&mut self, count: usize) -> Result<()> {
        if self.remaining() < count {
            return Err(ProtocolError::UnexpectedEof {
                needed: count,
                remaining: self.remaining(),
            });
        }
        self.position += count;
        Ok(())
    }

    /// Read exactly `count` bytes.
    pub fn read_slice(&mut self, count: usize) -> Result<&'a [u8]> {
        if self.remaining() < count {
            return Err(ProtocolError::UnexpectedEof {
                needed: count,
                remaining: self.remaining(),
            });
        }
        let slice = &self.data[self.position..self.position + count];
        self.position += count;
        Ok(slice)
    }

    /// Read an unsigned byte.
    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_slice(1)?[0])
    }

    /// Read a signed byte.
    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    /// Read a little-endian `u16`.
    pub fn read_u16(&mut self) -> Result<u16> {
        let mut bytes = [0u8; 2];
        bytes.copy_from_slice(self.read_slice(2)?);
        Ok(u16::from_le_bytes(bytes))
    }

    /// Read a little-endian `i16`.
    pub fn read_i16(&mut self) -> Result<i16> {
        Ok(self.read_u16()? as i16)
    }

    /// Read a big-endian `u16` (storage container fields).
    pub fn read_u16_be(&mut self) -> Result<u16> {
        let mut bytes = [0u8; 2];
        bytes.copy_from_slice(self.read_slice(2)?);
        Ok(u16::from_be_bytes(bytes))
    }

    /// Read a big-endian `u64` (storage container fields).
    pub fn read_u64_be(&mut self) -> Result<u64> {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(self.read_slice(8)?);
        Ok(u64::from_be_bytes(bytes))
    }

    /// Read a big-endian `i16`.
    pub fn read_i16_be(&mut self) -> Result<i16> {
        Ok(self.read_u16_be()? as i16)
    }

    /// Read a big-endian `i32`.
    pub fn read_i32_be(&mut self) -> Result<i32> {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(self.read_slice(4)?);
        Ok(i32::from_be_bytes(bytes))
    }

    /// Read a big-endian `i64`.
    pub fn read_i64_be(&mut self) -> Result<i64> {
        Ok(self.read_u64_be()? as i64)
    }

    /// Read a little-endian `u32`.
    pub fn read_u32(&mut self) -> Result<u32> {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(self.read_slice(4)?);
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read a little-endian `i32`.
    pub fn read_i32(&mut self) -> Result<i32> {
        Ok(self.read_u32()? as i32)
    }

    /// Read a little-endian `u64`.
    pub fn read_u64(&mut self) -> Result<u64> {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(self.read_slice(8)?);
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read a little-endian `i64`.
    pub fn read_i64(&mut self) -> Result<i64> {
        Ok(self.read_u64()? as i64)
    }

    /// Read a little-endian `f32`.
    pub fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    /// Read a little-endian `f64`.
    pub fn read_f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    /// Read a protocol boolean (one byte, zero means false).
    pub fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    /// Read a non-negative varint of at most five bytes.
    pub fn read_varint(&mut self) -> Result<u32> {
        let mut value = 0u32;
        for index in 0..5 {
            let byte = self.read_u8()?;
            if index == 4 && byte > 0x07 {
                return Err(ProtocolError::InvalidVarInt(
                    "fifth byte has bits above the 32-bit range",
                ));
            }
            value |= u32::from(byte & 0x7F) << (7 * index);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(ProtocolError::InvalidVarInt("more than five bytes"))
    }

    /// Read a length prefix and reject lengths above `max`.
    pub fn read_length(&mut self, field: &'static str, max: usize) -> Result<usize> {
        let length = self.read_varint()? as usize;
        if length > max {
            return Err(ProtocolError::LengthTooLarge { field, length, max });
        }
        Ok(length)
    }

    /// Read a varint-prefixed UTF-8 string with a byte-length bound.
    pub fn read_var_string(&mut self, field: &'static str, max: usize) -> Result<String> {
        let length = self.read_length(field, max)?;
        let bytes = self.read_slice(length)?;
        let text = std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidUtf8 { field })?;
        Ok(text.to_string())
    }

    /// Read a varint-prefixed ASCII string with a byte-length bound.
    pub fn read_var_ascii_string(&mut self, field: &'static str, max: usize) -> Result<String> {
        let length = self.read_length(field, max)?;
        let bytes = self.read_slice(length)?;
        read_ascii(field, bytes)
    }

    /// Read a fixed ASCII string with zero padding.
    pub fn read_fixed_ascii_string(
        &mut self,
        field: &'static str,
        length: usize,
    ) -> Result<String> {
        let bytes = self.read_slice(length)?;
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(length);
        read_ascii(field, &bytes[..end])
    }

    /// Read a fixed UTF-8 string with zero padding.
    pub fn read_fixed_string(&mut self, length: usize) -> Result<String> {
        let bytes = self.read_slice(length)?;
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(length);
        let text = std::str::from_utf8(&bytes[..end]).map_err(|_| ProtocolError::InvalidUtf8 {
            field: "fixed string",
        })?;
        Ok(text.to_string())
    }

    /// Read a UUID as two big-endian `i64`s.
    pub fn read_uuid(&mut self) -> Result<Uuid> {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(self.read_slice(16)?);
        Ok(Uuid::from_bytes(&bytes))
    }

    /// Read a varint-prefixed array with an element-count bound.
    pub fn read_array<T>(
        &mut self,
        field: &'static str,
        max: usize,
        mut read_element: impl FnMut(&mut Reader<'a>) -> Result<T>,
    ) -> Result<Vec<T>> {
        let length = self.read_length(field, max)?;
        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(read_element(self)?);
        }
        Ok(values)
    }

    /// Read an optional value whose presence lives in a nullable bitset.
    pub fn read_optional<T>(
        &mut self,
        bits: &NullableBitset,
        index: usize,
        read_value: impl FnOnce(&mut Reader<'a>) -> Result<T>,
    ) -> Result<Option<T>> {
        if bits.is_set(index)? {
            Ok(Some(read_value(self)?))
        } else {
            Ok(None)
        }
    }

    /// Read a nullable bitfield of `byte_len` bytes.
    pub fn read_nullable_bitfield(&mut self, byte_len: usize) -> Result<NullableBitset> {
        Ok(NullableBitset::from_bytes(
            self.read_slice(byte_len)?.to_vec(),
        ))
    }

    /// Consume one byte without using its value (useful for padding).
    pub fn skip_bytes(&mut self, count: usize) -> Result<()> {
        self.skip(count)
    }
}

fn read_ascii(field: &'static str, bytes: &[u8]) -> Result<String> {
    if bytes.iter().any(|byte| !byte.is_ascii()) {
        return Err(ProtocolError::InvalidAscii { field });
    }
    Ok(bytes.iter().map(|byte| char::from(*byte)).collect())
}

/// Growable writer for the wire layer.
#[derive(Debug, Default, Clone)]
pub struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    /// Create an empty writer.
    pub const fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Create an empty writer with capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    /// Number of bytes written.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True when nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Borrow the written bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    /// Take the written bytes.
    pub fn into_inner(self) -> Vec<u8> {
        self.bytes
    }

    /// Append raw bytes (internal framing/string helpers).
    pub fn write_slice(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    /// Write an unsigned byte.
    pub fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// Write a signed byte.
    pub fn write_i8(&mut self, value: i8) {
        self.write_u8(value as u8);
    }

    /// Write a little-endian `u16`.
    pub fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a little-endian `i16`.
    pub fn write_i16(&mut self, value: i16) {
        self.write_u16(value as u16);
    }

    /// Write a big-endian `u16` (storage container fields).
    pub fn write_u16_be(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `u64` (storage container fields).
    pub fn write_u64_be(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `i16`.
    pub fn write_i16_be(&mut self, value: i16) {
        self.write_u16_be(value as u16);
    }

    /// Write a big-endian `i32`.
    pub fn write_i32_be(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    /// Write a big-endian `i64`.
    pub fn write_i64_be(&mut self, value: i64) {
        self.write_u64_be(value as u64);
    }

    /// Write a little-endian `u32`.
    pub fn write_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a little-endian `i32`.
    pub fn write_i32(&mut self, value: i32) {
        self.write_u32(value as u32);
    }

    /// Write a little-endian `u64`.
    pub fn write_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a little-endian `i64`.
    pub fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64);
    }

    /// Write a little-endian `f32`.
    pub fn write_f32(&mut self, value: f32) {
        self.write_u32(value.to_bits());
    }

    /// Write a little-endian `f64`.
    pub fn write_f64(&mut self, value: f64) {
        self.write_u64(value.to_bits());
    }

    /// Write a protocol boolean as one byte.
    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    /// Write a non-negative varint.
    pub fn write_varint(&mut self, value: u32) -> Result<()> {
        if value <= 0x7F {
            self.write_u8(value as u8);
        } else if value <= 0x3FFF {
            self.write_u8(((value & 0x7F) | 0x80) as u8);
            self.write_u8((value >> 7) as u8);
        } else if value <= 0x1F_FFFF {
            self.write_u8(((value & 0x7F) | 0x80) as u8);
            self.write_u8((((value >> 7) & 0x7F) | 0x80) as u8);
            self.write_u8((value >> 14) as u8);
        } else if value <= 0x0FFF_FFFF {
            self.write_u8(((value & 0x7F) | 0x80) as u8);
            self.write_u8((((value >> 7) & 0x7F) | 0x80) as u8);
            self.write_u8((((value >> 14) & 0x7F) | 0x80) as u8);
            self.write_u8((value >> 21) as u8);
        } else {
            self.write_u8(((value & 0x7F) | 0x80) as u8);
            self.write_u8((((value >> 7) & 0x7F) | 0x80) as u8);
            self.write_u8((((value >> 14) & 0x7F) | 0x80) as u8);
            self.write_u8((((value >> 21) & 0x7F) | 0x80) as u8);
            self.write_u8((value >> 28) as u8);
        }
        Ok(())
    }

    /// Bytes needed for a varint value.
    pub fn varint_size(value: u32) -> usize {
        match value {
            0..=0x7F => 1,
            0x80..=0x3FFF => 2,
            0x4000..=0x1F_FFFF => 3,
            0x20_0000..=0x0FFF_FFFF => 4,
            _ => 5,
        }
    }

    /// Write a varint-prefixed UTF-8 string with a byte-length bound.
    pub fn write_var_string(&mut self, field: &'static str, value: &str, max: usize) -> Result<()> {
        let bytes = value.as_bytes();
        if bytes.len() > max {
            return Err(ProtocolError::LengthTooLarge {
                field,
                length: bytes.len(),
                max,
            });
        }
        self.write_varint(bytes.len() as u32)?;
        self.write_slice(bytes);
        Ok(())
    }

    /// Write a varint-prefixed ASCII string with a byte-length bound.
    pub fn write_var_ascii_string(
        &mut self,
        field: &'static str,
        value: &str,
        max: usize,
    ) -> Result<()> {
        if !value.is_ascii() {
            return Err(ProtocolError::InvalidAscii { field });
        }
        self.write_var_string(field, value, max)
    }

    /// Write a fixed ASCII string, zero-padding the remainder.
    pub fn write_fixed_ascii_string(
        &mut self,
        field: &'static str,
        value: &str,
        length: usize,
    ) -> Result<()> {
        if !value.is_ascii() {
            return Err(ProtocolError::InvalidAscii { field });
        }
        self.write_fixed_bytes(field, value.as_bytes(), length)
    }

    /// Write a fixed UTF-8 string, zero-padding the remainder.
    pub fn write_fixed_string(
        &mut self,
        field: &'static str,
        value: &str,
        length: usize,
    ) -> Result<()> {
        self.write_fixed_bytes(field, value.as_bytes(), length)
    }

    fn write_fixed_bytes(
        &mut self,
        field: &'static str,
        bytes: &[u8],
        length: usize,
    ) -> Result<()> {
        if bytes.len() > length {
            return Err(ProtocolError::FixedStringTooLong {
                field,
                length,
                value_len: bytes.len(),
            });
        }
        self.write_slice(bytes);
        self.bytes
            .resize(self.bytes.len() + (length - bytes.len()), 0);
        Ok(())
    }

    /// Write a UUID as two big-endian `i64`s.
    pub fn write_uuid(&mut self, value: Uuid) {
        self.bytes.extend_from_slice(&value.to_bytes());
    }

    /// Write a varint-prefixed array with an element-count bound.
    pub fn write_array<T>(
        &mut self,
        field: &'static str,
        values: &[T],
        max: usize,
        mut write_element: impl FnMut(&mut Writer, &T) -> Result<()>,
    ) -> Result<()> {
        if values.len() > max {
            return Err(ProtocolError::LengthTooLarge {
                field,
                length: values.len(),
                max,
            });
        }
        self.write_varint(values.len() as u32)?;
        for value in values {
            write_element(self, value)?;
        }
        Ok(())
    }

    /// Write a nullable bitset at the start of a fixed block.
    pub fn write_nullable_bitfield(&mut self, bits: &NullableBitset) {
        self.write_slice(bits.as_bytes());
    }
}

/// A nullable-field presence bitset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NullableBitset {
    bytes: Vec<u8>,
}

impl NullableBitset {
    /// Create an empty bitfield of `byte_len` bytes.
    pub fn new(byte_len: usize) -> Self {
        Self {
            bytes: vec![0; byte_len],
        }
    }

    /// Wrap existing bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// Borrow the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Number of bits in the field.
    pub fn bit_len(&self) -> usize {
        self.bytes.len() * 8
    }

    /// True when `index` is present.
    pub fn is_set(&self, index: usize) -> Result<bool> {
        if index >= self.bit_len() {
            return Err(ProtocolError::NullableIndex {
                index,
                bytes: self.bytes.len(),
            });
        }
        Ok(self.bytes[index / 8] & (1 << (index % 8)) != 0)
    }

    /// Set or clear `index`.
    pub fn set(&mut self, index: usize, present: bool) -> Result<()> {
        if index >= self.bit_len() {
            return Err(ProtocolError::NullableIndex {
                index,
                bytes: self.bytes.len(),
            });
        }
        let mask = 1 << (index % 8);
        if present {
            self.bytes[index / 8] |= mask;
        } else {
            self.bytes[index / 8] &= !mask;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nullable_bitset_crosses_bytes() {
        let mut bits = NullableBitset::new(2);
        for index in 0..8 {
            bits.set(index, true).unwrap();
        }
        assert_eq!(bits.as_bytes(), &[0xFF, 0x00]);
        bits.set(8, true).unwrap();
        bits.set(15, true).unwrap();
        assert_eq!(bits.as_bytes(), &[0xFF, 0x81]);
        assert!(bits.is_set(0).unwrap());
        assert!(bits.is_set(8).unwrap());
        assert!(!bits.is_set(9).unwrap());
        assert!(bits.is_set(15).unwrap());
        assert!(bits.is_set(16).is_err());
    }

    #[test]
    fn uuid_is_two_big_endian_longs() {
        let uuid = Uuid::new(0x0102_0304_0506_0708, 0x1112_1314_1516_1718);
        assert_eq!(
            uuid.to_bytes(),
            [1, 2, 3, 4, 5, 6, 7, 8, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18]
        );
        let bytes = uuid.to_bytes();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.read_uuid().unwrap(), uuid);
    }
}
