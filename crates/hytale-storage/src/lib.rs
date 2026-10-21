//! `hytale-storage` — Java-compatible persistent storage.
//!
//! This crate ports the vanilla `IndexedStorageFile` container format (header, blob
//! index table, segment chain, zstd payloads) together with the BSON chunk and
//! player component codecs and the backup/recovery rules. It is independent of the
//! asset and world layers so that formats can be tested in isolation.

#![deny(clippy::all)]

pub mod indexed_storage_file;

pub use indexed_storage_file::{
    IndexedStorageFile, StorageError, BLOB_HEADER_LENGTH, DEFAULT_BLOB_COUNT,
    DEFAULT_COMPRESSION_LEVEL, DEFAULT_SEGMENT_SIZE, HEADER_LENGTH, INDEX_SIZE, MAGIC,
    MAGIC_LENGTH, UNASSIGNED_INDEX, VERSION,
};
