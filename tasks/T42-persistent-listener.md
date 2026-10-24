# T42 — Persistent listener and multi-session runtime

**Status:** done (session 30)
**Package:** `hytale-server`, `hytale-net`, `client-bot`
**Depends on:** T39, T41
**Agent fit:** low/medium

## Goal

Replace the one-connection demo with the real server loop: one listener, one
universe, many independent sessions.

## Deliverables

- A persistent `serve` command (or equivalent) that owns:
  - the QUIC listener;
  - the universe and its worlds;
  - the shared/cached asset registry and config packets;
  - graceful shutdown.
- One independent session task per connection using the existing join state
  machine, auth modes and view tracking.
- Connection limits, idle/timeout handling, clean disconnect and half-closed
  stream handling.
- Per-session config/tag handoff from the shared cache (no re-encode).
- Keep `chunk-serve` as a debug demo or fold it into `serve --demo`; do not
  break the existing loopback tests.
- A loopback test with two or more clients joining the same world concurrently.

## Read first

- `crates/hytale-server/src/session.rs`
- `crates/hytale-server/src/main.rs` (`chunk-serve`, `world-boot`)
- `crates/hytale-net` listener/connection APIs
- `tools/client-bot/tests/auth_join.rs`

## Acceptance

```sh
cargo test -p hytale-server
cargo test -p client-bot --test auth_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Two bots join concurrently, each with its own 21-chunk view.
- Disconnecting one does not unload the other’s chunks or stop the listener.
- A new connection can join after a disconnect without restarting the process.
- Shutdown closes sessions, stops world threads and flushes state.

## Deviations

- Other-player entity tracking is T45, not this task; sessions are independent
  but do not yet see each other.
- TLS/OAuth production authority remains the local EdDSA profile until the auth
  backlog is scheduled.
- `serve` binds one selected/default world per listener. The universe may catalog
  more worlds and `UniverseRuntime` can start them, but session world selection
  needs protocol/routing work after T45.
- Sessions share the selected `WorldThread`'s cached `ConfigAssets`; each world
  thread still loads its own asset registry. Cross-world asset sharing is a
  later optimization.
- On shutdown the listener closes active QUIC connections, waits up to
  `--shutdown-timeout` for session tasks, aborts any remaining task, then stops
  world threads (which save chunks and config).
- Idle timeout applies to the live game loop; the existing handshake and
  world-join deadlines remain.
- `chunk-serve` is kept unchanged as the single-connection transport demo.

## Follow-ups

- T45 adds cross-session visibility; T44/T47 add player appearance and
  disconnect/reconnect state.
- A process-level shared `AssetRegistry` for multi-world listeners belongs with
  the universe-level config cache.

