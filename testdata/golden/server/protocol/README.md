# Protocol golden fixtures

Produced by `tools/protocol-dumper` from the generated Java protocol sources.
Do not edit these files by hand.

| File | Command | Contents |
|---|---|---|
| `inventory.json` | `protocol-dumper inventory --source <protocol-dir> --out inventory.json` | every generated class with kind, packet id, direction, channel, compression flag, and size constants |
| `connection.json` | `protocol-dumper connection --source <.../packets/connection> --out connection.json` | field-level IR for the 8 connection packets, 5 enums, and the reachable struct/union closure |
| `protocol.json` | `protocol-dumper all --source <protocol-dir> --out protocol.json` | field-level IR for all 389 packets, 457 struct/union entries, 175 top-level enums, plus `ProtocolSettings` |

Regenerate:

```sh
PROTOCOL=.hytale-source/hytale-shared-source/HytaleServer/Protocol/target/generated-sources/java/com/hypixel/hytale/protocol
cargo run -p protocol-dumper -- inventory --source "$PROTOCOL" --out testdata/golden/server/protocol/inventory.json
cargo run -p protocol-dumper -- connection --source "$PROTOCOL/packets/connection" --out testdata/golden/server/protocol/connection.json
cargo run -p protocol-dumper -- all --source "$PROTOCOL" --out testdata/golden/server/protocol/protocol.json
```

## Inventory counts

`inventory.json` parses **389 packets**, matching `ProtocolSettings.PACKET_COUNT`.
The source tree contains 443 concrete classes with a generated `computeSize()`,
13 abstract polymorphic bases (`Interaction`, `MusicContainer`, `Selector`,
`MapMarkerComponent`, …) and 175 top-level enums; `ProtocolSettings` declares
445 structs and 177 enums. The delta is recorded in `meta.discrepancies` rather
than hidden:

- 5 declarations are constant/registry helpers (`CameraSequenceFlags`,
  `DebugFlags`, `ModelTransformFields`, `ProtocolSettings`, `PacketRegistry`).
- `PacketRegistry.PacketDirection` is a nested enum (+1), not a separate file.
- `all` emits 457 struct entries: 443 concrete structs, 13 abstract union bases
  (the base carries the common fields its subtypes inherit), and the
  `CameraShakeEffect` packet, which is also used as a nested field type. The
  Java `STRUCT_COUNT` counts those bases differently; the Rust registry does not
  depend on the declared number.

No packet is silently skipped; all 389 are present in `classes` and
`protocol.json`.

## Connection IR

`connection.json` is the contract for T04 codegen. `Connect` carries the
validated constants (`PACKET_ID=0`, fixed block 30, variable block 46, four
variable fields) and the exact declared field order:

```text
protocolCrc, protocolBuildNumber, clientVersion, clientType,
identityToken, language, referralData, referralSource
```

Variable offset slots are emitted for multi-field variable blocks (`Connect`,
`InsecurePlayerOptions`) and omitted for the single-field `ServerDisconnect`,
matching the generated Java serializer.

## Full-protocol IR

`protocol.json` is the full field-level contract. It includes every packet,
every concrete struct, every union base and subtype, every enum, inherited
fields flattened onto their concrete subtypes, fixed and variable field
offsets, nullable bits, fixed/variable strings, arrays and maps (including
non-string keys), bit-packed booleans, fixed arrays and JOML vector fields.
`settings` carries the `ProtocolSettings` constants.

## Generated sources

`protocol-gen all` consumes `protocol.json` and writes one module per packet
family, `common.rs` (all structs/unions/enums), `wire.rs` (traits and vector
helpers) and `registry.rs` (389 entries plus `ProtocolSettings`):

```sh
cargo run -p protocol-gen -- all --ir testdata/golden/server/protocol/protocol.json --out crates/hytale-protocol/src/generated
cargo run -p protocol-gen -- --check
```

`--check` regenerates, formats with the toolchain `rustfmt`, and diffs. The
checked-in generated files are write-only; edit the dumper or generator instead.
The connection-only mode is retained for bisecting parser changes:

```sh
cargo run -p protocol-gen -- connection --ir testdata/golden/server/protocol/connection.json --out crates/hytale-protocol/src/generated
```
