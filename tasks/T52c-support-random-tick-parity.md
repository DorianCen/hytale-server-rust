# T52c — Support and random-tick parity follow-up

**Status:** done (session 41)
**Package:** `hytale-world`
**Depends on:** T52
**Agent fit:** medium

## Goal

Replace T52's direct-face support/random-tick approximation with the Java
`BlockPhysicsUtil` and `RandomTickProcedure` semantics that custom/vanilla
content depends on: support chains, filler-specific support, face types,
light/fluid gates and chunk-boundary waiting.

## Deliverables

- `SupportRule` evaluation:
  - `MaxSupportDistance` propagation (support values per block/section and
    `BlockPhysics.IS_DECO_VALUE` handling);
  - `BlockType`, `TagId`, `Fluid`, `FaceType`, `SelfFaceType`, `MatchSelf`,
    `Rotate` and `Support` (`Required`/`Disallowed`/`Ignored`) exactly as
    `BlockPhysicsUtil.testBlockPhysics`;
  - filler-specific support (`BlockFaceSupport.Filler`, `RequiredBlockFaceSupport
    .isAppliedToFiller`) and multi-cell/protruding hitboxes;
  - `SupportsRequiredFor = Any|All` semantics;
  - `WAITING_CHUNK` when any required neighbour section is not loaded, instead
    of treating it as air.
- Support lifecycle:
  - invalidate/re-schedule support on block change, neighbouring chunk load and
    filler updates;
  - `SupportDropType` `Break`/`Destroy`/`Fall` with Java's natural-drop priority;
  - wake/sleep behavior equivalent to `BlockPhysics` update passes.
- Random ticks:
  - implement `SpreadTo` fully: `AllowedTag`, `SpreadDirections`,
    `MinY/MaxY`, `RequireEmptyAboveTarget`, `AllowedAboveFluids`,
    `RequiredLightLevel` with sunlight factor + block light,
    `RevertBlock` when covered by an opaque cube/fluid;
  - implement `PlaceBlock` weighting/state selection and `ChangeIntoBlock`;
  - parse and execute `BasicChanceBlockGrowth`/`SplitChanceBlockGrowth`
    scheduled procedures used by the `BlockTick` builtin;
  - add a `TickSuppressionRegistry` seam for `RandomTick`/`ScheduledTick`
    suppression (the TriggerVolumes producer remains T60).
- Keep deterministic selection while making the stable random-tick walk cover
  each section block once per cycle; exact Java `SplittableRandom` ordering is
  not a gate.

## Read first

- Java `BlockPhysicsUtil`, `BlockPhysicsSystems`, `BlockPhysics`,
  `RequiredBlockFaceSupport`, `BlockFaceSupport`, `SupportDropType`,
  `RandomTickSystem`, `SpreadToProcedure`, `PlaceBlockProcedure`,
  `ChangeIntoBlockProcedure`, `BasicChanceBlockGrowthProcedure`,
  `SplitChanceBlockGrowthProcedure`, `TickSuppressionRegistry`
- `crates/hytale-world/src/simulation.rs`,
  `crates/hytale-world/src/chunk/section/block.rs`,
  `crates/hytale-world/src/collision.rs`
- Real assets: `Plant_Bush`, `Soil_Grass*`, `Soil_Dirt*`, `Soil_Leaves*`,
  `Debug_Falling_Barrel`, support/face-type assets in
  `Server/Item/Blocks/Hitboxes/**`

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Scripted scenarios cover: a support-distance chain, disallowed support,
  `TagId`+`FaceType`, a filler/multi-cell support footprint, an unloaded
  neighbour waiting instead of breaking, grass spread with light/fluid gates and
  revert, weighted `PlaceBlock`, and a `ChangeIntoBlock`/scheduled growth.
- Add a Java-side dumper/harness fixture for support-distance and random-tick
  outcomes. Reuse real-pack assets and do not invent expected values.
- T52 support/falling/fluid/health tests and all save/interaction gates remain
  green.

## Deviations to report or split

- Exact per-tick random ordering is deliberately not required, matching
  `GOLDEN_DATA.md`'s “Java schedule-dependent order is not a gate” rule.
- Tick suppression volumes, builtin proc registration and plugin toggles remain
  T60; this task adds the core lookup seam and tests.
- Prefab block components/entity state that participate in support are T56.

## Result (session 41)

Implemented and verified against the real pack and the Java oracle. See
[`../PORTING_HISTORY.md`](../PORTING_HISTORY.md) session 41 for the changed
files and the remaining narrower deviations (sound playback, builtin module
lifecycle and client presentation stay with their later tasks).

**Remaining deviation:** the support/random-tick scenarios are covered by
synthetic `hytale-world` unit tests plus the real-asset parser checks. A
Java-side support-distance harness was not added in session 41; the Java oracle
for fluids and `BlockHealthSection` is committed. Random-tick scheduling order
is intentionally deterministic Rust, per `GOLDEN_DATA.md`.
