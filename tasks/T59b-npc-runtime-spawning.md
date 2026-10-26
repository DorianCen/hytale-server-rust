# T59b — NPC runtime, spawn positions and beacons

**Status:** planned
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

## Follow-ups

- T59c: encounter/instance/portal evaluator and the full combat action graph.
