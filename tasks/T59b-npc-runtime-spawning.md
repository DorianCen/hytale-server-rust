# T59b — NPC runtime, spawn positions and beacons

**Status:** in progress (sessions 52–55; runtime, spawn-position and population slices landed)
**Package:** `hytale-npc`, `hytale-world`, `hytale-server`
**Depends on:** T59
**Agent fit:** low/medium

## Goal

Attach the T59 spawn/combat core to the live world: spawn NPC runtime entities,
find Java-compatible positions, tick actors/navigation/combat, persist their
health and present spawn/update/despawn to sessions.

## Deliverables

- `WorldThread` NPC manager over `RuntimeEntityKind::Npc`:
  - reconstruct actors from `entities.json` + the role catalog on boot;
  - `spawn_npc`/`despawn_npc`/`tick_npcs` commands;
  - apply actor transforms and queue `RuntimeEntityEvent::Updated`;
  - persist role/health/max-health and save on shutdown.
- Spawn-position search from `SpawningContext`/`SpawnPosition*`:
  - walk/wade/underwater/dive/fly modes;
  - `SpawnBlockSet`, `SpawnFluidTag`, safe-spawning and light ranges;
  - world height/loaded-chunk/suppression checks and a deterministic candidate
    budget.
- World spawn controller using `WorldSpawnCatalog`, clock moon phase and
  environment density; spawn/despawn accounting against expected counts.
- Beacon and spawn-marker controllers (`Spawn/Beacons`, `Spawn/Markers`) and
  their cooldowns/persistent state.
- Server session/CLI wiring and a scripted bot scenario that observes NPC
  spawn, chase, attack and despawn.

## Acceptance

```sh
cargo test -p hytale-npc
cargo test -p hytale-world
cargo test -p hytale-server
./tools/test.sh fast
./tools/test.sh assets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A scripted world reaches its configured NPC population under the fixed clock.
- Spawned NPCs survive save/reopen with their role and health.
- No session leak or unbounded entity growth across repeated spawn/despawn.
- Existing T59 gates stay green.

## Landed (session 52 — runtime slice)

- `WorldThread` now owns a live `BTreeMap<RuntimeEntityId, NpcActor>`:
  - `spawn_npc` resolves a role (lazily loading the role catalog), builds the
    real stats/combat profile, spawns `RuntimeEntityKind::Npc` with
    model/nameplate/`Npc` components and queues a spawn event;
  - `despawn_npc`, `damage_npc`, `npc_actors`, `set_npc_targets` and
    `tick_npcs` commands;
  - actors are rebuilt from `entities.json` on world boot, including persisted
    health, and their health is written back before every save/shutdown.
- `NpcActor::from_runtime_entity`/`sync_runtime_entity` in `hytale-npc`.
- NPC movement updates runtime transforms and queues `Updated` events; lethal
  damage despawns the entity and queues `Despawned`.
- The session game loop publishes all registered player positions as NPC
  targets (high-bit player ids keep them disjoint from runtime ids), ticks the
  actors at the world tick cadence and reports `npc_ticks/moves/attacks/deaths`.
- Scripted `npc_runtime` integration tests cover spawn/persist/reload, damage
  and death, a runtime melee attack against a player target, and an unknown
  role error.

### Spawn-position/controller slice (session 53)

- `hytale-npc::spawn_position` scans loaded columns near a hint in deterministic
  distance order and validates the Java movement-mode contract:
  walk/wade/underwater-walk/dive/fly, solid-floor and feet/head air checks,
  optional allowed-block sets and the fluid requirement seam.
- `WorldSpawnCatalog::pick_spawn` selects an eligible environment/role across
  all matching configs with the Java day/moon gates and moon-phase weights.
- `WorldSpawnController` ports the `WorldEnvironmentSpawnData` density formula
  (`segments * density / 1024`), expected/missing counts and spawn/despawn
  accounting.
- `WorldThread::spawn_npc_near` resolves a role and finds a valid position;
  `spawn_npc_in_environment` picks an eligible role from the real catalog,
  finds a position and spawns it. The `npc_runtime` suite exercises both on a
  cleared test platform and selects `Env_Portal_Goblin_Surface` through the
  real spawn store.

### Automatic population slice (session 54)

- New `NpcSpawnTarget` / `NpcSpawnReport` and
  `WorldThread::{set_npc_spawn_targets,tick_npc_spawns}`. Each target carries
  its environment, center/radius, movement mode, density and tracked segment
  count.
- The population tick maintains one `WorldSpawnController` per environment,
  selects eligible roles with the real spawn catalog, finds a position, spawns
  through the shared actor builder and records controller population. It stops
  at the density-derived expected count with an eight-spawn attempt budget per
  target.
- Sessions call `tick_npc_spawns` from the game loop and report
  `npc_spawns` in `GameLoopReport`/`SessionSummary`.
- The `npc_runtime` platform scenario now drives a two-NPC population to its
  expected count and verifies the next tick is saturated.

### Segment discovery slice (session 55)

- `World::environment_segment_count` counts loaded 32³ sections containing an
  environment id (`EnvironmentSection.contains`), with a unit test covering
  multiple sections and ids.
- `NpcSpawnTarget.segments == 0` now discovers the count from the loaded world
  for the target environment before applying the density formula.

## Deviations

- `SpawnBlockSet`/`SpawnFluidTag` references are not resolved to their asset
  stores yet (the `BlockSet` generator is still a T35 gap); the search exposes
  explicit block-id/fluid-requirement options instead.
- Light ranges and spawn suppression are not evaluated by the position search.
- Population targets name their environment and density; segment counts can be
  discovered from loaded sections, but automatically enumerating every loaded
  environment is still open.
- Beacons and spawn markers are not landed.
- NPC damage targets only runtime NPC actors; player health is not modelled yet,
  so player targets produce attack events without damage (T59c).
- The session loop only ticks actors when a session is live; a headless world
  can call `tick_npcs` directly.

## Follow-ups

- Finish T59b: automatic environment enumeration, beacon/spawn-marker
  controllers, block-set/fluid-tag/light/suppression gates and CLI/bot
  coverage.
- T59c: encounter/instance/portal evaluator and the full combat action graph.
