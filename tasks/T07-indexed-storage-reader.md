# T07 — IndexedStorageFile reader/writer

**Status:** done (session 1)
**Agent fit:** medium (one 1.1k-line Java class; bounded format work)
**Depends on:** T01
**Owns:** `crates/hytale-storage/src/indexed_storage_file.rs`,
`crates/hytale-storage/tests/indexed_storage.rs`,
`crates/hytale-storage/Cargo.toml`

## Goal

Port the vanilla `IndexedStorageFile` v1 format used by the default chunk
storage provider. Read, write, remove, flush, and enumerate blobs with the same
on-disk layout and zstd payloads as Java.

## Read

- `Storage/src/main/java/com/hypixel/hytale/storage/IndexedStorageFile.java`
- `CoreServer/.../universe/world/storage/provider/IndexedStorageChunkStorageProvider.java`
  (how region files and blob indexes are used)
- [`ARCHITECTURE.md`](../ARCHITECTURE.md) §7
- [`GOLDEN_DATA.md`](../GOLDEN_DATA.md) §3.4

Do not read the chunk BSON codecs yet; this task is the file container only.

## Work

1. Implement the header exactly:
   - magic `"HytaleIndexedStorage"` (20 bytes);
   - `u32` version `1`, `u32` blob count, `u32` segment size;
   - **big-endian**: the Java `ByteBuffer` default order. Session 1 confirmed
     this against a real Java-produced fixture; the earlier "little-endian"
     note in this task was wrong.
2. Implement the blob index table: `blob_count * 4` big-endian `i32` segment
   indexes immediately after the header. `0` = unassigned.
3. Implement segment addressing and blob headers:
   - segments start at index 1;
   - blob header is `[u32 BE source_length][u32 BE compressed_length]` followed
     by the zstd frame;
   - blobs occupy `ceil((header + compressed_len) / segment_size)` contiguous
     segments.
4. API:

   ```rust
   pub struct IndexedStorageFile { /* ... */ }

   impl IndexedStorageFile {
       pub fn create(path: &Path, blob_count: u32, segment_size: u32) -> Result<Self>;
       pub fn open(path: &Path, read_only: bool) -> Result<Self>;
       pub fn read_blob(&mut self, index: u32) -> Result<Option<Vec<u8>>>;
       pub fn write_blob(&mut self, index: u32, data: &[u8]) -> Result<()>;
       pub fn remove_blob(&mut self, index: u32) -> Result<()>;
       pub fn flush(&mut self) -> Result<()>;
       pub fn keys(&self) -> Result<Vec<u32>>;
       pub fn used_blob_count(&self) -> u32;
   }
   ```

5. Preserve Java's ordering: write data, force (if flush-on-write), then update
   the index; remove clears the index before reclaiming segments.
6. Free-segment search: first-fit contiguous run starting at segment 1, matching
   Java's `findFreeSegment`.
7. Tests:
   - create a file, write/read/overwrite/remove blobs, reopen, verify;
   - empty and zero-length blobs;
   - blob spanning many segments;
   - invalid magic/version/truncated file returns an error;
   - a Java-produced fixture: if `testdata/golden/server/storage/*.region.bin`
     exists, read its blobs; otherwise generate a synthetic file and assert the
     header/index bytes exactly.

## Out of scope

- BSON chunk documents and component codecs.
- Region file naming / `ChunkUtil` indexing (next task or `hytale-world`).
- RocksDB.

## Acceptance

```sh
cargo test -p hytale-storage
cargo clippy -p hytale-storage --all-targets -- -D warnings
cargo fmt -p hytale-storage -- --check
```

All tests pass; a Java fixture is read if present.

## Handoff

Report the API, format details that were ambiguous, and the fixture status.
