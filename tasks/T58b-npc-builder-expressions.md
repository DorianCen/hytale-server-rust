# T58b — NPC builder expressions and resolved archetypes

**Status:** done (session 50)
**Package:** `hytale-npc`, `tools/java-dumper`
**Depends on:** T58
**Agent fit:** medium

## Goal

Turn the T58 role-document catalog into usable archetype data: evaluate the
role builder expression subset, merge reference templates and expose resolved
core fields (`Appearance`, `MaxHealth`, `MotionControllerList`, view/hearing
range) without porting the full Java builder graph yet.

## Deliverables

- `hytale-npc::expr`:
  - Lexer/parser for numbers, strings, booleans, arrays, parentheses, unary
    `! + -`, binary `+ - * / %`, comparisons, equality, `&&`/`||`.
  - Java `StdLib` subset: `max`, `min`, `isEmpty`, `isEmptyStringArray`,
    `isEmptyNumberArray`, `makeRange`, `random`, `randomInRange`, `PI`,
    `true`/`false`.
  - Strict Java type checks and a pluggable RNG for the random builtins.
- Role resolution:
  - Follow `Reference` chains root→variant.
  - Merge parameter defaults and resolve `{"Compute": ...}` parameter values.
  - Merge top-level documents and resolve every `Compute` expression.
  - Preserve nested expressions that belong to a `Reference` slot scope
    instead of failing the whole role (the three `ReferenceSelector` test
    roles).
- Java fixture additions for expression results, plus real-pack tests that all
  1,147 roles resolve and that `Bear_Grizzly` inherits/resolves its archetype.

## Acceptance

```sh
cargo test -p hytale-npc
./tools/test.sh fast
./tools/test.sh assets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Expression results match the Java `Expression`/`StdScope` fixture.
- Every role resolves through its reference chain with a deterministic RNG.
- `Bear_Grizzly` resolves appearance `Bear_Grizzly`, max health `124` (its
  `Modify.MaxHealth`), view range `12` and the inherited Walk `MaxWalkSpeed` of
  `6`.
- Existing T58 navigation/steering/flock gates stay green.

## Landed (session 50 — second slice)

- `expr.rs` ported the expression subset and added fixture cases for
  arithmetic, comparisons, logic, arrays, strings, `max`/`min`/`isEmpty*`/
  `makeRange` and `PI`.
- `ResolvedRole` + `RoleCatalog::resolve`/`resolve_with_rng` merge templates,
  parameter defaults and computed parameter values, then resolve the inherited
  document.
- All 1,147 roles resolve; three `ReferenceSelector` development roles keep
  their slot-scoped `Display`/`Range` expressions in
  `unresolved_expressions`.
- `NpcDumper` now dumps the expression oracle; fixture README/GOLDEN_DATA/task
  docs updated.

## Deviations

- The builder graph itself (`BuilderBase`, slots, attributes, validators,
  reference-scope resolution) is still not ported. Resolution is a reference
  chain + parameter/expression pass, not byte-equivalent to the Java builder.
- `**`/bitwise operators are not used by role assets and remain unsupported.
- The full `AStarBase` half-step search, motion-controller probes, wander,
  avoidance, group forces and blackboard perception remain T58c/T59.

## Follow-ups

- T58c: builder graph/reference slots, full motion-controller A*, wander/
  avoidance/group steering and sensor state.
- T59: spawning/combat/encounters and the NPC ECS.
