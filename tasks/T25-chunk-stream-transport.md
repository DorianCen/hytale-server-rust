# T25 — Chunks QUIC stream transport and client-bot receiver

**Status:** done (session 15)
**Package:** `hytale-net`, `client-bot`, `hytale-server`
**Depends on:** T24 (chunk packet batches), T06 (QUIC transport)
**Agent fit:** medium

## Goal

Move the framed chunk batch across a real QUIC `Chunks` stream and teach the
client bot to receive and decode it. This is the transport half of the join
path; the connection state machine still decides when to open the stream.

## Deliverables

- `hytale-net::chunk_stream`:
  - `ChunkStreamWriter::open(connection)` opens a server→client unidirectional
    stream; `write_frame`, `write_frames`, `finish`.
  - `ChunkStreamReader::accept(connection)` accepts it; `next_frame` reads the
    8-byte header, validates the registry id/max size, and zstd-decompresses
    payloads when the packet registry marks them compressed.
  - `ChunkFrame { packet_id, payload }` and `ChunkStreamError`.
- `client-bot`:
  - `ChunkPacketSummary` for `SetColumn`, `SetChunkTintmap`, `SetChunk`,
    `SetChunkEnvironments`, `SetFluids`.
  - `summarize_chunk_packet(frame)` (generated `Wire` decoders).
  - `receive_chunk_stream(&ClientSession)` reads to FIN and returns summaries.
- Loopback tests (all `#[ignore]`d like the QUIC spike):
  - `hytale-net/tests/chunk_stream.rs`: two compressed frames round-trip over a
    real mTLS QUIC uni stream and decode with `SetColumn`/`SetChunk`.
  - `tools/client-bot/tests/chunk_stream.rs`: the bot's receiver decodes a
    stream into `ChunkPacketSummary`.
  - `crates/hytale-server/tests/chunk_transport.rs`: a real
    `ChunkPacketBatch::full` (with tint) frames, crosses QUIC and matches every
    packet id/payload on the reader side.

## Acceptance

```sh
cargo test -p hytale-net --test chunk_stream -- --ignored
cargo test -p client-bot --test chunk_stream -- --ignored
cargo test -p hytale-server --test chunk_transport -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Every ignored loopback test passes.
- The batch transport test sees exactly the same packet ids and uncompressed
  payloads the world built, including the compressed `SetColumn`/`SetChunk`
  frames.

## Deviations

- The stream is a server→client unidirectional QUIC stream. The exact Java
  channel/stream mapping still needs confirmation from a packet trace; this
  matches the architecture's “chunk stream has its own QUIC stream” model.
- One chunk stream per connection; no stream ids, multiplexing or channel
  negotiation yet.
- There is no server session/connection task yet. The tests wire the endpoints
  directly; the session task will open the stream after `Setup`/`Game`.
- `client-bot` does not yet wait for a chunk stream in its live CLI mode; the
  receiver is a library function.
- Flow control and batching across chunks are left at the `quinn` defaults.

## Follow-ups

- Open the chunk stream from the server connection state machine and feed it
  from `WorldThread::chunk_packets`.
- Add chunk-stream reception to the client-bot join flow.
- Confirm the Java stream/channel mapping with a packet trace.
- Add block/section update packets on the same stream.
