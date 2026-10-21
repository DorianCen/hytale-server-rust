# T14 — Full protocol codegen

**Status:** todo
**Agent fit:** medium (mechanical, but large output; split by family if needed)
**Depends on:** T04
**Owns:** `tools/protocol-gen/**`, `crates/hytale-protocol/src/generated/**`,
`crates/hytale-protocol/tests/registry.rs`

## Goal

Extend the connection-only generator to the full protocol: 389 packets, 445
structs, 177 enums, with a complete registry. This makes the wire layer usable
by assets, auth, setup, and world streaming.

## Read

- `protocol.json` from T02/T03 (all classes)
- `PacketRegistry.java` for id/channel/max-size ordering
- `ProtocolSettings.java`
- `crates/hytale-protocol/src/generated/connection.rs` (T04 style)
- [`PORTING.md`](../PORTING.md) §4

Do not read all generated Java files. The tool reads them; inspect only failures.

## Work

1. Generalize the IR parser from `packets/connection` to all packet families and
   all struct/enum files.
2. Handle the constructs that appear in the generated sources:
   - nullable bitfields;
   - fixed block + variable block offsets;
   - fixed ASCII/UTF-8 strings;
   - varint strings/arrays;
   - nested structs, arrays of structs, enums, unions (interfaces/abstract
     classes with type tags);
   - fixed-size arrays;
   - `computeSize` and `serialize` ordering.
3. Generate:
   - one module per packet family;
   - a `Packet` trait implementation for every packet;
   - `PacketRegistry` with id → type, direction, channel, max size;
   - `ProtocolSettings` constants;
   - `to_client`/`to_server` dispatch helpers.
4. `--check` must regenerate and diff.
5. Tests:
   - all generated packets compile;
   - registry has 389 entries and no duplicate ids;
   - round-trip a sample from every packet family;
   - decode rejects wrong channel/direction/oversize.
6. If a family is too large for one branch, split it into `T14a` (setup/auth),
   `T14b` (world/chunks), `T14c` (interface/inventory), and keep the generator
   shared.

## Out of scope

- No Java byte fixtures yet; T11/T12 supply those.
- No packet handlers or session logic.
- No hand edits to generated files.

## Acceptance

```sh
cargo run -p protocol-gen -- all --ir protocol.json --out crates/hytale-protocol/src/generated
cargo run -p protocol-gen -- --check
cargo test -p hytale-protocol
cargo clippy -p hytale-protocol --all-targets -- -D warnings
cargo fmt -p hytale-protocol -- --check
```

All 389 packets compile and are registered.

## Handoff

Report generated counts, unsupported constructs, and any family split.
