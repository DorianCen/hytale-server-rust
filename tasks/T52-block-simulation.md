# T52 — Block simulation: ticks, fluids and support

**Status:** done (session 40)
**Package:** `hytale-world`, `hytale-server`
**Depends on:** T49, T51
**Agent fit:** low/medium

## Goal

Run the minimum deterministic block simulation for a served world: scheduled and
random ticks, support loss, falling blocks, fluid flow, block health and block
drops, while keeping the region/chunk storage bytes Java-readable.

Java splits these across core `BlockSection`/`FluidSection` and the
`BlockTick`, `RandomTick`, `BlockPhysics`, `FallingBlocks` and `Fluid` builtins.
This task ports the core single-threaded behavior into `hytale-world` and wires
one simulation pass into the world thread; builtin module lifecycle stays T60.

## Deliverables

- `hytale-world::chunk::section::BlockSection`:
  - transient `scheduleTick` priority queue (`tick_requests`), `begin_tick`
    (pre-tick + copy/clear) and `finish_tick` (`Continue`/`Sleep`/`Ignored`/
    `WaitForAdjacentChunkLoad`);
  - no storage-layout changes.
- `hytale-world::simulation`:
  - `SimulationTables` from `BlockType`, `Fluid`, contained item tags and
    merged JSON: material/draw/opacity, `Support`/`Supporting`, `TagId`,
    `FaceType`, `SupportDropType`, `MaxSupportDistance`, `FallingBlockSettings`,
    `Gathering` drops, `RandomTickProcedure` and `TickProcedure`.
  - `BlockSimulation::tick`: drains changed blocks, schedules the 3×3×3
    neighbourhood, pre-ticks sections, processes fluid/support/scheduled ticks,
    runs deterministic random ticks, advances falling blocks and regenerates
    block health.
  - support removal (`Break`/`Destroy`/`Fall`) and natural-drop priority
    `Physics` → `Breaking` → `Soft` → `Harvest`.
  - `DefaultFluidTicker` core: flow-rate gating, demotion when unsupported,
    downward fill to the spread fluid's max level, horizontal level-1 flow,
    collision block conversion and collision map parsing.
  - falling-block one-cell-per-tick movement with landing/placement.
  - transient `BlockHealth` map with the Java 5 s delay and 0.1/s regeneration.
- `World`:
  - tick counter, changed-block drain, rotation/filler/fluid accessors,
    `set_block_full`, `set_fluid`, `damage_block`, `block_health`.
- `WorldThread`:
  - `simulate(dt)`, `force_random_tick`, `set_fluid`/`get_fluid`,
    `block_drops`, `fluid_id`, `damage_block`/`block_health` seams.
- `hytale-server`:
  - one `simulate(0.05)` per 50 ms game-loop tick; broadcast block and fluid
    changes through the existing view-filtered player queues;
  - session counters for simulation passes and block/fluid changes;
  - left-click break feeds the block-health map before replacing the block.
- Tests:
  - synthetic `hytale-world` walkthroughs for support/drop, falling block,
    fluid source spread and health regeneration;
  - real-pack metadata tests for stone drops/tags, grass `SpreadTo`, plant
    support, water source/flow ticker and falling barrel settings;
  - world-thread integration tests for support, falling, random tick, fluid
    spread, drops and health;
  - all existing server/client-bot suites stay green.

## Read first

- Java `BlockSection` (`scheduleTick`, `preTick`, `forEachTicking`),
  `ChunkBlockTickSystem`, `RandomTickSystem`, `SpreadToProcedure`,
  `FluidTicker`, `DefaultFluidTicker`, `FluidSystems.Ticking`,
  `BlockPhysicsUtil`, `SupportDropType`, `FallingBlock`,
  `BlockHealthSection`, `BlockHarvestUtils.naturallyRemoveBlockByPhysics`
- `crates/hytale-world/src/chunk/{column,section/*}.rs`
- `crates/hytale-world/src/world.rs`
- `crates/hytale-server/src/{world_thread,session,players,serve}.rs`
- Real-pack assets: `Soil_Grass`, `Soil_Dirt`, `Plant_Bush`, `Rock_Stone`,
  `Water_Source`, `Water`, `Debug_Falling_Barrel`

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test -p client-bot --test block_interactions -- --ignored
cargo test -p client-bot --test inventory_join --test multi_join -- --ignored
cargo test -p client-bot --test trace_gate --test auth_join -- --ignored
```

- A supported plant stays and is removed with its drops when the support is
  removed.
- A `SupportDropType: Fall` block becomes a one-cell-per-tick falling block and
  lands on the first support.
- A water source spreads into adjacent cells at the Java level 1/down level.
- A forced grass random tick replaces tagged `Spreadable=Grass` soil.
- Damaged blocks regenerate after 5 s at 0.1/s.
- Existing block edits, interactions, inventory, movement, trace and reconnect
  tests remain green; chunk/region bytes keep the existing Java-readable layout.

## Deviations

- Fluid flow treats only cubes/cube-with-model blocks as solid barriers and
  does not implement Java's partial-hitbox face-coverage checks, waterlogging
  edges, fire/finite tickers or sound events. `DisabledFluidTickers` is not
  wired.
- Support checks are direct-face only; `MaxSupportDistance` chains,
  filler-specific support, `SelfFaceType` and light-based random growth are
  parsed or recorded but not fully evaluated. The `TickSuppressionRegistry` is
  not ported.
- `SpreadTo` ignores `RequiredLightLevel`; `PlaceBlock` uses the first placement
  candidate and ignores weighted selection/state scoping.
- Random tick selection uses a deterministic `FastRandom` stream instead of
  Java's `SplittableRandom`/stable hash; the procedure semantics, not the exact
  per-tick ordering, are the gate.
- Falling blocks are simulated as block movement, not ECS entities; no impact
  break/explode effects or entity collision.
- Block health is transient in memory and not replicated through
  `UpdateBlockDamage`/`UpdateBlockDamages` (T57). Java `BlockHealthSection`
  storage is not written yet, so a save reload resets damage.
- Physics/support drops are reported by the simulation but are not spawned as
  item entities (T56) and are not granted to inventories.
- Builtin module registration/lifecycle for `BlockTick`, `RandomTick`,
  `BlockPhysics`, `FallingBlocks` and `Fluid` remains T60; this is native core
  behavior reachable from the world thread.

## Follow-ups

- **T52b** closes the fluid ticker gaps (partial-hitbox face coverage,
  waterlogging, finite/fire tickers, disabled tickers, collision details).
- **T52c** closes the support/random-tick gaps (`MaxSupportDistance` chains,
  filler/self-face support, chunk-boundary waiting, light/fluid gates, weighted
  placement, scheduled growth, tick-suppression seam).
- **T52d** moves health/fragility into the Java `BlockHealthSection` storage
  codec and exposes health deltas for T57.
- T56 spawns dropped items and prefab/block components.
- T57 adds block-damage/fragility presentation packets.
- T58 uses the tick/support/fluid model for NPC navigation and block-breaking.
- T60 maps the native simulation to builtin module toggles.
