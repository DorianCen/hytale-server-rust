# T24 — Chunk packet stream from world-thread events

**Status:** done (session 14)
**Package:** `hytale-world`, `hytale-server`
**Depends on:** T23 (world thread), T12/T04 (chunk packets)
**Agent fit:** medium

## Goal

Turn a loaded `ChunkColumn` into the exact packet batch a client receives on
join, frame/compress it for the QUIC chunk stream, and expose it through the
world thread. The connection task will be the consumer; this task proves the
event → packet → frame path.

## Deliverables

- `hytale-world::chunk::stream`:
  - `StreamPacket` (packet id, channel, compressed flag, max size, wire payload)
    with `frame(level)` (zstd level 3 default, then `encode_frame`).
  - `ChunkPacketBatch::initial` = `SetColumn` + per-section `SetChunk` and
    `SetChunkEnvironments` (CoreServer's join set).
  - `ChunkPacketBatch::full` = the above plus `SetChunkTintmap` and per-section
    `SetFluids`, in the Java `ChunkPacketDumper` order.
  - `frames(level)`, `payload_bytes()`, `chunk_packet_ids()`.
  - Unit tests: initial/full membership and frame → decode-frame →
    decompress → generated `Wire` decode round-trip.
- `WorldThread` command `ChunkPackets { x, z, full }` and
  `WorldThread::chunk_packets(x, z, full)`.
- Golden gate extended in `chunk_packets_golden.rs`:
  - `full` packets match every manifest packet id and payload byte-for-byte in
    manifest order;
  - `initial` matches exactly the `sentAtInit` subset.
- `world_thread` test consumes `WorldEvent::ChunkLoaded`, encodes the initial
  batch, checks every packet is on the `Chunks` channel and frames the batch.

## Acceptance

```sh
cargo test -p hytale-world --lib chunk::stream
cargo test -p hytale-world --test chunk_packets_golden
cargo test -p hytale-server --test world_thread
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The full batch has the same 46 golden payloads across the two Java chunks.
- The initial batch is exactly the Java join set (21 packets per chunk).
- Frames decode with the protocol decoder and decompress to the same payloads.

## Deviations

- Nothing is written to a connection yet: the batch produces QUIC-ready frames,
  but the network task/stream wiring is the next slice.
- Initial batches omit `SetChunkTintmap`/`SetFluids` to match Java's
  `sentAtInit = false`; `full` carries them for updates.
- Compression level defaults to Java `PacketIO`'s zstd default (3), configurable
  per call.
- Block-edit updates (`SetBlock`, section updates) still need their own packet
  builder/batch.
- No batching across chunks or priority/coalescing; one batch per chunk.

## Follow-ups

- Write the frames on the client's `Chunks` stream from the connection task.
- Teach `client-bot` to receive/decode chunk batches and assert content.
- Add block/section update packets for edits.
- Client-bot auth/setup/game join path.
