# T55 — Projectiles and traversal

**Status:** done (session 45)
**Package:** `hytale-entities`, `hytale-assets`, `hytale-server`, `client-bot`
**Depends on:** T51, T53
**Agent fit:** low/medium

## Goal

Add the first deterministic combat-movement consumers on top of the T51
movement integrator and the T53 damage core: legacy projectile launch/flight/
block-or-entity hit/miss/bounce/expiry, plus the server-side traversal state
machine for crouch-slide, safety roll, mantling and the basic mount/dismount
seam. Packets, explosions, prediction and full new-projectile physics stay
later work.

Java splits this across `entity/entities/ProjectileComponent`,
`modules/physics/{SimplePhysicsProvider,PhysicsMath}`, the legacy
`asset/type/projectile/config/Projectile` store, the newer
`modules/projectile/config/{ProjectileConfig,StandardPhysicsConfig}` store and
the client-authoritative movement states. The port keeps the server-side
deterministic subset and uses the real assets as the oracle.

## Deliverables

- `hytale-assets`:
  - Load `Server/Projectiles` (`Projectile`, `DefaultAssetMap`) and
    `Server/ProjectileConfigs` (`ProjectileConfig`, `DefaultAssetMap`).
  - Expose name lookups for both stores.
- `hytale-entities::projectile`:
  - `ProjectileAsset::from_value` for the legacy fields the simulation uses
    (appearance, radius/height, launch offsets, muzzle/terminal velocity,
    gravity, bounciness/impact slowdown, stick, compute flags, TTL, damage,
    dead/miss timers, sounds/particles as raw values).
  - `ProjectileConfigAsset::from_value` for the new config seam (launch force,
    spawn offset/rotation, model, standard physics fields).
  - `ProjectileBody::launch` with Java `computeStartOffset`/`vectorFromAngles`
    spawn math, and `ProjectileBody::step` with gravity, terminal-velocity cap,
    swept block/entity collision, bounce/impact slowdown, stick/rest,
    hit/miss/death timers and TTL expiry.
  - A caller-supplied target list (`ProjectileTarget { id, aabb }`) so the
    server/NPC layer can feed player/entity boxes; the projectile reports the
    hit id and the damage amount for `EntityGameplay::apply_damage`.
- `hytale-entities::movement` traversal:
  - `MovementState::{sliding, rolling, mantling}` flags and a deterministic
    `TraversalState` driven by `MinSlideEntrySpeed`, `SlideExitSpeed`,
    `Min/MaxFallSpeedToEngageRoll`, `RollStart/ExitSpeedModifier` and
    `RollTimeToComplete`.
  - A concrete `mantle` helper for a blocked forward move onto a one-block
    ledge (above the auto-step height) with an explicit input request.
  - `MountState` (mount/dismount, rider position/seat offset) as the basic
    mount seam; independent rider physics while mounted stays T58/T59.
- Java fixture: extend `EntityGameplayDumper`/`run-entity-gameplay.sh` with the
  sorted `Projectiles` and `ProjectileConfigs` maps and their gameplay fields;
  add the Rust fixture gate.
- Tests: real-pack `Arrow_NoCharge`/`Ice_Ball` launch/gravity/flight, a block
  hit and miss, bounce/rest, entity-hit damage handoff, TTL expiry, slide/roll
  transitions, mantle and mount/dismount.
- Existing world/server/client-bot gates stay green.

## Acceptance

```sh
cargo test -p hytale-entities
cargo test -p hytale-assets
cargo test -p hytale-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
tools/java-dumper/run-entity-gameplay.sh --check
```

- A projectile launched from a real asset follows the expected ballistic arc
  and stops/reflects on the first block or entity hit.
- A miss reports `Miss`, a hit reports the target id and applies the configured
  damage through the T53 health core; a low-speed bounce comes to rest and TTL
  expiry removes the projectile.
- Sliding starts at/above the configured entry speed and ends at/below the exit
  speed; a landing above the roll threshold starts a timed safety roll; a
  requested mantle raises a body onto a one-block ledge.
- Mount/dismount moves the rider with the carrier and restores independent
  movement on dismount.

## Deviations to report or split

- Legacy `Projectile` assets drive the scripted simulation. The newer
  `ProjectileConfig` `StandardPhysics` density/drag/buoyancy/rolling and
  `GroundFollow` physics are loaded and decoded but not simulated yet.
- Java is client-authoritative for slide/roll/mantle input; the Rust traversal
  state machine is the deterministic server-side model for later NPC/combat
  producers and tests.
- Projectile spawn/despawn/effect packets, explosions, particles/sounds,
  predicted projectiles, pierce/directional hitboxes, mounts as world entities,
  traversal animation and anti-cheat stay T56/T57/T58/T59/T62.
- No live client projectile loopback until the T57 presentation packets exist.
- Java's nested `appendInherited` conversion applies degrees→radians twice to
  inherited `SpawnRotationOffset` components; the config seam omits presentation
  spawn rotation rather than normalising that quirk.

## Landed (session 45)

- `hytale-assets` loads `Server/Projectiles` (86) and
  `Server/ProjectileConfigs` (188) with name lookups; `--validate-assets`
  reports 27 stores.
- `hytale-entities::projectile` has the legacy `ProjectileAsset` and new
  `ProjectileConfigAsset`/`StandardPhysics` decoders, Java launch-offset/
  angle math, a deterministic gravity/terminal-velocity/bounce/stick/TTL
  integrator, swept block and entity AABB collision, block rest/miss, entity
  hit ids and the T53 damage handoff.
- `hytale-entities::movement` adds slide/roll/mantle flags, the
  `TraversalState` entry/exit/roll-duration machine, `try_mantle`, the
  `MountState` seam, crouch-height collision for sliding/rolling and session
  movement-state mapping.
- The Java entity-gameplay fixture now contains the sorted projectile and
  config maps; `entity_gameplay.rs` checks every modelled field and scripts the
  real `Arrow_NoCharge` arc/wall miss and `Ice_Ball` entity hit + health damage.
- Unit tests cover slide entry/exit, the safety-roll window/expiry, mantle and
  mount/dismount.

## Follow-ups

- T56 spawns/despawns projectile and item entities in the world and persists
  them.
- T57 sends the projectile/effect/damage presentation packets and adds
  `UpdateProjectileConfigs` to the config burst.
- T58/T59 feed NPC target boxes and mount entities into the projectile/mount
  seams and simulate the new `ProjectileConfig` physics.
- T62 adds packet-rate and spawn-abuse limits.
