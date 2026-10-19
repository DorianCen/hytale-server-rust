// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/chunk/BlockPriorityChunk.java`
//! and `.../worldgen/chunk/BlockPriorityModifier.java`.
//!
//! Java `com.hypixel.hytale.server.worldgen.chunk.BlockPriorityChunk` — the
//! byte grid that arbitrates competing block writes. Every block write made
//! by the populators carries a priority (`FILLING < LAYER < COVER < WATER <
//! CAVE_COVER < CAVE < CAVE_PREFAB < PREFAB_CAVE < PREFAB`); a write only
//! replaces an existing one when its priority is greater or equal.
//!
//! The low five bits of a grid byte hold the priority, the remaining bits
//! hold modifier flags (currently only [`FLAG_SUBMERGE`]).

use hytale_math::chunk_util::{BITS, BITS2, SIZE, SIZE_MASK};

// ---------------------------------------------------------------------
// Legacy chunk constants
//
// `hytale-math` carries the Java `@Deprecated` marker on these constants (the
// Java deprecation is about legacy *storage* limits, not about the world
// generator), so they are re-exported here with the lint allowed and used
// from the rest of the crate through this module.
// ---------------------------------------------------------------------

/// Java `ChunkUtil.HEIGHT` — the legacy chunk height in blocks (`32 * 10`).
#[allow(deprecated)]
pub const HEIGHT: i32 = hytale_math::chunk_util::HEIGHT;

/// Java `ChunkUtil.HEIGHT_SECTIONS` — the number of chunk sections a chunk is
/// split into.
#[allow(deprecated)]
pub const HEIGHT_SECTIONS: i32 = hytale_math::chunk_util::HEIGHT_SECTIONS;

/// Java `ChunkUtil.HEIGHT_MASK` — `(Integer.highestOneBit(HEIGHT) << 1) - 1`,
/// i.e. `511` for `HEIGHT = 320`.
#[allow(deprecated)]
pub const HEIGHT_MASK: i32 = hytale_math::chunk_util::HEIGHT_MASK;

/// Java `BlockPriorityChunk.NO_CHANGE` — the "keep the current priority"
/// marker.
pub const NO_CHANGE: i8 = -1;

/// Java `BlockPriorityChunk.NONE` — no block written yet.
pub const NONE: i8 = 0;

/// Java `BlockPriorityChunk.FILLING`.
pub const FILLING: i8 = 1;

/// Java `BlockPriorityChunk.LAYER`.
pub const LAYER: i8 = 2;

/// Java `BlockPriorityChunk.COVER`.
pub const COVER: i8 = 3;

/// Java `BlockPriorityChunk.WATER`.
pub const WATER: i8 = 4;

/// Java `BlockPriorityChunk.CAVE_COVER` — Cover in a regular cave-node.
pub const CAVE_COVER: i8 = 5;

/// Java `BlockPriorityChunk.CAVE` — Regular cave-node.
pub const CAVE: i8 = 6;

/// Java `BlockPriorityChunk.CAVE_PREFAB` — Prefab in a regular cave-node.
pub const CAVE_PREFAB: i8 = 7;

/// Java `BlockPriorityChunk.PREFAB_CAVE` — Prefab cave-node (ie dungeon).
pub const PREFAB_CAVE: i8 = 8;

/// Java `BlockPriorityChunk.PREFAB` — Regular prefab.
pub const PREFAB: i8 = 9;

/// Java `BlockPriorityChunk.EXCLUSIVE_MAX_PRIORITY` = `1 << 5` (32).
///
/// Number of priority levels is limited to allow for a limited number of bit
/// flags for modifiers.
pub const EXCLUSIVE_MAX_PRIORITY: i8 = 1 << 5;

/// Java `BlockPriorityChunk.MASK` — mask used to clear flag bits from the
/// priority value.
pub const MASK: i8 = EXCLUSIVE_MAX_PRIORITY - 1;

/// Java `BlockPriorityChunk.FLAG_MASK` — mask used to extract flag bits from
/// the priority value.
///
/// Java computes this as `(byte) ~MASK`; `~31` in two's complement is `-32`,
/// so the constant keeps the sign of a Java `byte`.
pub const FLAG_MASK: i8 = !MASK;

/// Java `BlockPriorityChunk.FLAG_SUBMERGE` — flag to indicate that this block
/// can be submerged by fluids.
pub const FLAG_SUBMERGE: i8 = 1 << 5;

/// Number of entries of the priority grid: `SIZE * SIZE * HEIGHT`.
const PRIORITY_COUNT: usize = (SIZE * SIZE * HEIGHT) as usize;

/// Java `BlockPriorityChunk`.
///
/// The grid is a flat `byte` array of `SIZE * SIZE * HEIGHT` entries indexed
/// y-major (full height), then z, then x.
#[derive(Clone, PartialEq, Eq)]
pub struct BlockPriorityChunk {
    blocks: Vec<i8>,
}

impl BlockPriorityChunk {
    /// Java `new BlockPriorityChunk()`.
    pub fn new() -> Self {
        Self {
            blocks: vec![NONE; PRIORITY_COUNT],
        }
    }

    /// Java `reset()` — fills the whole grid with [`NONE`].
    ///
    /// Java returns `this` so the call can be chained; the Rust port returns
    /// `&mut Self` for the same reason.
    pub fn reset(&mut self) -> &mut Self {
        self.blocks.fill(NONE);
        self
    }

    /// Java `get(int x, int y, int z)` — the priority value *excluding* flags.
    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> i8 {
        self.blocks[index(x, y, z) as usize] & MASK
    }

    /// Java `getRaw(int x, int y, int z)` — the raw priority value *including*
    /// flags.
    #[inline]
    pub fn get_raw(&self, x: i32, y: i32, z: i32) -> i8 {
        self.blocks[index(x, y, z) as usize]
    }

    /// Java `set(int x, int y, int z, byte type)`.
    #[inline]
    pub fn set(&mut self, x: i32, y: i32, z: i32, type_: i8) {
        self.blocks[index(x, y, z) as usize] = type_;
    }

    /// The raw grid bytes (priority bits and flag bits interleaved).
    #[inline]
    pub fn blocks(&self) -> &[i8] {
        &self.blocks
    }
}

impl Default for BlockPriorityChunk {
    fn default() -> Self {
        Self::new()
    }
}

/// Block index within the full-height column: y-major, then z, then x.
///
/// Ported from the private `BlockPriorityChunk.index(int x, int y, int z)`:
/// `(y & ChunkUtil.HEIGHT_MASK) << ChunkUtil.BITS2 | (z & ChunkUtil.SIZE_MASK)
/// << ChunkUtil.BITS | (x & ChunkUtil.SIZE_MASK)`.
#[inline]
fn index(x: i32, y: i32, z: i32) -> i32 {
    (y & HEIGHT_MASK) << BITS2 | (z & SIZE_MASK) << BITS | (x & SIZE_MASK)
}

/// Java `BlockPriorityModifier`.
///
/// Allows simple modification of the current and target block priorities used
/// during the `ChunkGeneratorExecution#setBlock` logic to determine whether a
/// block can replace another at a given position.
///
/// By transforming the current, target, or both priority value(s), special
/// logic can be applied to handle certain edge-case scenarios that the default
/// priority comparison can't deal with — such as allowing regular caves to
/// intersect with prefab cave-nodes (ie dungeons).
pub trait BlockPriorityModifier {
    /// Java `modifyCurrent(byte current, byte target)`.
    ///
    /// Modify the block priority value read from the [`BlockPriorityChunk`]
    /// before the comparison logic is performed to determine whether a block
    /// can be set at a given position.
    ///
    /// * `current` — the current priority value held in the chunk.
    /// * `target` — the new priority value attempting to be set in the chunk.
    fn modify_current(&self, current: i8, target: i8) -> i8;

    /// Java `modifyTarget(byte original, byte target)`.
    ///
    /// Modify the block priority value to be set in the
    /// [`BlockPriorityChunk`] after the comparison logic has determined that a
    /// block can be set at a given position — ie the value that is written to
    /// the chunk.
    ///
    /// * `original` — the original unmodified priority value held in the
    ///   chunk.
    /// * `target` — the new priority value to be set in the chunk.
    fn modify_target(&self, original: i8, target: i8) -> i8;
}

/// Java `BlockPriorityModifier.NONE` — the identity modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoneModifier;

impl NoneModifier {
    /// Java `BlockPriorityModifier.NONE`.
    pub const NONE: Self = Self;
}

impl BlockPriorityModifier for NoneModifier {
    #[inline]
    fn modify_current(&self, current: i8, _target: i8) -> i8 {
        current
    }

    #[inline]
    fn modify_target(&self, _original: i8, target: i8) -> i8 {
        target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_constants_match_java() {
        assert_eq!(NO_CHANGE, -1);
        assert_eq!(NONE, 0);
        assert_eq!(FILLING, 1);
        assert_eq!(LAYER, 2);
        assert_eq!(COVER, 3);
        assert_eq!(WATER, 4);
        assert_eq!(CAVE_COVER, 5);
        assert_eq!(CAVE, 6);
        assert_eq!(CAVE_PREFAB, 7);
        assert_eq!(PREFAB_CAVE, 8);
        assert_eq!(PREFAB, 9);
        assert_eq!(EXCLUSIVE_MAX_PRIORITY, 32);
        assert_eq!(MASK, 31);
        assert_eq!(FLAG_SUBMERGE, 32);
    }

    #[test]
    fn flag_mask_is_the_two_complement_negation_of_mask() {
        // Java: `public static final byte FLAG_MASK = ~MASK;`
        // `~31` in two's complement is `-32` — the constant keeps the sign of
        // a Java byte.
        assert_eq!(FLAG_MASK, -32);
        assert_eq!(!MASK, -32i8);
        assert_eq!(FLAG_MASK, !31i8);

        // Flag bits survive `& FLAG_MASK`, priority bits survive `& MASK`.
        assert_eq!(FLAG_SUBMERGE & FLAG_MASK, FLAG_SUBMERGE);
        assert_eq!(FLAG_SUBMERGE & MASK, 0);
        assert_eq!(NO_CHANGE & MASK, 31);
        assert_eq!((FLAG_SUBMERGE | FILLING) & MASK, FILLING);
        assert_eq!((FLAG_SUBMERGE | FILLING) & FLAG_MASK, FLAG_SUBMERGE);
    }

    #[test]
    fn grid_index_is_y_major_then_z_then_x() {
        let mut chunk = BlockPriorityChunk::new();
        chunk.set(3, 5, 7, PREFAB);

        // The Java index math: (y & HEIGHT_MASK) << 10 | (z & 31) << 5 | (x & 31)
        let expected = (5 & HEIGHT_MASK) << BITS2 | (7 & SIZE_MASK) << BITS | (3 & SIZE_MASK);
        assert_eq!(chunk.blocks()[expected as usize], PREFAB);

        // Neighbouring cells stay untouched.
        assert_eq!(chunk.get(2, 5, 7), NONE);
        assert_eq!(chunk.get(4, 5, 7), NONE);
        assert_eq!(chunk.get(3, 4, 7), NONE);
        assert_eq!(chunk.get(3, 6, 7), NONE);
        assert_eq!(chunk.get(3, 5, 6), NONE);
        assert_eq!(chunk.get(3, 5, 8), NONE);
    }

    #[test]
    fn coordinates_are_masked_like_java() {
        let mut chunk = BlockPriorityChunk::new();
        chunk.set(-1, 0, 0, CAVE);
        assert_eq!(chunk.get(31, 0, 0), CAVE);
        assert_eq!(chunk.get(-1, 0, 0), CAVE);

        chunk.set(0, 0, -1, WATER);
        assert_eq!(chunk.get(0, 0, 31), WATER);
    }

    #[test]
    fn get_masks_flags_get_raw_does_not() {
        let mut chunk = BlockPriorityChunk::new();
        chunk.set(1, 2, 3, FLAG_SUBMERGE | FILLING);

        assert_eq!(chunk.get(1, 2, 3), FILLING);
        assert_eq!(chunk.get_raw(1, 2, 3), FLAG_SUBMERGE | FILLING);
    }

    #[test]
    fn reset_fills_the_grid_with_none() {
        let mut chunk = BlockPriorityChunk::new();
        chunk.set(0, 0, 0, PREFAB);
        chunk.set(31, 319, 31, CAVE);

        chunk.reset();

        assert_eq!(chunk.get(0, 0, 0), NONE);
        assert_eq!(chunk.get(31, 319, 31), NONE);
        assert!(chunk.blocks().iter().all(|&value| value == NONE));
        assert_eq!(chunk.blocks().len(), PRIORITY_COUNT);
    }

    #[test]
    fn grid_covers_the_full_chunk_height() {
        let mut chunk = BlockPriorityChunk::new();
        chunk.set(0, HEIGHT - 1, 0, PREFAB);
        assert_eq!(chunk.get(0, HEIGHT - 1, 0), PREFAB);
        assert_eq!(chunk.blocks().len(), (SIZE * SIZE * HEIGHT) as usize);
    }

    #[test]
    fn none_modifier_is_the_identity() {
        let modifier = NoneModifier::NONE;
        assert_eq!(modifier.modify_current(CAVE, PREFAB), CAVE);
        assert_eq!(modifier.modify_target(CAVE, PREFAB), PREFAB);
    }
}
