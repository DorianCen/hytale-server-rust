# T52b — Fluid ticker parity follow-up

**Status:** planned
**Package:** `hytale-world`, `hytale-server`
**Depends on:** T52
**Agent fit:** medium

## Goal

Close the gameplay-visible fluid gaps left by T52: partial-block face coverage,
waterlogging rules, the finite/fire tickers, collision conversion and the
world-config disabled-ticker list. Keep the T52 fast path for ordinary
cube-contained water/lava.

## Deliverables

- `SimulationTables`/`FluidBehavior`:
  - parse the ticker `Type` (`Default`, `Finite`, `Fire`) and keep the existing
    fields;
  - parse the full `Collisions` entries, including `PlaceFluid` and sound-event
    references;
  - expose the world's `DisabledFluidTickers` set to the simulation.
- Port `FluidTicker.blocksFluidFrom`:
  - full cube/cube-with-model blocks block every direction;
  - partial/slab/door/stair hitboxes use the rotated `BlockBoundingBoxes`
    coverage rules and filler offsets;
  - flat slabs allow horizontal flow over/around them; vertical slabs only
    block flow along their thin axis.
- Port `FluidTicker.isFullySolid`/waterlogging:
  - a full solid cube clears co-located fluid;
  - partial blocks can retain waterlogged fluid;
  - `Fluid.EMPTY_ID`/level transitions keep the existing T52 save layout.
- Port `DefaultFluidTicker` remaining edges:
  - `SupportedBy`, `SpreadFluid`, `CanDemote`, source/flow demotion and
    down-first/horizontal spread ordering;
  - collision conversion only when the target is empty or soft-breakable.
- Port `FiniteFluidTicker` and `FireFluidTicker` behavior for the assets that
  use them: finite level drain/replenish, fire ignition/spread/burn and
  extinguish rules; use the existing `BlockGathering.is_soft`/flammability
  metadata where possible.
- Preserve T52 scheduling: changed blocks wake the 3×3×3 neighbourhood and
  fluid ticks continue through the `BlockSection` queue.
- Broadcast `ServerSetFluid` changes as T52 already does. Sound-event delivery
  may be emitted through the existing asset/event seam or explicitly deferred
  with a follow-up.

## Read first

- Java `FluidTicker`, `DefaultFluidTicker`, `FiniteFluidTicker`,
  `FireFluidTicker`, `Fluid`, `FluidSystems.Ticking`
- `CoreServer/.../universe/world/WorldConfig`
  (`DisabledFluidTickers`) and `builtin/Fluid/DisabledFluidResource`
- `CoreServer/.../asset/type/blocktype/config/BlockGathering`,
  `BlockFaceSupport`, `BlockBoundingBoxes`
- `crates/hytale-world/src/simulation.rs`,
  `crates/hytale-world/src/collision.rs`,
  `crates/hytale-world/src/world.rs`
- Real assets: `Water`, `Water_Source`, `Water_Finite`, `Lava`,
  `Lava_Source`, `Fire`, `Dungeon_Fire_NoSpread`, slab/stair/bench blocks

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-server
cargo test -p client-bot --test movement_join --test multi_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Java-written/scripted scenarios cover: water beside a slab/stairs, water in a
  partial block, a full block clearing water, lava/water conversion, a finite
  water level draining, fire ignition/burn/extinguish and one disabled ticker.
- The Java oracle is the real `FluidTicker` implementation; add a small Java
  dumper/harness fixture if no existing assertion can be reused. Do not invent
  expected bytes or timings.
- T52's ordinary source/flow tests and all existing movement/block/save gates
  remain green.

## Deviations to report or split

- Exact sound playback is presentation; if it cannot be emitted through the
  current event seam cheaply, record it as a T57/audio follow-up.
- Weather/rain-driven fire behavior depends on T54; keep the fire ticker's
  rain input behind a seam.
- Builtin `Fluid` plugin lifecycle and `DisabledFluidTickers` command/config
  registration remain T60 unless this task needs the config field earlier.
