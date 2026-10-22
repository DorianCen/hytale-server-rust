//! Bridge from the P0 asset registry to the world generator's registry seam.
//!
//! `worldgen-model` intentionally knows nothing about asset packs; it consumes
//! name → id tables and the handful of `BlockType` fields the generator reads
//! (draw type, opacity, support). This module performs that one-way conversion
//! from [`hytale_assets::AssetRegistry`].
//!
//! Numeric ids come from the asset registry snapshot. They are not the Java
//! dump's ids for contained block types (see the `hytale-assets::registry`
//! module docs), but every consumer in the server uses this same snapshot, so
//! generation, packets and storage stay consistent.

use hytale_assets::AssetRegistry;
use hytale_protocol::generated::common::{
    DrawType as ProtocolDrawType, Opacity as ProtocolOpacity,
};
use thiserror::Error;
use worldgen_model::registry::{
    BlockRegistry, BlockSupport, BlockTypeInfo, DrawType, EnvironmentRegistry, FluidRegistry,
    Opacity, Registries, DEFAULT_MAX_FLUID_LEVEL, SUPPORT_ROTATION_COUNT,
};

/// Errors produced by the asset → worldgen registry conversion.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// The asset registry has no block, fluid or environment assets.
    #[error("the asset registry has no {store} assets")]
    EmptyStore {
        /// Store name.
        store: &'static str,
    },
    /// An indexed asset is missing its numeric id.
    #[error("asset `{key}` in {store} has no numeric id")]
    MissingId {
        /// Store name.
        store: &'static str,
        /// Asset key.
        key: String,
    },
}

/// Convert the P0 asset registry into the generator's [`Registries`].
pub fn registries_from_assets(assets: &AssetRegistry) -> Result<Registries, RegistryError> {
    let blocks = block_registry(assets)?;
    let fluids = fluid_registry(assets)?;
    let environments = environment_registry(assets)?;
    Ok(Registries::with_environments(blocks, fluids, environments))
}

fn block_registry(assets: &AssetRegistry) -> Result<BlockRegistry, RegistryError> {
    if assets.block_types.is_empty() {
        return Err(RegistryError::EmptyStore { store: "BlockType" });
    }

    let mut entries: Vec<(usize, &str, BlockTypeInfo)> =
        Vec::with_capacity(assets.block_types.len());
    let mut highest = 0usize;
    for record in assets.block_types.iter() {
        let id = record.id.ok_or_else(|| RegistryError::MissingId {
            store: "BlockType",
            key: record.key.clone(),
        })?;
        let index = id as usize;
        highest = highest.max(index);

        let draw_type = draw_type(record.value.draw_type);
        let opacity = opacity(record.value.opacity);
        let info = match &record.value.support {
            Some(support) => {
                let mut rotations = Vec::with_capacity(SUPPORT_ROTATION_COUNT);
                for rotation in 0..SUPPORT_ROTATION_COUNT as u8 {
                    let support = support.at(rotation);
                    rotations.push(BlockSupport {
                        down: support.down,
                        up: support.up,
                    });
                }
                BlockTypeInfo::new(draw_type)
                    .with_opacity(opacity)
                    .with_support_rotations(rotations)
            }
            None => BlockTypeInfo::new(draw_type).with_opacity(opacity),
        };
        entries.push((index, record.key.as_str(), info));
    }

    let mut blocks = vec![None; highest + 1];
    let mut names = vec![None; highest + 1];
    for (index, key, info) in entries {
        blocks[index] = Some(info);
        names[index] = Some(key.to_string());
    }
    Ok(BlockRegistry::with_names(blocks, names))
}

fn fluid_registry(assets: &AssetRegistry) -> Result<FluidRegistry, RegistryError> {
    if assets.fluids.is_empty() {
        return Err(RegistryError::EmptyStore { store: "Fluid" });
    }

    let mut entries = Vec::with_capacity(assets.fluids.len());
    let mut highest = 0usize;
    for record in assets.fluids.iter() {
        let id = record.id.ok_or_else(|| RegistryError::MissingId {
            store: "Fluid",
            key: record.key.clone(),
        })?;
        let index = id as usize;
        highest = highest.max(index);
        entries.push((index, record.key.as_str(), record.value.max_fluid_level));
    }

    let mut levels = vec![DEFAULT_MAX_FLUID_LEVEL; highest + 1];
    let mut names = vec![None; highest + 1];
    for (index, key, level) in entries {
        levels[index] = level;
        names[index] = Some(key.to_string());
    }
    Ok(FluidRegistry::with_names(levels, names))
}

fn environment_registry(assets: &AssetRegistry) -> Result<EnvironmentRegistry, RegistryError> {
    if assets.environments.is_empty() {
        return Err(RegistryError::EmptyStore {
            store: "Environment",
        });
    }

    let mut entries = Vec::with_capacity(assets.environments.len());
    let mut highest = 0usize;
    for record in assets.environments.iter() {
        let id = record.id.ok_or_else(|| RegistryError::MissingId {
            store: "Environment",
            key: record.key.clone(),
        })?;
        let index = id as usize;
        highest = highest.max(index);
        entries.push((index, record.key.as_str()));
    }

    let mut names = vec![String::new(); highest + 1];
    for (index, key) in entries {
        names[index] = key.to_string();
    }
    Ok(EnvironmentRegistry::new(names))
}

fn draw_type(draw_type: ProtocolDrawType) -> DrawType {
    match draw_type {
        ProtocolDrawType::Empty => DrawType::Empty,
        ProtocolDrawType::GizmoCube => DrawType::GizmoCube,
        ProtocolDrawType::Cube => DrawType::Cube,
        ProtocolDrawType::Model => DrawType::Model,
        ProtocolDrawType::CubeWithModel => DrawType::CubeWithModel,
    }
}

fn opacity(opacity: ProtocolOpacity) -> Opacity {
    match opacity {
        ProtocolOpacity::Solid => Opacity::Solid,
        ProtocolOpacity::Semitransparent => Opacity::Semitransparent,
        ProtocolOpacity::Cutout => Opacity::Cutout,
        ProtocolOpacity::Transparent => Opacity::Transparent,
    }
}
