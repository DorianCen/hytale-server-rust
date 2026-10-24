//! Shared worldgen boot: asset pack → phase-2 registry → worldgen registries →
//! `worldgen-server` worker pool → runtime [`ChunkColumn`].
//!
//! `headless` and the saved-world boot both need the same generator setup; this
//! module owns it so they cannot drift.

use std::path::PathBuf;
use std::sync::Arc;

use hytale_assets::{AssetError, AssetPack, AssetRegistry};
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::chunk::section::SectionError;
use hytale_world::registry::{registries_from_assets, RegistryError};
use thiserror::Error;
use worldgen_hytale::world_gen::{WorldGenError, WorldGenOptions};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::registry::Registries;
use worldgen_server::{ChunkProvider, ChunkProviderError, ChunkRequest, WorldGenChunkProvider};

/// Generator name passed to the world-gen loader.
pub const GENERATOR_NAME: &str = "Default";
/// Generator version passed to the world-gen loader.
pub const GENERATOR_VERSION: &str = "0.0.0";
/// Vanilla provider label used by the chunk goldens.
pub const SEED_LABEL: &str = "ChunkGenerator";

/// Paths and worker settings for a generator boot.
#[derive(Debug, Clone)]
pub struct GeneratorOptions {
    /// `Assets/Assets.zip` (or an unpacked directory).
    pub assets_pack: PathBuf,
    /// Unpacked asset tree for prefabs, environments and tint gradients.
    pub assets_root: PathBuf,
    /// World-gen data folder (`World.json`).
    pub world_dir: PathBuf,
    /// Worldgen worker count.
    pub workers: usize,
}

/// Errors from the generator boot or one chunk conversion.
#[derive(Debug, Error)]
pub enum GenerationError {
    /// The asset pack could not be loaded.
    #[error(transparent)]
    Asset(#[from] AssetError),
    /// The asset registry could not be bridged into the worldgen registries.
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// The generation service failed to load or generate.
    #[error(transparent)]
    Chunk(#[from] ChunkProviderError),
    /// The worldgen service failed to start.
    #[error(transparent)]
    WorldGen(#[from] WorldGenError),
    /// The generated chunk could not be converted.
    #[error(transparent)]
    Section(#[from] SectionError),
    /// A required path is missing.
    #[error("required path is missing: {path} ({what})")]
    MissingPath {
        /// Path that was checked.
        path: PathBuf,
        /// What the path should contain.
        what: &'static str,
    },
    /// The worker count is zero.
    #[error("the worldgen worker count must be positive")]
    NoWorkers,
}

/// One asset registry + generator pair.
pub struct WorldGenerator {
    assets: AssetRegistry,
    registries: Arc<Registries>,
    provider: WorldGenChunkProvider,
}

impl WorldGenerator {
    /// Load the asset pack and start the generator workers.
    pub fn open(options: &GeneratorOptions) -> Result<Self, GenerationError> {
        require_file(&options.assets_pack, "asset pack")?;
        require_dir(&options.assets_root, "unpacked asset tree")?;
        require_file(&options.world_dir.join("World.json"), "world configuration")?;
        if options.workers == 0 {
            return Err(GenerationError::NoWorkers);
        }

        let pack = AssetPack::open(&options.assets_pack)?;
        // Phase 2 loads every child store the join streams.
        let assets = AssetRegistry::load_phase2(&pack)?;
        let registries = Arc::new(registries_from_assets(&assets)?);
        let config = WorldGenConfig::new(
            options.world_dir.clone(),
            GENERATOR_NAME.to_string(),
            GENERATOR_VERSION.to_string(),
        );
        let provider = WorldGenChunkProvider::new(
            config,
            WorldGenOptions {
                workers: options.workers,
                seed_label: SEED_LABEL.to_string(),
                assets_root: Some(options.assets_root.clone()),
                registries_path: None,
                registries: Some(Arc::clone(&registries)),
            },
        )?;
        Ok(Self {
            assets,
            registries,
            provider,
        })
    }

    /// The phase-2 asset registry used for the storage/packet codecs.
    pub fn assets(&self) -> &AssetRegistry {
        &self.assets
    }

    /// Generate one chunk and convert it to the runtime column.
    pub fn generate(
        &self,
        seed: i32,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Result<ChunkColumn, GenerationError> {
        let result = self
            .provider
            .generate(ChunkRequest::new(seed, chunk_x, chunk_z))?;
        Ok(ChunkColumn::from_generated_chunk_with_registries(
            &result.chunk,
            &self.registries,
        )?)
    }

    /// Stop the generator workers.
    pub fn shutdown(&self) {
        self.provider.shutdown();
    }
}

fn require_file(path: &std::path::Path, what: &'static str) -> Result<(), GenerationError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(GenerationError::MissingPath {
            path: path.to_path_buf(),
            what,
        })
    }
}

fn require_dir(path: &std::path::Path, what: &'static str) -> Result<(), GenerationError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(GenerationError::MissingPath {
            path: path.to_path_buf(),
            what,
        })
    }
}
