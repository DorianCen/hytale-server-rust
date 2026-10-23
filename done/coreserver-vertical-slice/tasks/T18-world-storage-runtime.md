# T18 — Runtime world storage and the Java reload gate

**Status:** done (session 8)
**Package:** `hytale-world`, `hytale-server`, `tools/java-dumper`
**Depends on:** T16 (storage codec), T17 (headless boot)
**Agent fit:** medium

## Goal

Load a saved chunk from a region file into a runtime world, mutate a block, save
it back, and prove the Rust-written file with the real Java codecs. This closes
the reverse half of the storage compatibility gate: the Java fixture proved that
Rust can read Java saves; this task proves that Java can read Rust saves.

## Deliverables

- `hytale-world::world`:
  - `WorldStorage`: save-directory handle, region-file cache,
    `load_chunk`/`save_chunk`/`remove_chunk`, `stored_chunk_coordinates`,
    `flush`.
  - `World`: loaded-chunk map, dirty tracking, `load_chunk`, `insert_chunk`,
    `get_block`/`set_block`, `save_chunk`/`save_all`, `flush`.
- `parse_region_file_name` in `hytale-world::chunk::storage` (Java
  `fromFileName`) so region scans do not re-derive filenames.
- `hytale-server::world_tool`:
  - load assets + P0 registry once;
  - load a stored chunk through `World`/`WorldStorage`;
  - optionally edit one block;
  - save, reload with a fresh `World`, and verify the edited block.
- CLI: `hytale-server load-chunk [--assets-pack] [--save] [--chunk x,z]
  [--set-block x,y,z,id]`.
- `crates/hytale-world/tests/world_storage.rs`:
  - storage round-trip and chunk listing/removal;
  - world load → mutate → save → reload.
- `crates/hytale-server/tests/headless.rs` extends the boot test with
  load → mutate → save → reload through the runtime world.
- Java reverse gate:
  - `ChunkStoreBootstrap` extracts the `AssetModule`/`LegacyModule`/chunk
    component boot from `StorageDumper` (which now reuses it).
  - `WorldStorageProbe` opens the Rust-written region with the real
    `IndexedStorageFile`, `BsonUtil.readFromBuffer`,
    `ChunkStore.REGISTRY.deserialize`, `ChunkColumn`/`BlockSection` and resolves
    the block through the real `BlockType` asset map.
  - `run-reload-check.sh <save-dir> <chunkX> <chunkZ> <blockX> <blockY>
    <blockZ> <expected-name>`.
  - `run.sh` accepts `DUMPER_EXTRA_ARGS` and no longer requires JSON output
    (probe-only entry points print a verdict).

## Acceptance

```sh
rm -rf target/headless-smoke
cargo run -q -p hytale-server -- headless --save target/headless-smoke --chunk 4,-5
cargo run -q -p hytale-server -- load-chunk --save target/headless-smoke --chunk 4,-5 \
    --set-block 129,250,-159,4
tools/java-dumper/run-reload-check.sh target/headless-smoke 4 -5 129 250 -159 Debug_Test_Block
```

- The runtime load/edit/save path preserves every unedited block and the edited
  block after reload.
- The Java probe reads the Rust region and reports `Debug_Test_Block` (id 4) at
  the edited position.
- The storage determinism gate stays green after the bootstrap refactor:
  `tools/java-dumper/run-storage.sh --check`.
- Workspace gates: `cargo test --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`.

## Follow-ups

- `World.json`/`WorldConfig` load and player BSON
  (`DiskPlayerStorageProvider`).
- `WorldStorage::remove_chunk` leaves an empty region file in place; a
  compaction/unlink policy is deferred.
- `stored_chunk_coordinates` opens a separate read-only handle while the write
  cache may hold the same region open. Fine on Linux; revisit if Windows
  support needs exclusive handles.
- Saved-world generation caching (generate only missing chunks) and the live
  world thread.
