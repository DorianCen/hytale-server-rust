# T02 — Protocol source inventory

**Status:** done (session 1)
**Agent fit:** high (script over files; no large context)
**Depends on:** T01
**Owns:** `tools/protocol-dumper/**`, workspace `members` entry for the tool

## Goal

Build the first stage of the protocol pipeline: a deterministic inventory of
every generated protocol class, with packet ids, directions, channels,
compression flags, and size constants. This proves the source tree can be
machine-read before writing a full field parser.

## Read

- `Protocol/target/generated-sources/java/com/hypixel/hytale/protocol/ProtocolSettings.java`
- `.../PacketRegistry.java`
- `.../Packet.java`, `.../ToClientPacket.java`, `.../ToServerPacket.java`
- `.../NetworkChannel.java`
- A few sample classes: `Connect.java`, `Ping.java`, `Notification.java`
- [`GOLDEN_DATA.md`](../GOLDEN_DATA.md) §3.1

Do not read all generated files in the editor. The tool reads them.

## Work

1. Create a Rust binary crate at `tools/protocol-dumper` and add it to the
   workspace members.
2. CLI:

   ```text
   protocol-dumper inventory --source <protocol-dir> --out <file.json>
   ```

3. Walk the source tree. For each `*.java`:
   - `enum` if it declares `public enum`;
   - `packet` if it implements `Packet` and `ToClientPacket`/`ToServerPacket`;
   - `struct` otherwise.
4. For packets, extract:
   - class name and package;
   - `PACKET_ID`;
   - direction (`ToClient`/`ToServer`);
   - `getChannel()` return value (`NetworkChannel.*`);
   - `IS_COMPRESSED`;
   - `NULLABLE_BIT_FIELD_SIZE`, `FIXED_BLOCK_SIZE`, `VARIABLE_FIELD_COUNT`,
     `VARIABLE_BLOCK_START`, `MAX_SIZE`.
5. Emit stable, sorted JSON. Include `meta`: source path (relative to repo),
   generator version, counts by kind/direction/channel.
6. Compare counts with `ProtocolSettings`: 389 packets, 445 structs, 177 enums.
   If the parsed counts differ, report the exact classes in a `discrepancies`
   array rather than silently passing.
7. Add unit tests with a tiny fixture directory containing one packet, one
   struct, one enum, plus a malformed file.

## Out of scope

- No field parsing, no IR offsets, no Rust codegen.
- No changes to `crates/hytale-protocol`.
- No Java modifications.

## Acceptance

```sh
cargo run -p protocol-dumper -- inventory \
  --source .hytale-source/hytale-shared-source/HytaleServer/Protocol/target/generated-sources/java/com/hypixel/hytale/protocol \
  --out /tmp/protocol-inventory.json
jq '.meta' /tmp/protocol-inventory.json
cargo test -p protocol-dumper
cargo clippy -p protocol-dumper --all-targets -- -D warnings
cargo fmt -p protocol-dumper -- --check
```

`jq` shows the expected counts or a documented discrepancy list.

## Handoff

Report parsed counts, discrepancies, JSON schema, and the exact command.
