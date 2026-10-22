# protocol-gen

Protocol IR → Rust source.

The generator consumes the checked-in IR under
[`testdata/golden/server/protocol/`](../../testdata/golden/server/protocol/) and
writes the generated protocol files used by `hytale-protocol`:

```text
crates/hytale-protocol/src/generated/
  mod.rs         module exports
  wire.rs        Wire/Packet traits, channels, vector helpers
  common.rs      all structs, unions and enums
  <family>.rs    one module per packet family (connection, world, player, ...)
  registry.rs    PacketInfo table, lookup/validation, ProtocolSettings
```

Regenerate the full protocol:

```sh
cargo run -p protocol-gen -- all \
  --ir testdata/golden/server/protocol/protocol.json \
  --out crates/hytale-protocol/src/generated
```

CI uses:

```sh
cargo run -p protocol-gen -- --check
```

`--check` regenerates the full protocol, runs the toolchain `rustfmt`, and
diffs; it fails if any generated file is stale or missing. Generated files are
write-only. Edit the dumper (`tools/protocol-dumper`) or this generator, rerun,
and commit both.

## IR requirements

The dumper emits the full protocol plus the reachable type closure: packets,
structs, union bases/subtypes and enums. A field carries its fixed offset/size,
variable order, fixed-block offset slot, nullable byte/bit, string or array
bounds, map key/value types, enum/struct/union type name, bit-packed boolean
location and fixed-array size. Inherited fields are flattened onto their
concrete subtype in wire order.

The generator uses those values directly; it does not re-parse Java sources.

## Constructs handled

- nullable bitfields and nullable fixed/variable fields;
- fixed and variable blocks with canonical offset slots;
- fixed ASCII/UTF-8 strings and varint strings;
- varint arrays (primitives, strings, UUIDs, enums, structs, maps) and fixed
  arrays (`SetColumn.heightmap`);
- maps with string, integer, float, enum and struct keys and primitive,
  string, struct or array values;
- unions/interfaces with type ids and `Box` indirection where a cycle requires
  it;
- bit-packed booleans;
- JOML `Vector2f`/`Vector3f`/`Vector4f`/`Quaternionf` fields.

`registry.rs` exposes `PACKETS`, `packet_info`, `validate_packet`,
`check_all_samples` and `check_family_samples`; `tests/registry.rs` and
`tests/full.rs` exercise them.

## Connection-only mode

The original connection slice is retained for bisecting parser changes:

```sh
cargo run -p protocol-gen -- connection \
  --ir testdata/golden/server/protocol/connection.json \
  --out crates/hytale-protocol/src/generated
```

It writes only `connection.rs`, `common.rs`, `wire.rs` and `mod.rs`, so it is
not a replacement for `all`.
