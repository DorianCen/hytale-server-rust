//! Load a saved chunk from a region file, optionally mutate one block, save it
//! back and verify the reload.
//!
//! This is the runtime counterpart of the `headless` generation boot: it uses
//! the same asset registry and BSON codec, but starts from disk.

use std::path::PathBuf;

use hytale_assets::{AssetError, AssetPack, AssetRegistry};
use hytale_world::chunk::column::{ChunkColumn, SECTION_COUNT};
use hytale_world::chunk::storage::{region_path, ChunkCodec};
use hytale_world::world::{World, WorldError, WorldStorage};
use thiserror::Error;

/// Paths and the optional block edit for a stored-chunk check.
#[derive(Debug, Clone)]
pub struct ChunkToolOptions {
    /// `Assets/Assets.zip` (or an unpacked directory) used for the registry.
    pub assets_pack: PathBuf,
    /// World save root containing `chunks/`.
    pub save_dir: PathBuf,
    /// Chunk x coordinate.
    pub chunk_x: i32,
    /// Chunk z coordinate.
    pub chunk_z: i32,
    /// Optional block edit as `(x, y, z, block_id)` in world coordinates.
    pub set_block: Option<(i32, i32, i32, i32)>,
}

/// What the stored-chunk check produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkToolReport {
    /// Chunk x coordinate.
    pub chunk_x: i32,
    /// Chunk z coordinate.
    pub chunk_z: i32,
    /// Non-air blocks after the optional edit.
    pub non_air_blocks: usize,
    /// Block before the edit (`None` without `set_block`).
    pub block_before: Option<i32>,
    /// Block after the edit (`None` without `set_block`).
    pub block_after: Option<i32>,
    /// Region file that holds the chunk.
    pub region_file: PathBuf,
    /// True when the reloaded file matches the edited column.
    pub verified: bool,
}

/// Errors from the stored-chunk check.
#[derive(Debug, Error)]
pub enum ChunkToolError {
    /// The asset pack or registry failed.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The world storage failed.
    #[error(transparent)]
    World(#[from] WorldError),
    /// The requested chunk is not stored.
    #[error("chunk ({x}, {z}) is not stored below {save_dir}")]
    MissingChunk {
        /// Save directory that was scanned.
        save_dir: PathBuf,
        /// Chunk x coordinate.
        x: i32,
        /// Chunk z coordinate.
        z: i32,
    },
    /// The reloaded block differs from the edited column.
    #[error("reloaded block at ({x}, {y}, {z}) is {actual}, expected {expected}")]
    Verification {
        /// World x coordinate.
        x: i32,
        /// World y coordinate.
        y: i32,
        /// World z coordinate.
        z: i32,
        /// Value before the reload.
        expected: i32,
        /// Value read back.
        actual: i32,
    },
}

/// Load a stored chunk, optionally edit one block, save and reload it.
pub fn run(options: &ChunkToolOptions) -> Result<ChunkToolReport, ChunkToolError> {
    let pack = AssetPack::open(&options.assets_pack)?;
    let assets = AssetRegistry::load_p0(&pack)?;
    let codec = ChunkCodec::new(&assets);

    let mut world = World::new(WorldStorage::new(&options.save_dir));
    if !world.load_chunk(&codec, options.chunk_x, options.chunk_z)? {
        return Err(ChunkToolError::MissingChunk {
            save_dir: options.save_dir.clone(),
            x: options.chunk_x,
            z: options.chunk_z,
        });
    }

    let (block_before, block_after) = match options.set_block {
        Some((x, y, z, block_id)) => {
            let before = world.get_block(x, y, z);
            let changed = world.set_block(x, y, z, block_id)?;
            if changed {
                world.save_all(&codec)?;
            }
            (Some(before), Some(if changed { block_id } else { before }))
        }
        None => (None, None),
    };

    let column = world
        .chunk(options.chunk_x, options.chunk_z)
        .expect("loaded above");
    let non_air_blocks = count_non_air(column);

    let mut reloaded = World::new(WorldStorage::new(&options.save_dir));
    if !reloaded.load_chunk(&codec, options.chunk_x, options.chunk_z)? {
        return Err(ChunkToolError::MissingChunk {
            save_dir: options.save_dir.clone(),
            x: options.chunk_x,
            z: options.chunk_z,
        });
    }
    if let Some((x, y, z, _)) = options.set_block {
        let expected = block_after.unwrap_or(0);
        let actual = reloaded
            .chunk(options.chunk_x, options.chunk_z)
            .map(|column| column.get_block(x & 31, y, z & 31))
            .unwrap_or(0);
        if actual != expected {
            return Err(ChunkToolError::Verification {
                x,
                y,
                z,
                expected,
                actual,
            });
        }
    }

    Ok(ChunkToolReport {
        chunk_x: options.chunk_x,
        chunk_z: options.chunk_z,
        non_air_blocks,
        block_before,
        block_after,
        region_file: region_path(&options.save_dir, options.chunk_x, options.chunk_z),
        verified: true,
    })
}

fn count_non_air(column: &ChunkColumn) -> usize {
    let mut count = 0;
    for section in 0..SECTION_COUNT as i32 {
        for y in 0..32 {
            for z in 0..32 {
                for x in 0..32 {
                    if column.get_block(x, section * 32 + y, z) != 0 {
                        count += 1;
                    }
                }
            }
        }
    }
    count
}
