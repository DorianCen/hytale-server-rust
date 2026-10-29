# T59b — NPC runtime, spawn positions and beacons

**Status:** done (sessions 52–62)
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

### Block-set slice (session 56)

- New `hytale-npc::block_set` loads all 80
  `Server/Item/Block/Sets/**/*.json` documents and resolves
  `IncludeBlockTypes`/`ExcludeBlockTypes`/`IncludeBlockGroups`/
  `ExcludeBlockGroups` globs against the real block registry.
- `.append` block-set codecs do not inherit `Parent` fields; the catalog follows
  Java and evaluates each document's own fields.
- `SpawnNpcInEnvironment` and `tick_npc_spawns` resolve the selected role's
  `SpawnBlockSet` to allowed floor block ids before the position search.
- Real-asset tests cover Stone/StoneAndSoil/Soil membership; the platform
  population scenario now uses a real `Soil_Grass` floor.

### Fluid-tag slice (session 57)

- New `hytale-npc::fluid_tags::FluidTagIndex` builds `Tag -> fluid ids` from the
  real `Fluid.Tags.Fluid` data.
- `SpawnSearchOptions.allowed_fluids` requires the feet fluid to be in the
  resolved set; `require_fluid` remains the "any fluid" fallback.
- `SpawnNpcInEnvironment` and `tick_npc_spawns` resolve the selected role's
  `SpawnFluidTag` before the position search.
- Real-asset tests cover Water/Lava membership; the position-search test covers
  allowed and rejected fluid ids.

### Light-range slice (session 58)

- `World::light_sample` exposes the packed global light channels
  (`sky`/`red`/`green`/`blue`) and the Java total (`sky * sunlight factor` vs
  block light).
- New `hytale-npc::light::LightRanges` ports the `LightRangePredicate`
  percentage-to-packed conversion and the `Light`/`SkyLight`/`Sunlight`/RGB
  channels.
- `SpawnSearchOptions` carries the resolved ranges and the clock sunlight
  factor; population/environment spawns pass the selected config's
  `LightRanges`.
- Unit coverage sets a lit section and checks bright/dark ranges.

### Beacon/marker catalog slice (session 59)

- New `hytale-npc::spawn_special` loads all 96
  `Server/NPC/Spawn/Beacons/**` documents and 321 unique marker keys from
  `Server/NPC/Spawn/Markers/**` (322 files; one shared stem).
- Beacon entries reuse `RoleSpawnParameters` and parse the NPCSpawn time/moon/
  light fields plus beacon radius, concurrency, max NPCs and real/game
  cooldown ranges.
- Marker entries parse weighted `NPCs` configs, flock references, realtime/game
  respawn cooldowns, exclusion/drop/deactivation fields and the ISO-8601
  `P1D`/`PT5M` duration forms.
- Weighted role/config selection and cooldown lookup are unit-tested; real-pack
  tests cover the catalogs and Bat/Edible_Rat entries.
- Live beacon/marker controllers and persistent cooldown state remain open.

### CLI slice (session 60)

- `world-boot --spawn-npc ROLE` spawns a role at the player/world spawn,
  reports the runtime id, and persists it through the normal shutdown path.
- `serve --spawn-npc ROLE` spawns the role when the world starts, before
  accepting sessions.
- `WorldBootReport` carries `spawned_npc` and the world-boot gate asserts the
  persisted `entities.json` entry.

### Automatic environment enumeration (session 62)

- `World::environment_section_centers` returns the loaded 32³ section centers
  for an environment; the old segment count now derives from it.
- `WorldThread::{set_npc_auto_population,tick_npc_spawns}` discovers every
  loaded environment with a positive `Environment.SpawnDensity` and at least
  one eligible `Server/NPC/Spawn/World` config, then feeds the existing
  density controller. Auto targets use the loaded section extent for their
  search radius.
- Environment and population spawns persist `Npc.Environment`; each population
  tick recomputes the controller's live count from those tags, so deaths and
  save/reopen do not cause duplicate growth.
- `world-boot --auto-population` and `serve --auto-population` enable the
  discovery path.

### Spawn suppression (session 62)

- New `hytale-npc::spawn_suppression` loads all 114 `Server/NPC/Groups`
  documents and all 5 `Server/NPC/Spawn/Suppression` configs. Group membership
  follows Java's include-group → exclude-group → include-role → exclude-role
  order with role globs and recursive groups.
- `SpawnSuppressionIndex` resolves active suppressor positions to role and
  marker zones. `SpawnSearchOptions.suppressed_zones` rejects candidates inside
  a role-suppressing zone for `spawn_npc_near`, environment selection and
  automatic population.
- `WorldThread` exposes add/remove/list suppressor commands and persists the
  active suppressor map in `spawn-controllers.json` across save/reopen.

### Live marker/beacon controllers (session 62)

- `SpawnMarkerController` selects weighted marker roles, applies realtime or
  game-time respawn cooldowns, respects `ManualTrigger`/`DeactivationDistance`
  and serialises its cooldown/last role state.
- `BeaconController` applies the initial spawn delay, time/moon eligibility,
  concurrency and max-NPC limits, weighted roles and realtime/game-time spawn
  cooldowns.
- `WorldThread` owns live marker/beacon maps with add/remove/trigger/tick/state
  commands. It spawns real NPC runtime entities, honours marker suppression and
  role suppression, persists controller cooldowns in `spawn-controllers.json`
  and restores them on boot. Sessions tick both controllers with the game loop.
- New real-pack tests cover the Bat marker, Edible_Rat beacon, cooldown and
  state round trips, and world-thread persistence after save/reopen.

### Client-bot scenario (session 62)

- New ignored `client-bot` scenario joins a live `serve_session_connection`,
  observes the Rat spawn, chased transform updates and despawn, and asserts the
  server session recorded an NPC attack (`npc_attacks >= 1`). The world thread
  is shared as an `Arc` so the bot test can kill the NPC during observation.

## Deviations

- NPC damage targets only runtime NPC actors; player health is not modelled yet,
  so player targets produce attack events without damage (T59c/T60).
- Marker/beacon controllers are server-side state machines over runtime NPCs,
  not full Java `SpawnMarkerEntity`/`LegacySpawnBeaconEntity` components.
- Suppression is a 3D sphere approximation of Java's chunk-span/XZ + exact-Y
  suppression spans; role/group membership and marker suppression semantics are
  ported.
- The session loop only ticks actors/controllers when a session is live; a
  headless world can call the tick commands directly.

## Follow-ups

- T60: player health/combat, marker/beacon entity presentation and full builtin
  module lifecycle.
- T60: execute deferred encounter spawner/particle/sound actions and
  instance/portal transports.
- T62: keep-alive/idle/packet anti-abuse work for long-lived observers.
