# protocol-gen

Protocol IR → Rust source.

The connection slice consumes
[`testdata/golden/server/protocol/connection.json`](../../testdata/golden/server/protocol/connection.json)
and writes the generated protocol files used by `hytale-protocol`:

```text
crates/hytale-protocol/src/generated/
  mod.rs         module exports
  wire.rs        Wire/Packet traits, channels, fixed-block helpers
  connection.rs  connection enums + packets
  common.rs      nested structs and the ParamValue union
```

Regenerate:

```sh
cargo run -p protocol-gen -- connection \
  --ir testdata/golden/server/protocol/connection.json \
  --out crates/hytale-protocol/src/generated
```

CI uses:

```sh
cargo run -p protocol-gen -- --check
```

`--check` regenerates, runs the toolchain `rustfmt`, and diffs; it fails if any
generated file is stale or missing. Generated files are write-only. Edit the
dumper (`tools/protocol-dumper`) or this generator, rerun, and commit both.

## IR requirements

The dumper emits the reachable struct/union closure, not only the packet
fields. A field carries its fixed offset/size, variable order, fixed-block
offset slot, nullable byte/bit, string or array bounds, map key/value types, and
enum/struct/union type name. The generator uses those values directly; it does
not re-parse Java sources.

## Not yet generated

`all` mode (389 packets), `PacketRegistry`, and `ProtocolSettings` are T14.
Until then only the connection family and the structs it references are emitted.
