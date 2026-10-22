//! Block section: block/filler/rotation palettes, ticking and light.
//!
//! Ported from `CoreServer/.../world/chunk/section/BlockSection.java` (version
//! 7 storage). The storage layout is:
//!
//! ```text
//! i32 BE  block-migration asset count
//! u8      block palette type
//!         block palette (storage layout, byte keys)
//! [u16 BE ticking cardinality, u16 BE word count, u64 BE words]  (non-empty palette only)
//! u8      filler palette type + filler palette (short keys)
//! u8      rotation palette type + rotation palette (byte keys)
//!         local light data
//!         global light data
//! i16 BE  local change counter
//! i16 BE  global change counter
//! i64 BE  sky-lit heightmap stamp
//! ```

use std::collections::BTreeSet;

use hytale_protocol::io::{Reader, Writer};

use crate::chunk::palette::{
    ByteKey, KeyCodec, Palette, PaletteType, SetResult, ShortKey, BLOCK_COUNT,
};

use super::light::LightData;
use super::{Result, SectionError};

/// Ticking state and light arrays captured for a packet send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSectionPacket {
    /// Block/filler/rotation palette bytes (`BlockSection.serializeForPacket`).
    pub data: Vec<u8>,
    /// Local light packet bytes, when the cached light is current.
    pub local_light: Option<Vec<u8>>,
    /// Global light packet bytes, when the cached light is current.
    pub global_light: Option<Vec<u8>>,
}

/// One block section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSection {
    block_migration_asset_count: i32,
    blocks: Palette,
    fillers: Palette,
    rotations: Palette,
    ticking_blocks: BTreeSet<u16>,
    ticking_wait_adjacent: BTreeSet<u16>,
    local_light: LightData,
    local_change_counter: i16,
    global_light: LightData,
    global_change_counter: i16,
    sky_lit_stamp: i64,
}

impl Default for BlockSection {
    fn default() -> Self {
        Self::new()
    }
}

impl BlockSection {
    /// Java `BlockSection()`.
    pub fn new() -> Self {
        Self {
            block_migration_asset_count: 0,
            blocks: Palette::empty(),
            fillers: Palette::empty(),
            rotations: Palette::empty(),
            ticking_blocks: BTreeSet::new(),
            ticking_wait_adjacent: BTreeSet::new(),
            local_light: LightData::empty(0),
            local_change_counter: 0,
            global_light: LightData::empty(0),
            global_change_counter: 0,
            sky_lit_stamp: 0,
        }
    }

    /// Build a section with pre-built palettes (used by generated chunks,
    /// where Java hands the block palette from its run-count map but builds
    /// the filler/rotation palettes through `set`).
    pub fn from_palettes(blocks: Palette, fillers: Palette, rotations: Palette) -> Self {
        Self {
            block_migration_asset_count: 0,
            blocks,
            fillers,
            rotations,
            ticking_blocks: BTreeSet::new(),
            ticking_wait_adjacent: BTreeSet::new(),
            local_light: LightData::empty(0),
            local_change_counter: 0,
            global_light: LightData::empty(0),
            global_change_counter: 0,
            sky_lit_stamp: 0,
        }
    }

    /// Block palette.
    pub const fn blocks(&self) -> &Palette {
        &self.blocks
    }

    /// Filler palette.
    pub const fn fillers(&self) -> &Palette {
        &self.fillers
    }

    /// Rotation palette.
    pub const fn rotations(&self) -> &Palette {
        &self.rotations
    }

    /// Block-migration asset count written by the next serialize.
    pub const fn block_migration_asset_count(&self) -> i32 {
        self.block_migration_asset_count
    }

    /// Set the block-migration asset count written by the next serialize.
    pub fn set_block_migration_asset_count(&mut self, count: i32) {
        self.block_migration_asset_count = count;
    }

    /// Java `get(int index)`.
    pub fn get(&self, index: usize) -> i32 {
        self.blocks.get(index)
    }

    /// Java `get(int x, int y, int z)`.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> i32 {
        self.blocks.get(index_block(x, y, z))
    }

    /// Java `getFiller(int index)`.
    pub fn get_filler(&self, index: usize) -> i32 {
        self.fillers.get(index)
    }

    /// Java `getFiller(int x, int y, int z)`.
    pub fn get_filler_at(&self, x: i32, y: i32, z: i32) -> i32 {
        self.fillers.get(index_block(x, y, z))
    }

    /// Java `getRotationIndex(int index)`.
    pub fn get_rotation_index(&self, index: usize) -> i32 {
        self.rotations.get(index)
    }

    /// Java `getRotationIndex(int x, int y, int z)`.
    pub fn get_rotation_index_at(&self, x: i32, y: i32, z: i32) -> i32 {
        self.rotations.get(index_block(x, y, z))
    }

    /// Java `set(int blockIdx, int blockId, int rotation, int filler)`.
    pub fn set(&mut self, index: usize, block_id: i32, rotation: i32, filler: i32) -> Result<bool> {
        if !(0..=255).contains(&rotation) {
            return Err(SectionError::InvalidData("rotation index out of range"));
        }
        if !(0..=u16::MAX as i32).contains(&filler) {
            return Err(SectionError::InvalidData("filler offset out of range"));
        }
        let mut changed = set_palette(&mut self.blocks, index, block_id)?;
        changed |= set_palette(&mut self.fillers, index, filler)?;
        changed |= set_palette(&mut self.rotations, index, rotation)?;
        if changed {
            self.invalidate_local_light();
        }
        Ok(changed)
    }

    /// Java `set(int x, int y, int z, int blockId, int rotation, int filler)`.
    pub fn set_block(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        rotation: i32,
        filler: i32,
    ) -> Result<bool> {
        self.set(index_block(x, y, z), block_id, rotation, filler)
    }

    /// True when the palette contains `block_id`.
    pub fn contains(&self, block_id: i32) -> bool {
        self.blocks.contains(block_id)
    }

    /// True when every block is air.
    pub fn is_solid_air(&self) -> bool {
        self.blocks.is_solid(0)
    }

    /// Java `setTicking(int blockIdx, boolean ticking)`.
    pub fn set_ticking(&mut self, index: usize, ticking: bool) -> bool {
        if index >= BLOCK_COUNT {
            return false;
        }
        let index = index as u16;
        if ticking {
            self.ticking_blocks.insert(index)
        } else {
            self.ticking_blocks.remove(&index)
        }
    }

    /// Java `isTicking(int blockIdx, boolean)`.
    pub fn is_ticking(&self, index: usize) -> bool {
        u16::try_from(index)
            .map(|index| self.ticking_blocks.contains(&index))
            .unwrap_or(false)
    }

    /// Ticking values that still need an adjacent-block check.
    pub fn set_ticking_wait_adjacent(&mut self, index: usize, ticking: bool) -> bool {
        if index >= BLOCK_COUNT {
            return false;
        }
        let index = index as u16;
        if ticking {
            self.ticking_wait_adjacent.insert(index)
        } else {
            self.ticking_wait_adjacent.remove(&index)
        }
    }

    /// Java `mergeTickingBlocks`.
    pub fn merge_ticking_blocks(&mut self) {
        self.ticking_blocks
            .extend(self.ticking_wait_adjacent.iter().copied());
        self.ticking_wait_adjacent.clear();
    }

    /// Combined ticking cardinality.
    pub fn ticking_blocks_count(&self) -> usize {
        self.ticking_blocks
            .union(&self.ticking_wait_adjacent)
            .count()
    }

    /// Local light.
    pub const fn local_light(&self) -> &LightData {
        &self.local_light
    }

    /// Global light.
    pub const fn global_light(&self) -> &LightData {
        &self.global_light
    }

    /// Replace local light.
    pub fn set_local_light(&mut self, light: LightData) {
        self.local_light = light;
    }

    /// Replace global light.
    pub fn set_global_light(&mut self, light: LightData) {
        self.global_light = light;
    }

    /// Java `hasLocalLight()`.
    pub fn has_local_light(&self) -> bool {
        self.local_light.change_id() == self.local_change_counter
    }

    /// Java `hasGlobalLight()`.
    pub fn has_global_light(&self) -> bool {
        self.global_light.change_id() == self.global_change_counter
    }

    /// Java `invalidateLocalLight()`.
    pub fn invalidate_local_light(&mut self) {
        self.local_change_counter = self.local_change_counter.wrapping_add(1);
        self.invalidate_global_light();
    }

    /// Java `invalidateGlobalLight()`.
    pub fn invalidate_global_light(&mut self) {
        self.global_change_counter = self.global_change_counter.wrapping_add(1);
    }

    /// Local light change counter.
    pub const fn local_change_counter(&self) -> i16 {
        self.local_change_counter
    }

    /// Global light change counter.
    pub const fn global_change_counter(&self) -> i16 {
        self.global_change_counter
    }

    /// Sky-lit heightmap stamp.
    pub const fn sky_lit_stamp(&self) -> i64 {
        self.sky_lit_stamp
    }

    /// Set the sky-lit heightmap stamp.
    pub fn set_sky_lit_stamp(&mut self, stamp: i64) {
        self.sky_lit_stamp = stamp;
    }

    /// Java `serializeForPacket()` — block, filler and rotation palettes only.
    pub fn serialize_for_packet(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.write_u8(self.blocks.ordinal());
        self.blocks.serialize_for_packet(&mut writer);
        writer.write_u8(self.fillers.ordinal());
        self.fillers.serialize_for_packet(&mut writer);
        writer.write_u8(self.rotations.ordinal());
        self.rotations.serialize_for_packet(&mut writer);
        writer.into_inner()
    }

    /// Packet payload plus the current local/global light blobs.
    pub fn to_packet(&self) -> Result<BlockSectionPacket> {
        let local_light = if self.has_local_light() && !self.local_light.is_empty() {
            let mut writer = Writer::new();
            self.local_light.serialize_for_packet(&mut writer)?;
            Some(writer.into_inner())
        } else {
            None
        };
        let global_light = if self.has_global_light() && !self.global_light.is_empty() {
            let mut writer = Writer::new();
            self.global_light.serialize_for_packet(&mut writer)?;
            Some(writer.into_inner())
        } else {
            None
        };
        Ok(BlockSectionPacket {
            data: self.serialize_for_packet(),
            local_light,
            global_light,
        })
    }

    /// Java `serialize`.
    pub fn serialize(&self) -> Result<Vec<u8>> {
        self.serialize_with_keys(&ShortKey, &ShortKey, &ByteKey)
    }

    /// Java `serialize` with explicit storage key codecs.
    ///
    /// Java resolves blocks, fluids and environments to asset names through
    /// their registries; fillers and rotations stay numeric. The Java server
    /// path uses [`crate::chunk::palette::NamedKey`] for blocks and
    /// [`ShortKey`]/[`ByteKey`] for the other two palettes.
    pub fn serialize_with_keys(
        &self,
        blocks: &dyn KeyCodec,
        fillers: &dyn KeyCodec,
        rotations: &dyn KeyCodec,
    ) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.write_i32_be(self.block_migration_asset_count);
        writer.write_u8(self.blocks.ordinal());
        self.blocks.serialize(blocks, &mut writer)?;

        if self.blocks.kind() != PaletteType::Empty {
            let (count, words) = self.combined_ticking_words();
            writer.write_u16_be(count);
            writer.write_u16_be(
                u16::try_from(words.len())
                    .map_err(|_| SectionError::InvalidData("ticking word count exceeds u16"))?,
            );
            for word in words {
                writer.write_u64_be(word);
            }
        }

        writer.write_u8(self.fillers.ordinal());
        self.fillers.serialize(fillers, &mut writer)?;
        writer.write_u8(self.rotations.ordinal());
        self.rotations.serialize(rotations, &mut writer)?;
        self.local_light.serialize(&mut writer)?;
        self.global_light.serialize(&mut writer)?;
        writer.write_i16_be(self.local_change_counter);
        writer.write_i16_be(self.global_change_counter);
        writer.write_i64_be(self.sky_lit_stamp);
        Ok(writer.into_inner())
    }

    /// Decode `serialize_for_packet` bytes (block/filler/rotation only).
    pub fn deserialize_for_packet(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let block_type_byte = reader.read_u8()?;
        let block_type = PaletteType::from_ordinal(block_type_byte)
            .ok_or(SectionError::InvalidPaletteType(block_type_byte))?;
        let blocks = Palette::deserialize_for_packet(block_type, &mut reader)?;
        let filler_type_byte = reader.read_u8()?;
        let filler_type = PaletteType::from_ordinal(filler_type_byte)
            .ok_or(SectionError::InvalidPaletteType(filler_type_byte))?;
        let fillers = Palette::deserialize_for_packet(filler_type, &mut reader)?;
        let rotation_type_byte = reader.read_u8()?;
        let rotation_type = PaletteType::from_ordinal(rotation_type_byte)
            .ok_or(SectionError::InvalidPaletteType(rotation_type_byte))?;
        let rotations = Palette::deserialize_for_packet(rotation_type, &mut reader)?;
        if !reader.is_empty() {
            return Err(SectionError::InvalidData("trailing block packet bytes"));
        }
        Ok(Self {
            block_migration_asset_count: 0,
            blocks,
            fillers,
            rotations,
            ticking_blocks: BTreeSet::new(),
            ticking_wait_adjacent: BTreeSet::new(),
            local_light: LightData::empty(0),
            local_change_counter: 0,
            global_light: LightData::empty(0),
            global_change_counter: 0,
            sky_lit_stamp: 0,
        })
    }

    /// Java `deserialize` (storage version 6/7).
    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        Self::deserialize_with_keys(bytes, &ShortKey, &ShortKey, &ByteKey)
    }

    /// Java `deserialize` with explicit storage key codecs.
    pub fn deserialize_with_keys(
        bytes: &[u8],
        blocks: &dyn KeyCodec,
        fillers: &dyn KeyCodec,
        rotations: &dyn KeyCodec,
    ) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let block_migration_asset_count = reader.read_i32_be()?;
        let block_type_byte = reader.read_u8()?;
        let block_type = PaletteType::from_ordinal(block_type_byte)
            .ok_or(SectionError::InvalidPaletteType(block_type_byte))?;
        let blocks = Palette::deserialize(block_type, blocks, &mut reader)?;

        let mut ticking_blocks = BTreeSet::new();
        if block_type != PaletteType::Empty {
            let _count = reader.read_u16_be()?;
            let word_count = reader.read_u16_be()? as usize;
            if word_count > BLOCK_COUNT.div_ceil(64) {
                return Err(SectionError::InvalidData("ticking word count too large"));
            }
            for word_index in 0..word_count {
                let word = reader.read_u64_be()?;
                for bit in 0..64 {
                    if word & (1 << bit) != 0 {
                        let index = word_index * 64 + bit;
                        if index >= BLOCK_COUNT {
                            return Err(SectionError::InvalidData("ticking bit out of range"));
                        }
                        ticking_blocks.insert(index as u16);
                    }
                }
            }
        }

        let filler_type_byte = reader.read_u8()?;
        let filler_type = PaletteType::from_ordinal(filler_type_byte)
            .ok_or(SectionError::InvalidPaletteType(filler_type_byte))?;
        let fillers = Palette::deserialize(filler_type, fillers, &mut reader)?;
        let rotation_type_byte = reader.read_u8()?;
        let rotation_type = PaletteType::from_ordinal(rotation_type_byte)
            .ok_or(SectionError::InvalidPaletteType(rotation_type_byte))?;
        let rotations = Palette::deserialize(rotation_type, rotations, &mut reader)?;
        let local_light = LightData::deserialize(&mut reader)?;
        let global_light = LightData::deserialize(&mut reader)?;
        let local_change_counter = reader.read_i16_be()?;
        let global_change_counter = reader.read_i16_be()?;
        let sky_lit_stamp = reader.read_i64_be()?;

        Ok(Self {
            block_migration_asset_count,
            blocks,
            fillers,
            rotations,
            ticking_blocks,
            ticking_wait_adjacent: BTreeSet::new(),
            local_light,
            local_change_counter,
            global_light,
            global_change_counter,
            sky_lit_stamp,
        })
    }

    fn combined_ticking_words(&self) -> (u16, Vec<u64>) {
        let combined: BTreeSet<u16> = self
            .ticking_blocks
            .union(&self.ticking_wait_adjacent)
            .copied()
            .collect();
        if combined.is_empty() {
            return (0, Vec::new());
        }
        let word_count = (usize::from(*combined.iter().next_back().unwrap()) / 64) + 1;
        let mut words = vec![0u64; word_count];
        for index in combined {
            words[usize::from(index) / 64] |= 1 << (usize::from(index) % 64);
        }
        (self.ticking_blocks_count() as u16, words)
    }
}

fn set_palette(palette: &mut Palette, index: usize, value: i32) -> Result<bool> {
    let mut result = palette.set(index, value);
    if result == SetResult::RequiresPromote {
        palette.promote()?;
        result = palette.set(index, value);
        if result == SetResult::RequiresPromote {
            return Err(SectionError::InvalidData(
                "promoted palette still rejected value",
            ));
        }
    } else if palette.should_demote() {
        palette.demote()?;
    }
    Ok(result != SetResult::Unchanged)
}

#[inline]
fn index_block(x: i32, y: i32, z: i32) -> usize {
    hytale_math::chunk_util::index_block(x, y, z) as usize
}
