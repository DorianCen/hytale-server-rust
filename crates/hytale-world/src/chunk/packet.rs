//! Network packet builders for a runtime chunk column.
//!
//! Ported from the CoreServer chunk/section packet builders:
//!
//! | Packet | Java builder |
//! |---|---|
//! | `SetColumn` | `BlockChunk.getCachedColumnPacket()` |
//! | `SetChunkTintmap` | `BlockChunk.getCachedTintsPacket()` |
//! | `SetChunk` | `BlockSection.getCachedChunkPacket(x, y, z)` |
//! | `SetChunkEnvironments` | `EnvironmentSection.getCachedPacket(x, y, z)` |
//! | `SetFluids` | `FluidSection.getCachedPacket()` |
//!
//! The bytes are produced by [`hytale_protocol::generated::wire::Wire`]; this
//! module only fills the same fields the Java builders fill. No packet bytes
//! are hand-encoded.

use hytale_protocol::generated::world::{
    SetChunk, SetChunkEnvironments, SetChunkTintmap, SetColumn, SetFluids,
};
use hytale_protocol::io::Writer;

use super::column::ChunkColumn;
use super::section::{EnvironmentSection, Result, SectionError};

impl ChunkColumn {
    /// Java `BlockChunk.getCachedColumnPacket()` — heightmap ints plus tint.
    pub fn set_column_packet(&self) -> Result<SetColumn> {
        let heightmap = self
            .heightmap()
            .serialize_to_ints()
            .ok_or(SectionError::InvalidData(
                "SetColumn requires the legacy heightmap backend",
            ))?;
        Ok(SetColumn {
            x: self.x(),
            z: self.z(),
            heightmap,
            tintmap: to_i8_vec(self.tint_bytes()),
        })
    }

    /// Java `BlockChunk.getCachedTintsPacket()`.
    pub fn tintmap_packet(&self) -> SetChunkTintmap {
        SetChunkTintmap {
            x: self.x(),
            z: self.z(),
            tintmap: to_i8_vec(self.tint_bytes()),
        }
    }

    /// Java `BlockSection.getCachedChunkPacket(x, y, z)`.
    ///
    /// An absent section is the Java `BlockSection()` installed by
    /// `ChunkSystems.EnsureSection`: all air (`data = null`), empty local light
    /// and no global light. `BlockChunk.SEND_GLOBAL_LIGHTING_DATA` is `false`
    /// in CoreServer, so global light is never part of the initial packet.
    pub fn set_chunk_packet(&self, section_y: i32) -> Result<SetChunk> {
        let block_y = section_y * hytale_math::chunk_util::SIZE;
        let (data, local_light) = match self.section(block_y) {
            Some(section) => {
                let data = (!section.block.is_solid_air())
                    .then(|| to_i8_vec(section.block.serialize_for_packet()));
                let local_light = if section.block.has_local_light() {
                    let mut writer = Writer::new();
                    section
                        .block
                        .local_light()
                        .serialize_for_packet(&mut writer)?;
                    Some(to_i8_vec(writer.into_inner()))
                } else {
                    None
                };
                (data, local_light)
            }
            None => (None, Some(vec![0])),
        };
        Ok(SetChunk {
            x: self.x(),
            y: section_y,
            z: self.z(),
            local_light,
            global_light: None,
            data,
        })
    }

    /// Java `EnvironmentSection.getCachedPacket(x, y, z)`.
    pub fn set_chunk_environments_packet(&self, section_y: i32) -> Result<SetChunkEnvironments> {
        let block_y = section_y * hytale_math::chunk_util::SIZE;
        let environments = match self.section(block_y) {
            Some(section) => section.environment.serialize_for_packet()?,
            None => EnvironmentSection::new().serialize_for_packet()?,
        };
        Ok(SetChunkEnvironments {
            x: self.x(),
            y: section_y,
            z: self.z(),
            environments: to_i8_vec(environments),
        })
    }

    /// Java `FluidSection.getCachedPacket()` — `None` for empty sections, which
    /// are never sent (CoreServer only replicates non-empty fluid sections).
    pub fn set_fluids_packet(&self, section_y: i32) -> Option<SetFluids> {
        let block_y = section_y * hytale_math::chunk_util::SIZE;
        let section = self.section(block_y)?;
        if section.fluid.is_empty() {
            return None;
        }
        Some(SetFluids {
            x: self.x(),
            y: section_y,
            z: self.z(),
            data: Some(to_i8_vec(section.fluid.serialize_for_packet())),
        })
    }
}

fn to_i8_vec(bytes: Vec<u8>) -> Vec<i8> {
    bytes.into_iter().map(|byte| byte as i8).collect()
}
