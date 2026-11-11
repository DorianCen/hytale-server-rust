# T69b — Block placement follow-ups

**Status:** proposed (filed 2026-11-11 from T69's deliberate scope limit)
**Package:** `hytale-server`, `hytale-assets`, `hytale-interactions`,
`hytale-world`
**Depends on:** T50, T66, T69
**Agent fit:** medium/high

## Goal

Close the placement-fidelity work intentionally deferred by
[T69](T69-block-placement-fidelity.md). T69 covers the authoritative placement
cell, client rotation, `/setblock`, neighbour support/falling simulation and
rotation persistence. This packet covers the remaining state-variant and
interaction-flag behavior.

## Read first

- Java `CoreServer/.../modules/interaction/interaction/config/client/PlaceBlockInteraction.java`
  (`quickReplace`, `quickRetype`, `noPhysics`, `removeItemInHand`, block rotation
  reconciliation).
- Java `CoreServer/.../asset/type/blocktype/config/BlockType.java`
  (`getVariantRotation`, `getRotationYawPlacementOffset`, `getFlipType`,
  `RandomRotation`, connected/supporting maps).
- Java `CoreServer/.../asset/type/connectedblock/**` (or the
  `ConnectedBlockRuleSet` store) and the `BlockTypeConnectedBlockShape`
  evaluation path.
- Rust `crates/hytale-assets/src/packets/mod.rs` (the already-encoded
  `ConnectedBlockRuleSet` packet data) and
  `crates/hytale-server/src/session.rs` (`handle_block_interaction`,
  `try_block_use`, `patched_interaction_data`).
- Rust `crates/hytale-world/src/world.rs` (`set_block_full`, block components,
  containers) and `crates/hytale-world/src/simulation.rs`.

## Deliverables

### 1. Connected-block state variants

- Load/evaluate `ConnectedBlockRuleSet` server-side for a placed block and its
  neighbours (face tags, shape rules, `And`/`Or`/`Not`, templated shapes).
- Resolve the resulting block type key/state variant and apply it with the
  current rotation/filler through the T69 API.
- Schedule neighbour re-evaluation after placement, break and `/setblock`.
- Tests against real pack assets: at least one connected shape (e.g. fence,
  wall or pipe) and a negative case with no rule match.

### 2. `PlaceBlockInteraction` flags

- Parse the interaction's `quickReplace`, `quickRetype`, `noPhysics` and
  `removeItemInHand` fields from the chain/asset path where they are available.
- `quickReplace`/`quickRetype`: swap the targeted block type while preserving
  shape/rotation, matching Java's creative-only rule.
- `noPhysics`: skip the neighbour/physics scheduling for that edit while still
  broadcasting and persisting the block.
- Tests: scripted chains for each flag and a regression that normal placement
  still consumes the held item and schedules physics.

### 3. Block-entity metadata

- Extend the existing container-only block entity storage to persist additional
  `BlockEntity.Components` documents that placement can create (state/marker
  components used by the real pack).
- Keep the Java component-id/JSON shape; add a save/reload test and a
  placement→save→reopen→inspect path.
- Do not invent codecs for components with no Java dumper/golden; list those as
  follow-ups instead.

## Acceptance

```sh
./tools/test.sh fast
cargo test -p hytale-server --lib
cargo test -p hytale-interactions
cargo test -p hytale-world
cargo test -p client-bot --test block_interactions -- --ignored
cargo clippy -p hytale-server -p hytale-interactions -p hytale-world --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A connected block resolves its variant and neighbour update from real assets.
- `quickReplace`/`quickRetype` preserve shape/rotation; `noPhysics` suppresses
  the scheduled physics pass.
- Placement-created block entity components survive save/reload.
- Existing T69 rotation/`/setblock`/falling tests stay green.

## Follow-ups

- Components that still lack a Java golden/dumper.
- Client prediction parity for connected shapes if a release-client trace shows
  a divergence.
- Remaining `RandomRotation`/`RotationYawPlacementOffset` parity if a client
  scenario exposes it.
