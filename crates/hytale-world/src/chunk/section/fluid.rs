//! Fluid section: fluid-id palette plus four-bit levels.
//!
//! Ported from `CoreServer/.../world/chunk/section/FluidSection.java`. Both the
//! storage and packet layouts are `[palette type][palette][has levels][levels]`;
//! only the palette table endianness differs.

use hytale_protocol::io::{Reader, Writer};

use crate::chunk::palette::{ByteKey, KeyCodec, Palette, PaletteType, SetResult, BLOCK_COUNT};

use super::{Result, SectionError};

/// Number of bytes in the packed level array (`SIZE_BLOCKS >> 1`).
pub const LEVEL_DATA_SIZE: usize = BLOCK_COUNT / 2;

/// One fluid section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FluidSection {
    fluids: Palette,
    level_data: Option<Vec<u8>>,
    non_zero_levels: usize,
}

impl Default for FluidSection {
    fn default() -> Self {
        Self::new()
    }
}

impl FluidSection {
    /// Java `FluidSection()`.
    pub fn new() -> Self {
        Self {
            fluids: Palette::empty(),
            level_data: None,
            non_zero_levels: 0,
        }
    }

    /// Fluid palette.
    pub const fn palette(&self) -> &Palette {
        &self.fluids
    }

    /// True when every block is empty and every level is zero.
    pub fn is_empty(&self) -> bool {
        self.fluids.is_solid(0) && self.non_zero_levels == 0
    }

    /// Java `getFluidId(int index)`.
    pub fn get_fluid_id(&self, index: usize) -> i32 {
        if index >= BLOCK_COUNT {
            return 0;
        }
        self.fluids.get(index)
    }

    /// Java `getFluidLevel(int index)`.
    pub fn get_fluid_level(&self, index: usize) -> i8 {
        if index >= BLOCK_COUNT {
            return 0;
        }
        self.level_data
            .as_ref()
            .map(|data| {
                let byte = data[index >> 1];
                if index & 1 == 0 {
                    (byte & 0x0F) as i8
                } else {
                    ((byte >> 4) & 0x0F) as i8
                }
            })
            .unwrap_or(0)
    }

    /// Java `setFluid(int index, int fluidId, byte level)`.
    pub fn set_fluid(&mut self, index: usize, fluid_id: i32, level: i8) -> Result<bool> {
        if index >= BLOCK_COUNT {
            return Err(SectionError::InvalidData("fluid index out of range"));
        }
        let mut fluid_id = fluid_id;
        let mut level = level & 0x0F;
        if level == 0 {
            fluid_id = 0;
        }
        if fluid_id == 0 {
            level = 0;
        }
        let mut changed = set_palette(&mut self.fluids, index, fluid_id)?;
        changed |= self.set_level(index, level);
        Ok(changed)
    }

    /// Java `setFluid(int x, int y, int z, int fluidId, byte level)`.
    pub fn set_fluid_at(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        level: i8,
    ) -> Result<bool> {
        self.set_fluid(index_block(x, y, z), fluid_id, level)
    }

    /// Java `getFluidId(int x, int y, int z)`.
    pub fn get_fluid_id_at(&self, x: i32, y: i32, z: i32) -> i32 {
        self.get_fluid_id(index_block(x, y, z))
    }

    /// Java `getFluidLevel(int x, int y, int z)`.
    pub fn get_fluid_level_at(&self, x: i32, y: i32, z: i32) -> i8 {
        self.get_fluid_level(index_block(x, y, z))
    }

    fn set_level(&mut self, index: usize, level: i8) -> bool {
        let level = level & 0x0F;
        if self.level_data.is_none() {
            if level == 0 {
                return false;
            }
            self.level_data = Some(vec![0; LEVEL_DATA_SIZE]);
        }
        let data = self.level_data.as_mut().expect("allocated above");
        let byte_index = index >> 1;
        let byte_value = data[byte_index];
        let current = if index & 1 == 0 {
            (byte_value & 0x0F) as i8
        } else {
            ((byte_value >> 4) & 0x0F) as i8
        };
        if current == level {
            return false;
        }
        if current == 0 {
            self.non_zero_levels += 1;
        } else if level == 0 {
            self.non_zero_levels -= 1;
            if self.non_zero_levels == 0 {
                self.level_data = None;
                return true;
            }
        }
        let data = self.level_data.as_mut().expect("still allocated");
        data[byte_index] = if index & 1 == 0 {
            (byte_value & 0xF0) | (level as u8)
        } else {
            (byte_value & 0x0F) | ((level as u8) << 4)
        };
        true
    }

    /// Java `serialize` (storage).
    pub fn serialize(&self) -> Result<Vec<u8>> {
        self.serialize_with_key(&ByteKey)
    }

    /// Java `serialize` with an explicit storage key codec.
    pub fn serialize_with_key(&self, key: &dyn KeyCodec) -> Result<Vec<u8>> {
        let mut writer = Writer::new();
        writer.write_u8(self.fluids.ordinal());
        self.fluids.serialize(key, &mut writer)?;
        writer.write_bool(self.level_data.is_some());
        if let Some(data) = &self.level_data {
            writer.write_slice(data);
        }
        Ok(writer.into_inner())
    }

    /// Java `serializeForPacket`.
    pub fn serialize_for_packet(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.write_u8(self.fluids.ordinal());
        self.fluids.serialize_for_packet(&mut writer);
        writer.write_bool(self.level_data.is_some());
        if let Some(data) = &self.level_data {
            writer.write_slice(data);
        }
        writer.into_inner()
    }

    /// Decode the storage layout.
    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        Self::deserialize_with_key(bytes, &ByteKey)
    }

    /// Decode the storage layout with an explicit storage key codec.
    pub fn deserialize_with_key(bytes: &[u8], key: &dyn KeyCodec) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let type_byte = reader.read_u8()?;
        let palette_type = PaletteType::from_ordinal(type_byte)
            .ok_or(SectionError::InvalidPaletteType(type_byte))?;
        let fluids = Palette::deserialize(palette_type, key, &mut reader)?;
        let level_data = if reader.read_bool()? {
            Some(reader.read_slice(LEVEL_DATA_SIZE)?.to_vec())
        } else {
            None
        };
        if !reader.is_empty() {
            return Err(SectionError::InvalidData("trailing fluid section bytes"));
        }
        let non_zero_levels = level_data.as_ref().map_or(0, |data| {
            data.iter()
                .map(|value| (value & 0x0F != 0) as usize + (value & 0xF0 != 0) as usize)
                .sum()
        });
        Ok(Self {
            fluids,
            level_data,
            non_zero_levels,
        })
    }

    /// Decode the packet layout.
    pub fn deserialize_for_packet(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);
        let type_byte = reader.read_u8()?;
        let palette_type = PaletteType::from_ordinal(type_byte)
            .ok_or(SectionError::InvalidPaletteType(type_byte))?;
        let fluids = Palette::deserialize_for_packet(palette_type, &mut reader)?;
        let level_data = if reader.read_bool()? {
            Some(reader.read_slice(LEVEL_DATA_SIZE)?.to_vec())
        } else {
            None
        };
        let non_zero_levels = level_data.as_ref().map_or(0, |data| {
            data.iter()
                .map(|value| (value & 0x0F != 0) as usize + (value & 0xF0 != 0) as usize)
                .sum()
        });
        Ok(Self {
            fluids,
            level_data,
            non_zero_levels,
        })
    }
}

fn set_palette(palette: &mut Palette, index: usize, value: i32) -> Result<bool> {
    let mut result = palette.set(index, value);
    if result == SetResult::RequiresPromote {
        palette.promote()?;
        result = palette.set(index, value);
        if result == SetResult::RequiresPromote {
            return Err(SectionError::InvalidData(
                "promoted fluid palette rejected value",
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
