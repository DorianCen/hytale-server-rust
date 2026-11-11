# T75 — Per-join player identity and spawn providers

**Status:** proposed (filed session 99)
**Package:** `hytale-server`, `hytale-world`, `tools/client-bot`, docs
**Depends on:** T44, T65, T66
**Agent fit:** medium

## Goal

Remove the remaining single-player assumption from world startup and implement
Java `ISpawnProvider` parity for join, respawn, portal arrival and spawn markers.
Each authenticated UUID must get its own player document and Java-selected spawn.

## Current gaps

- `WorldThreadOptions.player_uuid`/startup chooses the first stored player when
  no UUID is supplied. The listener can therefore preload a default player even
  though sessions join with their own UUID.
- Join spawn resolution uses a custom surface search near the origin rather than
  the world's `SpawnProvider`.
- `WorldConfig.spawn_provider` is stored as opaque JSON; only a direct
  `SpawnPoint` lookup is used.
- Portal instance `SpawnProviderOverride` is not resolved through the provider
  codec.
- Spawn-marker presentation does not use the provider's per-player spawn point.

## Deliverables

- Player identity:
  - make the listener start worlds without creating a synthetic first player;
  - load/create only the joining session's UUID;
  - ensure save/reconnect/portal transfer and `/who` use that identity;
  - keep CLI `world-boot`/`chunk-serve` explicit-player behavior compatible.
- Spawn providers:
  - add a Rust `SpawnProvider` representation/loader for the Java codec map
    (`GlobalSpawnProvider`, `IndividualSpawnProvider`,
    `FitToHeightMapSpawnProvider`, and `SpawnProviderOverride`);
  - resolve the provider for new players, legacy `(0,0,0)` transforms, respawn,
    portal arrival and spawn markers;
  - keep surface/height-map behavior deterministic for the existing worlds.
- Tests:
  - Java fixture configs for each provider type;
  - unit tests for `isWithinSpawnDistance` and fallback/override selection;
  - ignored loopback: two auth UUIDs join with distinct transforms/documents,
    reconnect and restart preserve each one.
- Docs: update `NEXT_STEPS.md`, `ARCHITECTURE.md`, task board.

## Read first

- Java:
  - `CoreServer/.../universe/world/spawn/ISpawnProvider.java`
  - `.../spawn/GlobalSpawnProvider.java`
  - `.../spawn/IndividualSpawnProvider.java`
  - `.../spawn/FitToHeightMapSpawnProvider.java`
  - `.../universe/world/WorldConfig.java` (`SpawnProvider`/legacy spawn)
  - `.../entity/entities/Player.java` (`getSpawnPointAsync`)
  - `.../asset/type/portalworld/PortalSpawnConfig.java`
  - `.../modules/entity/spawn/**` and `SpawnMarkerProvider.java`
- Rust:
  - `crates/hytale-server/src/world_thread.rs` (`WorldThreadOptions`,
    `player_snapshot`, startup player selection)
  - `crates/hytale-server/src/session.rs` (join snapshot, respawn, portal)
  - `crates/hytale-server/src/universe_runtime.rs`
  - `crates/hytale-world/src/world_config.rs`
  - `crates/hytale-world/src/world.rs` (`default_spawn`)
  - `crates/hytale-world/src/instance.rs`
  - `tools/client-bot/tests/{multiplayer_persistence,portal_session}.rs`

## Acceptance

```sh
cargo test -p hytale-world world_config
cargo test -p hytale-server --lib
cargo test -p client-bot --test multiplayer_persistence -- --ignored
cargo test -p client-bot --test portal_session -- --ignored
./tools/test.sh fast
cargo clippy -p hytale-world -p hytale-server -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A listener boot creates no synthetic player; each auth UUID gets its own
  document on first join.
- Reconnect and universe restart restore the correct per-UUID player.
- Java provider fixtures select the same spawn as Java; portal overrides win.
- Existing world-boot/chunk-serve CLI defaults remain usable.

## Notes

- Preserve the current deterministic surface fallback for worlds with no
  provider; it is the compatibility path for generated universes.
- Do not invent provider JSON shapes; use the Java codec and real fixture
  documents.
