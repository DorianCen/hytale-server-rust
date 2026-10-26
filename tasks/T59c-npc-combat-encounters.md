# T59c — Combat action graph and encounters

**Status:** planned
**Package:** `hytale-npc`, `hytale-interactions`, `hytale-server`
**Depends on:** T59, T59b
**Agent fit:** low/medium

## Goal

Finish T59 by replacing the scripted primary attack with the Java
`ActionAttack`/interaction damage path and adding the `EncounterManager`
runtime (encounters, instances and portals).

## Deliverables

- Attack selection from the role `Attack` root and `InteractionVars`:
  selectors, aiming, charge time, ballistic/melee checks, line of sight,
  attack pause/cooldown and friendly-fire avoidance.
- `DamageCalculator` application through `DamageEntityInteraction`:
  absolute/DPS per cause, random percentage, targeted/angled damage,
  `DamageEffects` (knockback, effects) and the damage-calculator system order.
- NPC combat conditions/sensors used by the first encounters (`State`,
  `HasTarget`, `TargetDistance`, stat percentage, line-of-sight, timers).
- `hytale-npc::encounter`:
  - load `Server/EncounterManager/**` (builder variants, macros, forward slots);
  - sensor/instruction evaluator subset and state transitions;
  - boss bar/audio/weather actions used by `Examples/Example_Boss.json`;
  - instance/portal seams with explicit deferred actions.
- Scripted encounter fixtures and a bot scenario that runs an encounter from
  trigger to completion.

## Acceptance

```sh
cargo test -p hytale-npc
./tools/test.sh fast
./tools/test.sh assets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A real encounter document runs its state transitions and actions in order.
- Melee damage, effects and cooldown match the Java interaction path for the
  scripted NPC cases.
- Existing T59/T59b gates stay green.

## Follow-ups

- Builtin EncounterManager/NPC module parity moves with T60 if the full builder
  graph is required.
- Instance/portal transports and editor tooling stay later.
