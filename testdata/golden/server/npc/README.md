# NPC navigation/steering fixture

`npc.json` is written by
[`tools/java-dumper/run-npc.sh`](../../../../tools/java-dumper/run-npc.sh) from
the real NPC classes (`NpcDumper`).

It records only deterministic, RNG-free Java paths:

| Object | Contents |
|---|---|
| `searchDirections` | `SearchDirections.create` tables for walk/plane/fly with and without diagonal moves: step vectors, distances, inverse indices and crossing-step normals |
| `steering` | `SteeringForcePursue`, `SteeringForceEvade` and `SteeringForceRotate` fixed-input outputs (result, translation/yaw flags and values) |
| `perception` | `NPCPhysicsMath.isInViewCone` results for fixed viewer/direction/object inputs |
| `expressions` | `Expression`/`StdScope` results for the role-builder expression subset (`max`, `min`, `isEmpty*`, `makeRange`, arrays, comparisons and logic) |
| `damageRanges` | `DamageCalculator.computeDamageRange` min/max for absolute/DPS base damage and random-percentage cases |

The random wander force, the motion-controller-dependent A* expansion
(`probeMove`/`ProbeMoveData`) and the builder expression language are not part
of this fixture. The Rust planner is a block-grid A* with the same direction
tables, Euclidean costs and node limits; its fractional half-block nodes and
controller probes remain a documented T58 follow-up.

Regenerate and verify:

```sh
tools/java-dumper/run-npc.sh
tools/java-dumper/run-npc.sh --check
```

The consumer gate is
[`crates/hytale-npc/tests/java_fixture.rs`](../../../../crates/hytale-npc/tests/java_fixture.rs).
