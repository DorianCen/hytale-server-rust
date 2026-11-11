# T69b — Block placement follow-ups

**Status:** done (session 98; patterned connected blocks, placement flags,
block-entity metadata)
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

## Landed

- **Connected-block rule sets:** new `hytale-world::connected` catalog parses
  the `Patterned` rule-set assets plus the deprecated `CustomTemplate` templates
  (fence/wall), `Stair` and `Roof` rule sets. It evaluates templated shapes,
  custom face-tag/shape rules, face tags, `Shape` rules and `And`/`Or`/`Not`,
  and ports Java's 90-degree rotation matrix composition, shape-relative
  rotations, bounded 3-deep neighbour BFS and the
  `PlaceUpdate`/`UpdatePlaceUpdate`/`IgnoreUpdates` modes. Roof evaluation also
  covers valley/reverse-valley corners, four-way toppers and width-aware
  additional-block splitting.
- **Integration:** `WorldThread` keeps one parsed catalog per world. Placement,
  block break and `/setblock` re-evaluate the changed cell and its neighbours;
  resulting variants/rotations are queued as `BlockChangeEvent`s so sessions
  broadcast the authoritative state.
- **Placement flags:** `PlaceBlockFlags` resolves `BlockTypeToPlace`,
  `RemoveItemInHand`, `QuickReplace`, `QuickRetype` and `NoPhysics` from the
  root interaction graph. Creative-only quick flags are applied by
  `PlaceBlockOptions`; both quick-retype and quick-replace keep the target block's
  rotation and compatible shape variant, and no-physics suppresses the scheduled
  support/physics pass.
- **Block entities:** placement accepts a Java `BlockHolder` document from item
  metadata and stores it in the existing `block-components.json` store. The
  generic component map is preserved without inventing a new codec.
- **Tests:** real-asset patterned pipe shape/neighbour and negative cases,
  an iron-bars `CustomTemplate` corner, stair straight/corner resolution,
  unsupported-roof hollow, roof valley corner, four-way roof topper and a wide
  roof splitting its old corner arm (`hytale-world/tests/connected_blocks.rs`),
  world-thread quick-retype/
  quick-replace shape/rotation preservation, no-physics and block-component
  save/reload (`hytale-server/tests/world_thread.rs`), and nested/real flag
  parsing (`hytale-interactions/tests/interactions.rs`).

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

## Verification (session 98)

```sh
cargo test -p hytale-server --test world_thread placement_flags_preserve_shape_and_block_components
# 1 passed (quick retype/replace, noPhysics, component save/reload)

cargo test -p hytale-world
# all suites passed, including connected_blocks (8 tests: pipe, fence,
# stair, roof hollow/valley/topper/wide and negative)

cargo test -p hytale-interactions
# 8 integration tests plus 5 unit tests passed

cargo test -p client-bot --test block_interactions -- --ignored
# 2 passed (container window/map image and door state)

./tools/test.sh fast
# 1632 passed, 29 skipped

cargo clippy -p hytale-server -p hytale-interactions -p hytale-world --all-targets -- -D warnings
cargo fmt --all -- --check
# both clean
```

## Deviations

- `CustomTemplate` cardinal pattern transforms are ported and tested against the
  real iron-bars fence template. Mirror transforms are parsed but not expanded;
  the fence/wall templates in the release pack use cardinal rotations.
- `Stair` and `Roof` cover straight/corner variants, roof hollow/valley/topper
  selection and width-aware additional-block splitting. Multi-cell filler
  packing is validated against the Java 5-bit filler layout.
- Quick-retype and quick-replace both keep the old rotation and resolve the
  equivalent shape variant when the new block uses a compatible connected rule
  set. The acceptance requires preserved shape/rotation for both flags.
- Placement-created block entities use the opaque Java `BlockHolder` document
  from item metadata. No new per-component codec was invented; components without
  a Java dumper/golden remain future work.

## Follow-ups

- Expand `CustomTemplate` mirror transforms if a release-client scenario
  exercises them.
- Components that still lack a Java golden/dumper.
- Client prediction parity for connected shapes if a release-client trace shows
  a divergence.
- Remaining `RandomRotation`/`RotationYawPlacementOffset` parity if a client
  scenario exposes it.
