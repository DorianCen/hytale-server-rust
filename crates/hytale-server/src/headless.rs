//! Headless world boot: load assets, generate one chunk, convert it to the
//! runtime column and save a Java-compatible region blob.
//!
//! This is the first vertical slice: it exercises the same components the live
//! server will compose (P0 asset registry → worldgen registries →
//! `worldgen-server` worker → runtime `ChunkColumn` → BSON codec →
//! `IndexedStorageFile`), with no network or ECS layers yet.

use std::path::{Path, PathBuf};

use hytale_assets::{AssetError, AssetPack, AssetRegistry};
use hytale_world::chunk::column::{ChunkColumn, SECTION_COUNT};
use hytale_world::chunk::storage::{
    chunk_blob_index, region_path, ChunkCodec, ChunkStorageError, RegionFile,
};
use hytale_world::chunk::SectionError;
use hytale_world::registry::{registries_from_assets, RegistryError};
use thiserror::Error;
use worldgen_hytale::world_gen::{WorldGenError, WorldGenOptions};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_server::{ChunkProvider, ChunkProviderError, ChunkRequest, WorldGenChunkProvider};

/// Paths and coordinates of a headless boot.
#[derive(Debug, Clone)]
pub struct HeadlessOptions {
    /// `Assets/Assets.zip` (or an unpacked directory) used for the registry.
    pub assets_pack: PathBuf,
    /// The unpacked asset tree used for prefabs, environments and tint
    /// gradients (normally `Assets/extracted`).
    pub assets_root: PathBuf,
    /// The world-gen data folder (normally `testdata/Default`).
    pub world_dir: PathBuf,
    /// World save root; the region file is written below `chunks/`.
    pub save_dir: PathBuf,
    /// World seed.
    pub seed: i32,
    /// Chunk x coordinate.
    pub chunk_x: i32,
    /// Chunk z coordinate.
    pub chunk_z: i32,
    /// Worldgen worker count.
    pub workers: usize,
}

impl HeadlessOptions {
    /// Options for the repository layout and the first seed-0 chunk.
    pub fn for_repo(repo_root: impl AsRef<Path>, save_dir: impl Into<PathBuf>) -> Self {
        let repo_root = repo_root.as_ref();
        Self {
            assets_pack: repo_root.join("Assets/Assets.zip"),
            assets_root: repo_root.join("Assets/extracted"),
            world_dir: repo_root.join("testdata/Default"),
            save_dir: save_dir.into(),
            seed: 0,
            chunk_x: 0,
            chunk_z: 0,
            workers: 1,
        }
    }
}

/// What a headless boot produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessReport {
    /// World seed.
    pub seed: i32,
    /// Chunk x coordinate.
    pub chunk_x: i32,
    /// Chunk z coordinate.
    pub chunk_z: i32,
    /// Number of P0 block entries in the asset registry.
    pub asset_blocks: usize,
    /// Number of non-air blocks in the generated chunk.
    pub non_air_blocks: usize,
    /// Region file that was written.
    pub region_file: PathBuf,
    /// Blob index inside the region file.
    pub blob_index: u32,
    /// Uncompressed BSON document size.
    pub document_bytes: usize,
    /// True when the save was reopened and decoded back to the same values.
    pub verified: bool,
}

/// Errors from the headless boot.
#[derive(Debug, Error)]
pub enum HeadlessError {
    /// The asset pack could not be loaded.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The asset registry could not be bridged into the worldgen registries.
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// The generation service failed to load or generate.
    #[error(transparent)]
    Generation(#[from] ChunkProviderError),
    /// The worldgen service failed to start.
    #[error(transparent)]
    WorldGen(#[from] WorldGenError),
    /// The generated chunk could not be converted.
    #[error(transparent)]
    Section(#[from] SectionError),
    /// The chunk could not be encoded or decoded.
    #[error(transparent)]
    Storage(#[from] ChunkStorageError),
    /// A required path is missing.
    #[error("required path is missing: {path} ({what})")]
    MissingPath {
        /// Path that was checked.
        path: PathBuf,
        /// What the path should contain.
        what: &'static str,
    },
    /// The saved chunk did not decode back to the generated values.
    #[error("saved chunk verification failed: {0}")]
    Verification(String),
}

/// Run one headless boot and save one region blob.
pub fn run(options: &HeadlessOptions) -> Result<HeadlessReport, HeadlessError> {
    require_file(&options.assets_pack, "asset pack")?;
    require_dir(&options.assets_root, "unpacked asset tree")?;
    require_file(&options.world_dir.join("World.json"), "world configuration")?;
    if options.workers == 0 {
        return Err(HeadlessError::Verification(
            "the worker count must be positive".to_string(),
        ));
    }

    let pack = AssetPack::open(&options.assets_pack)?;
    let assets = AssetRegistry::load_p0(&pack)?;
    let registries = std::sync::Arc::new(registries_from_assets(&assets)?);

    let config = WorldGenConfig::new(
        options.world_dir.clone(),
        "Default".to_string(),
        "0.0.0".to_string(),
    );
    let provider = WorldGenChunkProvider::new(
        config,
        WorldGenOptions {
            workers: options.workers,
            seed_label: "ChunkGenerator".to_string(),
            assets_root: Some(options.assets_root.clone()),
            registries_path: None,
            registries: Some(std::sync::Arc::clone(&registries)),
        },
    )?;

    let result = provider.generate(ChunkRequest::new(
        options.seed,
        options.chunk_x,
        options.chunk_z,
    ))?;
    provider.shutdown();

    let column = ChunkColumn::from_generated_chunk_with_registries(&result.chunk, &registries)?;
    let codec = ChunkCodec::new(&assets);
    let document = codec.encode(&column)?;

    let region_file = region_path(&options.save_dir, options.chunk_x, options.chunk_z);
    let mut region =
        RegionFile::open_for_chunk(&options.save_dir, options.chunk_x, options.chunk_z)?;
    region.write_chunk(options.chunk_x, options.chunk_z, &document)?;
    region.flush()?;
    drop(region);

    let mut stored_region = RegionFile::open_read_only(&region_file)?;
    let stored = stored_region
        .read_chunk(options.chunk_x, options.chunk_z)?
        .ok_or_else(|| {
            HeadlessError::Verification(format!(
                "chunk ({}, {}) is missing from {}",
                options.chunk_x,
                options.chunk_z,
                region_file.display()
            ))
        })?;
    if stored != document {
        return Err(HeadlessError::Verification(
            "the stored blob differs from the encoded document".to_string(),
        ));
    }
    let decoded = codec.decode_at(&stored, options.chunk_x, options.chunk_z)?;
    verify_column(&column, &decoded)?;

    Ok(HeadlessReport {
        seed: options.seed,
        chunk_x: options.chunk_x,
        chunk_z: options.chunk_z,
        asset_blocks: assets.block_types.len(),
        non_air_blocks: count_non_air(&column),
        region_file,
        blob_index: chunk_blob_index(options.chunk_x, options.chunk_z),
        document_bytes: document.len(),
        verified: true,
    })
}

fn require_file(path: &Path, what: &'static str) -> Result<(), HeadlessError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(HeadlessError::MissingPath {
            path: path.to_path_buf(),
            what,
        })
    }
}

fn require_dir(path: &Path, what: &'static str) -> Result<(), HeadlessError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(HeadlessError::MissingPath {
            path: path.to_path_buf(),
            what,
        })
    }
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

fn verify_column(expected: &ChunkColumn, decoded: &ChunkColumn) -> Result<(), HeadlessError> {
    for y in 0..SECTION_COUNT as i32 * 32 {
        for z in 0..32 {
            for x in 0..32 {
                if expected.get_block(x, y, z) != decoded.get_block(x, y, z) {
                    return Err(HeadlessError::Verification(format!(
                        "block ({x}, {y}, {z}) differs after reload"
                    )));
                }
                if expected.get_environment(x, y, z) != decoded.get_environment(x, y, z) {
                    return Err(HeadlessError::Verification(format!(
                        "environment ({x}, {y}, {z}) differs after reload"
                    )));
                }
                if expected.get_fluid_id(x, y, z) != decoded.get_fluid_id(x, y, z)
                    || expected.get_fluid_level(x, y, z) != decoded.get_fluid_level(x, y, z)
                {
                    return Err(HeadlessError::Verification(format!(
                        "fluid ({x}, {y}, {z}) differs after reload"
                    )));
                }
            }
        }
    }
    for z in 0..32 {
        for x in 0..32 {
            if expected.tint(x, z) != decoded.tint(x, z) {
                return Err(HeadlessError::Verification(format!(
                    "tint ({x}, {z}) differs after reload"
                )));
            }
        }
    }
    if expected.heightmap() != decoded.heightmap() {
        return Err(HeadlessError::Verification(
            "the heightmap differs after reload".to_string(),
        ));
    }
    if expected.needs_physics() != decoded.needs_physics() {
        return Err(HeadlessError::Verification(
            "needsPhysics differs after reload".to_string(),
        ));
    }
    Ok(())
}
