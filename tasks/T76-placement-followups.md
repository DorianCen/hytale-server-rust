# T76 — Placement follow-ups

**Status:** done (session 105; mirrors, filler, placement rotation, component golden)
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

## Landed (session 105)

- **`CustomTemplate` mirrors:** `connected.rs` now expands
  `PatternRotationDefinition` in Java order (no mirror, mirror X, mirror Z,
  with the cardinal variants after each mirror), flips rule coordinates before
  rotation, un-rotates/un-mirrors the neighbour yaw through its `FlipType`,
  rotates the packed 5-bit filler, and composes the result block's pattern
  rotation, `YawToApplyAddReplacedBlockType`, mirror and total rotation
  (including Java's orthogonal upside-down yaw adjustment). `BlockPattern`
  output strings are parsed with weights and `|Yaw/Pitch/Roll` offsets and
  selected with Java's hash-seeded `FastRandom`/`WeightedMap` semantics.
  The new real-asset regression is `a_rail_slope_uses_the_mirrored_custom_template_transform`:
  `Rail`'s `Slope` shape only matches through `MirrorZ` and resolves to the
  `Slope` state at yaw 180. The release pack's fence/wall templates do not set
  `MirrorX`/`MirrorZ`; rails are the real mirror case.
- **Placement rotation:** new `hytale-world::placement` parses
  `RandomRotation`, `RotationYawPlacementOffset` and `FlipType`, implements
  `BlockFlipType.flipYaw`, and provides a position-seeded deterministic
  `RandomRotation` selection plus the additive yaw offset. `PlaceBlockOptions`
  gained `client_rotation`: real session placements keep the client's final
  rotation (matching the Java server, which never recomputes it), while
  server-authored placements (NPC interactions/admin/test seams) apply the
  block-type metadata. Unit tests cover every enum value and the
  `world_thread` regression uses the real `Furniture_Goblin_Brazier`
  (`TwoSeventy`) and `Deco_Bone_Pile` (`YawStep1`) assets.
- **Block components:** no typed component codec was invented. The canonical
  prefab deserializer used by `tools/java-dumper` deliberately skips block
  component holders, so there is no Java component dump to gate a codec
  against. `crates/hytale-server/tests/prefab.rs` now asserts that the pasted
  `BlockHolder` document is byte-for-byte the `Components` object in the real
  `Hunt/Generic/Site.prefab.json`; the opaque store remains the runtime path.
- **Client prediction:** no release-client trace is available locally and the
  bot does not run the client renderer's connected-shape prediction. This is
  recorded as a negative result; the real-asset server tests remain the gate.

## Verification (session 105)

```sh
cargo test -p hytale-world --lib                              # 87 passed
cargo test -p hytale-world --test connected_blocks            # 9 passed
cargo test -p hytale-server --lib                             # 59 passed
cargo test -p hytale-server --test world_thread server_placements_apply_block_type_rotation_metadata
cargo test -p hytale-server --test world_thread placement_flags_preserve_shape_and_block_components
# The full world_thread suite still has 4 pre-existing failures
# (config cache ordering, save/reload, view unload, extracted-pack boot);
# they reproduce at parent 05fc66a and are unrelated to T76.
cargo test -p hytale-server --test prefab                     # 1 passed
cargo test -p client-bot --test block_interactions -- --ignored # 2 passed
./tools/test.sh fast                                          # 1668 passed, 34 skipped
cargo clippy -p hytale-world -p hytale-server --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Deviations

- The Java reference server does not recompute `RandomRotation` or
  `RotationYawPlacementOffset` for player placements; it stores the client's
  `BlockRotation`. Rust keeps that behavior when `client_rotation` is set and
  only applies the metadata to server-authored placements. The random
  selection is position-seeded (`HashUtil.hash(x,y,z)` + `FastRandom`) because
  the client-side renderer algorithm has no server source or local trace.
  `YawStep1` and `YawStep90` collapse to the same candidate set because the
  wire `Rotation` only has 90-degree steps.
- Multi-cell output filler is not applied by the Rust placement path (it
  stores filler 0 for connected mutations); the mirror/filler rotation is used
  for rule matching. Existing T69b filler-layout coverage remains the only
  placement-filler gate.
- Block-component codecs remain opaque until the Java dumper can boot and dump
  the component holders.

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
