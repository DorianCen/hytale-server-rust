# T03 — Connection packet IR

**Status:** done (session 1)
**Agent fit:** medium (strict parser over 13 small generated files)
**Depends on:** T02
**Owns:** `tools/protocol-dumper/src/connection*.rs`,
`tools/protocol-dumper/tests/connection_ir.rs`

## Goal

Produce the first field-level IR: every packet and enum under
`packets/connection/`, with fields in wire order, fixed offsets, nullability,
and variable-field order. Validate it against the constants already visible in
`Connect.java`.

## Read

- `packets/connection/*.java` (13 files; read all, they are small)
- `Connect.java` constants and static getters as the reference
- `Ping.java`, `Pong.java`, `NetworkTick.java`, `ServerDisconnect.java`
- `ClientDisconnect.java`, `InsecurePlayerOptions.java`,
  `RequestInsecurePlayerOptions.java`
- `Protocol/src/main/java/com/hypixel/hytale/protocol/io/PacketIO.java` only for
  the primitive names (`readVarString`, `readFixedAsciiString`, `PROTO_INT`, …)
- [`GOLDEN_DATA.md`](../GOLDEN_DATA.md) §3.1

## Work

1. Add a `connection` mode to `protocol-dumper`:

   ```text
   protocol-dumper connection --source <packets/connection> --out connection.json
   ```

2. Emit for each class:
   - class name, packet id, channel, compression, size constants;
   - fields in serialization order, each with:
     - name and declared type;
     - `nullable` and nullable-bit index (if present);
     - `fixedOffset` for fixed fields;
     - `variableOrder` for variable fields;
     - string encoding and max length;
     - array element type and max length;
     - enum/nested-type name.
3. Parse enums from `ClientType`, `DisconnectType`, `PongType`,
   `QuicApplicationErrorCode`, `ClientDisconnectReason` with their numeric
   values and names.
4. Validate `Connect`:
   - `PACKET_ID == 0`;
   - `FIXED_BLOCK_SIZE == 30`;
   - `VARIABLE_BLOCK_START == 46`;
   - `VARIABLE_FIELD_COUNT == 4`;
   - fields match the Java constructor/serialize order.
5. Keep the parser strict: if a file does not match the generated grammar, fail
   with the file and line, not a silent skip.
6. Add tests against the real connection directory (relative path from repo
   root) and a tiny synthetic fixture for an unexpected construct.

## Out of scope

- No auth/setup/other packet families.
- No Rust packet structs.
- No byte encoding; that is T04.

## Acceptance

```sh
cargo run -p protocol-dumper -- connection \
  --source .hytale-source/hytale-shared-source/HytaleServer/Protocol/target/generated-sources/java/com/hypixel/hytale/protocol/packets/connection \
  --out /tmp/connection.json
jq '.packets[] | select(.name=="Connect")' /tmp/connection.json
cargo test -p protocol-dumper
cargo clippy -p protocol-dumper --all-targets -- -D warnings
cargo fmt -p protocol-dumper -- --check
```

The `Connect` object shows the expected constants, four variable fields, and
ordered fields.

## Handoff

Report the JSON schema and any packet that needed a manual exception.
