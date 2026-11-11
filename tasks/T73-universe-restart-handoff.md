# T73 — Universe restart and multi-world handoff smoke

**Status:** proposed (filed session 99)
**Package:** `hytale-server`, `tools/client-bot`, docs
**Depends on:** T41, T60b, T70
**Agent fit:** low/medium

## Goal

Prove the listener-owned universe survives a real process-level restart and a
live multi-world/portal handoff with more than one world running at once. T70
restarts one `WorldThread`; T60b proves portal transfer; neither covers a
`UniverseRuntime`/`serve` restart.

## Deliverables

- Add an ignored loopback test (for example
  `tools/client-bot/tests/universe_restart.rs`):
  - create/open a universe root and start `Default` plus a second world;
  - join clients to both worlds through one listener;
  - edit blocks, move a player, hold an item and spawn a runtime entity;
  - move one player through a portal into the second world and back;
  - shut the listener and `UniverseRuntime` down;
  - reopen the same universe root, start the same worlds and reconnect;
  - assert world registry/layout, both player documents, block edits, held
    items, NPCs and portal return state.
- Cover listener shutdown while a session is connected and a clean restart
  without stale `PlayerRegistry`/`WorldThread` entries.
- Document the restart procedure and any file-layout assumptions (save configs,
  `universe.json`, world directories) in `NEXT_STEPS.md`/`BENCHMARKS.md`.
- Keep the test asset-backed and `#[ignore]`d, like T70.

## Read first

- `crates/hytale-server/src/universe_runtime.rs`
  (`open`, `create_and_start`, `start_world`, `stop_world`, `shutdown`,
  `open_portal_instance`, `transfer_live_player`, `return_from_instance`)
- `crates/hytale-server/src/serve.rs` (listener lifecycle, portal router)
- `crates/hytale-server/tests/portal_transfer.rs`
- `tools/client-bot/tests/portal_session.rs`
- `tools/client-bot/tests/multiplayer_persistence.rs`
- `crates/hytale-world/src/universe.rs`, `world_config.rs`

## Acceptance

```sh
cargo test -p client-bot --test universe_restart -- --ignored --nocapture
cargo test -p hytale-server --test portal_transfer
./tools/test.sh fast
cargo clippy -p hytale-server -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A connected client sees a clean listener shutdown and can rejoin after the
  universe restart.
- Two worlds and their player documents remain distinct across the restart.
- Portal transfer/return still preserves the source transform and inventory.
- No stale sessions/views remain after restart.

## Notes

- Reuse T70's `run_clients`/shared-registry pattern and T60b's fixture worlds
  rather than inventing a new save layout.
- If the listener cannot be reopened on the same address deterministically, use
  a new port but the same universe root and document why.
