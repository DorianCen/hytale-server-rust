# Java world save fixture

Produced by `com.hypixel.hytale.tools.golden.StorageDumper` via
`tools/java-dumper/run-storage.sh`. Do not edit these files by hand.

- `chunks/0.0.region.bin`: Java `IndexedStorageFile`, chunk `(0,0)` at blob 0.
- `chunks/0.-1.region.bin`: Java `IndexedStorageFile`, chunk `(4,-5)` at blob
  868 (`indexColumn(4, 27)`).
- `manifest.json`: seed, producer metadata and the chunk → region/index map.

Every blob is the real `ChunkStore.REGISTRY.serialize(holder)` document written
by `BsonUtil.writeToBytes`. The holder contains `WorldChunk`, `BlockChunk`,
`ChunkColumn` (sections with `Block`, `Fluid`, `Environment`) and
`HeightmapColumn` (legacy backend). The section payloads are the Java storage
layouts, not network layouts: palette keys are asset names, filler/rotation keys
are big-endian numbers, and the legacy heightmap is the packed 10-bit
`ShortBytePalette`.

Regenerate deterministically:

```sh
tools/java-dumper/run-storage.sh
tools/java-dumper/run-storage.sh --check
```

The Rust gate is `crates/hytale-world/tests/chunk_storage.rs`. It decodes both
chunks and compares every block/filler/rotation, fluid level, per-block
environment, tint and heightmap against the same seed-0 chunk generated in Rust.
