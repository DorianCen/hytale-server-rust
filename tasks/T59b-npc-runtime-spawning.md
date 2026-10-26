# T59b — NPC runtime, spawn positions and beacons

**Status:** in progress (session 52; runtime slice landed)
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

## Deviations

- Spawn-position search, block-set/fluid-tag/light validation and the
  world-environment spawn controller are not landed yet; callers spawn a named
  role at an explicit transform.
- Beacons and spawn markers are not landed.
- NPC damage targets only runtime NPC actors; player health is not modelled yet,
  so player targets produce attack events without damage (T59c).
- The session loop only ticks actors when a session is live; a headless world
  can call `tick_npcs` directly.

## Follow-ups

- Finish T59b: spawn-position search, spawn controller/expected counts,
  beacons/markers and CLI/bot coverage.
- T59c: encounter/instance/portal evaluator and the full combat action graph.
