# T51 — Player movement physics and collision authority

**Status:** done (session 39)
**Package:** `hytale-world`, `hytale-entities`, `hytale-assets`, `hytale-server`, `client-bot`
**Depends on:** T48
**Agent fit:** low/medium

## Goal

Make the server the authority for player position updates: validate every
`ClientMovement` target against Java's finite/bounds rules, resolve it against
the block collision shapes the client walks through, and keep the accepted
position stable so a normal client loop is never rubber-banded.

Java remains client-authoritative for the walking simulation itself
(`PlayerInput.AbsoluteMovement`/`RelativeMovement` call `Player.moveTo`), but it
does reject non-finite positions/orientations and treats unloaded blocks as
solid. This task ports that validation seam plus the collision geometry the
server needs for movement, and adds deterministic gravity/jump/step helpers for
server-side simulation and T58 NPC use.

## Deliverables

- `hytale-world`:
  - `collision` module with `Aabb`, `BlockCollisionTable` built from
    `BlockType` material + `HitboxType` + `BlockBoundingBoxes`, including Java's
    64 rotation slots and `FillerBlockUtil` origin handling.
  - `CollisionWorld`/`WorldCollision` query over loaded chunks.
  - `World::{get_rotation, get_filler}`.
- `hytale-assets`:
  - internal `MovementConfig` store (`Server/Entity/MovementConfig`) with the
    Java `DEFAULT_MOVEMENT` fallback, every packet field and optional `Fly`.
  - `GameplayConfig.Player.MovementConfig` selection.
- `hytale-entities`:
  - `movement` module: Java player hitbox (`0.65 × 1.85 × 0.65`), AABB sweep
    resolver with axis order Y→X→Z, one-block step-up, ground/ceiling contacts
    and overlap escape.
  - deterministic `MovementBody` integrator using Java gravity (`32`), jump,
    swim-jump, fly and climb values from `MovementSettings`.
  - `MovementTuning`, `MovementInput`, `MovementState`, `MovementBounds`.
- `hytale-server`:
  - load the selected movement config once per world and send
    `UpdateMovementSettings` (id `110`) during the join burst.
  - validate `ClientMovement.absolutePosition`/`relativePosition` and directions
    with Java `ValidateUtil` semantics; disconnect on non-finite values, reject
    clearly impossible coordinates and count rejections.
  - resolve accepted target positions through the world collision table, apply
    step-up, send an authoritative `ClientTeleport` only when the resolved
    position differs, and count corrections.
  - keep the existing physical view update and 10 Hz other-player broadcast on
    the resolved position.
- `client-bot`:
  - movement loopback assertions that a normal walk is accepted with zero
    corrections (no rubber-banding).
  - a scripted collision case where a move into a placed wall is corrected.
- Tests:
  - pure `hytale-entities` walk/jump/fall/collide/step/anti-cheat cases with a
    documented `1e-4` block tolerance and config defaults checked against Java
    `DEFAULT_MOVEMENT`.
  - real-pack collision table test (`Rock_Stone`, half slab, bench hitbox,
    rotation) and movement config load.
  - world-thread movement resolution against placed blocks.
  - existing `hytale-server`, `client-bot` join/trace suites stay green.

## Read first

- Java `GamePacketHandler.handle(ClientMovement)`, `ValidateUtil`,
  `PlayerInput`, `PlayerProcessMovementSystem`, `MovementManager`,
  `MovementConfig`, `BlockBoundingBoxes`, `BlockDataProvider`,
  `BlockCollisionProvider`, `CollisionMath`, `PhysicsConstants`
- `crates/hytale-assets/src/{registry,gameplay}.rs`
- `crates/hytale-world/src/{world,player_data}.rs`
- `crates/hytale-entities/src/lib.rs`
- `crates/hytale-server/src/{session,world_thread}.rs`
- `tools/client-bot/src/lib.rs`, `tests/multi_join.rs`

## Acceptance

```sh
cargo test -p hytale-entities
cargo test -p hytale-assets
cargo test -p hytale-world
cargo test -p hytale-server
cargo test -p client-bot --test movement_join -- --ignored
cargo test -p client-bot --test multi_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A normal absolute walk is accepted bit-for-bit and is broadcast at the same
  position.
- A move into a solid wall is clamped to the wall face; a half-block obstacle is
  stepped onto; a 1.5-block wall is not.
- Non-finite positions/orientations are rejected; impossible coordinates never
  reach the world thread.
- The movement config packet matches the selected `MovementConfig` asset and
  Java defaults when the asset is missing.
- Existing block editing, inventory, interaction, trace and reconnect paths
  remain green.

## Deviations

- The Java server does not simulate player walking; the client sends absolute
  positions. The Rust port keeps that model and only validates/resolves the
  target. `MovementBody` is the deterministic integrator shared with later NPC
  work, not a replacement for client input.
- The collision pass uses the solid `BlockMaterial` rule only. Fluid fill
  levels, submerge/damage/trigger boxes, ladders as special blocks, mounts,
  parkour/mantling, fall damage and entity collisions remain T52/T55/T58.
- Unloaded chunks are treated as air for movement so a crossing into the next
  view chunk is not blocked; Java treats them as unknown/solid. The view update
  still runs after the accepted move.
- Filler blocks use the Java origin rule for collision; client-side filler
  placement/rotation corrections remain T46/T52.
- Absolute movement is accepted bit-for-bit when the whole path is free. This
  intentionally departs from a strict per-packet speed check so the existing
  bot/debug teleports keep working; real anti-cheat rate limits are T62.
- Correction teleports use an incrementing id but do not yet have Java's
  `TeleportAckTracker` queue; the existing session accepts any ack. Full ack
  tracking is T62.
- The one-block step height is the block-grid auto-jump ceiling; Java exposes
  slope/wall angles, not a scalar step height.
- `MovementBody` is covered by unit tests against the Java constants and is not
  yet driven by network input (the client owns walking simulation).

## Follow-ups

- T52 adds fluids, ticking and support; T55 adds projectiles/traversal; T58
  consumes `MovementBody` for NPC navigation.
- T62 adds real movement rate/teleport anti-cheat with per-packet timestamps.
- T57 presents movement/block-health UI state.
