# T41 — Universe and world lifecycle

**Status:** planned (session 25+)
**Package:** `hytale-world`, `hytale-server`, `worldgen-server`
**Depends on:** T40
**Agent fit:** low/medium

## Goal

Own multiple worlds for the process lifetime instead of one scripted save.

## Deliverables

- `Universe`:
  - create/load/list/unload/save worlds;
  - default world selection;
  - world name/UUID resolution;
  - per-world save root and config.
- Per-world `WorldThread` lifecycle:
  - start/stop;
  - spawn point;
  - view registry;
  - save-on-unload and save-on-shutdown.
- World provider seam: `Default`, `Flat`, `Void`, `Dummy`. `Default` wraps the
  existing `worldgen-server`; the others may return empty/constant chunks until
  M5, but the seam must exist.
- Player save load/save on world join/leave.
- A small CLI surface for tests/operations (`world`/`universe` create/load/list/
  save/delete, exact names up to the implementer).
- Integration tests for two worlds and clean shutdown.

## Read first

- `crates/hytale-server/src/world_thread.rs`
- `crates/hytale-world/src/world.rs`, `world_save.rs`, `world_config.rs`,
  `player_data.rs`
- `crates/worldgen-server/src/lib.rs`
- Existing `world-boot`/`save-check` gates.

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-server
cargo test --workspace
```

- Two worlds can be created, edited and saved concurrently, then reloaded.
- Java save/read gates still pass.
- Shutdown flushes dirty chunks and player data exactly once.
- Missing/corrupt worlds fail closed with a clear error.

## Deviations

- Lighting/weather/world-map state remains M5/M6 scope.
- Provider-specific generation beyond `Default` can be a stub if documented.
