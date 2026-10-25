# T53 — Entity stats, effects and combat basics

**Status:** done (session 44)
**Package:** `hytale-entities`, `hytale-assets`, `hytale-server`
**Depends on:** T48 (and T52d for block-health presentation later)
**Agent fit:** low/medium

## Goal

Add the first authoritative entity gameplay layer: stat types/values, transient
stat modifiers, active entity effects, damage causes/resistance and
health/death handling, and load the `EntityStatType` store so item stat-HUD and
utility payloads resolve names to ids.

Java splits this across `modules/entitystats`, `asset/type/entityeffect`,
`entity/effect` and `modules/entity/damage`. This task ports the core models
and deterministic systems into Rust; client presentation, full condition
evaluation and builtin module lifecycle stay later work.

## Deliverables

- `hytale-entities::stats`:
  - `EntityStatType` asset fields (`InitialValue`, `Min`, `Max`, `Shared`,
    `Regenerating`, min/max effects, `ResetType`, `IgnoreInvulnerability`,
    `HideFromTooltip`).
  - `EntityStatValue` with min/max modifiers (`Additive`/`Multiplicative`),
    clamp/as-percentage, regeneration timers and Java `ResetType` behavior.
  - `EntityStatMap` add/set/maximize/minimize/reset, modifier add/remove,
    pending self/other updates and deterministic `tick` regeneration.
- `hytale-entities::effects`:
  - `EntityEffectAsset` core fields (duration, overlap, debuff, icon,
    invulnerability, stat modifiers, damage resistance/outgoing modifiers,
    removal behavior).
  - `ActiveEntityEffect` / `EffectController` add/tick/remove with
    `Ignore`/`Overwrite`/`Extend` overlap behavior, infinite effects and
    invulnerability aggregation.
- `hytale-entities::damage`:
  - `DamageCause` asset fields and inheritance lookup.
  - `DamageSource`/`Damage` model.
  - deterministic resistance/outgoing-damage sequence, health-stat subtraction,
    clamping at zero and `DeathComponent`-equivalent state.
- `hytale-assets`:
  - load `Server/Entity/Stats` as `EntityStatType` ids, including Java's
    preloaded `Unknown` at id 0.
  - resolve `Utility.EntityStatsToClear` and `DisplayEntityStatsHUD` string
    names through that store in the item packet encoder.
- Tests:
  - Java asset fixture checks for stat ids/fields and effect/damage assets.
  - scripted stat add/clamp/modifier/regeneration tests.
  - scripted effect overlap/expiry/invulnerability tests.
  - scripted damage/resistance/death tests and item stat-id resolution.
- Existing world/server/client-bot gates stay green.

## Read first

- Java `modules/entitystats/{EntityStatMap,EntityStatValue,EntityStatsSystems,RegeneratingValue}`,
  `asset/EntityStatType`, `modifier/StaticModifier`
- Java `asset/type/entityeffect/config/{EntityEffect,ApplicationEffects,OverlapBehavior,RemovalBehavior}`,
  `entity/effect/{ActiveEntityEffect,EffectControllerComponent}`
- Java `modules/entity/damage/{Damage,DamageCause,DamageSystems,ResistanceModifier,DeathComponent}`
- `crates/hytale-entities/src/lib.rs`, `crates/hytale-entities/src/movement.rs`
- `crates/hytale-assets/src/{registry.rs,packets/item.rs}`
- Assets: `Server/Entity/Stats/*`, `Server/Entity/Effects/**`,
  `Server/Entity/Damage/*`

## Acceptance

```sh
cargo test -p hytale-entities
cargo test -p hytale-assets
cargo test -p hytale-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A stat starts at `InitialValue`, clamps on add/set, honors additive and
  multiplicative min/max modifiers and regenerates on the Java interval.
- Effects overwrite/extend/ignore according to `OverlapBehavior`, expire after
  their duration, and contribute invulnerability/stat modifiers while active.
- Damage subtracts from the health stat through resistance/outgoing-damage
  modifiers, never goes below zero, and produces a death state at zero.
- `Utility.EntityStatsToClear` and `DisplayEntityStatsHUD` resolve real asset
  names to the same deterministic ids as the stat store.

## Deviations to report or split

- Full `Condition` evaluation, damage calculators, effect damage-over-time,
  knockback/animations/particles/sounds, item loss/respawn and client stat/
  effect packets are presentation or later combat work (T55/T57/T59).
- ECS registration/world wiring is kept behind a small component seam; the
  existing world thread receives it when NPC/combat producers land.
- A Java dumper for stats/effects/damage is added when the asset fixtures are
  generated; do not invent expected ids.

## Landed (session 44)

- `Server/Entity/Effects` and `Server/Entity/Damage` are loaded by the P0
  registry with Java id order; `DamageCause` uses Java's non-inheriting
  `Parent` rule (only `DeathMessageKey` is inherited).
- `EntityEffectAsset::from_value` decodes duration/overlap/removal/infinite/
  debuff/icon/invulnerability/value type, named stat values, `RawStatModifiers`,
  damage resistance/outgoing modifiers and the raw damage calculator/apply
  conditions. Java defaults (`Ignore`, `Complete`, `Absolute`) are honoured.
- `hytale-entities::components::EntityGameplay` bundles the stat map, active
  effects, effect-modifier sync, damage application, death state and the
  region/revive seam for T55/T57/T59.
- `Regenerating.ClampAtZero` is parsed and applied.
- `tools/java-dumper/run-entity-gameplay.sh` emits
  `testdata/golden/server/entity/entity-gameplay.json` (13 stats incl.
  `Unknown`, 260 effects, 20 damage causes) from the real pipeline, including
  the entity-condition codec variants that the P0 asset goldens intentionally
  do not register.
- `crates/hytale-entities/tests/entity_gameplay.rs` checks every id and the
  modelled fields against that fixture; `--validate-assets` now reports 25
  stores.

## Follow-ups

- T55 projectiles and traversal consume the damage model.
- T57 adds stat/effect/damage presentation packets and the
  `UpdateEntityStatTypes`/`UpdateEntityEffects` config-burst parity.
- T59 NPC combat and encounters use the same stat/effect/damage core.
- T60 maps the stat/effect/damage systems into the builtin module lifecycle.
