# Protocol golden fixtures

Produced by `tools/protocol-dumper` from the generated Java protocol sources.
Do not edit these files by hand.

| File | Command | Contents |
|---|---|---|
| `inventory.json` | `protocol-dumper inventory --source <protocol-dir> --out inventory.json` | every generated class with kind, packet id, direction, channel, compression flag, and size constants |
| `connection.json` | `protocol-dumper connection --source <.../packets/connection> --out connection.json` | field-level IR for the 8 connection packets and 5 enums |

Regenerate:

```sh
PROTOCOL=.hytale-source/hytale-shared-source/HytaleServer/Protocol/target/generated-sources/java/com/hypixel/hytale/protocol
cargo run -p protocol-dumper -- inventory --source "$PROTOCOL" --out testdata/golden/server/protocol/inventory.json
cargo run -p protocol-dumper -- connection --source "$PROTOCOL/packets/connection" --out testdata/golden/server/protocol/connection.json
```

## Inventory counts

`inventory.json` parses **389 packets**, matching `ProtocolSettings.PACKET_COUNT`.
The source tree contains 443 classes with a generated `computeSize()` and 175
top-level enums; `ProtocolSettings` declares 445 structs and 177 enums. The
delta is recorded in `meta.discrepancies` rather than hidden:

- 13 declarations are Java interfaces (`Interaction`, `MusicContainer`,
  `Selector`, `MapMarkerComponent`, …), not wire structs.
- 5 declarations are constant/registry helpers (`CameraSequenceFlags`,
  `DebugFlags`, `ModelTransformFields`, `ProtocolSettings`, `PacketRegistry`).
- `PacketRegistry.PacketDirection` is a nested enum (+1), not a separate file;
  the remaining enum delta is source/schema bookkeeping rather than a skipped
  generated file.

No packet is silently skipped; all 389 are present in `classes`.

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
