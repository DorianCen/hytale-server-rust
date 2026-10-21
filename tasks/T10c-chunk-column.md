# T10c — ChunkColumn and generated-chunk conversion

**Status:** todo
**Agent fit:** medium (integration over T10a/T10b and worldgen-model)
**Depends on:** T10a, T10b
**Owns:** `crates/hytale-world/src/chunk/column.rs`,
`crates/hytale-world/src/chunk/mod.rs`,
`crates/hytale-world/tests/column.rs`

## Goal

Assemble sections into a runtime `ChunkColumn` and implement
`from_generated_chunk`, the one bridge from `worldgen-model::GeneratedChunk` to
the runtime model. No storage or network yet.

## Read

- `CoreServer/.../world/chunk/ChunkColumn.java`
- `CoreServer/.../world/chunk/WorldChunk.java` (flags/keep-alive only)
- `CoreServer/.../world/chunk/BlockChunk.java` (tint/height/environment fields)
- `CoreServer/.../world/chunk/heightmap/HeightmapColumn.java` and
  `TreeHeightmap.java` (structure only)
- `crates/worldgen-model/src/chunk.rs` (all accessors, especially `blocks`,
  `rotation`, `filler`, `tint`, `environment`, `fluid`, `fluid_level`)
- [`ARCHITECTURE.md`](../ARCHITECTURE.md) §5

## Work

1. `ChunkColumn`:
   - 10 sections, lazily allocated like Java (or `Option<Box<Section>>`);
   - block/fluid/environment sections per index;
   - tint, environment column/base, legacy/tree heightmap placeholder, flags;
   - `get/set_block(x, y, z, id, rotation, filler)` and fluid/environment
     accessors with `ChunkUtil`-compatible index math;
   - `section(y)` and iteration in Java order.
2. `from_generated_chunk(&GeneratedChunk) -> ChunkColumn`:
   - copy every block/rotation/filler/fluid/level/environment value;
   - copy tint and environment column;
   - initialize only sections that contain data (mirror Java’s lazy sections);
   - assert the source length matches `SIZE*SIZE*HEIGHT`.
3. Tests:
   - convert a small synthetic `GeneratedChunk` and read back every value;
   - convert a real golden chunk if a loader fixture is available, sample
     blocks/fluids/environments/tint at boundary coordinates;
   - section promotion while converting a mixed-id chunk;
   - index math agrees with `hytale_math::chunk_util` for negative/local coords.

## Out of scope

- No storage BSON, no network packets.
- No lighting propagation, no entity/block components.
- No worldgen changes.

## Acceptance

```sh
cargo test -p hytale-world column
cargo clippy -p hytale-world --all-targets -- -D warnings
cargo fmt -p hytale-world -- --check
```

Conversion is lossless for all `GeneratedChunk` fields that exist today.

## Handoff

Report the column API, the section initialization rule, and any field that has
no runtime home yet.
