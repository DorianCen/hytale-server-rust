# T33 — BlockType and Item config asset packets

**Status:** done (session 23)
**Package:** `hytale-assets`, `hytale-server`, `client-bot`
**Depends on:** T32 (Environment/Fluid + tag registry), T15 (asset registry)
**Agent fit:** low/medium (split into two independent encoders)

## Goal

Stream the two remaining P0 content stores in the join flow: `BlockType`
(`UpdateBlockTypes`, id 40) and `Item` (`UpdateItems`, id 54). Both are built
from the same Rust registry snapshot that worldgen and storage use.

`BlockType` ids are Rust-owned (see `hytale-assets::registry`), so the payload
cannot be byte-compared with `BlockType.bin`. The golden payload is still the
oracle: the test decodes it with the generated Rust codec and compares every
field that is not Rust-owned (ids, groups, tags, child-store indexes) by asset
key. `Item` keys are strings, so the same key-based comparison covers the item
payload.

This task lands the join-critical/core fields. The large nested trees that need
stores the server does not stream yet (`states`, `interactions`, `bench`,
`connectedBlockRuleSet`, tool/weapon/armor payloads) are explicitly deferred and
recorded under Deviations.

## Deliverables

### Registry

- `BlockTypeAsset::container_item`: the top-level item document that contains a
  block type (`BlockType.getItem()`), `None` for standalone/preloaded blocks.
- Expose the existing `Face` alias expansion so the packet encoder can build
  Java's `support`/`supporting` maps.

### `hytale-assets::packets::block_type`

`AssetRegistry::block_type_packet(&TagRegistry)` → `UpdateBlockTypes`:

- `Init`, `maxId = count`, `updateBlockTextures/updateModelTextures/
  updateModels/updateMapGeometry = true`, ascending Rust id keys.
- Per block: `name`, `item` (container item), `unknown`, `drawType`,
  `material`, `opacity`, `shaderEffect` (Java default `[None]`), `hitbox` /
  `interactionHitbox` resolved through `BlockBoundingBoxes` (default `Full`,
  missing interaction hitbox = Java `NOT_FOUND`), `model`, `modelTexture`,
  `modelScale`, `modelAnimation`, `modelAnimationSpeed`, `looping`,
  `maxSupportDistance`, `blockSupportsRequiredFor`, `requiresAlphaBlending`,
  `cubeTextures`, `cubeSideMaskTexture`, `cubeShadingMode`, `randomRotation`,
  `variantRotation`, `rotationYawPlacementOffset`, `physicalMaterialIndex`,
  `soundOcclusionOpacity`, `blockParticleSetId`, `blockBreakingDecalId`,
  `particleColor`, `textureComputedColor`, `light`, `tint`, `biomeTint`,
  `group` (Rust-owned deterministic group ids), `transitionTexture`,
  `transitionToGroups` (same group registry), `movementSettings`,
  `interactionHint`, `support`, `supporting`, `gathering`, `rail`,
  `ignoreSupportWhenPlaced`, `tagIndexes`.
- Java's `processConfig` defaults: interaction hint, `support` for empty
  material outside `@Tech`, `supporting` for solid cubes, default hitbox and
  physical material, default model textures/shader.
- `blockSoundSetIndex` / `ambientSoundEventIndex` stay `0` until those stores
  are streamed.

### `hytale-assets::packets::item`

`AssetRegistry::item_packet(&TagRegistry)` → `UpdateItems`:

- `Init`, `updateModels/updateIcons = true`, one entry per registry item keyed
  by the asset key.
- Per item: `id`, `model`, `scale`, `texture`, `animation`,
  `playerAnimationsId`, `usePlayerAnimations`, `maxStack` (Java's `-1`
  fallback), `reticleIndex`, `icon`, `iconProperties`,
  `translationProperties`, `itemLevel`, `qualityIndex`, `resourceTypes`,
  `consumable`, `variant`, `blockId` (Rust `BlockType` id for the contained
  block), `utility`, `itemEntity` (Java defaults for block/item),
  `set`, `categories`, `subCategory`, `particles`, `firstPersonParticles`,
  `light`, `durability`, `carryInteractions`, `carryHudInputBindings`,
  `interactionVars`, `interactionConfig` (Java defaults),
  `droppedItemAnimation`, `tagIndexes`, `displayEntityStatsHUD`,
  `clipsGeometry`, `renderDeployablePreview`.
- `soundEventIndex`, `itemSoundSetIndex`, interaction ids, tool/weapon/armor
  payloads stay `0`/`None` until those stores are streamed (Deviations).

### Join flow

- `config_assets()` order becomes `[41, 87, 83, 61, 40, 54]` so the
  `BlockType` packet can resolve hitbox/physical-material ids and `Item` can
  resolve block ids.
- `client-bot` recognises the two packet ids and the auth loopback test checks
  them.

### Tests

- `crates/hytale-assets/tests/block_item_packets.rs` decodes `BlockType.bin` and
  `Item.bin` and compares the implemented fields by `name`/key:
  - counts (6135 Java blocks, 3971 Java items; the Rust registry holds 6138
    blocks and 4008 items because it keeps assets the canonical Java dump could
    not decode), `maxId`, packet flags and config order;
  - every Java-loaded key is present;
  - exact field comparison for the core fields, with `support` normalized over
    Rust-owned ids and representative blocks (`Rock_Stone`, `Soil_Grass`,
    `Fluid_Water`) checked field-for-field;
  - item `blockId` compared by block name, `interactionConfig` (including
    priorities), translations and icons compared directly;
  - all tag indexes (including `transitionToTag`) present in `ServerTags`;
  - item keys that Rust holds but Java failed to decode are reported, not
    treated as mismatches.
- Both generated payloads decode with the generated codecs; the item map is
  sorted and duplicate-free.
- `client-bot` auth loopback still completes the join and the config list
  contains `40` and `54`.

## Acceptance

```sh
cargo test -p hytale-assets --test block_item_packets
cargo test -p hytale-assets --test asset_packets
cargo test -p client-bot --test auth_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The authenticated join reports config packets `[41, 87, 83, 61, 40, 54]`.
- Every Java-loaded block/item key is present with the core fields equal.
- The bot still completes the teleport/movement loop.

## Deviations

- Rust-owned block ids, group ids and tag indexes (existing policy). Id
  references are compared/sent through the Rust registries.
- Item `State` contained assets (`*<item>_State_<name>`) are now extracted and
  resolved; the 37 Rust-only items are the ones the canonical Java dump failed
  to decode.
- `blockSoundSetIndex`, `ambientSoundEventIndex`, `soundEventIndex`,
  `itemSoundSetIndex`, `qualityIndex`, `reticleIndex` and interaction ids are
  `0` because those child stores are not loaded/streamed.
- Inherited values that Java reads from an already-`processConfig`-ed parent
  differ on a small set: 24 `supporting` maps, 10 `interactionHint`s, 10
  `textureComputedColor`s, 3 `particleColor`s, 2 `blockParticleSetId`s, 1
  `biomeTint`, plus the code-defined `Debug_*`/`Spawner_Rat` variant/model
  entries. The test compares these on representative fully-resolved blocks and
  records the known differences.
- 818 `itemEntity` values depend on `ItemQuality`; 61 `playerAnimationsId`s
  depend on generated `ItemPlayerAnimations` child assets; 3 `utility`
  payloads (`Food_Salad_*`) differ in Java's decoded `Compatible` flag. Those
  stores are not ported yet.
- `BlockType.states`, `interactions`, `bench`, `connectedBlockRuleSet`,
  `placementSettings`, `conditionalSounds` and `display` are not emitted yet.
- `Item.tool`, `weapon`, `armor`, `gliderConfig`, `ability`,
  `blockSelectorTool`, `builderToolData`, `trails`, `itemAppearanceConditions`,
  `pullbackConfig`, `hudUI` and named `displayEntityStatsHUD` are not emitted
  yet.
- Payload sizes are therefore smaller than `BlockType.bin`/`Item.bin`; the
  key-based golden comparison covers what is emitted.

## Follow-ups

- Stream `BlockSoundSet`, `AmbientSoundEvent`, `ItemSoundSet`, `FluidFX` and
  `BlockParticleSet` so indexes and sound/particle references become real.
- Emit block states, interaction ids, benches, connected-block rules and item
  tool/weapon/armor payloads.
- Add `UpdateTagPatterns` and the remaining P1/P2 stores.
