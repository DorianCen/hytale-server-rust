# T26 — Live chunk session bridge and client-bot stream mode

**Status:** done (session 16)
**Package:** `hytale-server`, `client-bot`
**Depends on:** T23 (world thread), T24 (packet batch), T25 (QUIC chunk stream)
**Agent fit:** medium

## Goal

Connect the real world thread to a real QUIC `Chunks` stream and drive it from
the client bot, producing the first end-to-end generated-chunk delivery.

## Deliverables

- `hytale-server::chunk_streamer`:
  - `send_view(thread, writer, center, radius, compression_level)` sets the
    view, ticks the world thread until the queue is empty, then sends every
    loaded chunk in the view nearest-first as initial packet batches.
  - `send_chunk` writes one batch and accumulates a `ChunkStreamReport`
    (`chunks`, `packets`, `frames`, `payload_bytes`).
  - `ChunkSessionError` covers world-thread, packet-encoding and transport
    failures.
- `WorldThread::loaded_chunks()` command (the initial snapshot does not depend
  on event ordering and does not double-send preloaded chunks).
- `WorldThread` is now `Sync` (event receiver behind a mutex) so a connection
  task can hold `&WorldThread` across `.await`.
- `hytale-server::chunk_serve`:
  - `serve_connection(thread, connection, center, radius, level)` opens the
    stream, sends the view and finishes the stream.
  - `run(ChunkServeOptions)` builds a tokio runtime, spawns the world thread,
    binds a QUIC endpoint, accepts one client and streams the view.
  - `ChunkServeOptions`/`ChunkServeReport`/`ChunkServeError`.
- CLI `hytale-server chunk-serve [--bind] [--save] [--view-radius]
  [--chunks-per-tick] [--compression-level] ...` — a transport demo without the
  auth/setup state machine.
- `client-bot --chunks`: after `Connect`, receives the `Chunks` stream with the
  T25 decoder and prints `ChunkPacketSummary` lines.
- `crates/hytale-server/tests/chunk_serve.rs` (`#[ignore]`, needs the pack):
  WorldThread → `serve_connection` → QUIC → `ChunkStreamReader`, asserting one
  `SetColumn`, ten `SetChunk`, ten `SetChunkEnvironments` (21 frames) for a
  radius-0 view.

## Acceptance

```sh
rm -rf target/chunk-serve-smoke
cargo run -p hytale-server -- chunk-serve \
    --save target/chunk-serve-smoke --bind 127.0.0.1:5521 --view-radius 0
# in another terminal:
cargo run -p client-bot -- --host 127.0.0.1:5521 --chunks --timeout 60

cargo test -p hytale-server --test chunk_serve -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The server reports `streamed 1 chunks (21 packets, 21 frames, …)`.
- The bot prints 21 decoded chunk packet summaries and exits 0.
- The loopback test asserts the exact packet-id counts and frame totals.

## Deviations

- This is a transport demo, not the join flow: it accepts the first QUIC
  connection and opens the chunk stream immediately, with no
  Initial/Authentication/Setup/Game packet exchange.
- One client, one connection, one view per process. Incremental view updates
  are not resent after the snapshot (the `WorldEvent` path exists for that).
- The initial snapshot is built from the thread's loaded set after the demand
  queue drains; queued events are discarded so preloaded chunks are not sent
  twice.
- `ChunkServeOptions::run` constructs its own tokio runtime and cannot be called
  from inside an async runtime; tests use `serve_connection`.
- The spike still accepts any client/server certificate (no JWT binding check).
- The view is square and the server does not send unloads/`ClearChunks`.

## Follow-ups

- Add the connection/session state machine and open the chunk stream after
  `Setup`/`Game`.
- Send incremental `ChunkLoaded` events after player movement; send unload
  packets/`ClearChunks`.
- Drive `client-bot` through auth/setup and keep the chunk receiver attached.
- Confirm the Java stream/channel mapping with a packet trace.
