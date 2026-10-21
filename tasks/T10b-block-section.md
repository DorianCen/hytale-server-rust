# T10b — BlockSection and section serialization

**Status:** todo
**Agent fit:** medium (one large Java class, but mostly mechanical)
**Depends on:** T10a
**Owns:** `crates/hytale-world/src/chunk/section/block.rs`,
`crates/hytale-world/src/chunk/section/fluid.rs`,
`crates/hytale-world/src/chunk/section/environment.rs`,
`crates/hytale-world/tests/section_*.rs`

## Goal

Port the per-section data that is shared by network and storage: block palette +
filler + rotation, fluid palette + nibble levels, and the environment runs.
Match Java's serialized byte layout.

## Read

- `CoreServer/.../world/chunk/section/BlockSection.java` (CODEC, serialize,
  deserialize, `serializeForPacket`)
- `.../section/FluidSection.java`
- `.../section/EnvironmentSection.java`
- `.../section/palette/*` from T10a
- `Protocol/.../packets/world/PaletteType.java`
- [`ARCHITECTURE.md`](../ARCHITECTURE.md) §5

Read one class at a time; do not load all three into context simultaneously.

## Work

1. `BlockSection`:
   - block palette, filler palette (`u16` keys), rotation palette (`u8` keys);
   - ticking bit sets, local/global light arrays, change counters, sky-lit stamp;
   - `get/set_block`, `get_filler`, `get_rotation`;
   - `serialize_for_packet` and `serialize` (storage) with the exact layout from
     Java, including the block-migration asset count/version and palette type
     bytes;
   - `deserialize` accepting the Java storage layout.
2. `FluidSection`: fluid palette + nibble level data (`SIZE_BLOCKS >> 1`),
   zero-level handling, serialize/deserialize, packet format.
3. `EnvironmentSection`: environment id runs, column/base + overrides, serialization.
4. Keep one code path for packet and storage serialization; do not duplicate the
   palette logic.
5. Tests:
   - round-trip each section type through storage bytes;
   - round-trip through packet bytes;
   - mixed palette promotion (Empty → HalfByte → Byte → Short) preserves values;
   - filler/rotation values survive;
   - fluid levels 0..15 survive;
   - environment overrides survive.

## Out of scope

- No `ChunkColumn`, no heightmaps/lighting manager, no chunk store.
- No BSON wrapper; raw section bytes only.
- No Java fixture yet (T12).

## Acceptance

```sh
cargo test -p hytale-world section
cargo clippy -p hytale-world --all-targets -- -D warnings
cargo fmt -p hytale-world -- --check
```

Round-trips pass. Note any byte-layout uncertainty for T12’s golden fixture.

## Handoff

Report the API, the exact palette type byte mapping, and the sections that still
need a Java byte fixture.
