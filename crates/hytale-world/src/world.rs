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
