# T58 — NPC core: archetypes, navigation, flocking and perception

**Status:** done (session 50)
**Package:** new `hytale-npc`
**Depends on:** T48, T57b
**Agent fit:** low/medium

## Goal

Start M6 with the deterministic NPC core: load role/flock asset definitions,
follow navigation paths, compute flock/steering forces and evaluate the Java
perception cone. Spawning, combat and the ECS/blackboard runtime stay with T59.

The Java reference is `HytaleServer/NPC` (`NPC/`, 1,008 files). This slice ports
the pure pieces first and leaves the `MotionController`/builder machinery as an
explicit follow-up.

## Deliverables

- New `crates/hytale-npc`:
  - Role catalog over `Server/NPC/Roles/**` (`Type`, `Reference`, `Parameters`,
    raw document) with duplicate-key reporting.
  - Flock catalog over `Server/NPC/Flocks/**` (`Range`/`Weighted` size rules,
    `MaxGrowSize`, `BlockedRoles`), size selection with an injectable RNG and
    `PersistentFlockData` role filtering.
  - `Steering` output plus pursue/evade/rotate steering forces, ported from
    `Steering.java` and `movement/steeringforces/*`.
  - Java `SearchDirections` tables (walk/plane/fly, diagonal on/off) and a
    block-grid A* using the same direction tables, Euclidean costs and
    path/open/total node limits, with nearest-node fallback.
  - `PathFollower` waypoint advancement, passed-waypoint projection, path
    rejection and final-stage freezing.
  - `NPCPhysicsMath.isInViewCone` perception test.
- Java fixture `testdata/golden/server/npc/npc.json` produced by
  `tools/java-dumper/run-npc.sh` (`NpcDumper`), covering the direction tables,
  steering outputs and view-cone cases.
- Scripted tests for walk-around-wall, unreachable-goal closest path, 3D fly,
  waypoint following/rejection, flock size selection/role filtering and the real
  role/flock stores.
- Docs: golden README, dumper README, task board, plan, next steps and porting
  history.

## Acceptance

```sh
cargo test -p hytale-npc
./tools/test.sh fast
./tools/test.sh assets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The Java `SearchDirections`/steering/view-cone fixture passes.
- The real pack loads 1,147 unique role keys (one development duplicate is
  reported) and all 8 flock assets.
- Scripted navigation reaches the target around a wall and returns the closest
  node when it cannot; 3D fly uses all axes.
- Flock size picks respect the range/weighted formulas and blocked roles.
- Existing v1/T56/T57/T57b gates stay green.

## Landed (session 50 — first slice)

- New `hytale-npc` crate with `role`, `flock`, `steering`, `navigation`, `path`
  and `perception` modules.
- `NpcDumper` + `run-npc.sh` emit the Java fixture; `run.sh` now packages the
  NPC module and adds it to the dumper classpath.
- Real-pack role/flock tests and the Java fixture gate are green.
- `hytale-math::trig_math_util` supplies the Java sin/cos/atan2 lookups used by
  the steering heading helpers (no standard-library drift).

## Landed (session 50 — second slice)

- T58b adds [`hytale-npc::expr`](T58b-npc-builder-expressions.md), reference
  chain/parameter merging and `ResolvedRole`, so the 1,147 role documents now
  resolve to usable archetype fields. The three `ReferenceSelector`
  development roles keep nested slot-scoped expressions for a later builder
  pass.
- T58c adds [`WorldGrid`](T58c-npc-world-grid-group.md) (real-world walk/fly
  planning) and the Java group-steering accumulator. The T58 core goal is
  closed; deep builder-graph and motion-controller parity move with T59/T60.

## Deviations

- The full builder graph is not ported. `NpcRole` keeps the raw document and
  T58b resolves the reference chain, parameters and `Compute` expressions; the
  `BuilderBase` slots/attributes/validators remain later work.
- `AStarPlanner` searches block-grid nodes. Java's fractional half-block
  nodes, `MotionController.probeMove` reachability and `ProbeMoveData` are not
  ported; the direction tables/costs/limits are.
- `PathFollower` omits motion-controller path smoothing and heading blending.
- Wander (RNG), collision avoidance, group forces, blackboard sensors and the
  NPC ECS are not landed.
- `PersistentFlockData` models the persistent role/size state only; the ECS
  damage buffers and stored holders remain T59.

## Follow-ups

- T58b (done): role expression evaluation and resolved archetypes.
- T59: NPC spawning/combat/encounters, runtime entity navigation and the NPC
  ECS; builder-graph/reference-slot and full motion-controller parity move with
  the builtin NPC module (T60).
- T59: NPC spawning, combat, encounters, runtime entity producers and the
  `EntityGameplay` component attach.
- T62: NPC path/search and spawn rate ceilings.
