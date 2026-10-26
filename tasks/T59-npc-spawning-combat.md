# T59 — NPC spawning, combat and encounters

**Status:** in progress (session 51; first slice landed)
**Package:** `hytale-npc`, `hytale-world` (+ `hytale-server` in T59b)
**Depends on:** T58, T58b, T58c
**Agent fit:** low/medium

## Goal

Turn the T58 NPC core into living world actors: load the world spawn
configurations, select roles by Java time/moon/flock rules, attach stats and
combat data to NPC entities, drive chase/melee/death behaviour, and start the
encounter/instance/portal layer.

The Java references are `NPC/spawning/**` (`WorldNPCSpawn`,
`WorldSpawnWrapper`, `WorldNPCSpawnStat`, `WorldEnvironmentSpawnData`),
`spawning/assets/spawns/config/RoleSpawnParameters`,
`npc/corecomponents/combat/ActionAttack`, and
`builtin/EncounterManager`. This task is split like T58: the deterministic
spawn/combat core lands first; the world producer, position search and
encounter evaluator follow in T59b/T59c.

## Deliverables

- `hytale-npc::spawn`:
  - `WorldSpawnCatalog` over `Server/NPC/Spawn/World/**` (98 documents).
  - `WorldSpawn` fields: `Environments`, `NPCs`, `Despawn`, `DayTimeRange`,
    `MoonPhaseRange`, `LightRanges`, `ScaleDayTimeRange`,
    `MoonPhaseWeightModifiers`.
  - `RoleSpawnParameters`: role id, weight, `SpawnBlockSet`, `SpawnFluidTag`,
    movement-mode weights, safe-spawning flag, named/inline flocks.
  - Java day/scaled-day/moon-range gates, per-phase weight modifiers,
    `RandomExtra.randomWeightedElement` selection and the flock-minimum
    `missingCount` rule.
- `hytale-world::world_clock` Java helpers: `day_progress`,
  `scaled_day_time`, `is_day_time_within_range`,
  `is_scaled_day_time_within_range`, `is_moon_phase_within_range`.
- `hytale-npc::combat`:
  - `DamageCalculator` decode/`compute_range`/roll matching Java.
  - `CombatProfile` from a resolved role (`MaxHealth`, view/hearing/target/
    attack ranges, speeds, leash and attack pause; reachable
    `InteractionVars` damage calculators).
  - `NpcActor` with real `EntityGameplay` stats, target acquisition, A* chase,
    melee attacks, damage/death and runtime component generation.
- Extend `NpcDumper`/`npc.json` with `DamageCalculator.computeDamageRange`
  cases.
- Real-pack tests for all spawn documents, time/moon/flock selection, Rat/
  Bear/Goblin combat profiles and actor death.

## Acceptance

```sh
cargo test -p hytale-npc
cargo test -p hytale-world
tools/java-dumper/run-npc.sh --check
./tools/test.sh fast
./tools/test.sh assets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- 98 world spawn documents / 271 role entries load, with environments, weights,
  day ranges and inline/named flocks preserved.
- Java scaled-day/moon wrapping matches the real configs at fixed clock times.
- `Bear_Grizzly`, `Rat` and `Goblin_Scrapper` resolve their combat fields and
  damage ranges from the real role documents.
- An actor acquires a target, paths to it on a real `World`, attacks, takes
  lethal damage and produces runtime model/nameplate/`Npc` components.
- Existing T58/T58b/T58c gates stay green.

## Landed (session 51 — first slice)

- `hytale-world::world_clock` gained the Java day-progress/scaled-time/moon
  helpers used by spawning (unit-gated).
- New `hytale-npc::spawn` with the world spawn catalog, Java time/moon gates,
  moon-phase weight modifiers, weighted role selection, flock minimum size and
  missing-count math.
- New `hytale-npc::combat` with `DamageCalculator`, role `CombatProfile`,
  `NpcActor` targeting/chase/melee/damage/death and runtime component output.
- Role resolution now flattens `Modify` over the inherited document, so
  variant fields such as `MaxHealth`, `_InteractionVars` and attack ranges are
  visible to consumers (`Bear_Grizzly` is 124 HP, `Rat` 10, `Goblin_Scrapper`
  36).
- `NpcDumper` emits `damageRanges`; the Java fixture now gates
  `computeDamageRange` for absolute/DPS and random-modifier cases.
- Real-pack tests cover the 98 spawn documents / 271 entries, fixed-time
  eligibility and the Rat/Bear/Goblin combat profiles.

## Deviations

- T59b's runtime slice attaches `NpcActor` to `WorldThread`, ticks it, persists
  health and broadcasts spawn/despawn/transform. Spawn-position search, the
  world spawn controller, beacons/markers and live encounter producers remain
  T59b/T59c.
- Spawn-position search, block-set/fluid-tag/light checks, spawn suppression,
  beacons and markers are T59b. This slice only exposes the config data.
- `NpcActor` uses the `PathFollower` block-centre path without Java
  `MotionController` smoothing; it chases the nearest in-range target and uses
  the highest-damage specific attack. Full `ActionAttack` selector/aiming,
  conditions, knockback and per-cause damage events are T59c.
- Encounters, instances and portals are T59c; no encounter asset is parsed yet.
- Player health/damage is not modelled here, so live NPC→player combat waits
  for the player gameplay attach in T59b/T59c.

## Follow-ups

- T59b: runtime producer in `WorldThread` (landed session 52) and
  spawn-position/environment selection plus the density controller (session 53)
  explicit population ticks (session 54), loaded-section segment discovery
  (session 55) and real `BlockSet` floor resolution (session 56); automatic
  environment enumeration, beacons/markers, fluid-tag/light/suppression gates
  and CLI/bot coverage remain.
- T59c: full combat action graph and the `EncounterManager` evaluator
  (encounters/instances/portals).
