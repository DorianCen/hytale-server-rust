//! Column tint palette.
//!
//! Ported from `CoreServer/.../world/chunk/palette/IntBytePalette.java`. The
//! tint map is 32×32 `i32` colours stored through a 10-bit internal-id array.
//! The serialized layout is the one `BlockChunk.getCachedColumnPacket` and
//! `getCachedTintsPacket` embed in `SetColumn`/`SetChunkTintmap`:
//!
//! ```text
//! u16 LE  key count
//! i32 LE  key count keys (insertion order; index 0 starts as 0)
//! i32 LE  packed bit-field byte length (always 32*32*10/8 = 1280)
//! u8[]    packed 10-bit internal ids, LSB-first
//! ```
//!
//! Java inserts keys in the order columns are first set, which for generated
//! chunks is the generator's `x`-major, `z`-minor scan. The insertion order is
//! observable in the packet bytes, so it is kept rather than sorted.

const SIZE: usize = 32;
const LENGTH: usize = SIZE * SIZE;
const BITS: usize = 10;
const PACKED_BYTES: usize = (LENGTH * BITS).div_ceil(8);

/// A 32×32 colour map using Java's `IntBytePalette` layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TintPalette {
    keys: Vec<i32>,
    values: Vec<u16>,
}

impl Default for TintPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl TintPalette {
    /// Java `new IntBytePalette()` — one key (0) and an all-zero array.
    pub fn new() -> Self {
        Self {
            keys: vec![0],
            values: vec![0; LENGTH],
        }
    }

    /// Java `get(int x, int z)`.
    pub fn get(&self, x: i32, z: i32) -> i32 {
        self.keys[self.values[column_index(x, z)] as usize]
    }

    /// Java `set(int x, int z, int key)`.
    pub fn set(&mut self, x: i32, z: i32, key: i32) {
        let index = column_index(x, z);
        let internal = match self.keys.iter().position(|candidate| *candidate == key) {
            Some(internal) => internal,
            None => {
                if self.keys.len() >= LENGTH {
                    self.compact();
                }
                self.keys.push(key);
                self.keys.len() - 1
            }
        };
        self.values[index] = internal as u16;
    }

    /// Java `serialize()`.
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.keys.len() * 4 + 4 + PACKED_BYTES);
        bytes.extend_from_slice(&(self.keys.len() as u16).to_le_bytes());
        for key in &self.keys {
            bytes.extend_from_slice(&key.to_le_bytes());
        }
        bytes.extend_from_slice(&(PACKED_BYTES as i32).to_le_bytes());

        let mut packed = vec![0u8; PACKED_BYTES];
        for (index, value) in self.values.iter().enumerate() {
            let bit = index * BITS;
            let byte = bit / 8;
            let offset = bit % 8;
            let value = u32::from(*value);
            packed[byte] |= (value << offset) as u8;
            if offset + BITS > 8 {
                packed[byte + 1] |= (value >> (8 - offset)) as u8;
            }
            if offset + BITS > 16 {
                packed[byte + 2] |= (value >> (16 - offset)) as u8;
            }
        }
        bytes.extend_from_slice(&packed);
        bytes
    }

    /// Decode the Java `serialize()` layout.
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 6 {
            return None;
        }
        let count = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        if count == 0 || count > LENGTH {
            return None;
        }
        let key_end = 2 + count * 4;
        if bytes.len() < key_end + 4 {
            return None;
        }
        let mut keys = Vec::with_capacity(count);
        for index in 0..count {
            let start = 2 + index * 4;
            keys.push(i32::from_le_bytes([
                bytes[start],
                bytes[start + 1],
                bytes[start + 2],
                bytes[start + 3],
            ]));
        }
        let packed_len = i32::from_le_bytes([
            bytes[key_end],
            bytes[key_end + 1],
            bytes[key_end + 2],
            bytes[key_end + 3],
        ]);
        if packed_len < 0 || bytes.len() < key_end + 4 + packed_len as usize {
            return None;
        }
        let packed = &bytes[key_end + 4..key_end + 4 + packed_len as usize];
        let mut values = vec![0u16; LENGTH];
        for (index, value) in values.iter_mut().enumerate() {
            let bit = index * BITS;
            let byte = bit / 8;
            let offset = bit % 8;
            let mut accumulator = 0u32;
            for shift in 0..3 {
                if let Some(byte) = packed.get(byte + shift) {
                    accumulator |= u32::from(*byte) << (shift * 8);
                }
            }
            let internal = ((accumulator >> offset) & ((1 << BITS) - 1)) as usize;
            if internal >= count {
                return None;
            }
            *value = internal as u16;
        }
        Some(Self { keys, values })
    }

    /// Rebuild the key table from the values currently referenced, preserving
    /// first-use order. Java's `optimize` does the same when the 10-bit id
    /// space is exhausted.
    fn compact(&mut self) {
        let mut keys = Vec::with_capacity(LENGTH);
        let mut remap = vec![0u16; self.keys.len()];
        for value in &self.values {
            let old = *value as usize;
            if let Some(new) = keys.iter().position(|key| *key == self.keys[old]) {
                remap[old] = new as u16;
            } else {
                remap[old] = keys.len() as u16;
                keys.push(self.keys[old]);
            }
        }
        for value in &mut self.values {
            *value = remap[*value as usize];
        }
        self.keys = keys;
    }
}

#[inline]
fn column_index(x: i32, z: i32) -> usize {
    ((x & 31) | ((z & 31) << 5)) as usize
}
