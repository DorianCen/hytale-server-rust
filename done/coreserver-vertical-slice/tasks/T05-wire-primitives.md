# T05 — Wire primitives and framing

**Status:** done (session 1)
**Agent fit:** high (isolated format code with known vectors)
**Depends on:** T01
**Owns:** `crates/hytale-protocol/src/io.rs`,
`crates/hytale-protocol/src/frame.rs`,
`crates/hytale-protocol/tests/io.rs`,
`crates/hytale-protocol/Cargo.toml`

## Goal

Implement the hand-written wire layer used by every generated packet: varints,
fixed primitives, UUIDs, nullable bitsets, bounded strings/arrays, packet
framing, and zstd payloads.

## Read

- `Protocol/src/main/java/com/hypixel/hytale/protocol/io/VarInt.java`
- `.../PacketIO.java` sections: fixed reading, fixed/variable strings, UUID,
  compression, framed packet I/O
- `.../netty/PacketEncoder.java` and `PacketDecoder.java` for framing rules
- [`ARCHITECTURE.md`](../ARCHITECTURE.md) §3

Do not read the generated packet classes for this task.

## Work

1. Define a `Reader`/`Writer` abstraction over `bytes::Buf`/`BytesMut` (or a
   small cursor). All reads are bounds-checked and return `ProtocolError`.
2. Implement:
   - varint encode/decode, max 5 bytes, non-negative only;
   - `i8/u8/i16/u16/i32/u32/i64/u64/f32/f64/bool` little-endian;
   - UUID as two big-endian `i64`;
   - fixed ASCII/UTF-8 strings with zero padding and max length;
   - variable UTF-8 strings with varint length and max length;
   - `Option<T>` via a nullable-bitset helper (read/write bit index);
   - arrays/`Vec<T>` with varint length and max element count.
3. Implement framing:
   - `[u32 LE payload length][u32 LE packet id][payload]`;
   - reject negative/oversized length, unknown id (caller supplies the
     allowed max), and truncated payloads;
   - zero-length payload is valid.
4. Implement zstd compress/decompress with a maximum decompressed size and a
   content-size check. Compressed packet payloads are the zstd frame only; the
   frame header carries the uncompressed size.
5. Unit tests:
   - varint boundary values `0, 127, 128, 16383, 16384, 2_097_151,
     2_097_152, 268_435_455, 268_435_456, i32::MAX`;
   - every fixed type's bytes;
   - UUID byte order;
   - nullable bit positions 0..7 across bytes;
   - string/array bounds and malformed lengths;
   - frame round-trip and rejection cases;
   - zstd round-trip and oversize rejection.

## Out of scope

- No packet structs, no registry, no Java fixtures.
- No QUIC.

## Acceptance

```sh
cargo test -p hytale-protocol
cargo clippy -p hytale-protocol --all-targets -- -D warnings
cargo fmt -p hytale-protocol -- --check
```

All tests pass; every invalid input returns `Err`, never panics.

## Handoff

Report the public API (`Reader`, `Writer`, `ProtocolError`, frame functions) and
any deliberate deviation from the Java helper.
