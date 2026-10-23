# T10a — Section palette types

**Status:** done (session 1)
**Agent fit:** high (isolated data-structure + format work)
**Depends on:** T01
**Owns:** `crates/hytale-world/src/chunk/palette/**`,
`crates/hytale-world/tests/palette.rs`

## Goal

Implement the four palette types used by every 32³ chunk section: `Empty`,
`HalfByte`, `Byte`, `Short`. This is the foundation for `BlockSection`,
`FluidSection`, and the network/storage byte layouts.

## Read

- `CoreServer/.../world/chunk/section/palette/AbstractSectionPalette.java`
- `EmptySectionPalette.java`, `HalfByteSectionPalette.java`,
  `ByteSectionPalette.java`, `ShortSectionPalette.java`
- `PaletteTypeEnum.java`
- `Protocol/.../packets/world/PaletteType.java`
- [`ARCHITECTURE.md`](../ARCHITECTURE.md) §5

Read only the public methods and the serialize/deserialize sections; skip the
occupancy/statistics helpers unless needed for compilation.

## Work

1. Define `PaletteType` with the wire ordinals `Empty=0`, `HalfByte=1`,
   `Byte=2`, `Short=3` (Java serializes the enum ordinal).
2. Define a `Palette` enum or trait with:
   - `get(index) -> i32`, `set(index, id) -> SetResult`;
   - `count`, `contains`, `is_solid`;
   - `promote`/`demote` per Java thresholds (`HalfByte::MAX_SIZE = 16`,
     `Byte::MAX_SIZE = 256`, `Short::MAX_SIZE = 65536`);
   - `serialize_for_packet(writer)`, `serialized_packet_size()`;
   - `serialize(key_serializer, writer)` / `deserialize(key_deserializer, reader)`
     for the storage format, where the key serializer controls the external-id
     width (byte for blocks, short for fillers, byte for rotations).
3. Preserve Java's run-length encoding and palette table order. The packet and
   storage formats are different; implement both and test both.
4. Tests:
   - each palette type gets/sets across the full index range and promotes at the
     correct threshold;
   - `Empty` rejects non-zero sets with `RequiresPromote`;
   - serialize → deserialize round-trip for each type, including a full section
     of one id and alternating ids;
   - palette table order is deterministic for a known insertion sequence.

## Out of scope

- No `BlockSection`, no ticking/light, no chunk column.
- No network packets; test the byte writer directly.

## Acceptance

```sh
cargo test -p hytale-world palette
cargo clippy -p hytale-world --all-targets -- -D warnings
cargo fmt -p hytale-world -- --check
```

Round-trips pass and the palette type byte matches the protocol ordinal.

## Handoff

Report the public API and any format detail that needs a golden fixture in T12.
