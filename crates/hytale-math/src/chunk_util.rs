//! Chunk index packing, unpacking and coordinate helpers.
//!
//! Ported from: `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/ChunkUtil.java`
//!
//! Rust name collisions: Java overloads `chunkCoordinate` by parameter type —
//! ported as `chunk_coordinate_f64` / `chunk_coordinate_i32` /
//! `chunk_coordinate_i64`. Likewise `byte[]` converters are
//! `short_to_byte_array` / `int_to_byte_array`.

/// Bits for the local chunk coordinate (32 = 2^5).
pub const BITS: i32 = 5;
/// Chunk size: `1 << BITS`.
pub const SIZE: i32 = 1 << BITS;
/// `SIZE * SIZE`.
pub const SIZE_2: i32 = SIZE * SIZE;
/// `SIZE - 1`.
pub const SIZE_MINUS_1: i32 = SIZE - 1;
/// `SIZE - 1`.
pub const SIZE_MASK: i32 = SIZE - 1;
/// `SIZE * SIZE` (columns).
pub const SIZE_COLUMNS: i32 = SIZE * SIZE;
/// `SIZE_COLUMNS - 1`.
pub const SIZE_COLUMNS_MASK: i32 = SIZE_COLUMNS - 1;
/// `SIZE * SIZE * SIZE` (blocks per section).
pub const SIZE_BLOCKS: i32 = SIZE * SIZE * SIZE;
/// `SIZE_BLOCKS - 1`.
pub const SIZE_BLOCKS_MASK: i32 = SIZE_BLOCKS - 1;
/// `BITS << 1`.
pub const BITS2: i32 = BITS << 1;
/// `!SIZE_MASK`.
pub const NON_CHUNK_MASK: i32 = !SIZE_MASK;

/// Limit only applies to legacy storage/world gens. You shouldn't assume this
/// limit anymore and instead check for null section references.
#[deprecated]
pub const HEIGHT_SECTIONS: i32 = 10;
/// Legacy chunk height in blocks (`SIZE * HEIGHT_SECTIONS`).
#[deprecated]
#[allow(deprecated)]
pub const HEIGHT: i32 = SIZE * HEIGHT_SECTIONS;
/// `HEIGHT - 1`.
#[deprecated]
#[allow(deprecated)]
pub const HEIGHT_MINUS_1: i32 = HEIGHT - 1;
/// `HighestOneBit(HEIGHT) << 1 - 1`.
#[deprecated]
pub const HEIGHT_MASK: i32 = (HEIGHT_HIGHEST_ONE_BIT << 1) - 1;
/// `SIZE_BLOCKS * HEIGHT_SECTIONS`.
#[deprecated]
#[allow(deprecated)]
pub const SIZE_BLOCKS_COLUMN: i32 = SIZE_BLOCKS * HEIGHT_SECTIONS;

/// `Integer.highestOneBit(HEIGHT)` for HEIGHT = 320 -> 256. Kept as a private
/// helper constant so the `#[deprecated]` constants can use it in a const
/// context.
const HEIGHT_HIGHEST_ONE_BIT: i32 = 256;

/// Java `ChunkUtil.NOT_FOUND = indexChunk(Integer.MIN_VALUE, Integer.MIN_VALUE)`.
pub const NOT_FOUND: i64 = index_chunk(i32::MIN, i32::MIN);

/// Java `ChunkUtil.MIN_Y`.
pub const MIN_Y: i32 = 0;
/// Java `ChunkUtil.MIN_ENTITY_Y` (`MIN_Y - SIZE`).
pub const MIN_ENTITY_Y: i32 = MIN_Y - SIZE;
/// Java `ChunkUtil.MIN_SECTION`.
pub const MIN_SECTION: i32 = 0;

/// Theoretical inclusive minimum chunk coordinate such that
/// [`world_coord_from_local_coord`] does not under- or overflow for inputs
/// `chunk_coord = MIN_CHUNK_COORD` and `local_coord in [0, 32)`.
///
/// Note: the practical minimum chunk coordinate that systems throughout the
/// engine actually support may be much higher than this value.
pub const MIN_CHUNK_COORD: i32 = i32::MIN >> BITS;

/// Theoretical inclusive maximum chunk coordinate such that
/// [`world_coord_from_local_coord`] does not under- or overflow for inputs
/// `chunk_coord = MAX_CHUNK_COORD` and `local_coord in [0, 32)`.
///
/// Note: the practical maximum chunk coordinate that systems throughout the
/// engine actually support may be much lower than this value.
pub const MAX_CHUNK_COORD: i32 = i32::MAX >> BITS;

/// Java `ChunkUtil.shortToByteArray(short[])` — little-endian byte array.
pub fn short_to_byte_array(data: &[i16]) -> Vec<u8> {
    // Java: ByteBuffer.allocate(len * 2).order(LITTLE_ENDIAN).asShortBuffer().put(data)
    debug_assert_eq!(std::mem::size_of::<i16>(), 2);
    let mut bytes = Vec::with_capacity(data.len() * 2);
    for v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Java `ChunkUtil.intToByteArray(int[])` — little-endian byte array.
pub fn int_to_byte_array(data: &[i32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for v in data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Java `ChunkUtil.indexColumn(int x, int z)`: `(z & SIZE_MASK) << BITS | (x & SIZE_MASK)`.
#[inline]
pub fn index_column(x: i32, z: i32) -> i32 {
    (z & SIZE_MASK) << BITS | (x & SIZE_MASK)
}

/// Java `ChunkUtil.xFromColumn(int)`.
#[inline]
pub fn x_from_column(index: i32) -> i32 {
    index & SIZE_MASK
}

/// Java `ChunkUtil.zFromColumn(int)`.
#[inline]
pub fn z_from_column(index: i32) -> i32 {
    (index >> BITS) & SIZE_MASK
}

/// Java `ChunkUtil.indexSection(int y)`: `y >> BITS`.
#[inline]
pub fn index_section(y: i32) -> i32 {
    y >> BITS
}

/// Java `ChunkUtil.indexBlockFromColumn(int column, int y)`.
#[inline]
pub fn index_block_from_column(column: i32, y: i32) -> i32 {
    (y & SIZE_MASK) << BITS2 | (column & ((SIZE_MASK << BITS) | SIZE_MASK))
}

/// Java `ChunkUtil.indexBlock(int x, int y, int z)`.
#[inline]
pub fn index_block(x: i32, y: i32, z: i32) -> i32 {
    (y & SIZE_MASK) << BITS2 | (z & SIZE_MASK) << BITS | (x & SIZE_MASK)
}

/// Java `ChunkUtil.xFromIndex(int)`.
#[inline]
pub fn x_from_index(index: i32) -> i32 {
    index & SIZE_MASK
}

/// Java `ChunkUtil.yFromIndex(int)`.
#[inline]
pub fn y_from_index(index: i32) -> i32 {
    (index >> BITS2) & SIZE_MASK
}

/// Java `ChunkUtil.zFromIndex(int)`.
#[inline]
pub fn z_from_index(index: i32) -> i32 {
    (index >> BITS) & SIZE_MASK
}

/// Java `ChunkUtil.localCoordinate(long)`: `(int) (v & SIZE_MASK)`.
#[inline]
pub fn local_coordinate(v: i64) -> i32 {
    (v & SIZE_MASK as i64) as i32
}

/// Java `ChunkUtil.chunkCoordinate(double)`:
/// `MathUtil.floor(block) >> BITS`.
#[inline]
pub fn chunk_coordinate_f64(block: f64) -> i32 {
    crate::math_util::floor(block) >> BITS
}

/// Java `ChunkUtil.chunkCoordinate(int)`: `block >> BITS`.
#[inline]
pub fn chunk_coordinate_i32(block: i32) -> i32 {
    block >> BITS
}

/// Java `ChunkUtil.chunkCoordinate(long)`: `(int) (block >> BITS)`.
#[inline]
pub fn chunk_coordinate_i64(block: i64) -> i32 {
    (block >> BITS) as i32
}

/// Java `ChunkUtil.minBlock(int)`: `index << BITS`.
#[inline]
pub fn min_block(index: i32) -> i32 {
    index << BITS
}

/// Java `ChunkUtil.maxBlock(int)`: `(index << BITS) + SIZE_MINUS_1`.
#[inline]
pub fn max_block(index: i32) -> i32 {
    (index << BITS) + SIZE_MINUS_1
}

/// Java `ChunkUtil.isWithinLocalChunk(int, int)`.
#[inline]
pub fn is_within_local_chunk(x: i32, z: i32) -> bool {
    x >= 0 && z >= 0 && x < SIZE && z < SIZE
}

/// Java `ChunkUtil.isBorderBlock(int, int)`.
#[inline]
pub fn is_border_block(x: i32, z: i32) -> bool {
    x == 0 || z == 0 || x == SIZE_MINUS_1 || z == SIZE_MINUS_1
}

/// Java `ChunkUtil.isBorderBlockGlobal(int, int)`.
#[inline]
pub fn is_border_block_global(x: i32, z: i32) -> bool {
    let (x, z) = (x & SIZE_MASK, z & SIZE_MASK);
    is_border_block(x, z)
}

/// Java `ChunkUtil.isInsideChunk(int chunkX, int chunkZ, int x, int z)`.
#[inline]
pub fn is_inside_chunk(chunk_x: i32, chunk_z: i32, x: i32, z: i32) -> bool {
    chunk_coordinate_i32(x) == chunk_x && chunk_coordinate_i32(z) == chunk_z
}

/// Java `ChunkUtil.isSameChunk(int, int, int, int)`.
#[inline]
pub fn is_same_chunk(x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
    chunk_coordinate_i32(x0) == chunk_coordinate_i32(x1)
        && chunk_coordinate_i32(z0) == chunk_coordinate_i32(z1)
}

/// Java `ChunkUtil.isSameChunkSection(…)`.
#[allow(clippy::too_many_arguments)]
#[inline]
pub fn is_same_chunk_section(x0: i32, y0: i32, z0: i32, x1: i32, y1: i32, z1: i32) -> bool {
    chunk_coordinate_i32(x0) == chunk_coordinate_i32(x1)
        && chunk_coordinate_i32(y0) == chunk_coordinate_i32(y1)
        && chunk_coordinate_i32(z0) == chunk_coordinate_i32(z1)
}

/// Java `ChunkUtil.isInsideChunkRelative(int, int)`.
#[inline]
pub fn is_inside_chunk_relative(x: i32, z: i32) -> bool {
    (x & SIZE_MASK) == x && (z & SIZE_MASK) == z
}

/// Java `ChunkUtil.xOfChunkIndex(long)`.
#[inline]
pub fn x_of_chunk_index(index: i64) -> i32 {
    (index >> 32) as i32
}

/// Java `ChunkUtil.zOfChunkIndex(long)`.
#[inline]
pub fn z_of_chunk_index(index: i64) -> i32 {
    index as i32
}

/// Java `ChunkUtil.indexChunk(int x, int z)` — combines 2×32-bit int chunk
/// coordinates into one 64-bit index: x is the most significant 32 bits, z the
/// least significant.
#[inline]
pub const fn index_chunk(x: i32, z: i32) -> i64 {
    ((x as i64) << 32) | (z as i64 & 0xFFFF_FFFF)
}

/// Java `ChunkUtil.indexChunkFromBlock(int, int)`.
#[inline]
pub fn index_chunk_from_block(block_x: i32, block_z: i32) -> i64 {
    index_chunk(chunk_coordinate_i32(block_x), chunk_coordinate_i32(block_z))
}

/// Java `ChunkUtil.indexChunkFromBlock(double, double)`.
#[inline]
pub fn index_chunk_from_block_f64(block_x: f64, block_z: f64) -> i64 {
    index_chunk_from_block(
        crate::math_util::floor(block_x),
        crate::math_util::floor(block_z),
    )
}

/// Java `ChunkUtil.worldCoordFromLocalCoord(int, int)` — translate a local
/// coord within a chunk to a world coord.
#[inline]
pub fn world_coord_from_local_coord(chunk_coord: i32, local_coord: i32) -> i32 {
    (chunk_coord << BITS) | local_coord
}

/// Java `ChunkUtil.isValidChunkIndex(long)`.
#[inline]
pub fn is_valid_chunk_index(chunk_index: i64) -> bool {
    is_valid_chunk_coords(x_of_chunk_index(chunk_index), z_of_chunk_index(chunk_index))
}

/// Java `ChunkUtil.isValidChunkCoords(int, int)`.
#[inline]
pub fn is_valid_chunk_coords(chunk_coord_x: i32, chunk_coord_z: i32) -> bool {
    is_valid_chunk_coord(chunk_coord_x) && is_valid_chunk_coord(chunk_coord_z)
}

/// Java `ChunkUtil.isValidChunkCoord(int)`.
#[inline]
pub fn is_valid_chunk_coord(chunk_coord: i32) -> bool {
    (MIN_CHUNK_COORD..=MAX_CHUNK_COORD).contains(&chunk_coord)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from Java `ChunkUtilTest` (`STEP = 2`).
    const STEP: usize = 2;

    /// Ported from Java `ChunkUtilTest.indexBlock()`.
    #[test]
    fn index_block_round_trip() {
        for x in (0..SIZE as usize).step_by(STEP) {
            for z in (0..SIZE as usize).step_by(STEP) {
                for y in (0..SIZE as usize).step_by(STEP) {
                    let block_index = index_block(x as i32, y as i32, z as i32);
                    assert_eq!(x as i32, x_from_index(block_index), "X");
                    assert_eq!(y as i32, y_from_index(block_index), "Y");
                    assert_eq!(z as i32, z_from_index(block_index), "Z");
                }
            }
        }
    }

    /// Ported from Java `ChunkUtilTest.indexBlockFromColumn()`.
    #[test]
    fn index_block_from_column_matches_index_block() {
        for x in (0..SIZE as usize).step_by(STEP) {
            for z in (0..SIZE as usize).step_by(STEP) {
                let column = index_column(x as i32, z as i32);
                for y in (0..SIZE as usize).step_by(STEP) {
                    let block_index = index_block_from_column(column, y as i32);
                    assert_eq!(
                        block_index,
                        index_block(x as i32, y as i32, z as i32),
                        "column({x},{z}) y={y}"
                    );
                }
            }
        }
    }

    #[test]
    fn chunk_index_packing_round_trip() {
        for &x in &[i32::MIN, -1, 0, 1, i32::MAX] {
            for &z in &[i32::MIN, -1, 0, 1, i32::MAX] {
                let index = index_chunk(x, z);
                assert_eq!(x_of_chunk_index(index), x);
                assert_eq!(z_of_chunk_index(index), z);
            }
        }
        // Java: NOT_FOUND == indexChunk(MIN_VALUE, MIN_VALUE) and it must not
        // collide with any real chunk index packing of valid coords.
        assert_eq!(NOT_FOUND, index_chunk(i32::MIN, i32::MIN));
        assert_ne!(NOT_FOUND, index_chunk(i32::MIN + 1, i32::MIN));
    }

    #[test]
    fn byte_array_converters_are_little_endian() {
        assert_eq!(short_to_byte_array(&[1, -2]), [1, 0, 0xFE, 0xFF]);
        assert_eq!(int_to_byte_array(&[1]), [1, 0, 0, 0]);
        assert_eq!(int_to_byte_array(&[0x1234_5678]), [0x78, 0x56, 0x34, 0x12]);
    }

    #[test]
    fn chunk_coordinates() {
        // floor(9.5) >> 5 == 0; floor(-0.25) >> 5 == -1
        assert_eq!(chunk_coordinate_f64(9.5), 0);
        assert_eq!(chunk_coordinate_f64(-0.25), -1);
        assert_eq!(chunk_coordinate_i32(-1), -1);
        assert_eq!(min_block(2), 64);
        assert_eq!(max_block(2), 95);
        assert_eq!(local_coordinate(32), 0);
        assert_eq!(local_coordinate(-1), 31);
        assert_eq!(world_coord_from_local_coord(-1, 31), -1);
    }
}
