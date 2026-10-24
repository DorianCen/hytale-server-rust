# T47 — Disconnect, reconnect and session teardown

**Status:** done (session 35)
**Package:** `hytale-server`, `client-bot`
**Depends on:** T43–T46
**Agent fit:** medium

## Goal

Make the v1 lifecycle reliable: a session releases every resource it owns, a
graceful or server-initiated disconnect never strands the world thread/listener,
and a reconnecting player resumes the saved transform and world edits.

## Deliverables

- `hytale-server::session`:
  - decode `ClientDisconnect` and finish the game loop without a session error;
  - keep idle-timeout and transport-close paths (server-initiated disconnect);
  - load the joining player's transform/appearance by UUID;
  - persist the live transform during teardown, even when the game loop fails;
  - release the session chunk view and unregister the player on every exit path.
- `WorldThread`:
  - per-session registered views (`add_view`/`update_view`/`remove_view`) whose
    loaded-chunk union is shared by every live session;
  - first session view replaces the startup/CLI base view, so the last session
    leaving unloads session-owned chunks;
  - `player_snapshot(uuid)` and `save_player_transform(uuid, transform)` over the
    normal player storage path, preserving all other saved components.
- `chunk_streamer`:
  - `send_view_registered` streams an already-registered view, so a joining
    session does not replace every other session's view.
- `client-bot`:
  - record the self-spawn transform;
  - send `ClientDisconnect` at the disconnect deadline while still observing;
  - the loopback test covers join → move → edit → disconnect → reconnect, plus
    an idle-timeout teardown and a second join after it.
- Tests:
  - registered views share chunks and unload only when the last viewer leaves;
  - per-UUID player transform save/reload (including a world restart);
  - reconnect restores the saved position and sees the edited block;
  - idle timeout tears down one session and the listener accepts another.

## Read first

- `crates/hytale-server/src/session.rs`, `world_thread.rs`, `players.rs`,
  `chunk_streamer.rs`, `serve.rs`
- `crates/hytale-world/src/world.rs`, `player_data.rs`, `view.rs`
- `tools/client-bot/src/lib.rs`, `tests/multi_join.rs`
- Java `GamePacketHandler.handleClientDisconnect`, `PlayerListModule`,
  `PlayerStorage`/`TransformComponent`, `ChunkTracker` view release

## Acceptance

```sh
cargo test -p hytale-server
cargo test -p client-bot --test multi_join -- --ignored
cargo test -p client-bot --test auth_join -- --ignored
cargo test -p client-bot --test dev_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A player joins, moves, edits a block, disconnects gracefully, reconnects with
  the same UUID, and gets the saved transform/edited block back.
- An idle session is torn down and a new client can still join.
- Entity/player-list removal reaches the remaining clients.
- No view/player entry survives teardown, including error paths.

## Deviations

- Player saves still store only the `Transform` update; inventory/held-item live
  state remains T49. Other existing components are preserved unchanged.
- The chunk event queue is still world-wide. Per-session view registration fixes
  chunk ownership/unloading, but a session can drain another session's
  `ChunkLoaded`/`ChunkUnloaded` event. Filtering events per view is follow-up
  work once multiple players regularly edit/view different regions.
- Chunks are released from memory on the last view leaving; the small startup
  base view is retained only for non-session callers and is dropped when the
  first session view registers.
- The server closes the QUIC connection after sending `ServerDisconnect`; it
  does not wait for a client acknowledgement packet (Java has no such packet).
- Reconnect while the previous session is still registered creates a second
  player entry; duplicate-identity takeover/kick is not implemented.

## Follow-ups

- T48 replays the real-client join/disconnect/reconnect trace.
- T49 persists inventory and held-item state alongside the transform.
- T62 adds rate limits/backpressure for broadcasts and duplicate identities.
- T63 covers soak testing of repeated join/leave cycles.
