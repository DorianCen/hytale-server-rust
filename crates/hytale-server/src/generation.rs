//! Shared worldgen boot: asset pack → phase-2 registry → worldgen registries →
//! `worldgen-server` worker pool → runtime [`ChunkColumn`].
//!
//! `headless` and the saved-world boot both need the same generator setup; this
//! module owns it so they cannot drift.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};

use hytale_assets::{AssetError, AssetPack, AssetRegistry, ConfigAssets};
use hytale_interactions::{InteractionCatalog, InteractionError};
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::chunk::section::SectionError;
use hytale_world::registry::{registries_from_assets, RegistryError};
use thiserror::Error;
use worldgen_hytale::prefab::buffer::prefab_buffer::PrefabBuffer;
use worldgen_hytale::prefab::prefab_store::PrefabStore;
use worldgen_hytale::prefab::prefab_store_root::PrefabStoreRoot;
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
    /// Interaction assets failed to load for the shared context.
    #[error(transparent)]
    Interactions(#[from] InteractionError),
    /// The shared config burst could not be encoded.
    #[error(transparent)]
    AssetPackets(#[from] hytale_assets::AssetPacketError),
    /// The worker count is zero.
    #[error("the worldgen worker count must be positive")]
    NoWorkers,
}

/// Asset and packet state that is independent of one world.
///
/// A process can serve several worlds from one asset pack; constructing the
/// phase-2 registry, interaction catalog and encoded config burst once avoids
/// repeating the expensive decode/encode work per world. Sharing is opt-in with
/// `HYTALE_SHARED_ASSETS=1` (the test scripts enable it).
#[derive(Clone)]
pub struct SharedGeneratorAssets {
    assets: Arc<AssetRegistry>,
    registries: Arc<Registries>,
    interactions: Arc<InteractionCatalog>,
    config_assets: Arc<ConfigAssets>,
}

impl SharedGeneratorAssets {
    /// The phase-2 asset registry.
    pub fn assets(&self) -> &AssetRegistry {
        &self.assets
    }

    /// The worldgen registries derived from the asset pack.
    pub fn registries(&self) -> &Arc<Registries> {
        &self.registries
    }

    /// The interaction catalog loaded from the asset pack.
    pub fn interactions(&self) -> Arc<InteractionCatalog> {
        Arc::clone(&self.interactions)
    }

    /// The encoded config asset burst shared by every session.
    pub fn config_assets(&self) -> Arc<ConfigAssets> {
        Arc::clone(&self.config_assets)
    }
}

static SHARED_GENERATOR_ASSETS: OnceLock<Mutex<HashMap<PathBuf, Arc<SharedGeneratorAssets>>>> =
    OnceLock::new();

fn shared_assets_enabled() -> bool {
    matches!(
        std::env::var("HYTALE_SHARED_ASSETS").as_deref(),
        Ok("1" | "true" | "on" | "yes")
    )
}

/// Load (or reuse) the process-wide asset context for one pack.
fn shared_generator_assets(
    options: &GeneratorOptions,
) -> Result<Arc<SharedGeneratorAssets>, GenerationError> {
    let key =
        std::fs::canonicalize(&options.assets_pack).unwrap_or_else(|_| options.assets_pack.clone());
    let cache = SHARED_GENERATOR_ASSETS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(shared) = cache.get(&key) {
        return Ok(Arc::clone(shared));
    }

    let pack = AssetPack::open(&options.assets_pack)?;
    let assets = Arc::new(AssetRegistry::load_phase2(&pack)?);
    let registries = Arc::new(registries_from_assets(&assets)?);
    let interactions = Arc::new(InteractionCatalog::load(&pack)?);
    let config_assets = Arc::new(assets.config_assets()?);
    let shared = Arc::new(SharedGeneratorAssets {
        assets,
        registries,
        interactions,
        config_assets,
    });
    cache.insert(key, Arc::clone(&shared));
    Ok(shared)
}

/// One asset registry + generator pair.
pub struct WorldGenerator {
    assets: Arc<AssetRegistry>,
    registries: Arc<Registries>,
    provider: WorldGenChunkProvider,
    /// CoreServer asset-pack prefabs (`Server/Prefabs`), resolved with the
    /// same block/fluid registries the generator uses.
    prefabs: PrefabStore,
    /// Process-wide asset/interaction/config context when sharing is enabled.
    shared: Option<Arc<SharedGeneratorAssets>>,
}

impl WorldGenerator {
    /// Load the asset pack and start the generator workers.
    pub fn open(options: &GeneratorOptions) -> Result<Self, GenerationError> {
        require_pack(&options.assets_pack, "asset pack")?;
        require_dir(&options.assets_root, "unpacked asset tree")?;
        require_file(&options.world_dir.join("World.json"), "world configuration")?;
        if options.workers == 0 {
            return Err(GenerationError::NoWorkers);
        }

        let shared = if shared_assets_enabled() {
            Some(shared_generator_assets(options)?)
        } else {
            None
        };
        let (assets, registries) = match &shared {
            Some(shared) => (Arc::clone(&shared.assets), Arc::clone(&shared.registries)),
            None => {
                let pack = AssetPack::open(&options.assets_pack)?;
                // Phase 2 loads every child store the join streams.
                let assets = Arc::new(AssetRegistry::load_phase2(&pack)?);
                let registries = Arc::new(registries_from_assets(&assets)?);
                (assets, registries)
            }
        };
        let prefab_root = PrefabStoreRoot::resolve_prefab_store(
            PrefabStoreRoot::Assets,
            &options.world_dir,
            &options.assets_root,
        );
        let prefabs = PrefabStore::new(prefab_root, Rc::new((*registries).clone()));
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
            prefabs,
            shared,
        })
    }

    /// The phase-2 asset registry used for the storage/packet codecs.
    pub fn assets(&self) -> &AssetRegistry {
        &self.assets
    }

    /// Shared handle to the phase-2 asset registry.
    pub fn assets_arc(&self) -> Arc<AssetRegistry> {
        Arc::clone(&self.assets)
    }

    /// The shared asset context, when `HYTALE_SHARED_ASSETS` is enabled.
    pub fn shared_assets(&self) -> Option<Arc<SharedGeneratorAssets>> {
        self.shared.as_ref().map(Arc::clone)
    }

    /// The prefab store rooted at `Assets/<pack>/Server/Prefabs`.
    pub fn prefab_store(&self) -> &PrefabStore {
        &self.prefabs
    }

    /// Resolve and load one prefab by its dotted name.
    ///
    /// Returns `None` when the name matches no prefab file.
    pub fn prefab(&self, name: &str) -> Option<Rc<PrefabBuffer>> {
        let paths = self.prefabs.resolve(name);
        let path = paths.first()?;
        Some(self.prefabs.load(path))
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

/// Asset packs can be a `.zip` or an extracted directory.
fn require_pack(path: &std::path::Path, what: &'static str) -> Result<(), GenerationError> {
    if path.is_file() || path.is_dir() {
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
