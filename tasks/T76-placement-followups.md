# T76 — Placement follow-ups

**Status:** proposed (filed session 99)
**Package:** `hytale-world`, `hytale-server`, `tools/client-bot`, docs
**Depends on:** T69b, T66
**Agent fit:** medium

## Goal

Close the remaining T69b placement residuals: `CustomTemplate` mirror
transforms, block components without a Java golden/dumper, connected-shape
client-prediction parity, and `RandomRotation`/`RotationYawPlacementOffset`.

## Current gaps

- `connected.rs` parses `MirrorX`/`MirrorZ` into `CustomTemplatePattern` but
  deliberately does not expand mirrored variants.
- Some block-entity components still use the opaque `BlockHolder` path because
  no Java dumper/golden exists for a typed codec.
- Connected-shape evaluation is server-authoritative; local client prediction
  can differ if the client's shape transform is not identical.
- `BlockType.RandomRotation` and `RotationYawPlacementOffset` are parsed in Java
  but not applied by the Rust placement path, so decoration blocks can face the
  wrong way.

## Deliverables

- `CustomTemplate` mirrors:
  - port `CustomConnectedBlockPattern`/`PatternRotationDefinition` mirror
    expansion and `FlipType.flipYaw` behavior;
  - compute mirrored filler offsets and output rotation;
  - real-asset fence/wall regression with a mirror rotation.
- Placement rotation:
  - parse `RandomRotation` (`None`, `YawStep1`, `YawStep90`, ...) and
    `RotationYawPlacementOffset` from the real block type;
  - apply Java's deterministic placement RNG/step selection and yaw offset;
  - add unit/golden tests for each enum value.
- Block components:
  - extend the Java dumper/golden set for the missing components;
  - add typed codecs only where the golden exists;
  - keep the opaque `BlockHolder` path for components without a canonical dump.
- Client prediction parity:
  - if a release-client trace shows a connected-shape divergence, compare the
    client shape rotation/filler to the server result and close the gap;
  - otherwise record the negative result and keep the real-asset server tests.
- Update T69b follow-ups and verification notes.

## Read first

- `crates/hytale-world/src/connected.rs`
- `crates/hytale-server/src/world_thread.rs` (placement/update functions)
- `crates/hytale-interactions/src/catalog.rs` (`PlaceBlockFlags`)
- `crates/hytale-world/tests/connected_blocks.rs`
- `crates/hytale-server/tests/world_thread.rs` (quick flags/components)
- Java:
  - `CoreServer/.../connectedblocks/CustomConnectedBlockPattern.java`
  - `.../connectedblocks/PatternRotationDefinition.java`
  - `.../asset/type/blocktype/config/BlockType.java` (`RandomRotation`,
    `RotationYawPlacementOffset`, `FlipType`)
  - `.../modules/interaction/interaction/config/world/PlaceBlockInteraction.java`
- `tools/java-dumper/**` and `GOLDEN_DATA.md`

## Acceptance

```sh
cargo test -p hytale-world --test connected_blocks
cargo test -p hytale-server --test world_thread
cargo test -p client-bot --test block_interactions -- --ignored
./tools/test.sh fast
cargo clippy -p hytale-world -p hytale-server --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Mirrored `CustomTemplate` fences/walls select and rotate the Java variant.
- `RandomRotation`/`RotationYawPlacementOffset` placements match Java for every
  enum value.
- Any new component codec is backed by a dumper/golden; no invented layout.
- Existing connected/valley/topper/wide and quick-flag tests remain green.

## Notes

- Golden-first: do not add a block-component codec without a Java dump.
- If client-prediction parity cannot be reproduced locally, leave the task
  explicitly blocked on a release-client trace rather than guessing.
