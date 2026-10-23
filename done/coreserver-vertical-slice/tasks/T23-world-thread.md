# T23 — World thread, view-distance tracking and demand scheduling

**Status:** done (session 13)
**Package:** `hytale-world`, `hytale-server`
**Depends on:** T22 (demand loading)
**Agent fit:** medium

## Goal

Move chunk demand onto a dedicated world thread, order it nearest-first, cancel
work when the view changes, unload chunks that leave the view, and expose the
events a client chunk stream will consume.

## Deliverables

- `hytale-world::view`:
  - `ChunkView` (center chunk + square radius) with `desired()` and
    `plan(loaded)`.
  - `ChunkPlan` with `loads` ordered by squared distance from the center and
    `unloads` ordered by X/Z.
  - Unit tests for radius 0, ring priority, out-of-view unloads, view
    replacement, and negative centers.
- `World::loaded_chunk_coordinates()` and
  `World::unload_chunk(codec, x, z)` (saves dirty chunks before dropping them).
- `hytale-server::world_thread::WorldThread`:
  - owns `WorldSave`, `WorldGenerator` and the pending queue on one OS thread
    named `hytale-world`;
  - commands: `SetView`, `Tick`, `LoadChunk`, `SetBlock`, `Save`, `Shutdown`;
  - `SetView` replaces the pending queue (cancelling loads outside the new view),
    unloads chunks that left, and orders the new work;
  - `Tick` loads/generates at most `chunks_per_tick` chunks and emits
    `WorldEvent::ChunkLoaded` / `ChunkUnloaded`;
  - `LoadChunk` explicitly loads/generates a chunk outside the view (edits,
    teleports);
  - `WorldStartReport`, `ViewReport`, `TickReport`, `LoadOutcome`.
- `hytale-server world-boot` now drives the thread: spawn, drain ticks, load the
  edit chunk, edit, shutdown. New `--chunks-per-tick` flag.
- Tests:
  - `crates/hytale-server/tests/world_thread.rs` ticks a radius-0 view, moves the
    view one chunk, asserts the old chunk unloads and the new one generates;
  - the existing `world_boot`/`headless` tests cover the thread-driven boot.

## Acceptance

```sh
rm -rf target/save-smoke
cargo run -q -p hytale-server -- headless --save target/save-smoke --chunk 4,-5
cargo run -q -p hytale-server -- world-config \
    --input testdata/golden/server/world-config/config.json --save target/save-smoke
cargo run -q -p hytale-server -- player-data \
    --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
    --save target/save-smoke
cargo run -q -p hytale-server -- world-boot --save target/save-smoke \
    --view-radius 1 --chunks-per-tick 4 --set-block 129,250,-159,4
tools/java-dumper/run-save-check.sh target/save-smoke 4 -5 129 250 -159 Debug_Test_Block
cargo test -p hytale-server --test world_thread
```

- `world-boot` reports `stored=1 requested=9 loaded=1 generated=9 saved=10`.
- The whole-save Java gate passes and a generated region still reads in Java.
- `cargo test -p hytale-server --test world_thread` passes.

## Deviations

- Generation is synchronous inside the world thread. A `SetView` that arrives
  while a chunk is generating is handled after it finishes; "cancellation"
  covers queued chunks, not an in-flight generator call.
- `Tick` processes a fixed budget of chunks per command; the live loop will
  drive it from its tick schedule.
- The view is a square. View-distance shape, per-player views and priority aging
  are not implemented.
- Unloading saves dirty chunks synchronously on the world thread.
- No network consumer yet: `WorldEvent` is the seam the chunk stream will read.

## Follow-ups

- Drive `Tick` from a real world tick loop and add async generation completion.
- Stream `WorldEvent::ChunkLoaded` to a player as chunk packets.
- Per-player views and a non-square view-distance shape.
- Client-bot auth/setup/game with the packet-trace fixture.
