//! Runtime world storage: load, mutate and save chunks through region files.
//!
//! [`WorldStorage`] owns the region-file cache for one save directory and maps
//! chunk coordinates to `IndexedStorageFile` blobs. [`World`] keeps the loaded
//! runtime columns, tracks dirty chunks and flushes them through the codec.
//!
//! The storage layer is codec-generic: callers pass the [`ChunkCodec`] bound to
//! their asset registry, so the same runtime uses the server's own ids.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use hytale_math::chunk_util::index_chunk;
use thiserror::Error;

use crate::chunk::column::ChunkColumn;
use crate::chunk::section::SectionError;
use crate::chunk::storage::{
    parse_region_file_name, region_coords, region_path, ChunkCodec, ChunkStorageError, RegionFile,
};
use crate::player_data::{
    PlayerData, PlayerDataError, PlayerStorage, PlayerTransform, Position, Rotation,
};
use crate::world_config::{WorldConfig, WorldConfigError, WorldUuid};

/// Errors from the runtime world storage.
#[derive(Debug, Error)]
pub enum WorldError {
    /// The BSON/region codec failed.
    #[error(transparent)]
    Storage(#[from] ChunkStorageError),
    /// A block/fluid/environment mutation failed.
    #[error(transparent)]
    Section(#[from] SectionError),
    /// A filesystem operation failed.
    #[error("world storage path error: {0}")]
    Io(#[from] std::io::Error),
    /// The save directory contains a file whose name is not a region file.
    #[error("invalid region file name: {0}")]
    InvalidRegionFile(String),
    /// The world `config.json` failed.
    #[error(transparent)]
    Config(#[from] WorldConfigError),
    /// A player save failed.
    #[error(transparent)]
    Player(#[from] PlayerDataError),
    /// A missing chunk could not be generated.
    #[error("chunk generation failed: {0}")]
    Generation(String),
}

/// Result alias for the world storage.
pub type Result<T> = std::result::Result<T, WorldError>;

/// Region files below one world save directory.
pub struct WorldStorage {
    save_dir: PathBuf,
    regions: BTreeMap<(i32, i32), RegionFile>,
}

impl WorldStorage {
    /// Create a storage handle for a world save directory.
    ///
    /// The directory (and `chunks/`) is created lazily on the first write.
    pub fn new(save_dir: impl Into<PathBuf>) -> Self {
        Self {
            save_dir: save_dir.into(),
            regions: BTreeMap::new(),
        }
    }

    /// World save root (`.../chunks/<region>.region.bin`).
    pub fn save_dir(&self) -> &Path {
        &self.save_dir
    }

    /// `chunks/` directory inside the save root.
    pub fn chunks_dir(&self) -> PathBuf {
        self.save_dir.join("chunks")
    }

    /// Load one stored chunk, or `None` when the region/blob is absent.
    pub fn load_chunk(
        &mut self,
        codec: &ChunkCodec<'_>,
        x: i32,
        z: i32,
    ) -> Result<Option<ChunkColumn>> {
        let key = region_coords(x, z);
        if !self.regions.contains_key(&key) {
            let path = self.region_path(x, z);
            if !path.is_file() {
                return Ok(None);
            }
            self.regions.insert(key, RegionFile::open(&path)?);
        }
        let Some(bytes) = self
            .regions
            .get_mut(&key)
            .expect("region inserted above")
            .read_chunk(x, z)?
        else {
            return Ok(None);
        };
        Ok(Some(codec.decode_at(&bytes, x, z)?))
    }

    /// Save one chunk into its region file.
    pub fn save_chunk(&mut self, codec: &ChunkCodec<'_>, column: &ChunkColumn) -> Result<()> {
        let (x, z) = (column.x(), column.z());
        let bytes = codec.encode(column)?;
        let key = region_coords(x, z);
        if !self.regions.contains_key(&key) {
            self.regions
                .insert(key, RegionFile::open_for_chunk(&self.save_dir, x, z)?);
        }
        self.regions
            .get_mut(&key)
            .expect("region inserted above")
            .write_chunk(x, z, &bytes)?;
        Ok(())
    }

    /// Remove one stored chunk.
    pub fn remove_chunk(&mut self, x: i32, z: i32) -> Result<()> {
        let key = region_coords(x, z);
        if !self.regions.contains_key(&key) {
            let path = self.region_path(x, z);
            if !path.is_file() {
                return Ok(());
            }
            self.regions.insert(key, RegionFile::open(&path)?);
        }
        self.regions
            .get_mut(&key)
            .expect("region inserted above")
            .remove_chunk(x, z)?;
        Ok(())
    }

    /// Flush every open region file.
    pub fn flush(&mut self) -> Result<()> {
        for region in self.regions.values_mut() {
            region.flush()?;
        }
        Ok(())
    }

    /// Every chunk coordinate stored below the save directory, sorted.
    ///
    /// Empty blobs and directories are ignored. This scans the region files
    /// with a separate read-only handle and does not populate the write cache.
    pub fn stored_chunk_coordinates(&self) -> Result<Vec<(i32, i32)>> {
        let chunks_dir = self.chunks_dir();
        if !chunks_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut coordinates = Vec::new();
        for entry in std::fs::read_dir(&chunks_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.ends_with(".region.bin") {
                continue;
            }
            let (region_x, region_z) = parse_region_file_name(&name)
                .ok_or_else(|| WorldError::InvalidRegionFile(name.to_string()))?;
            let region = RegionFile::open_read_only(&entry.path())?;
            for index in region.keys() {
                coordinates.push((
                    region_x * 32 + RegionFile::x_from_index(index),
                    region_z * 32 + RegionFile::z_from_index(index),
                ));
            }
        }
        coordinates.sort_unstable();
        coordinates.dedup();
        Ok(coordinates)
    }

    fn region_path(&self, x: i32, z: i32) -> PathBuf {
        region_path(&self.save_dir, x, z)
    }
}

impl std::fmt::Debug for WorldStorage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorldStorage")
            .field("save_dir", &self.save_dir)
            .field("open_regions", &self.regions.len())
            .finish()
    }
}

/// Loaded runtime chunks of one world plus their dirty state.
#[derive(Debug)]
pub struct World {
    storage: WorldStorage,
    chunks: HashMap<i64, ChunkColumn>,
    dirty: BTreeSet<i64>,
}

impl World {
    /// Create a world over a save directory.
    pub fn new(storage: WorldStorage) -> Self {
        Self {
            storage,
            chunks: HashMap::new(),
            dirty: BTreeSet::new(),
        }
    }

    /// Storage handle.
    pub fn storage(&self) -> &WorldStorage {
        &self.storage
    }

    /// Mutable storage handle (for region scans and explicit flushes).
    pub fn storage_mut(&mut self) -> &mut WorldStorage {
        &mut self.storage
    }

    /// Load a stored chunk into the runtime map.
    ///
    /// Returns `false` when no region/blob exists. An already loaded chunk is
    /// left untouched (and is not marked dirty).
    pub fn load_chunk(&mut self, codec: &ChunkCodec<'_>, x: i32, z: i32) -> Result<bool> {
        let index = index_chunk(x, z);
        if self.chunks.contains_key(&index) {
            return Ok(true);
        }
        let Some(column) = self.storage.load_chunk(codec, x, z)? else {
            return Ok(false);
        };
        self.chunks.insert(index, column);
        Ok(true)
    }

    /// Insert a freshly generated (or otherwise built) column.
    pub fn insert_chunk(&mut self, column: ChunkColumn) {
        let index = column.index();
        self.chunks.insert(index, column);
        self.dirty.insert(index);
    }

    /// A loaded chunk.
    pub fn chunk(&self, x: i32, z: i32) -> Option<&ChunkColumn> {
        self.chunks.get(&index_chunk(x, z))
    }

    /// A loaded chunk, marked dirty for the next save.
    pub fn chunk_mut(&mut self, x: i32, z: i32) -> Option<&mut ChunkColumn> {
        let index = index_chunk(x, z);
        if !self.chunks.contains_key(&index) {
            return None;
        }
        self.dirty.insert(index);
        self.chunks.get_mut(&index)
    }

    /// Java `BlockChunk.getBlock` on a loaded chunk (air when absent).
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> i32 {
        self.chunk(x >> 5, z >> 5)
            .map(|column| column.get_block(x & 31, y, z & 31))
            .unwrap_or(0)
    }

    /// Set a block in a loaded chunk and mark it dirty.
    ///
    /// Returns `false` when the chunk is not loaded or the block is unchanged.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_id: i32) -> Result<bool> {
        let index = index_chunk(x >> 5, z >> 5);
        let Some(column) = self.chunks.get_mut(&index) else {
            return Ok(false);
        };
        let (local_x, local_z) = (x & 31, z & 31);
        let rotation = column.get_rotation(local_x, y, local_z);
        let filler = column.get_filler(local_x, y, local_z);
        let changed = column.set_block(local_x, y, local_z, block_id, rotation, filler)?;
        if changed {
            self.dirty.insert(index);
        }
        Ok(changed)
    }

    /// Save one loaded chunk if it is dirty.
    pub fn save_chunk(&mut self, codec: &ChunkCodec<'_>, x: i32, z: i32) -> Result<bool> {
        let index = index_chunk(x, z);
        if !self.dirty.contains(&index) {
            return Ok(false);
        }
        let Some(column) = self.chunks.get(&index) else {
            self.dirty.remove(&index);
            return Ok(false);
        };
        self.storage.save_chunk(codec, column)?;
        self.dirty.remove(&index);
        Ok(true)
    }

    /// Save every dirty chunk.
    pub fn save_all(&mut self, codec: &ChunkCodec<'_>) -> Result<usize> {
        let dirty: Vec<i64> = self.dirty.iter().copied().collect();
        let mut saved = 0;
        for index in dirty {
            let Some(column) = self.chunks.get(&index) else {
                self.dirty.remove(&index);
                continue;
            };
            self.storage.save_chunk(codec, column)?;
            self.dirty.remove(&index);
            saved += 1;
        }
        self.storage.flush()?;
        Ok(saved)
    }

    /// Flush open region files without saving new chunks.
    pub fn flush(&mut self) -> Result<()> {
        self.storage.flush()
    }

    /// Loaded chunk count.
    pub fn loaded_chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Dirty chunk indexes, sorted.
    pub fn dirty_chunks(&self) -> Vec<i64> {
        self.dirty.iter().copied().collect()
    }
}

/// Chunk coordinates in a square around a center chunk, sorted by Z then X.
pub fn chunk_coordinates_around(center_x: i32, center_z: i32, radius: u32) -> Vec<(i32, i32)> {
    let radius = i32::try_from(radius).unwrap_or(i32::MAX);
    let side = (radius as i64 * 2 + 1) as usize;
    let mut coordinates = Vec::with_capacity(side * side);
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            coordinates.push((center_x + dx, center_z + dz));
        }
    }
    coordinates
}

/// What a demand-load pass did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DemandReport {
    /// Unique chunks requested.
    pub requested: usize,
    /// Chunks already in memory.
    pub already_loaded: usize,
    /// Chunks loaded from region files.
    pub loaded: usize,
    /// Chunks generated and inserted.
    pub generated: usize,
}

/// A complete saved world: `config.json`, player files and chunk regions.
///
/// This is the boot-time bundle the live server will own. It loads or creates
/// the world config, exposes the runtime [`World`] for chunks, and manages
/// `<save>/players/` through [`PlayerStorage`]. Saving writes dirty chunks,
/// then the config.
#[derive(Debug)]
pub struct WorldSave {
    world: World,
    config: WorldConfig,
    players: PlayerStorage,
    config_path: PathBuf,
}

impl WorldSave {
    /// Open a save directory, loading `config.json` or creating one.
    ///
    /// `seed`, `display_name` and `uuid` are only used when the config does not
    /// exist yet. Returns the bundle and whether a new config was created.
    pub fn open(
        save_dir: impl Into<PathBuf>,
        seed: i64,
        display_name: Option<String>,
        uuid: WorldUuid,
    ) -> Result<(Self, bool)> {
        let save_dir = save_dir.into();
        let config_path = WorldConfig::path_in(&save_dir);
        let (config, created) = if config_path.is_file() {
            (WorldConfig::load(&config_path)?, false)
        } else {
            let config = WorldConfig::new(uuid, seed, display_name);
            config.save(&config_path)?;
            (config, true)
        };
        let world = World::new(WorldStorage::new(save_dir.clone()));
        let players = PlayerStorage::for_save_dir(&save_dir);
        Ok((
            Self {
                world,
                config,
                players,
                config_path,
            },
            created,
        ))
    }

    /// Build a bundle from an already-created world and config.
    pub fn with_config(world: World, config: WorldConfig) -> Self {
        let config_path = WorldConfig::path_in(world.storage().save_dir());
        let players = PlayerStorage::for_save_dir(world.storage().save_dir());
        Self {
            world,
            config,
            players,
            config_path,
        }
    }

    /// The runtime chunk world.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Mutable runtime chunk world.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// The per-world config.
    pub fn config(&self) -> &WorldConfig {
        &self.config
    }

    /// Mutable config; call [`Self::save_config`] after changing it.
    pub fn config_mut(&mut self) -> &mut WorldConfig {
        &mut self.config
    }

    /// Player storage under `<save>/players/`.
    pub fn players(&self) -> &PlayerStorage {
        &self.players
    }

    /// Load a player, or create one at the config spawn point.
    ///
    /// Returns the holder and whether it was newly created (and written).
    pub fn load_or_create_player(&mut self, uuid: WorldUuid) -> Result<(PlayerData, bool)> {
        if let Some(data) = self.players.load(uuid)? {
            return Ok((data, false));
        }
        let mut data = PlayerData::new(uuid, self.default_spawn());
        data.set_world_gen_id(0);
        self.players.save(uuid, &data)?;
        Ok((data, true))
    }

    /// Write a player holder.
    pub fn save_player(&self, uuid: WorldUuid, data: &PlayerData) -> Result<()> {
        self.players.save(uuid, data)?;
        Ok(())
    }

    /// Write `config.json`.
    pub fn save_config(&self) -> Result<()> {
        self.config.save(&self.config_path)?;
        Ok(())
    }

    /// Save every dirty chunk, flush the region files and write the config.
    pub fn save_all(&mut self, codec: &ChunkCodec<'_>) -> Result<usize> {
        let saved = self.world.save_all(codec)?;
        self.save_config()?;
        Ok(saved)
    }

    /// Ensure the requested chunks are loaded, generating missing ones.
    ///
    /// Chunks already in memory are counted, then stored chunks are read from
    /// disk, then `generate` is called for the rest. Generated columns are
    /// inserted as dirty, so [`Self::save_all`] writes them.
    pub fn ensure_chunks<F>(
        &mut self,
        codec: &ChunkCodec<'_>,
        coordinates: impl IntoIterator<Item = (i32, i32)>,
        mut generate: F,
    ) -> Result<DemandReport>
    where
        F: FnMut(i32, i32) -> std::result::Result<ChunkColumn, WorldError>,
    {
        let mut coordinates: Vec<(i32, i32)> = coordinates.into_iter().collect();
        coordinates.sort_unstable();
        coordinates.dedup();
        let mut report = DemandReport {
            requested: coordinates.len(),
            ..DemandReport::default()
        };
        for (x, z) in coordinates {
            if self.world.chunk(x, z).is_some() {
                report.already_loaded += 1;
                continue;
            }
            if self.world.load_chunk(codec, x, z)? {
                report.loaded += 1;
                continue;
            }
            let column = generate(x, z)?;
            self.world.insert_chunk(column);
            report.generated += 1;
        }
        Ok(report)
    }

    /// The config file path.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// The save root.
    pub fn save_dir(&self) -> &Path {
        self.world.storage().save_dir()
    }

    fn default_spawn(&self) -> PlayerTransform {
        let position = self
            .config
            .spawn_provider
            .as_ref()
            .and_then(|provider| provider.get("SpawnPoint"))
            .and_then(|point| {
                Some(Position {
                    x: point.get("X")?.as_f64()?,
                    y: point.get("Y")?.as_f64()?,
                    z: point.get("Z")?.as_f64()?,
                })
            })
            .unwrap_or(Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            });
        // The config's spawn rotation is stored in degrees (Transform.CODEC_DEGREES);
        // a new player starts facing the default direction.
        PlayerTransform::new(
            position,
            Rotation {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
            },
        )
    }
}
