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
use crate::containers::BlockContainerStore;
use crate::player_data::{
    PlayerData, PlayerDataError, PlayerStorage, PlayerTransform, Position, Rotation,
};
use crate::runtime::{
    BlockComponent, BlockComponentStore, RuntimeEntity, RuntimeEntityId, RuntimeEntityKind,
    RuntimeEntityStore,
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
    /// Runtime entity/block-component state failed.
    #[error(transparent)]
    Runtime(#[from] crate::runtime::RuntimeStateError),
    /// A prefab could not be pasted.
    #[error("prefab paste failed: {0}")]
    PrefabPaste(String),
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

/// One block's global light channels (`0..=15`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LightSample {
    /// Sky light.
    pub sky: u8,
    /// Red block light.
    pub red: u8,
    /// Green block light.
    pub green: u8,
    /// Blue block light.
    pub blue: u8,
}

impl LightSample {
    /// Maximum RGB block-light channel.
    pub fn block_light(&self) -> u8 {
        self.red.max(self.green).max(self.blue)
    }

    /// Java `LightRangePredicate.calculateLightValue` (`SkyLight * factor`
    /// against the strongest block-light channel).
    pub fn total(&self, sunlight_factor: f64) -> u8 {
        ((f64::from(self.sky) * sunlight_factor).round() as u8).max(self.block_light())
    }
}

/// Loaded runtime chunks of one world plus their dirty state.
#[derive(Debug)]
pub struct World {
    storage: WorldStorage,
    chunks: HashMap<i64, ChunkColumn>,
    dirty: BTreeSet<i64>,
    /// World tick, advanced by [`World::advance_tick`].
    tick: u64,
    /// Blocks touched since the simulation last drained this set.
    changed_blocks: BTreeSet<(i32, i32, i32)>,
    /// Transient Java `BlockPhysics` support values (`0..=15`, `15` = deco).
    block_support: BTreeMap<(i32, i32, i32), u8>,
    /// Runtime entities (prefabs, items, projectiles, NPCs).
    entities: RuntimeEntityStore,
    /// Per-position block-component documents (Java `BlockComponentSection`).
    block_components: BlockComponentStore,
    /// Per-position block container inventories (Java `ItemContainerBlock`).
    containers: BlockContainerStore,
}

impl World {
    /// Create a world over a save directory.
    pub fn new(storage: WorldStorage) -> Self {
        Self {
            storage,
            chunks: HashMap::new(),
            dirty: BTreeSet::new(),
            tick: 0,
            changed_blocks: BTreeSet::new(),
            block_support: BTreeMap::new(),
            entities: RuntimeEntityStore::new(),
            block_components: BlockComponentStore::new(),
            containers: BlockContainerStore::new(),
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
            if let Some(section) = column.section_mut(y) {
                section.health.clear(index_local(x, y, z));
            }
            self.dirty.insert(index);
            self.changed_blocks.insert((x, y, z));
            self.block_support.remove(&(x, y, z));
            self.block_components.remove(x, y, z);
            self.containers.remove(x, y, z);
        }
        Ok(changed)
    }

    /// Java `BlockSection.getRotationIndex` on a loaded chunk.
    pub fn get_rotation(&self, x: i32, y: i32, z: i32) -> i32 {
        self.chunk(x >> 5, z >> 5)
            .map(|column| column.get_rotation(x & 31, y, z & 31))
            .unwrap_or(0)
    }

    /// Java `BlockSection.getFiller` on a loaded chunk.
    pub fn get_filler(&self, x: i32, y: i32, z: i32) -> i32 {
        self.chunk(x >> 5, z >> 5)
            .map(|column| column.get_filler(x & 31, y, z & 31))
            .unwrap_or(0)
    }

    /// Set one block preserving a specific rotation/filler.
    pub fn set_block_full(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        rotation: i32,
        filler: i32,
    ) -> Result<bool> {
        let index = index_chunk(x >> 5, z >> 5);
        let Some(column) = self.chunks.get_mut(&index) else {
            return Ok(false);
        };
        let changed = column.set_block(x & 31, y, z & 31, block_id, rotation, filler)?;
        if changed {
            if let Some(section) = column.section_mut(y) {
                section.health.clear(index_local(x, y, z));
            }
            self.dirty.insert(index);
            self.changed_blocks.insert((x, y, z));
            self.block_support.remove(&(x, y, z));
            self.block_components.remove(x, y, z);
            self.containers.remove(x, y, z);
        }
        Ok(changed)
    }

    /// Number of loaded 32³ sections that contain `environment_id`.
    ///
    /// Java tracks environment segments through `ChunkSpawnData`; this is the
    /// runtime-world approximation used by the spawn controller.
    pub fn environment_segment_count(&self, environment_id: i32) -> usize {
        self.chunks
            .values()
            .flat_map(|chunk| chunk.sections().flatten())
            .filter(|section| section.environment.contains(environment_id))
            .count()
    }

    /// Global packed-light sample at a block (`None` when the section is absent).
    pub fn light_sample(&self, x: i32, y: i32, z: i32) -> Option<LightSample> {
        let section = self.chunk(x >> 5, z >> 5)?.section(y)?;
        let packed = section
            .block
            .global_light()
            .get(usize::from(index_local(x, y, z)));
        Some(LightSample {
            sky: ((packed >> 12) & 0xF) as u8,
            red: (packed & 0xF) as u8,
            green: ((packed >> 4) & 0xF) as u8,
            blue: ((packed >> 8) & 0xF) as u8,
        })
    }

    /// Effective environment at a block (`UNKNOWN` when absent).
    pub fn get_environment(&self, x: i32, y: i32, z: i32) -> i32 {
        self.chunk(x >> 5, z >> 5)
            .map(|column| column.get_environment(x & 31, y, z & 31))
            .unwrap_or(0)
    }

    /// Read one fluid id on a loaded chunk.
    pub fn get_fluid_id(&self, x: i32, y: i32, z: i32) -> i32 {
        self.chunk(x >> 5, z >> 5)
            .map(|column| column.get_fluid_id(x & 31, y, z & 31))
            .unwrap_or(0)
    }

    /// Read one fluid level on a loaded chunk.
    pub fn get_fluid_level(&self, x: i32, y: i32, z: i32) -> i8 {
        self.chunk(x >> 5, z >> 5)
            .map(|column| column.get_fluid_level(x & 31, y, z & 31))
            .unwrap_or(0)
    }

    /// Set one fluid and mark the chunk dirty/changed.
    pub fn set_fluid(&mut self, x: i32, y: i32, z: i32, fluid_id: i32, level: i8) -> Result<bool> {
        let index = index_chunk(x >> 5, z >> 5);
        let Some(column) = self.chunks.get_mut(&index) else {
            return Ok(false);
        };
        let changed = column.set_fluid(x & 31, y, z & 31, fluid_id, level)?;
        if changed {
            self.dirty.insert(index);
            self.changed_blocks.insert((x, y, z));
        }
        Ok(changed)
    }

    /// The current world tick.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Advance the world tick and return the new value.
    pub fn advance_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    /// Drain the blocks touched since the last simulation pass.
    pub fn take_changed_blocks(&mut self) -> Vec<(i32, i32, i32)> {
        std::mem::take(&mut self.changed_blocks)
            .into_iter()
            .collect()
    }

    /// Runtime entity store.
    pub fn entities(&self) -> &RuntimeEntityStore {
        &self.entities
    }

    /// Mutable runtime entity store.
    pub fn entities_mut(&mut self) -> &mut RuntimeEntityStore {
        &mut self.entities
    }

    /// Spawn a runtime entity.
    pub fn spawn_entity(
        &mut self,
        kind: RuntimeEntityKind,
        prefab: Option<String>,
        transform: PlayerTransform,
        components: serde_json::Value,
    ) -> RuntimeEntityId {
        self.entities
            .spawn(RuntimeEntity::new(0, kind, prefab, transform, components))
    }

    /// Remove a runtime entity.
    pub fn despawn_entity(&mut self, id: RuntimeEntityId) -> Option<RuntimeEntity> {
        self.entities.despawn(id)
    }

    /// One runtime entity.
    pub fn entity(&self, id: RuntimeEntityId) -> Option<&RuntimeEntity> {
        self.entities.get(id)
    }

    /// Block-component store.
    pub fn block_components(&self) -> &BlockComponentStore {
        &self.block_components
    }

    /// Mutable block-component store.
    pub fn block_components_mut(&mut self) -> &mut BlockComponentStore {
        &mut self.block_components
    }

    /// One block component.
    pub fn block_component(&self, x: i32, y: i32, z: i32) -> Option<&BlockComponent> {
        self.block_components.get(x, y, z)
    }

    /// Install/replace a block component.
    pub fn set_block_component(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        components: serde_json::Value,
    ) {
        self.block_components.set(
            x,
            y,
            z,
            BlockComponent {
                block_id,
                components,
            },
        );
    }

    /// Remove one block component.
    pub fn remove_block_component(&mut self, x: i32, y: i32, z: i32) -> Option<BlockComponent> {
        self.block_components.remove(x, y, z)
    }

    /// Block-container store.
    pub fn containers(&self) -> &BlockContainerStore {
        &self.containers
    }

    /// Mutable block-container store.
    pub fn containers_mut(&mut self) -> &mut BlockContainerStore {
        &mut self.containers
    }

    /// One block's container inventory.
    pub fn container(
        &self,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<&hytale_inventory::SimpleItemContainer> {
        self.containers.get(x, y, z)
    }

    /// Install/replace one block's container inventory.
    pub fn set_container(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        container: hytale_inventory::SimpleItemContainer,
    ) {
        self.containers.set(x, y, z, container);
    }

    /// Get or create one block's container inventory.
    pub fn container_or_create(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        capacity: u16,
    ) -> &mut hytale_inventory::SimpleItemContainer {
        self.containers.get_or_create(x, y, z, capacity)
    }

    /// Remove one block's container inventory.
    pub fn remove_container(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<hytale_inventory::SimpleItemContainer> {
        self.containers.remove(x, y, z)
    }

    /// Load `entities.json`, `block-components.json` and
    /// `block-containers.json` from the save root.
    pub fn load_runtime_state(&mut self) -> Result<()> {
        let save_dir = self.storage.save_dir().to_path_buf();
        self.entities = RuntimeEntityStore::load(&save_dir)?;
        self.block_components = BlockComponentStore::load(&save_dir)?;
        self.containers = BlockContainerStore::load(&save_dir)?;
        Ok(())
    }

    /// Save the runtime stores when they are dirty.
    pub fn save_runtime_state(&mut self) -> Result<bool> {
        let save_dir = self.storage.save_dir().to_path_buf();
        let entities = self.entities.save(&save_dir)?;
        let blocks = self.block_components.save(&save_dir)?;
        let containers = self.containers.save(&save_dir)?;
        Ok(entities || blocks || containers)
    }

    /// Remaining block health (`1.0` when undamaged).
    pub fn block_health(&self, x: i32, y: i32, z: i32) -> f32 {
        self.chunk(x >> 5, z >> 5)
            .and_then(|column| column.section(y))
            .map_or(1.0, |section| section.health.health(index_local(x, y, z)))
    }

    /// Damaged blocks in every loaded chunk as `(x, y, z, absolute health)`.
    ///
    /// This is the T57 load-packet seam: Java sends the section's damaged
    /// entries after its block data so a client never applies damage to a
    /// block it has not received yet.
    pub fn block_health_entries(&self) -> Vec<(i32, i32, i32, f32)> {
        let mut entries = Vec::new();
        for (&chunk_index, column) in &self.chunks {
            let chunk_x = (chunk_index >> 32) as i32;
            let chunk_z = chunk_index as i32;
            collect_health_entries(column, chunk_x, chunk_z, &mut entries);
        }
        entries
    }

    /// Damaged blocks of one loaded chunk.
    pub fn block_health_entries_in_chunk(&self, x: i32, z: i32) -> Vec<(i32, i32, i32, f32)> {
        let Some(column) = self.chunk(x, z) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        collect_health_entries(column, x, z, &mut entries);
        entries
    }

    /// Take and clear one block's pending health replication delta.
    pub fn take_block_health_delta(&mut self, x: i32, y: i32, z: i32) -> Option<f32> {
        self.chunks
            .get_mut(&index_chunk(x >> 5, z >> 5))
            .and_then(|column| column.section_mut(y))
            .and_then(|section| section.health.take_delta(index_local(x, y, z)))
    }

    /// Java `BlockHealthSection.damage`: subtract `amount` and return the rest.
    pub fn damage_block(&mut self, x: i32, y: i32, z: i32, amount: f32) -> f32 {
        if !amount.is_finite() || amount <= 0.0 {
            return self.block_health(x, y, z);
        }
        let now_millis = (self.tick as i64).saturating_mul(50);
        let Some(column) = self.chunks.get_mut(&index_chunk(x >> 5, z >> 5)) else {
            return self.block_health(x, y, z);
        };
        let Some(section) = column.ensure_section(y) else {
            return 1.0;
        };
        let remaining = section
            .health
            .damage(index_local(x, y, z), amount, now_millis);
        self.dirty.insert(index_chunk(x >> 5, z >> 5));
        remaining
    }

    /// Mark a block fragile for `seconds`.
    pub fn make_block_fragile(&mut self, x: i32, y: i32, z: i32, seconds: f32) {
        let Some(column) = self.chunks.get_mut(&index_chunk(x >> 5, z >> 5)) else {
            return;
        };
        if let Some(section) = column.ensure_section(y) {
            section.health.make_fragile(index_local(x, y, z), seconds);
            self.dirty.insert(index_chunk(x >> 5, z >> 5));
        }
    }

    /// Remaining fragility seconds for one block (`0.0` when not fragile).
    pub fn block_fragility(&self, x: i32, y: i32, z: i32) -> f32 {
        self.chunk(x >> 5, z >> 5)
            .and_then(|column| column.section(y))
            .map_or(0.0, |section| {
                section.health.fragility(index_local(x, y, z))
            })
    }

    /// Java `BlockPhysics.get`: transient support distance (`0` when unset).
    pub fn support_value(&self, x: i32, y: i32, z: i32) -> i32 {
        self.block_support
            .get(&(x, y, z))
            .map_or(0, |value| i32::from(*value))
    }

    /// Java `BlockPhysics.set`: store a `0..=15` support distance.
    pub fn set_support_value(&mut self, x: i32, y: i32, z: i32, value: i32) -> bool {
        let value = value.clamp(0, 15) as u8;
        if value == 0 {
            self.block_support.remove(&(x, y, z)).is_some()
        } else {
            self.block_support.insert((x, y, z), value);
            true
        }
    }

    /// Clear one block's health, fragility and pending delta.
    pub fn clear_block_health(&mut self, x: i32, y: i32, z: i32) -> bool {
        self.chunks
            .get_mut(&index_chunk(x >> 5, z >> 5))
            .and_then(|column| column.section_mut(y))
            .is_some_and(|section| {
                let index = index_local(x, y, z);
                let had_health = section.health.entries().any(|(entry, _)| entry == index);
                let had_fragility = section.health.fragility(index) > 0.0;
                section.health.clear(index);
                had_health || had_fragility
            })
    }

    /// Regenerate damaged blocks and count down fragility.
    ///
    /// Java `BlockHealthSystems` uses the game-time instant; the native world
    /// maps one 50 ms tick to one game millisecond until T54 owns the clock.
    pub(crate) fn tick_block_health(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        const SECONDS_UNTIL_REGENERATION: i64 = 5;
        const HEALING_PER_SECOND: f32 = 0.1;
        let now_millis = (self.tick as i64).saturating_mul(50);
        for column in self.chunks.values_mut() {
            let mut changed = false;
            for section in column.sections_mut() {
                if section.health.is_empty() {
                    continue;
                }
                changed = true;
                section.health.tick_fragility(dt);
                section.health.tick_regeneration(
                    now_millis,
                    dt,
                    SECONDS_UNTIL_REGENERATION,
                    HEALING_PER_SECOND,
                );
            }
            if changed {
                self.dirty.insert(column.index());
            }
        }
    }

    /// Drain pending health deltas as absolute `(x, y, z, delta)` entries.
    pub fn take_block_health_deltas(&mut self) -> Vec<(i32, i32, i32, f32)> {
        let mut out = Vec::new();
        for (chunk_index, column) in &mut self.chunks {
            let chunk_x = (*chunk_index >> 32) as i32;
            let chunk_z = *chunk_index as i32;
            for (section_index, section) in column.sections_mut().enumerate() {
                for (index, delta) in section.health.take_pending_deltas() {
                    let index = usize::from(index);
                    out.push((
                        (chunk_x << 5) + (index & 31) as i32,
                        (section_index as i32 * 32) + ((index >> 10) & 31) as i32,
                        (chunk_z << 5) + ((index >> 5) & 31) as i32,
                        delta,
                    ));
                }
            }
        }
        out
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

    /// Loaded chunk coordinates, sorted by X then Z.
    pub fn loaded_chunk_coordinates(&self) -> Vec<(i32, i32)> {
        let mut coordinates: Vec<(i32, i32)> = self
            .chunks
            .keys()
            .map(|&index| ((index >> 32) as i32, index as i32))
            .collect();
        coordinates.sort_unstable();
        coordinates
    }

    /// Save a dirty chunk (if needed) and drop it from memory.
    ///
    /// Returns `false` when the chunk was not loaded.
    pub fn unload_chunk(&mut self, codec: &ChunkCodec<'_>, x: i32, z: i32) -> Result<bool> {
        let index = index_chunk(x, z);
        if !self.chunks.contains_key(&index) {
            return Ok(false);
        }
        if self.dirty.contains(&index) {
            let column = self.chunks.get(&index).expect("checked above");
            self.storage.save_chunk(codec, column)?;
            self.dirty.remove(&index);
        }
        self.chunks.remove(&index);
        Ok(true)
    }
}

fn index_local(x: i32, y: i32, z: i32) -> u16 {
    hytale_math::chunk_util::index_block(x & 31, y & 31, z & 31) as u16
}

fn collect_health_entries(
    column: &ChunkColumn,
    chunk_x: i32,
    chunk_z: i32,
    entries: &mut Vec<(i32, i32, i32, f32)>,
) {
    for (section_index, section) in column.sections().enumerate() {
        let Some(section) = section else {
            continue;
        };
        for (index, entry) in section.health.entries() {
            let index = i32::from(index);
            entries.push((
                (chunk_x << 5) + (index & 31),
                (section_index as i32 * 32) + ((index >> 10) & 31),
                (chunk_z << 5) + ((index >> 5) & 31),
                entry.health,
            ));
        }
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
        let mut world = World::new(WorldStorage::new(save_dir.clone()));
        world.load_runtime_state()?;
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
        self.world.save_runtime_state()?;
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
