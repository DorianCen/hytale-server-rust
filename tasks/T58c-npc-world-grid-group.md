# T58c — NPC world grid and group steering

**Status:** done (session 50)
**Package:** `hytale-npc`
**Depends on:** T58, T58b
**Agent fit:** medium

## Goal

Close the T58 core by planning over real runtime-world blocks and adding the
Java group-steering accumulator used by flock alignment/cohesion/separation.

## Deliverables

- `hytale-npc::worldgrid::WorldGrid`:
  - Wrap a `hytale_world::World` with a pluggable block-solidity rule.
  - `is_walkable` (feet/head air + solid floor), `is_flyable` and loaded-chunk
    checks.
  - `plan_walk_path`/`plan_fly_path` over the existing A* planner.
- `hytale-npc::group::GroupSteeringAccumulator`:
  - Port `GroupSteeringAccumulator` range/view-cone filtering, unweighted and
    weighted neighbour processing, normalized-distance handling and `end`
    averaging.
  - Snapshot-based `GroupMember` input instead of ECS references.
- Scripted real-`World` tests: a wall detour with a solid floor and a fly path
  across the same wall.
- Group accumulator tests for range/view-cone rejection and weighted falloff.

## Acceptance

```sh
cargo test -p hytale-npc
./tools/test.sh fast
./tools/test.sh assets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Real-world walk/fly plans reach the target and respect the solid rule.
- Group steering ignores out-of-range/out-of-cone members and applies the Java
  falloff.
- Existing T58/T58b gates stay green.

## Landed (session 50)

- `worldgrid.rs` and `group.rs` added; 4 new tests.
- `WorldGrid` uses the shared A* tables and `AStarPlanner` directly.

## Deviations

- `WorldGrid` takes a solidity closure; the server will pass the
  `BlockTypeAsset.material == Solid` rule in T59. Unloaded chunks are blocked.
- The Java accumulator's random tiny-distance jitter is replaced by a
  deterministic `+X` epsilon direction.
- Motion-controller collision probes, path smoothing and the full builder graph
  remain T59/T60 follow-ups.

## Follow-ups

- T59: attach NPC runtime entities to the world, tick navigation, spawn/
  despawn presentation and combat.
- T60: builder/reference-slot graph and full `AStarBase` half-step probes when
  the builtin NPC module lands.
