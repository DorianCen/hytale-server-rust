// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/CoreServer/src/main/java/com/hypixel/hytale/server/core/universe/world/worldgen/GeneratedBlockChunk.java`
//! and `.../worldgen/GeneratedChunkSection.java`,
//! `.../worldgen/GeneratedEnvironment.java`,
//! `.../world/chunk/section/FluidSection.java`.
//!
//! Rust chunk data model — replaces CoreServer's `GeneratedBlockChunk`
//! (and the `GeneratedChunkSection` / `GeneratedEnvironment` / `FluidSection`
//! classes it delegates to).
//!
//! Java keeps the block data in `HEIGHT_SECTIONS` (10) lazily created
//! `GeneratedChunkSection`s, each holding a `SIZE_BLOCKS` (32³) `int[]` plus
//! two palette-backed side channels (fillers and rotations). The Rust model
//! flattens all of that into plain `Vec`s, one entry per block, laid out
//! y-major (full height), then z, then x — the exact concatenation order of
//! the Java sections.

use std::fmt;

use hytale_math::chunk_util::{BITS, BITS2, MIN_Y, SIZE, SIZE_BLOCKS, SIZE_MASK};

// The legacy chunk height constants are re-exported by `block_priority` with
// the `deprecated` lint allowed (`hytale-math` keeps the Java `@Deprecated`
// marker, which is about legacy storage, not about the world generator).
use crate::block_priority::{HEIGHT, HEIGHT_SECTIONS};

/// Java `WorldGenConstants.ENVIRONMENT_NOT_SET` = `Integer.MIN_VALUE` — the
/// "no environment assigned" marker.
pub const ENVIRONMENT_NOT_SET: i32 = i32::MIN;

/// Java `Environment.UNKNOWN_ID` = `0` — the environment a column holds
/// before any environment has been assigned to it.
pub const ENVIRONMENT_UNKNOWN_ID: i32 = 0;

/// Java `FillerBlockUtil.NO_FILLER` = `0` — no filler offset.
pub const NO_FILLER: i32 = 0;

/// Java `BlockType.EMPTY_ID` = `0` — air.
pub const EMPTY_BLOCK_ID: i32 = 0;

/// Java `Fluid.EMPTY_ID` = `0` — no fluid.
pub const EMPTY_FLUID_ID: i32 = 0;

/// Java `RotationTuple.NONE_INDEX` = `0` — no rotation.
pub const ROTATION_NONE_INDEX: i32 = 0;

/// Number of block entries of the flat chunk arrays: `SIZE * SIZE * HEIGHT`.
const BLOCK_COUNT: usize = (SIZE * SIZE * HEIGHT) as usize;

/// Number of column entries (tint, environment column): `SIZE * SIZE`.
const COLUMN_COUNT: usize = (SIZE * SIZE) as usize;

/// The number of chunk sections a full-height chunk holds.
pub const SECTION_COUNT: i32 = HEIGHT_SECTIONS;

/// Rust chunk data model — the buffer the world generator populators write
/// into.
///
/// Ported from Java `GeneratedBlockChunk`; the differences are deliberate and
/// documented per method:
///
/// * the lazily created, palette-backed chunk sections become flat `Vec`s,
/// * the run-length encoded `GeneratedEnvironment` becomes a per-column base
///   value plus per-block overrides,
/// * the ECS-bound `Holder<ChunkStore>` / `BlockChunk` conversion and the
///   entity chunk are not ported at all (see `plan.md`).
///
/// ## Not ported: `GeneratedBlockChunk.getHeight(int x, int z)`
///
/// The Java method scans the chunk sections downwards and stops at the first
/// block whose `BlockType` is not `Opacity.Transparent`, i.e. it needs the
/// block type registry (`BlockType.getAssetMap()`), which this crate does not
/// have yet — the block/fluid/environment registries arrive with the
/// registry slice of the chunk core. The world generator itself does not use
/// `getHeight` (it reads the height from the interpolator table instead).
#[derive(Clone, PartialEq, Eq)]
pub struct GeneratedChunk {
    /// Java `protected long index` — the packed chunk index.
    index: i64,
    /// Java `protected int x` — the chunk x coordinate.
    x: i32,
    /// Java `protected int z` — the chunk z coordinate.
    z: i32,
    /// Java `GeneratedChunkSection.data` — the block id of every block.
    blocks: Vec<i32>,
    /// Java `GeneratedChunkSection.rotations` — the rotation index of every
    /// block (`RotationTuple` index).
    rotation: Vec<i32>,
    /// Java `GeneratedChunkSection.fillers` — the packed filler offset of
    /// every block.
    filler: Vec<i32>,
    /// Java `GeneratedBlockChunk.tint` — the ARGB tint of every column.
    tint: Vec<i32>,
    /// Java `GeneratedEnvironment.columns` — the environment of every column.
    environment_column: Vec<i32>,
    /// Per-block environment overrides; [`ENVIRONMENT_NOT_SET`] means "inherit
    /// the column environment".
    environment: Vec<i32>,
    /// Java `FluidSection.typePalette` — the fluid id of every block.
    fluid: Vec<i32>,
    /// Java `FluidSection.levelData` — the fluid level (0-15) of every block.
    fluid_level: Vec<i8>,
    /// Bit mask of the chunk sections that have been initialized, mirroring
    /// Java's `GeneratedChunkSection[] chunkSections` where an unwritten
    /// section is `null`.
    initialized_sections: u32,
}

impl GeneratedChunk {
    /// Java `new GeneratedBlockChunk()`.
    pub fn new() -> Self {
        Self::with_coordinates(0, 0, 0)
    }

    /// Java `new GeneratedBlockChunk(long index, int x, int z)`.
    pub fn with_coordinates(index: i64, x: i32, z: i32) -> Self {
        Self {
            index,
            x,
            z,
            blocks: vec![EMPTY_BLOCK_ID; BLOCK_COUNT],
            rotation: vec![ROTATION_NONE_INDEX; BLOCK_COUNT],
            filler: vec![NO_FILLER; BLOCK_COUNT],
            tint: vec![0; COLUMN_COUNT],
            environment_column: vec![ENVIRONMENT_UNKNOWN_ID; COLUMN_COUNT],
            environment: vec![ENVIRONMENT_NOT_SET; BLOCK_COUNT],
            fluid: vec![EMPTY_FLUID_ID; BLOCK_COUNT],
            fluid_level: vec![0; BLOCK_COUNT],
            initialized_sections: 0,
        }
    }

    /// Java `setCoordinates(long index, int x, int z)`.
    pub fn set_coordinates(&mut self, index: i64, x: i32, z: i32) {
        self.index = index;
        self.x = x;
        self.z = z;
    }

    /// Java `getIndex()`.
    #[inline]
    pub fn get_index(&self) -> i64 {
        self.index
    }

    /// Java `getX()` — the chunk x coordinate.
    #[inline]
    pub fn get_x(&self) -> i32 {
        self.x
    }

    /// Java `getZ()` — the chunk z coordinate.
    #[inline]
    pub fn get_z(&self) -> i32 {
        self.z
    }

    /// Java `getTint(int x, int z)` — the ARGB tint of the column.
    #[inline]
    pub fn get_tint(&self, x: i32, z: i32) -> i32 {
        self.tint[column_index(x, z) as usize]
    }

    /// Java `setTint(int x, int z, int tint)`.
    #[inline]
    pub fn set_tint(&mut self, x: i32, z: i32, tint: i32) {
        self.tint[column_index(x, z) as usize] = tint;
    }

    /// Java `setEnvironmentColumn(int x, int z, int environment)`.
    ///
    /// Java's `EnvironmentColumn.set(value)` replaces the *whole* column, so
    /// per-block overrides of that column are dropped here as well.
    pub fn set_environment_column(&mut self, x: i32, z: i32, environment: i32) {
        self.environment_column[column_index(x, z) as usize] = environment;
        for y in MIN_Y..HEIGHT {
            self.environment[block_index(x, y, z) as usize] = ENVIRONMENT_NOT_SET;
        }
    }

    /// Java `setEnvironment(int x, int y, int z, int environment)`.
    ///
    /// Java's `EnvironmentColumn.set(y, value)` splits the run that contains
    /// `y`; the flat Rust model stores the override directly. Writes outside
    /// the world height are ignored (Java would corrupt the lowest run of the
    /// column instead).
    pub fn set_environment(&mut self, x: i32, y: i32, z: i32, environment: i32) {
        if !in_world_height(y) {
            return;
        }
        self.environment[block_index(x, y, z) as usize] = environment;
    }

    /// Java `getEnvironment(int x, int y, int z)`.
    ///
    /// Returns the per-block override when one was written, otherwise the
    /// environment of the column.
    pub fn get_environment(&self, x: i32, y: i32, z: i32) -> i32 {
        let column = self.environment_column[column_index(x, z) as usize];
        if !in_world_height(y) {
            return column;
        }
        let value = self.environment[block_index(x, y, z) as usize];
        if value == ENVIRONMENT_NOT_SET {
            column
        } else {
            value
        }
    }

    /// Java `getRotationIndex(int x, int y, int z)`.
    #[inline]
    pub fn get_rotation_index(&self, x: i32, y: i32, z: i32) -> i32 {
        if !in_world_height(y) {
            return 0;
        }
        self.rotation[block_index(x, y, z) as usize]
    }

    /// Java `getBlock(int x, int y, int z)` — the block id, `0` (air) outside
    /// the world height.
    #[inline]
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> i32 {
        if !in_world_height(y) {
            return 0;
        }
        self.blocks[block_index(x, y, z) as usize]
    }

    /// Java `GeneratedChunkSection.getFiller(int x, int y, int z)` — the
    /// packed filler offset of the block.
    #[inline]
    pub fn get_filler(&self, x: i32, y: i32, z: i32) -> i32 {
        if !in_world_height(y) {
            return 0;
        }
        self.filler[block_index(x, y, z) as usize]
    }

    /// Java `setBlock(int x, int y, int z, int blockId, int rotation, int filler)`.
    ///
    /// Java logs an info message and drops the write when `y` is outside the
    /// world bounds; the Rust model silently ignores it (this crate has no
    /// logger dependency).
    ///
    /// Java also refuses to create a section for an air block ("Do not
    /// initialize ChunkSection if the block is air"), so writing air into a
    /// section that was never written to is a complete no-op — including the
    /// rotation and filler values. The `initialized_sections` mask keeps that
    /// behaviour.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_id: i32, rotation: i32, filler: i32) {
        if !in_world_height(y) {
            return;
        }
        let section = y >> BITS;
        if !self.section_initialized(section) {
            // Java: "Do not initialize ChunkSection if the block is air."
            if block_id == EMPTY_BLOCK_ID {
                return;
            }
            self.mark_section_initialized(section);
        }
        let index = block_index(x, y, z) as usize;
        self.blocks[index] = block_id;
        self.rotation[index] = rotation;
        self.filler[index] = filler;
    }

    /// Java `FluidSection.setFluid(int x, int y, int z, int fluidId, byte level)`.
    ///
    /// The level is masked to four bits; a zero level forces the fluid to
    /// [`EMPTY_FLUID_ID`] and an empty fluid forces the level to zero. Returns
    /// whether anything changed, like the Java method.
    ///
    /// Writes outside the world height are ignored (the Java
    /// `ChunkGeneratorExecution.setFluid` performs that bounds check before
    /// it touches the fluid section; the flat Rust model has no section to
    /// wrap a stray y into).
    pub fn set_fluid(&mut self, x: i32, y: i32, z: i32, fluid: i32, level: i8) -> bool {
        if !in_world_height(y) {
            return false;
        }
        let mut fluid = fluid;
        let mut level = level & 0x0F;
        if level == 0 {
            fluid = EMPTY_FLUID_ID;
        }
        if fluid == EMPTY_FLUID_ID {
            level = 0;
        }
        let index = block_index(x, y, z) as usize;
        let changed = self.fluid[index] != fluid || self.fluid_level[index] != level;
        self.fluid[index] = fluid;
        self.fluid_level[index] = level;
        changed
    }

    /// Java `FluidSection.getFluidId(int x, int y, int z)`.
    #[inline]
    pub fn get_fluid_id(&self, x: i32, y: i32, z: i32) -> i32 {
        if !in_world_height(y) {
            return EMPTY_FLUID_ID;
        }
        self.fluid[block_index(x, y, z) as usize]
    }

    /// Java `FluidSection.getFluidLevel(int x, int y, int z)` — the fluid level
    /// in the range `0..=15`.
    #[inline]
    pub fn get_fluid_level(&self, x: i32, y: i32, z: i32) -> i8 {
        if !in_world_height(y) {
            return 0;
        }
        self.fluid_level[block_index(x, y, z) as usize]
    }

    /// The block ids of the whole chunk.
    #[inline]
    pub fn blocks(&self) -> &[i32] {
        &self.blocks
    }

    /// The rotation indices of the whole chunk.
    #[inline]
    pub fn rotation(&self) -> &[i32] {
        &self.rotation
    }

    /// The filler offsets of the whole chunk.
    #[inline]
    pub fn filler(&self) -> &[i32] {
        &self.filler
    }

    /// The column tints of the whole chunk.
    #[inline]
    pub fn tint(&self) -> &[i32] {
        &self.tint
    }

    /// The per-column environments of the whole chunk.
    #[inline]
    pub fn environment_column(&self) -> &[i32] {
        &self.environment_column
    }

    /// The per-block environment overrides of the whole chunk.
    #[inline]
    pub fn environment(&self) -> &[i32] {
        &self.environment
    }

    /// The fluid ids of the whole chunk.
    #[inline]
    pub fn fluid(&self) -> &[i32] {
        &self.fluid
    }

    /// The fluid levels of the whole chunk.
    #[inline]
    pub fn fluid_level(&self) -> &[i8] {
        &self.fluid_level
    }

    /// Whether the chunk section holding `y` has been written to.
    #[inline]
    fn section_initialized(&self, section: i32) -> bool {
        self.initialized_sections & (1 << section) != 0
    }

    /// Marks the chunk section holding `y` as written to.
    #[inline]
    fn mark_section_initialized(&mut self, section: i32) {
        self.initialized_sections |= 1 << section;
    }
}

impl Default for GeneratedChunk {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for GeneratedChunk {
    /// A compact summary — the flat arrays hold hundreds of thousands of
    /// entries each and must not be dumped.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GeneratedChunk")
            .field("index", &self.index)
            .field("x", &self.x)
            .field("z", &self.z)
            .field("blocks", &self.blocks.len())
            .field("rotation", &self.rotation.len())
            .field("filler", &self.filler.len())
            .field("tint", &self.tint.len())
            .field("environment_column", &self.environment_column.len())
            .field("environment", &self.environment.len())
            .field("fluid", &self.fluid.len())
            .field("fluid_level", &self.fluid_level.len())
            .field("initialized_sections", &self.initialized_sections)
            .finish()
    }
}

/// Java `ChunkUtil.indexColumn(int x, int z)` — `(z & SIZE_MASK) << BITS |
/// (x & SIZE_MASK)`.
#[inline]
fn column_index(x: i32, z: i32) -> i32 {
    (z & SIZE_MASK) << BITS | (x & SIZE_MASK)
}

/// Java `y < ChunkUtil.MIN_Y || y >= ChunkUtil.HEIGHT` — the world height
/// bounds check.
#[inline]
fn in_world_height(y: i32) -> bool {
    (MIN_Y..HEIGHT).contains(&y)
}

/// Java `ChunkUtil.indexBlock(int x, int y, int z)` — the index of a block
/// inside a single 32³ chunk section: y-major, then z, then x.
#[inline]
fn section_block_index(x: i32, y: i32, z: i32) -> i32 {
    (y & SIZE_MASK) << BITS2 | (z & SIZE_MASK) << BITS | (x & SIZE_MASK)
}

/// The index of a block inside the flat, full-height chunk arrays.
///
/// Java splits the chunk into `HEIGHT_SECTIONS` (10) sections; the section of
/// a block is `ChunkUtil.indexSection(y) == y >> BITS` and the in-section
/// index is `ChunkUtil.indexBlock(x, y, z)` (which masks `y` with
/// `SIZE_MASK`). The flat Rust model concatenates the sections in order:
///
/// ```text
/// (y >> 5) * SIZE_BLOCKS + (y & 31) << 10 | (z & 31) << 5 | (x & 31)
///     == y << 10 | z << 5 | x            (for 0 <= y < HEIGHT)
/// ```
#[inline]
pub fn block_index(x: i32, y: i32, z: i32) -> i32 {
    let section = y >> BITS;
    section * SIZE_BLOCKS + section_block_index(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_index_is_y_major_then_z_then_x() {
        // The flat layout must be the concatenation of the Java sections.
        assert_eq!(block_index(0, 0, 0), 0);
        assert_eq!(block_index(1, 0, 0), 1);
        assert_eq!(block_index(0, 0, 1), 1 << BITS);
        assert_eq!(block_index(0, 1, 0), 1 << BITS2);
        assert_eq!(block_index(3, 5, 7), (5 << BITS2) | (7 << BITS) | 3);

        // Section boundaries: the second section starts right after the first.
        assert_eq!(block_index(0, 32, 0), SIZE_BLOCKS);
        assert_eq!(block_index(0, 319, 0), 9 * SIZE_BLOCKS + (31 << BITS2));
        assert_eq!(block_index(0, HEIGHT - 1, 0) as usize, BLOCK_COUNT - 1024);
    }

    #[test]
    fn set_block_get_block_round_trip() {
        let mut chunk = GeneratedChunk::new();
        for &(x, y, z) in &[(0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1), (31, 319, 31)] {
            chunk.set_block(x, y, z, 42, 3, 7);
            assert_eq!(chunk.get_block(x, y, z), 42);
            assert_eq!(chunk.get_rotation_index(x, y, z), 3);
            assert_eq!(chunk.get_filler(x, y, z), 7);
        }

        // Neighbouring blocks are untouched.
        chunk.set_block(1, 2, 3, 99, 0, 0);
        assert_eq!(chunk.get_block(0, 2, 3), 0);
        assert_eq!(chunk.get_block(2, 2, 3), 0);
        assert_eq!(chunk.get_block(1, 1, 3), 0);
        assert_eq!(chunk.get_block(1, 3, 3), 0);
        assert_eq!(chunk.get_block(1, 2, 2), 0);
        assert_eq!(chunk.get_block(1, 2, 4), 0);
    }

    #[test]
    fn set_block_out_of_bounds_y_is_ignored() {
        let mut chunk = GeneratedChunk::new();
        chunk.set_block(0, HEIGHT, 0, 42, 1, 1);
        chunk.set_block(0, -1, 0, 42, 1, 1);

        assert_eq!(chunk.get_block(0, HEIGHT, 0), 0);
        assert_eq!(chunk.get_block(0, -1, 0), 0);
        assert_eq!(chunk.get_rotation_index(0, HEIGHT, 0), 0);
        assert_eq!(chunk.get_rotation_index(0, -1, 0), 0);
        assert!(chunk.blocks().iter().all(|&block| block == 0));
    }

    #[test]
    fn writing_air_into_a_fresh_section_is_a_noop() {
        let mut chunk = GeneratedChunk::new();

        // Java does not create a chunk section for an air block, so neither
        // the rotation nor the filler are stored.
        chunk.set_block(4, 70, 4, 0, 5, 9);
        assert_eq!(chunk.get_block(4, 70, 4), 0);
        assert_eq!(chunk.get_rotation_index(4, 70, 4), 0);
        assert_eq!(chunk.get_filler(4, 70, 4), 0);

        // Once the section exists, air writes do store rotation and filler.
        chunk.set_block(4, 70, 4, 12, 0, 0);
        chunk.set_block(4, 70, 4, 0, 5, 9);
        assert_eq!(chunk.get_block(4, 70, 4), 0);
        assert_eq!(chunk.get_rotation_index(4, 70, 4), 5);
        assert_eq!(chunk.get_filler(4, 70, 4), 9);
    }

    #[test]
    fn tint_round_trip() {
        let mut chunk = GeneratedChunk::new();
        assert_eq!(chunk.get_tint(0, 0), 0);

        chunk.set_tint(3, 7, 0xFF00_7F3Cu32 as i32);
        assert_eq!(chunk.get_tint(3, 7), 0xFF00_7F3Cu32 as i32);
        assert_eq!(chunk.get_tint(3, 6), 0);
        assert_eq!(chunk.get_tint(2, 7), 0);
    }

    #[test]
    fn environment_column_and_per_block() {
        let mut chunk = GeneratedChunk::new();

        // Unknown until a column environment is assigned.
        assert_eq!(chunk.get_environment(0, 0, 0), ENVIRONMENT_UNKNOWN_ID);

        chunk.set_environment_column(1, 2, 17);
        assert_eq!(chunk.get_environment(1, 0, 2), 17);
        assert_eq!(chunk.get_environment(1, 319, 2), 17);
        assert_eq!(chunk.get_environment(2, 0, 1), ENVIRONMENT_UNKNOWN_ID);

        // A per-block environment overrides the column environment.
        chunk.set_environment(1, 64, 2, 23);
        assert_eq!(chunk.get_environment(1, 64, 2), 23);
        assert_eq!(chunk.get_environment(1, 65, 2), 17);

        // Re-assigning the column drops the per-block override, like the
        // Java `EnvironmentColumn.set(value)`.
        chunk.set_environment_column(1, 2, 31);
        assert_eq!(chunk.get_environment(1, 64, 2), 31);
    }

    #[test]
    fn fluid_id_and_level_round_trip() {
        let mut chunk = GeneratedChunk::new();
        assert_eq!(chunk.get_fluid_id(0, 0, 0), EMPTY_FLUID_ID);
        assert_eq!(chunk.get_fluid_level(0, 0, 0), 0);

        assert!(chunk.set_fluid(2, 30, 2, 5, 8));
        assert_eq!(chunk.get_fluid_id(2, 30, 2), 5);
        assert_eq!(chunk.get_fluid_level(2, 30, 2), 8);

        // Writing the same values again is not a change.
        assert!(!chunk.set_fluid(2, 30, 2, 5, 8));

        // The level is masked to four bits.
        assert!(chunk.set_fluid(2, 30, 2, 5, 0x13));
        assert_eq!(chunk.get_fluid_level(2, 30, 2), 3);

        // A zero level forces the fluid to empty, an empty fluid forces the
        // level to zero.
        assert!(chunk.set_fluid(2, 30, 2, 5, 0));
        assert_eq!(chunk.get_fluid_id(2, 30, 2), EMPTY_FLUID_ID);
        assert_eq!(chunk.get_fluid_level(2, 30, 2), 0);

        assert!(chunk.set_fluid(2, 30, 2, 6, 4));
        assert!(chunk.set_fluid(2, 30, 2, 0, 4));
        assert_eq!(chunk.get_fluid_id(2, 30, 2), EMPTY_FLUID_ID);
        assert_eq!(chunk.get_fluid_level(2, 30, 2), 0);
        // Nothing changed, so the Java method reports no change.
        assert!(!chunk.set_fluid(2, 30, 2, 0, 4));

        // Writes outside the world height are ignored.
        assert!(!chunk.set_fluid(2, HEIGHT, 2, 5, 8));
        assert!(!chunk.set_fluid(2, -1, 2, 5, 8));
        assert_eq!(chunk.get_fluid_id(2, HEIGHT, 2), EMPTY_FLUID_ID);
        assert_eq!(chunk.get_fluid_level(2, HEIGHT, 2), 0);
    }

    #[test]
    fn coordinates_round_trip() {
        let mut chunk = GeneratedChunk::new();
        assert_eq!(chunk.get_index(), 0);
        assert_eq!(chunk.get_x(), 0);
        assert_eq!(chunk.get_z(), 0);

        chunk.set_coordinates(0x0000_0001_FFFF_FFFE, -3, 7);
        assert_eq!(chunk.get_index(), 0x0000_0001_FFFF_FFFE);
        assert_eq!(chunk.get_x(), -3);
        assert_eq!(chunk.get_z(), 7);

        let chunk = GeneratedChunk::with_coordinates(42, 1, 2);
        assert_eq!(chunk.get_index(), 42);
        assert_eq!(chunk.get_x(), 1);
        assert_eq!(chunk.get_z(), 2);
    }

    #[test]
    fn default_chunk_is_empty() {
        let chunk = GeneratedChunk::default();
        assert_eq!(chunk.blocks().len(), BLOCK_COUNT);
        assert_eq!(chunk.rotation().len(), BLOCK_COUNT);
        assert_eq!(chunk.filler().len(), BLOCK_COUNT);
        assert_eq!(chunk.tint().len(), COLUMN_COUNT);
        assert_eq!(chunk.environment_column().len(), COLUMN_COUNT);
        assert_eq!(chunk.environment().len(), BLOCK_COUNT);
        assert_eq!(chunk.fluid().len(), BLOCK_COUNT);
        assert_eq!(chunk.fluid_level().len(), BLOCK_COUNT);
        assert!(chunk.blocks().iter().all(|&block| block == EMPTY_BLOCK_ID));
        assert!(chunk.rotation().iter().all(|&r| r == ROTATION_NONE_INDEX));
        assert!(chunk.filler().iter().all(|&f| f == NO_FILLER));
        assert!(chunk
            .environment_column()
            .iter()
            .all(|&e| e == ENVIRONMENT_UNKNOWN_ID));
        assert!(chunk
            .environment()
            .iter()
            .all(|&e| e == ENVIRONMENT_NOT_SET));
        assert!(chunk.fluid().iter().all(|&f| f == EMPTY_FLUID_ID));
        assert!(chunk.fluid_level().iter().all(|&l| l == 0));
    }
}
