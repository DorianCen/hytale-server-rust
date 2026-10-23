# T27 — Incremental view updates on the live chunk stream

**Status:** done (session 17)
**Package:** `hytale-world`, `hytale-server`, `client-bot`
**Depends on:** T26 (live chunk session)
**Agent fit:** medium

## Goal

Stream view changes after the initial snapshot: send newly loaded chunks and
`UnloadChunk` packets when the player/view moves. This is the streaming half that
the real session state machine will drive once it can reach `Game`.

The full Initial/Auth/Setup/Game flow is blocked on the P0/P1 asset packet
encoders and a packet-trace fixture; this task completes the chunk-stream side so
only session sequencing remains.

## Deliverables

- `hytale-world::chunk::stream::unload_chunk(x, z)`:
  - builds a 135 `UnloadChunk` packet on the `Chunks` channel,
  - unit test frames it, decodes the frame and checks the coordinates.
- `hytale-server::chunk_streamer`:
  - `ChunkStreamReport` gained `unloaded`.
  - `send_updates(thread, writer, level)` drains `WorldEvent`s: `ChunkLoaded`
    sends the chunk batch, `ChunkUnloaded` sends `UnloadChunk`.
  - `send_view` forwards unloads caused by its initial `SetView` before the
    snapshot.
- `hytale-server chunk-serve --move-demo`:
  - after the snapshot, moves the view one chunk, generates the new chunk and
    streams the unload + new batch.
- `client-bot`:
  - `ChunkPacketSummary::Unload` (id 135) and its decoder.
- Tests:
  - `crates/hytale-server/tests/chunk_serve.rs` now runs the move demo and
    asserts 2 `SetColumn`, 20 `SetChunk`, 20 `SetChunkEnvironments`, 1
    `UnloadChunk` (43 frames total).
  - `tools/client-bot/tests/chunk_stream.rs` asserts the `Unload` summary.

## Acceptance

```sh
cargo test -p hytale-world --lib chunk::stream
cargo test -p hytale-server --test chunk_serve -- --ignored
cargo test -p client-bot --test chunk_stream -- --ignored

rm -rf target/chunk-serve-move
cargo run -p hytale-server -- chunk-serve \
    --save target/chunk-serve-move --bind 127.0.0.1:5522 --view-radius 0 --move-demo
# another terminal:
cargo run -p client-bot -- --host 127.0.0.1:5522 --chunks --timeout 60
```

- The bot reports `chunk packets: 43` and prints `Unload { x: 0, z: 0 }`.
- The server reports `streamed 2 chunks (43 packets, 43 frames, …)`.
- Workspace tests, clippy and fmt are green.

## Deviations

- Incremental updates are driven by the demo's one explicit view move. Player
  movement packets are not wired yet; there is no per-player view loop.
- Unloads are per-chunk `UnloadChunk`; `UnloadSection` and `ClearChunks` are not
  emitted.
- The demo still has no Initial/Auth/Setup/Game exchange and opens the stream on
  accept.
- The snapshot path discards `ChunkLoaded` events while the queue drains and
  reads the loaded set; incremental updates use the event path.
- No prioritisation/backpressure between event draining and stream writes.

## Follow-ups

- Add the connection/session state machine (blocked on asset packet encoders and
  a packet trace); call `send_view` after `Setup`/`Game`.
- Feed view changes from player movement and stream `send_updates` continuously.
- Add section unloads / `ClearChunks` where appropriate.
- Drive `client-bot` through the real handshake with the trace fixture.
