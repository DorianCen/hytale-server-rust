# T16 — BSON chunk storage and the Java world fixture

**Status:** done (session 6)
**Package:** `hytale-world`, `tools/java-dumper`
**Depends on:** T07 (container), T10c (runtime chunks), T15 (asset registry)
**Agent fit:** medium

## Goal

Persist a runtime `ChunkColumn` in the vanilla Java world layout: a BSON
`ChunkStore` holder document inside an `IndexedStorageFile` region blob, and
prove the codec by decoding a Java-written save.

## Deliverables

- `hytale-world::chunk::storage`:
  - `ChunkCodec` for `Components` → `WorldChunk`, `BlockChunk`, `ChunkColumn`
    sections and `HeightmapColumn`.
  - Java storage layouts for block/filler/rotation, fluid and environment
    sections, including name-keyed palettes, ticking bits, light, change
    counters, the sky-lit stamp, `needsPhysics` and the tint palette.
  - Legacy heightmap `ShortBytePalette` encode/decode.
  - Region helpers and `RegionFile` over `IndexedStorageFile`
    (`chunks/<regionX>.<regionZ>.region.bin`, blob `indexColumn(x & 31, z & 31)`).
- `palette::NameLookup` / `NamedKey`; section storage methods with explicit key
  codecs.
- `StorageDumper` + `tools/java-dumper/run-storage.sh`; committed
  `testdata/golden/server/storage/java-world/` for seed-0 chunks `(0,0)` and
  `(4,-5)`.
- `crates/hytale-world/tests/chunk_storage.rs`: fixture decode against the
  Rust-generated chunk, plus a BSON/region round-trip. Asset-free unit tests
  cover the named-palette codec, legacy heightmap and region indexing.

## Acceptance

```sh
cargo test -p hytale-world
cargo clippy -p hytale-world --all-targets -- -D warnings
cargo fmt --all -- --check
tools/java-dumper/run-storage.sh --check
```

- Every block, filler, rotation, fluid level, per-block environment, tint and
  heightmap in the Java fixture matches the same seed-0 Rust-generated chunk.
- No Java source is modified.
- The fixture is reproducible byte-for-byte.

## Follow-ups

- Player BSON (`DiskPlayerStorageProvider`) and `World.json`.
- Tree heightmap storage (only the legacy backend is implemented; Tree returns
  `Unsupported`).
- One registry for generation, packets and storage.
- Java reload of a Rust-written region (the mutation compatibility gate).
