# T43 — World clock and `GameplayConfig`

**Status:** done (session 31)
**Package:** `hytale-server`, `hytale-world`, `hytale-assets`
**Depends on:** T42
**Agent fit:** medium

## Goal

Make world time advance (and stop) from the configured day/night cycle instead
of protocol defaults, and build the `WorldGameplayConfig` packet from the loaded
`GameplayConfig` asset with Java-compatible fallbacks.

## Deliverables

- A `WorldClock` in `hytale-world`:
  - game time as epoch seconds/nanos (`WorldTimeResource` semantics);
  - Java's segmented real-time → game-time progression using the configured
    daytime/nighttime durations;
  - pause/resume, day/night duration overrides and time wrap;
  - ISO-8601 `GameTime` load/save through `WorldConfig`.
- `GameplayConfig` loaded as an internal asset store (no packet generator):
  - typed world/combat/player sections and Java defaults;
  - tag resolution for `BlockBreakingBypassTag` and
    `BlockedRootInteractionTags` through the `ServerTags` map;
  - `to_packet()` producing `WorldGameplayConfig`.
- `WorldThread` owns the clock and the encoded gameplay packet:
  - tick progression from wall-clock deltas;
  - `Save`/`Shutdown` persist `GameTime` and `IsGameTimePaused`;
  - pause/advance seams for tests and future commands.
- The session join sends the loaded `WorldGameplayConfig`, and the game loop
  broadcasts `UpdateTime` once per real second while time is running, plus
  `UpdateTimeSettings` when the pause state changes.
- `client-bot` captures the gameplay config/time packets and can observe live
  `UpdateTime` progression.
- Tests for clock progression, pause, duration overrides, hour/day wrap, config
  fallback, save/reload and live client time updates.

## Read first

- `crates/hytale-server/src/world_thread.rs`, `session.rs`
- `crates/hytale-world/src/world_config.rs`, `world.rs`
- `crates/hytale-assets/src/registry.rs`, `packets/mod.rs`
- Java `WorldTimeResource.java`, `TimePacketSystem.java`,
  `GameplayConfig.java`, `gameplay/{WorldConfig,CombatConfig,PlayerConfig}.java`

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-assets
cargo test -p hytale-server
cargo test -p client-bot --test auth_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A client sees time advance after joining.
- Gameplay flags match the loaded `GameplayConfig` (Java defaults when the
  asset is missing or cannot be parsed).
- Clock state survives save/reload through `config.json`.
- Pausing stops progression; resuming continues from the same time.

## Deviations

- Java's canonical asset dump reports `GameplayConfig` count 0 because all
  eight configs fail validation on the missing camera-effect store. Rust loads
  the same eight documents as an internal `DefaultAssetMap` store and resolves
  the selected id; a missing id falls back to `GameplayConfig.DEFAULT` values.
- Time packets are written per session from the shared world clock; the
  broadcast registry arrives with T45. `UpdateTimeSettings` is only re-sent when
  the pause flag changes.
- Sleep/day-time interpolation commands and moon-phase events stay in T54 and
  the command framework.

## Follow-ups

- T44 replaces the empty self-spawn appearance payload.
- T45 can fan the same clock snapshot out to all world sessions.
- T46 will consume `WorldGameplayConfig` flags when applying block edits.

## Acceptance evidence

```sh
cargo test -p hytale-world
cargo test -p hytale-assets
cargo test -p hytale-server
cargo test -p client-bot --test auth_join -- --ignored
cargo test -p client-bot --test multi_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- `world_clock` unit tests: 30 game seconds per real second at the default
  ratio, full-day cycle, hour rollover, pause/resume, duration overrides, range
  wrap and ISO-8601 round trips.
- `world_thread` integration: overrides, one game day per 200 real seconds at
  100/100, gameplay packet flags, pause freeze and exact save/reload.
- `auth_join` loopback: gameplay config flags plus two or more live `UpdateTime`
  packets with increasing game time.
