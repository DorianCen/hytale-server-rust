# Entity gameplay fixture

`entity-gameplay.json` is written by
[`tools/java-dumper/run-entity-gameplay.sh`](../../../../tools/java-dumper/run-entity-gameplay.sh)
from the real CoreServer asset pipeline (`EntityGameplayDumper`).

It records the loaded entity gameplay stores:

| Object | Count | Contents |
|---|---|---|
| `stats` | 13 | `Server/Entity/Stats` incl. Java's preloaded `Unknown` at id 0 |
| `effects` | 260 | `Server/Entity/Effects` |
| `damageCauses` | 20 | `Server/Entity/Damage` |
| `projectiles` | 86 | legacy `Server/Projectiles` (`DefaultAssetMap`, no ids) |
| `projectileConfigs` | 188 | new-style `Server/ProjectileConfigs` (`DefaultAssetMap`, no ids) |

Each stat/effect/damage-cause entry carries the `IndexedLookupTableAssetMap` id
plus the fields the Rust gameplay core models. Projectiles and projectile
configs are keyed maps without numeric ids:

- stats: `InitialValue`, `Min`, `Max`, `Shared`, `ResetType`,
  `IgnoreInvulnerability`, `HideFromTooltip` and every `Regenerating` rule
  (interval, amount, `ClampAtZero`, regen type, condition/modifier counts);
- effects: name, duration, overlap/removal behavior, infinite/debuff/icon,
  invulnerability, value type, named `StatModifiers` (stat values),
  `RawStatModifiers` (min/max modifiers), `DamageResistance`/`OutgoingDamage`
  (keyed by damage cause), damage-calculator presence/cooldown and the
  apply-condition count;
- damage causes: `Inherits`, raw damage text color, durability/stamina/bypass
  flags, hurt/death animations and `DeathMessageKey`;
- legacy projectiles: appearance, radius/height, launch offsets, muzzle/
  terminal velocity, gravity, bounciness/impact slowdown, sticks/compute
  flags, rotation mode, TTL, damage, dead/miss timers, density/water fields,
  sound ids and particle/explosion presence;
- projectile configs: model/scale, launch force, spawn offset and rotate flags,
  `Standard`/`GroundFollow` physics (density, gravity, bounce, rotation,
  terminal velocities, water/rolling fields, climb/drop).

Presentation-only fields (models, particles, sounds, cameras, UI text) are not
dumped. Spawn-rotation offsets are omitted because Java's nested
`appendInherited` conversion applies degrees→radians twice to inherited
components; the Rust config seam leaves presentation rotation to T57. The
fixture is deterministic: keys are sorted, ids come from the Java store and no
timestamps or absolute paths are written.

Regenerate and verify:

```sh
tools/java-dumper/run-entity-gameplay.sh
tools/java-dumper/run-entity-gameplay.sh --check
```

The consumer gate is
[`crates/hytale-entities/tests/entity_gameplay.rs`](../../../../crates/hytale-entities/tests/entity_gameplay.rs).
Note that the dumper opts into the entity-condition and projectile-physics codec
variants; the P0 asset goldens intentionally do not, so their item payloads stay
unchanged.
