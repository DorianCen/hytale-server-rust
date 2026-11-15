# T76 — Placement follow-ups

**Status:** blocked (session 107): mirrors/filler/yaw-offset/wall regression and the canonical block-component holder golden are landed; only the release-client `RandomRotation` trace is open
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

## Landed (sessions 105–106)

- **`CustomTemplate` mirrors:** `connected.rs` expands
  `PatternRotationDefinition` in Java order (no mirror, mirror X, mirror Z,
  with the cardinal variants after each mirror), flips rule coordinates before
  rotation, un-rotates/un-mirrors the neighbour yaw through its `FlipType`,
  rotates the packed 5-bit filler, and composes the result block's pattern
  rotation, `YawToApplyAddReplacedBlockType`, mirror and total rotation
  (including Java's orthogonal upside-down yaw adjustment). `BlockPattern`
  output strings are parsed with weights and `|Yaw/Pitch/Roll` offsets and
  selected with Java's hash-seeded `FastRandom`/`WeightedMap` semantics.
  - Real-asset mirror regressions:
    `a_rail_slope_uses_the_mirrored_custom_template_transform` (the only used
    release template with `MirrorZ`) and
    `connected::wall_mirror_tests::a_mirrored_wall_template_resolves_a_fence_corner`.
    The release fence/wall template itself has no mirror flags, so the wall
    test adds `MirrorX` to the parsed real template and drives real
    `Deco_Iron_Bars` blocks through it; it resolves the Corner block at yaw 90.
- **Placement rotation:** new `hytale-world::placement` parses every
  `RandomRotation` value, `RotationYawPlacementOffset` and `FlipType`, and
  implements `BlockFlipType.flipYaw`. `RotationYawPlacementOffset` is applied
  only to server-authored placements (NPC/admin/test seams); real session
  placements keep the client's final rotation, matching the Java server.
  `RandomRotation` is parsed but **not applied**: the reference server never
  computes it and the client renderer's selection has no local trace (see
  Blockers).
- **Block components:** no typed component codec was invented, but the opaque
  holder now has a canonical Java golden.
  `tools/java-dumper/run-block-components.sh` compiles
  `BlockComponentDumper` against the installed release `HytaleServer.jar`,
  registers the real `BlockMapMarker` codec, and
  deserializes/reserializes the real `Hunt/Generic/Site.prefab.json`
  `components` document through `ChunkStore.REGISTRY`.
  `testdata/golden/server/block-components/hunt-site.json` is committed and
  `crates/hytale-server/tests/prefab.rs` asserts the persisted opaque
  `BlockHolder` equals it verbatim. `run-block-components.sh --check` proves the
  dump is deterministic.
- **Client prediction:** no release-client trace is available locally and the
  bot does not run the client renderer's connected-shape prediction. Recorded
  as a negative result; the real-asset server tests remain the gate.

## Blockers

1. **`RandomRotation` selection.** The Java server only forwards the client's
   `BlockRotation`; the selection algorithm lives in the client renderer and is
   not in the reference sources. The Rust server therefore does not guess it.
   Closing this needs a release-client trace (or equivalent client-side
   reference) covering `None`, `YawPitchRollStep1`, `YawStep1`, `YawStep1XZ` and
   `YawStep90`. The trace seam is ready: set
   `HYTALE_PLACEMENT_TRACE=target/trace/placements.jsonl` on `serve`, place each
   target block at several positions in one client session, then run
   `tools/parse-placement-trace.py target/trace/placements.jsonl`. The
   authenticated server/client run needs the live session tokens only the user
   can provide.
2. **Full `world_thread` suite.** Four failures
   (`config_assets_are_cached_per_world`,
   `block_edits_resolve_items_and_survive_save_reload`,
   `ticks_the_view_and_unloads_chunks_that_leave_it`,
   `an_extracted_asset_pack_can_boot_a_world`) reproduce at parent `05fc66a`.
   They are pre-existing and tracked separately from T76; the targeted
   placement tests pass.

## Release-client `RandomRotation` runbook

The server-side seam and parser are ready; the remaining step is one
authenticated release-client session (the live session tokens are user-owned).

1. Start the server with the placement trace enabled and a creative world:
   ```sh
   HYTALE_PLACEMENT_TRACE=target/trace/placements.jsonl \
     ./target/debug/hytale-server serve \
       --universe target/universe --world T76 \
       --auth-mode authenticated --bind 127.0.0.1:5520 --view-radius 4
   ```
2. Launch the release client through the Hytale launcher and join
   `localhost:5520` (the launcher supplies the authenticated session).
3. Place at least eight copies of each target block without rotating the
   preview, so the requested tuple is `0`:
   - `Deco_Target` — `None`
   - `Deco_Bone_Pile` — `YawStep1`
   - `Deco_Bone_Spike_Large` — `YawStep90`
   - `Deco_SpiderWeb_Full` — `YawPitchRollStep1`
   - `Deco_Trash_Banana_Peel` — `YawStep1XZ`
   Use `/gamemode creative` and `/give` (or the creative inventory).
4. Stop the server and summarize the capture:
   ```sh
   tools/parse-placement-trace.py target/trace/placements.jsonl
   ```
5. For each block type, compare the observed tuples with the candidate
   algorithms (position-seeded LCG, `HashUtil`/`FastRandom`, or a per-placement
   client RNG). Commit the trace-derived fixture and the selected model; do not
   infer from the client renderer.

## Verification (session 107)

```sh
cargo test -p hytale-world --lib                              # 86 passed
cargo test -p hytale-world --test connected_blocks            # 9 passed
cargo test -p hytale-server --lib                             # 59 passed
cargo test -p hytale-server --test world_thread server_placements_apply_yaw_offset_and_keep_client_rotation
cargo test -p hytale-server --test world_thread placement_flags_preserve_shape_and_block_components
cargo test -p hytale-server --test prefab                     # 1 passed
cargo test -p hytale-server --test placement_trace            # 1 passed
tools/java-dumper/run-block-components.sh --check             # deterministic
cargo test -p client-bot --test block_interactions -- --ignored # 2 passed
./tools/test.sh fast                                          # 1668 passed, 34 skipped
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

## Deviations

- `RandomRotation` is parse-only until the client trace exists; server-authored
  decoration placements keep the requested tuple instead of guessing.
- `YawStep1`/`YawStep90` semantics are not claimed; only the enum wire names are
  verified.
- Multi-cell output filler is not applied by the Rust connected mutation path
  (it stores filler 0); the mirror/filler rotation is used for rule matching.
- Block-component codecs remain opaque; the canonical holder golden gates the
  persisted document, not a typed Rust component layout.

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
