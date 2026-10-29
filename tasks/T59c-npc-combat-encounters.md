# T59c — Combat action graph and encounters

**Status:** done (session 61)
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

## Landed (session 61)

- `hytale-interactions::combat` owns `DamageCalculator` (absolute/DPS,
  sequential modifiers), `DamageEffects`, knockback modes and the
  `DamageEntity`/targeted/angled damage documents. A graph walker follows
  selector/`Replace`/`Chaining`/serial nodes and overlays role
  `InteractionVars`.
- `hytale-npc::combat` resolves role `Attack` roots and every
  `"Type": "Attack"` action, including aiming/charge timers, attack-pause
  ranges, line-of-sight checks, ballistic/friendly-fire flags and
  action-level interaction vars. `NpcActor` attacks through the resolved
  damage node and reports the source, runtime and effects.
- `hytale-npc::encounter` loads all 67 `Server/EncounterManager` documents
  (41 spawnable), resolves `Variant`/`Abstract` references, `Instruction`
  macros, `Modify`, `_InterfaceParameters` and `_ForwardedModifiers`, and
  evaluates the instruction tree with per-state `Once` flags and blocking
  action lists.
- The evaluator covers `State`/`ParentState` transitions, timers, flags,
  `Player`/`Any`/`And`/`Or`/`Not`/`Target`/`Mob`/`Count`/`Stat`/line-of-sight/
  `Beacon` sensors and the boss-bar, music, audio-state, weather, target-role/
  invulnerability, spawner, particle, sound, effect and portal-timer actions.
  World-owned actions are emitted as `DeferredEncounterAction`s.
- `WorldThread` gains live encounter spawn/tick/per-tick state, target-slot and
  beacon commands. `Example_Boss` runs from the player trigger through
  `Phase_1`, `Phase_2`, `BossDead` and `Complete` in the `npc_encounter`
  server scenario; `Example_State_Transitions` and `Example_Encounter` are
  gated against the real pack in `hytale-npc`.
- The Rat real-asset attack graph resolves `Bite_Damage` (9 ± 10% Physical),
  the selector runtime and its knockback/effects through `Root_NPC_Rat_Attack`.

## Deviations

- Motion-controller steering/blackboard execution still uses the scripted
  `NpcActor`; the unity builder graph and NPC ECS remain T60.
- Instance/portal transports, live spawner/particle/sound execution and
  editor tooling remain deferred. Encounter world actions are ordered events,
  not packet/entity side effects yet.
- The Java `NpcDumper` has no encounter fixture; the real-pack tests are the
  gate for the catalog/evaluator.

## Follow-ups

- Builtin EncounterManager/NPC module parity moves with T60 if the full builder
  graph is required.
- Instance/portal transports and editor tooling stay later.
