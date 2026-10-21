//! Vanilla `IndexedStorageFile` v1 container.
//!
//! Ported from `Storage/src/main/java/com/hypixel/hytale/storage/IndexedStorageFile.java`.
//! The file layout is:
//!
//! ```text
//! magic          20 bytes   "HytaleIndexedStorage"
//! version         4 bytes   u32 BE, currently 1
//! blob count      4 bytes   u32 BE
//! segment size    4 bytes   u32 BE
//! blob indexes    blob_count * 4 bytes, i32 BE, 0 = unassigned
//! segments        ...
//! ```
//!
//! Every blob is `[u32 BE source length][u32 BE compressed length][zstd frame]`
//! and occupies a contiguous run of `ceil((8 + compressed_len) / segment_size)`
//! segments. Segment indexes start at 1.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Magic bytes at the start of every file.
pub const MAGIC: &[u8; 20] = b"HytaleIndexedStorage";

/// Current format version.
pub const VERSION: u32 = 1;

/// Default number of blob slots per file.
pub const DEFAULT_BLOB_COUNT: u32 = 1024;

/// Default segment size in bytes.
pub const DEFAULT_SEGMENT_SIZE: u32 = 4096;

/// Default zstd compression level.
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Magic length in bytes.
pub const MAGIC_LENGTH: usize = MAGIC.len();

/// Offset of the version field.
pub const VERSION_OFFSET: u64 = MAGIC_LENGTH as u64;

/// Offset of the blob-count field.
pub const BLOB_COUNT_OFFSET: u64 = VERSION_OFFSET + 4;

/// Offset of the segment-size field.
pub const SEGMENT_SIZE_OFFSET: u64 = BLOB_COUNT_OFFSET + 4;

/// Size of the fixed header.
pub const HEADER_LENGTH: u64 = SEGMENT_SIZE_OFFSET + 4;

/// Size of one blob-index entry.
pub const INDEX_SIZE: u64 = 4;

/// Size of the per-blob header.
pub const BLOB_HEADER_LENGTH: u64 = 8;

/// Segment index meaning "unassigned".
pub const UNASSIGNED_INDEX: i32 = 0;

/// First valid segment index.
pub const FIRST_SEGMENT_INDEX: i32 = 1;

/// Errors produced by the storage layer.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Underlying I/O failed.
    #[error("{path}: {source}")]
    Io {
        /// Path being accessed.
        path: PathBuf,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
    /// The file does not start with the expected magic.
    #[error("{path}: invalid magic (expected {MAGIC:?})")]
    InvalidMagic {
        /// Path being accessed.
        path: PathBuf,
    },
    /// The version is outside the supported range.
    #[error("{path}: unsupported IndexedStorageFile version {version}")]
    UnsupportedVersion {
        /// Path being accessed.
        path: PathBuf,
        /// Version read from the header.
        version: u32,
    },
    /// The header contains an invalid count or size.
    #[error("{path}: invalid header: {message}")]
    InvalidHeader {
        /// Path being accessed.
        path: PathBuf,
        /// What is wrong.
        message: String,
    },
    /// A blob index was outside the file's slot count.
    #[error("blob index {index} is outside 0..{blob_count}")]
    IndexOutOfRange {
        /// Requested index.
        index: u32,
        /// Configured blob count.
        blob_count: u32,
    },
    /// A blob header or payload was truncated or inconsistent.
    #[error("{path}: corrupt blob {index}: {message}")]
    Corrupt {
        /// Path being accessed.
        path: PathBuf,
        /// Blob index.
        index: u32,
        /// What is wrong.
        message: String,
    },
    /// zstd failed.
    #[error("zstd: {0}")]
    Zstd(String),
}

type Result<T> = std::result::Result<T, StorageError>;

/// A Java-compatible indexed storage file.
#[derive(Debug)]
pub struct IndexedStorageFile {
    path: PathBuf,
    file: File,
    read_only: bool,
    flush_on_write: bool,
    compression_level: i32,
    version: u32,
    blob_count: u32,
    segment_size: u32,
    indexes: Vec<i32>,
    used_segments: Vec<bool>,
    used_blob_count: u32,
}

impl IndexedStorageFile {
    /// Create a new file with explicit blob count and segment size.
    ///
    /// Fails if the path already exists, mirroring Java's `CREATE_NEW`.
    pub fn create(path: &Path, blob_count: u32, segment_size: u32) -> Result<Self> {
        Self::create_with_level(path, blob_count, segment_size, DEFAULT_COMPRESSION_LEVEL)
    }

    /// Create a new file with an explicit compression level.
    pub fn create_with_level(
        path: &Path,
        blob_count: u32,
        segment_size: u32,
        compression_level: i32,
    ) -> Result<Self> {
        if blob_count == 0 {
            return Err(StorageError::InvalidHeader {
                path: path.to_path_buf(),
                message: "blob count must be > 0".to_string(),
            });
        }
        if segment_size == 0 {
            return Err(StorageError::InvalidHeader {
                path: path.to_path_buf(),
                message: "segment size must be > 0".to_string(),
            });
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|source| StorageError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        let mut storage = Self {
            path: path.to_path_buf(),
            file,
            read_only: false,
            flush_on_write: false,
            compression_level,
            version: VERSION,
            blob_count,
            segment_size,
            indexes: vec![UNASSIGNED_INDEX; blob_count as usize],
            used_segments: vec![false; 1],
            used_blob_count: 0,
        };
        storage.write_header()?;
        storage.write_index_table()?;
        Ok(storage)
    }

    /// Open an existing file, optionally read-only.
    pub fn open(path: &Path, read_only: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(!read_only)
            .open(path)
            .map_err(|source| StorageError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        let mut storage = Self {
            path: path.to_path_buf(),
            file,
            read_only,
            flush_on_write: false,
            compression_level: DEFAULT_COMPRESSION_LEVEL,
            version: 0,
            blob_count: 0,
            segment_size: 0,
            indexes: Vec::new(),
            used_segments: vec![false; 1],
            used_blob_count: 0,
        };
        storage.read_header()?;
        storage.read_index_table()?;
        storage.read_used_segments()?;
        Ok(storage)
    }

    /// Path backing this file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Maximum blob slots.
    pub fn blob_count(&self) -> u32 {
        self.blob_count
    }

    /// Configured segment size.
    pub fn segment_size(&self) -> u32 {
        self.segment_size
    }

    /// Format version.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// zstd level used by subsequent writes.
    pub fn compression_level(&self) -> i32 {
        self.compression_level
    }

    /// Set the zstd level used by subsequent writes.
    pub fn set_compression_level(&mut self, level: i32) {
        self.compression_level = level;
    }

    /// Whether writes and removes force data to disk.
    pub fn flush_on_write(&self) -> bool {
        self.flush_on_write
    }

    /// Set whether writes and removes force data to disk.
    pub fn set_flush_on_write(&mut self, flush_on_write: bool) {
        self.flush_on_write = flush_on_write;
    }

    /// Number of assigned blob slots.
    pub fn used_blob_count(&self) -> u32 {
        self.used_blob_count
    }

    /// Read one blob. Returns `None` for an unassigned slot.
    pub fn read_blob(&mut self, index: u32) -> Result<Option<Vec<u8>>> {
        self.check_index(index)?;
        let first = self.indexes[index as usize];
        if first == UNASSIGNED_INDEX {
            return Ok(None);
        }
        let (source_length, compressed_length) = self.read_blob_header(index, first)?;
        if u64::from(compressed_length) > self.size()? {
            return Err(StorageError::Corrupt {
                path: self.path.clone(),
                index,
                message: format!("compressed length {compressed_length} exceeds file size"),
            });
        }
        let compressed = self.read_at(
            self.segment_position(first) + BLOB_HEADER_LENGTH,
            compressed_length as usize,
        )?;
        let declared = zstd::zstd_safe::get_frame_content_size(&compressed)
            .map_err(|error| StorageError::Zstd(format!("invalid frame header: {error:?}")))?;
        if declared != Some(u64::from(source_length)) {
            return Err(StorageError::Corrupt {
                path: self.path.clone(),
                index,
                message: format!(
                    "blob header declares {source_length} bytes but zstd frame declares {declared:?}"
                ),
            });
        }
        let output = zstd::bulk::decompress(&compressed, source_length as usize)
            .map_err(|error| StorageError::Zstd(error.to_string()))?;
        if output.len() != source_length as usize {
            return Err(StorageError::Corrupt {
                path: self.path.clone(),
                index,
                message: format!(
                    "declared source length {source_length} but decoded {}",
                    output.len()
                ),
            });
        }
        Ok(Some(output))
    }

    /// Write (or overwrite) a blob.
    pub fn write_blob(&mut self, index: u32, data: &[u8]) -> Result<()> {
        self.check_writable()?;
        self.check_index(index)?;

        if data.len() > u32::MAX as usize {
            return Err(StorageError::InvalidHeader {
                path: self.path.clone(),
                message: format!("blob length {} exceeds u32", data.len()),
            });
        }
        let compressed = zstd::bulk::compress(data, self.compression_level)
            .map_err(|error| StorageError::Zstd(error.to_string()))?;
        if compressed.len() > u32::MAX as usize {
            return Err(StorageError::InvalidHeader {
                path: self.path.clone(),
                message: format!("compressed length {} exceeds u32", compressed.len()),
            });
        }
        let total = BLOB_HEADER_LENGTH as usize + compressed.len();
        let segment_count = self.required_segments(total);

        let old_first = self.indexes[index as usize];
        let old_segment_count = if old_first == UNASSIGNED_INDEX {
            0
        } else {
            let (_, old_compressed) = self.read_blob_header(index, old_first)?;
            self.required_segments(BLOB_HEADER_LENGTH as usize + old_compressed as usize)
        };

        let first = self.find_free_segments(segment_count)?;

        let mut blob = Vec::with_capacity(total);
        blob.extend_from_slice(&(data.len() as u32).to_be_bytes());
        blob.extend_from_slice(&(compressed.len() as u32).to_be_bytes());
        blob.extend_from_slice(&compressed);
        self.write_at(self.segment_position(first), &blob)?;

        // Force the payload before pointing the index at it, matching Java.
        if self.flush_on_write {
            self.force()?;
        }

        self.indexes[index as usize] = first;
        self.write_index(index)?;
        let was_assigned = old_first != UNASSIGNED_INDEX;
        if !was_assigned {
            self.used_blob_count += 1;
        }
        self.mark_used(first, segment_count, true);

        if old_segment_count > 0 {
            self.mark_used(old_first, old_segment_count, false);
        }
        Ok(())
    }

    /// Remove a blob and free its segments.
    pub fn remove_blob(&mut self, index: u32) -> Result<()> {
        self.check_writable()?;
        self.check_index(index)?;
        let first = self.indexes[index as usize];
        if first == UNASSIGNED_INDEX {
            return Ok(());
        }
        let (_, compressed_length) = self.read_blob_header(index, first)?;
        let segment_count =
            self.required_segments(BLOB_HEADER_LENGTH as usize + compressed_length as usize);

        // Clear the index first, then free the segments.
        self.indexes[index as usize] = UNASSIGNED_INDEX;
        self.write_index(index)?;
        if self.flush_on_write {
            self.force()?;
        }
        self.used_blob_count -= 1;
        self.mark_used(first, segment_count, false);
        Ok(())
    }

    /// Force file data to disk.
    pub fn flush(&mut self) -> Result<()> {
        self.file.sync_data().map_err(|source| StorageError::Io {
            path: self.path.clone(),
            source,
        })
    }

    /// All assigned blob indexes, ascending.
    pub fn keys(&self) -> Vec<u32> {
        self.indexes
            .iter()
            .enumerate()
            .filter_map(|(index, first)| {
                if *first != UNASSIGNED_INDEX {
                    Some(index as u32)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Total segments currently referenced by blobs.
    pub fn used_segment_count(&self) -> usize {
        self.used_segments.iter().filter(|used| **used).count()
    }

    /// Current file size in bytes.
    pub fn size(&self) -> Result<u64> {
        self.file
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|source| StorageError::Io {
                path: self.path.clone(),
                source,
            })
    }

    fn write_header(&mut self) -> Result<()> {
        let mut header = Vec::with_capacity(HEADER_LENGTH as usize);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&self.version.to_be_bytes());
        header.extend_from_slice(&self.blob_count.to_be_bytes());
        header.extend_from_slice(&self.segment_size.to_be_bytes());
        self.write_at(0, &header)
    }

    fn read_header(&mut self) -> Result<()> {
        let header = self.read_at(0, HEADER_LENGTH as usize)?;
        if &header[..MAGIC_LENGTH] != MAGIC {
            return Err(StorageError::InvalidMagic {
                path: self.path.clone(),
            });
        }
        self.version = u32::from_be_bytes(read_u32(&header[VERSION_OFFSET as usize..])?);
        if self.version == 0 || self.version > VERSION {
            return Err(StorageError::UnsupportedVersion {
                path: self.path.clone(),
                version: self.version,
            });
        }
        self.blob_count = u32::from_be_bytes(read_u32(&header[BLOB_COUNT_OFFSET as usize..])?);
        self.segment_size = u32::from_be_bytes(read_u32(&header[SEGMENT_SIZE_OFFSET as usize..])?);
        if self.blob_count == 0 || self.segment_size == 0 {
            return Err(StorageError::InvalidHeader {
                path: self.path.clone(),
                message: format!(
                    "blob count {} and segment size {} must both be > 0",
                    self.blob_count, self.segment_size
                ),
            });
        }
        Ok(())
    }

    fn write_index_table(&mut self) -> Result<()> {
        let mut bytes = Vec::with_capacity(self.indexes.len() * 4);
        for value in &self.indexes {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        self.write_at(HEADER_LENGTH, &bytes)
    }

    fn read_index_table(&mut self) -> Result<()> {
        let bytes = self.read_at(
            HEADER_LENGTH,
            self.blob_count as usize * INDEX_SIZE as usize,
        )?;
        self.indexes = bytes
            .chunks_exact(4)
            .map(|chunk| i32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        Ok(())
    }

    fn read_used_segments(&mut self) -> Result<()> {
        let mut count = 0u32;
        let mut used = vec![false; 1];
        for index in 0..self.blob_count {
            let first = self.indexes[index as usize];
            if first == UNASSIGNED_INDEX {
                continue;
            }
            count += 1;
            let (_, compressed_length) = self.read_blob_header(index, first)?;
            if compressed_length > 0 {
                let segments = self
                    .required_segments(BLOB_HEADER_LENGTH as usize + compressed_length as usize);
                let end = first as usize + segments;
                if used.len() < end {
                    used.resize(end, false);
                }
                for slot in &mut used[first as usize..end] {
                    *slot = true;
                }
            }
        }
        self.used_segments = used;
        self.used_blob_count = count;
        Ok(())
    }

    fn write_index(&mut self, index: u32) -> Result<()> {
        let offset = HEADER_LENGTH + u64::from(index) * INDEX_SIZE;
        self.write_at(offset, &self.indexes[index as usize].to_be_bytes())
    }

    fn read_blob_header(&mut self, index: u32, first: i32) -> Result<(u32, u32)> {
        if first <= 0 {
            return Err(StorageError::Corrupt {
                path: self.path.clone(),
                index,
                message: format!("invalid first segment {first}"),
            });
        }
        let header = self.read_at(self.segment_position(first), BLOB_HEADER_LENGTH as usize)?;
        Ok((
            u32::from_be_bytes(read_u32(&header[..4])?),
            u32::from_be_bytes(read_u32(&header[4..8])?),
        ))
    }

    fn find_free_segments(&mut self, count: usize) -> Result<i32> {
        if count == 0 {
            return Err(StorageError::InvalidHeader {
                path: self.path.clone(),
                message: "blob requires zero segments".to_string(),
            });
        }
        let mut start = FIRST_SEGMENT_INDEX as usize;
        let mut found = 0usize;
        while found < count {
            let index = start + found;
            if index >= self.used_segments.len() || !self.used_segments[index] {
                found += 1;
            } else {
                start = index + 1;
                found = 0;
            }
        }
        if start > i32::MAX as usize || count > i32::MAX as usize {
            return Err(StorageError::InvalidHeader {
                path: self.path.clone(),
                message: "segment index overflow".to_string(),
            });
        }
        let end = start + count;
        if end > self.used_segments.len() {
            self.used_segments.resize(end, false);
        }
        Ok(start as i32)
    }

    fn mark_used(&mut self, first: i32, count: usize, used: bool) {
        let start = first as usize;
        let end = start + count;
        if self.used_segments.len() < end {
            self.used_segments.resize(end, false);
        }
        for slot in &mut self.used_segments[start..end] {
            *slot = used;
        }
    }

    fn required_segments(&self, data_length: usize) -> usize {
        data_length.div_ceil(self.segment_size as usize)
    }

    fn segment_position(&self, segment_index: i32) -> u64 {
        self.segments_base() + (segment_index as u64 - 1) * u64::from(self.segment_size)
    }

    fn segments_base(&self) -> u64 {
        HEADER_LENGTH + u64::from(self.blob_count) * INDEX_SIZE
    }

    fn read_at(&mut self, position: u64, length: usize) -> Result<Vec<u8>> {
        self.file
            .seek(SeekFrom::Start(position))
            .map_err(|source| StorageError::Io {
                path: self.path.clone(),
                source,
            })?;
        let mut bytes = vec![0u8; length];
        self.file
            .read_exact(&mut bytes)
            .map_err(|source| StorageError::Io {
                path: self.path.clone(),
                source,
            })?;
        Ok(bytes)
    }

    fn write_at(&mut self, position: u64, bytes: &[u8]) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(position))
            .map_err(|source| StorageError::Io {
                path: self.path.clone(),
                source,
            })?;
        self.file
            .write_all(bytes)
            .map_err(|source| StorageError::Io {
                path: self.path.clone(),
                source,
            })
    }

    fn force(&mut self) -> Result<()> {
        self.file.sync_data().map_err(|source| StorageError::Io {
            path: self.path.clone(),
            source,
        })
    }

    fn check_index(&self, index: u32) -> Result<()> {
        if index >= self.blob_count {
            return Err(StorageError::IndexOutOfRange {
                index,
                blob_count: self.blob_count,
            });
        }
        Ok(())
    }

    fn check_writable(&self) -> Result<()> {
        if self.read_only {
            return Err(StorageError::InvalidHeader {
                path: self.path.clone(),
                message: "file is open read-only".to_string(),
            });
        }
        Ok(())
    }
}

fn read_u32(bytes: &[u8]) -> std::result::Result<[u8; 4], StorageError> {
    bytes
        .get(..4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| StorageError::Zstd("internal: short u32 slice".to_string()))
}
