//! Runtime chunk column.
//!
//! A column owns ten lazily allocated 32³ sections plus the column-level tint,
//! environment base values, heightmap placeholder and chunk flags. It is the
//! only place where `worldgen_model::GeneratedChunk` is converted into runtime
//! state.

use hytale_math::chunk_util::{SIZE, SIZE_BLOCKS};

/// Legacy full column height in blocks.
const HEIGHT: i32 = SECTION_COUNT as i32 * SIZE;

use worldgen_model::chunk::{GeneratedChunk, ENVIRONMENT_UNKNOWN_ID};
use worldgen_model::registry::Registries;

use crate::chunk::palette::Palette;
use crate::chunk::section::{BlockSection, EnvironmentSection, FluidSection};
use crate::chunk::tint::TintPalette;

/// Number of 32³ sections in a legacy full-height column.
pub const SECTION_COUNT: usize = 10;
/// Number of column entries (`SIZE * SIZE`).
pub const COLUMN_COUNT: usize = (SIZE * SIZE) as usize;
/// Number of block entries (`SIZE * SIZE * HEIGHT`).
pub const BLOCK_COUNT: usize = (SIZE * SIZE * HEIGHT) as usize;

/// One section's block, fluid and environment components.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkSection {
    /// Block/filler/rotation palettes, ticking and light.
    pub block: BlockSection,
    /// Fluid palette and levels.
    pub fluid: FluidSection,
    /// Vertical environment runs.
    pub environment: EnvironmentSection,
}

/// Chunk flags mirrored from Java's `ChunkFlag`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChunkFlags {
    /// Chunk was newly generated.
    pub newly_generated: bool,
    /// Chunk has a column record on disk.
    pub on_disk: bool,
    /// Chunk came from a pre-split-column format and needs a full rewrite.
    pub needs_format_rewrite: bool,
    /// Chunk has unsaved changes.
    pub needs_saving: bool,
}

/// Java `HeightmapColumn.NO_HEIGHT` — no opaque block was found in a column.
pub const NO_HEIGHT: i32 = i32::MIN;

/// The legacy/tree heightmap of one column.
///
/// The legacy backend stores one `int` per column in memory (`serializeToInts`
/// widens the persisted `short`), so the height can be [`NO_HEIGHT`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heightmap {
    /// Legacy per-column top (Java `LegacyHeightmap`) — highest non-transparent
    /// block y, or [`NO_HEIGHT`].
    Legacy(Vec<i32>),
    /// Tree heightmap bytes.
    Tree(Vec<u8>),
}

impl Default for Heightmap {
    fn default() -> Self {
        Self::Legacy(vec![NO_HEIGHT; COLUMN_COUNT])
    }
}

impl Heightmap {
    /// Java `IHeightmap.serializeToInts()` for the legacy backend — one `int`
    /// per column in `z * SIZE + x` order (`ChunkUtil.indexColumn`).
    pub fn serialize_to_ints(&self) -> Option<Vec<i32>> {
        match self {
            Self::Legacy(heights) => Some(heights.clone()),
            Self::Tree(_) => None,
        }
    }
}

/// A runtime chunk column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkColumn {
    index: i64,
    x: i32,
    z: i32,
    sections: [Option<Box<ChunkSection>>; SECTION_COUNT],
    tint: TintPalette,
    environment_column: Vec<i32>,
    heightmap: Heightmap,
    flags: ChunkFlags,
}

impl ChunkColumn {
    /// Create an empty column at `(x, z)`.
    pub fn new(x: i32, z: i32) -> Self {
        Self {
            index: hytale_math::chunk_util::index_chunk(x, z),
            x,
            z,
            sections: std::array::from_fn(|_| None),
            tint: TintPalette::new(),
            environment_column: vec![ENVIRONMENT_UNKNOWN_ID; COLUMN_COUNT],
            heightmap: Heightmap::default(),
            flags: ChunkFlags::default(),
        }
    }

    /// Column x coordinate.
    pub const fn x(&self) -> i32 {
        self.x
    }

    /// Column z coordinate.
    pub const fn z(&self) -> i32 {
        self.z
    }

    /// Packed chunk index.
    pub const fn index(&self) -> i64 {
        self.index
    }

    /// Chunk flags.
    pub const fn flags(&self) -> ChunkFlags {
        self.flags
    }

    /// Mutable chunk flags.
    pub fn flags_mut(&mut self) -> &mut ChunkFlags {
        &mut self.flags
    }

    /// Heightmap placeholder.
    pub const fn heightmap(&self) -> &Heightmap {
        &self.heightmap
    }

    /// Replace the heightmap placeholder.
    pub fn set_heightmap(&mut self, heightmap: Heightmap) {
        self.heightmap = heightmap;
    }

    /// Section at `y`'s section index, if allocated.
    pub fn section(&self, y: i32) -> Option<&ChunkSection> {
        let index = section_index(y)?;
        self.sections[index].as_deref()
    }

    /// Mutable section at `y`'s section index, if allocated.
    pub fn section_mut(&mut self, y: i32) -> Option<&mut ChunkSection> {
        let index = section_index(y)?;
        self.sections[index].as_deref_mut()
    }

    /// Sections in Java order (`0..10`).
    pub fn sections(&self) -> impl Iterator<Item = Option<&ChunkSection>> {
        self.sections.iter().map(|section| section.as_deref())
    }

    /// Allocate a section if needed.
    pub fn ensure_section(&mut self, y: i32) -> Option<&mut ChunkSection> {
        let index = section_index(y)?;
        Some(self.sections[index].get_or_insert_with(|| Box::new(ChunkSection::default())))
    }

    /// True when the section holding `y` is allocated.
    pub fn has_section(&self, y: i32) -> bool {
        section_index(y)
            .map(|index| self.sections[index].is_some())
            .unwrap_or(false)
    }

    /// Java `BlockChunk.getTint(int x, int z)`.
    pub fn tint(&self, x: i32, z: i32) -> i32 {
        self.tint.get(x, z)
    }

    /// Java `BlockChunk.setTint(int x, int z, int tint)`.
    pub fn set_tint(&mut self, x: i32, z: i32, tint: i32) {
        if self.tint.get(x, z) != tint {
            self.tint.set(x, z, tint);
            self.flags.needs_saving = true;
        }
    }

    /// The serialized Java `IntBytePalette` form embedded in `SetColumn` and
    /// `SetChunkTintmap`.
    pub fn tint_bytes(&self) -> Vec<u8> {
        self.tint.serialize()
    }

    /// Java `BlockChunk.getEnvironmentColumn(int x, int z)` equivalent.
    pub fn environment_column(&self, x: i32, z: i32) -> i32 {
        self.environment_column[column_index(x, z)]
    }

    /// Replace one column environment and clear per-block overrides.
    pub fn set_environment_column(&mut self, x: i32, z: i32, environment: i32) {
        self.environment_column[column_index(x, z)] = environment;
        for index in 0..SECTION_COUNT {
            if let Some(section) = self.sections[index].as_mut() {
                section.environment.set_column(x, z, environment);
            }
        }
    }

    /// Java `BlockChunk.getBlock(int x, int y, int z)` / `BlockSection.get`.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> i32 {
        self.section(y)
            .map(|section| section.block.get_block(x, y & (SIZE - 1), z))
            .unwrap_or(0)
    }

    /// Java `BlockSection.getFiller`.
    pub fn get_filler(&self, x: i32, y: i32, z: i32) -> i32 {
        self.section(y)
            .map(|section| section.block.get_filler_at(x, y & (SIZE - 1), z))
            .unwrap_or(0)
    }

    /// Java `BlockSection.getRotationIndex`.
    pub fn get_rotation(&self, x: i32, y: i32, z: i32) -> i32 {
        self.section(y)
            .map(|section| section.block.get_rotation_index_at(x, y & (SIZE - 1), z))
            .unwrap_or(0)
    }

    /// Set one block; allocates the section on first write.
    pub fn set_block(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        rotation: i32,
        filler: i32,
    ) -> crate::chunk::section::Result<bool> {
        let Some(section) = self.ensure_section(y) else {
            return Ok(false);
        };
        let changed = section
            .block
            .set_block(x, y & (SIZE - 1), z, block_id, rotation, filler)?;
        if changed {
            self.flags.needs_saving = true;
        }
        Ok(changed)
    }

    /// Get a fluid id.
    pub fn get_fluid_id(&self, x: i32, y: i32, z: i32) -> i32 {
        self.section(y)
            .map(|section| section.fluid.get_fluid_id_at(x, y & (SIZE - 1), z))
            .unwrap_or(0)
    }

    /// Get a fluid level.
    pub fn get_fluid_level(&self, x: i32, y: i32, z: i32) -> i8 {
        self.section(y)
            .map(|section| section.fluid.get_fluid_level_at(x, y & (SIZE - 1), z))
            .unwrap_or(0)
    }

    /// Set one fluid; allocates the section on first write.
    pub fn set_fluid(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        level: i8,
    ) -> crate::chunk::section::Result<bool> {
        let Some(section) = self.ensure_section(y) else {
            return Ok(false);
        };
        let changed = section
            .fluid
            .set_fluid_at(x, y & (SIZE - 1), z, fluid_id, level)?;
        if changed {
            self.flags.needs_saving = true;
        }
        Ok(changed)
    }

    /// Get the effective environment at a block.
    pub fn get_environment(&self, x: i32, y: i32, z: i32) -> i32 {
        self.section(y)
            .map(|section| section.environment.get(x, y & (SIZE - 1), z))
            .unwrap_or_else(|| self.environment_column(x, z))
    }

    /// Set one block environment; allocates the section on first write.
    pub fn set_environment(&mut self, x: i32, y: i32, z: i32, environment: i32) -> bool {
        let Some(section) = self.ensure_section(y) else {
            return false;
        };
        let changed = section.environment.set(x, y & (SIZE - 1), z, environment);
        if changed {
            self.flags.needs_saving = true;
        }
        changed
    }

    /// Convert a generated chunk into the runtime model.
    ///
    /// Sections are allocated only when they contain block, fluid, environment
    /// or non-default rotation/filler data. Every value present in
    /// `GeneratedChunk` is copied; tint and the environment base columns are
    /// column-level fields.
    ///
    /// The legacy heightmap is left at [`NO_HEIGHT`] because computing it needs
    /// the block opacity assets; use
    /// [`ChunkColumn::from_generated_chunk_with_registries`] when the registries
    /// are available (the packet and storage paths always should be).
    pub fn from_generated_chunk(generated: &GeneratedChunk) -> crate::chunk::section::Result<Self> {
        Self::convert_generated(generated, None)
    }

    /// Convert a generated chunk and compute the Java `LegacyHeightmap`.
    ///
    /// The heightmap is the highest non-transparent block in each column
    /// (Java `HeightmapColumn.onSectionGenerated` fed by
    /// `GetOpacity.FROM_SECTION`): block id 0 and ids missing from the block
    /// registry are transparent.
    pub fn from_generated_chunk_with_registries(
        generated: &GeneratedChunk,
        registries: &Registries,
    ) -> crate::chunk::section::Result<Self> {
        Self::convert_generated(generated, Some(registries))
    }

    fn convert_generated(
        generated: &GeneratedChunk,
        registries: Option<&Registries>,
    ) -> crate::chunk::section::Result<Self> {
        let blocks = generated.blocks();
        let rotations = generated.rotation();
        let fillers = generated.filler();
        let fluids = generated.fluid();
        let levels = generated.fluid_level();
        let environments = generated.environment();
        let tint = generated.tint();
        let environment_column = generated.environment_column();
        if blocks.len() != BLOCK_COUNT
            || rotations.len() != BLOCK_COUNT
            || fillers.len() != BLOCK_COUNT
            || fluids.len() != BLOCK_COUNT
            || levels.len() != BLOCK_COUNT
            || environments.len() != BLOCK_COUNT
        {
            return Err(crate::chunk::section::SectionError::InvalidData(
                "generated chunk block arrays have the wrong length",
            ));
        }
        if tint.len() != COLUMN_COUNT || environment_column.len() != COLUMN_COUNT {
            return Err(crate::chunk::section::SectionError::InvalidData(
                "generated chunk column arrays have the wrong length",
            ));
        }

        let mut column = Self::new(generated.get_x(), generated.get_z());
        column.index = generated.get_index();
        // Java's generator sets the tint in `x`-major, `z`-minor order; the
        // palette's key insertion order is part of the packet bytes.
        for x in 0..SIZE {
            for z in 0..SIZE {
                column.tint.set(x, z, tint[column_index(x, z)]);
            }
        }
        column
            .environment_column
            .copy_from_slice(environment_column);
        column.flags.newly_generated = true;
        if let Some(registries) = registries {
            column.heightmap = Heightmap::Legacy(legacy_heightmap(generated, registries));
        }

        for section_index in 0..SECTION_COUNT {
            let start = section_index * SIZE_BLOCKS as usize;
            let end = start + SIZE_BLOCKS as usize;
            let needs_section = blocks[start..end].iter().any(|value| *value != 0)
                || rotations[start..end].iter().any(|value| *value != 0)
                || fillers[start..end].iter().any(|value| *value != 0)
                || fluids[start..end].iter().any(|value| *value != 0)
                || levels[start..end].iter().any(|value| *value != 0)
                || environments[start..end]
                    .iter()
                    .any(|value| *value != i32::MIN)
                || environment_column
                    .iter()
                    .any(|value| *value != ENVIRONMENT_UNKNOWN_ID);
            if !needs_section {
                continue;
            }
            // Java `GeneratedChunkSection.toChunkSection`: the block palette is
            // built from the run-count map (fastutil iteration order), while
            // filler/rotation palettes keep the internal ids the live
            // generation `set` calls handed out (first-use order).
            let block_palette = Palette::from_generated_ids(&blocks[start..end])?;
            let filler_palette = Palette::from_ids_with_order(
                &fillers[start..end],
                generated.filler_first_use(section_index as i32),
            )?;
            let rotation_palette = Palette::from_ids_with_order(
                &rotations[start..end],
                generated.rotation_first_use(section_index as i32),
            )?;
            let section = column.sections[section_index].get_or_insert_with(|| {
                Box::new(ChunkSection {
                    block: BlockSection::from_palettes(
                        block_palette,
                        filler_palette,
                        rotation_palette,
                    ),
                    ..ChunkSection::default()
                })
            });

            for index in start..end {
                let local = index - start;
                section
                    .fluid
                    .set_fluid(local, fluids[index], levels[index])?;
            }

            let first_environment = (start..end)
                .map(|index| {
                    let local = index - start;
                    let x = (local & (SIZE as usize - 1)) as i32;
                    let z = ((local >> 5) & (SIZE as usize - 1)) as i32;
                    let y = (local >> 10) as i32;
                    generated.get_environment(x, section_index as i32 * SIZE + y, z)
                })
                .next()
                .unwrap_or(ENVIRONMENT_UNKNOWN_ID);
            section.environment.set_uniform(first_environment);
            for index in start..end {
                let local = index - start;
                let x = (local & (SIZE as usize - 1)) as i32;
                let z = ((local >> 5) & (SIZE as usize - 1)) as i32;
                let y = (local >> 10) as i32;
                let environment = generated.get_environment(x, section_index as i32 * SIZE + y, z);
                if environment != first_environment {
                    section.environment.set(x, y, z, environment);
                }
            }
        }
        Ok(column)
    }
}

fn section_index(y: i32) -> Option<usize> {
    if !(0..HEIGHT).contains(&y) {
        return None;
    }
    let index = (y >> 5) as usize;
    (index < SECTION_COUNT).then_some(index)
}

/// Java `LegacyHeightmap.onSectionGenerated` for every section, collapsed to
/// the equivalent topmost-non-transparent scan per column.
///
/// `GetOpacity.FROM_SECTION` treats block id 0 and ids missing from the block
/// map as `Opacity.Transparent`.
fn legacy_heightmap(generated: &GeneratedChunk, registries: &Registries) -> Vec<i32> {
    let mut heights = vec![NO_HEIGHT; COLUMN_COUNT];
    for z in 0..SIZE {
        for x in 0..SIZE {
            let mut height = NO_HEIGHT;
            for y in (0..HEIGHT).rev() {
                let block_id = generated.get_block(x, y, z);
                if block_id == 0 {
                    continue;
                }
                let opaque = registries
                    .blocks
                    .get(block_id)
                    .is_some_and(|block| !block.opacity.is_transparent());
                if opaque {
                    height = y;
                    break;
                }
            }
            heights[column_index(x, z)] = height;
        }
    }
    heights
}

#[inline]
fn column_index(x: i32, z: i32) -> usize {
    hytale_math::chunk_util::index_column(x, z) as usize
}
