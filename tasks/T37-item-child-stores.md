# T37 — Item child stores and packet encoders

**Status:** planned (session 25+)
**Package:** `hytale-assets`
**Depends on:** T35
**Agent fit:** medium

## Goal

Resolve the item-side child stores: quality, sound set, reticle, categories and
generated player animations. This closes the remaining item-packet deviations
that are not Rust-owned ids.

## Deliverables

- Registry collections and loaders for:
  - `ItemSoundSet` — `Audio/ItemSounds`, IndexedLookupTableAssetMap.
  - `ItemQuality` — `Item/Qualities`, IndexedLookupTableAssetMap.
  - `ItemReticleConfig` — `Item/Reticles`, IndexedLookupTableAssetMap.
  - `ItemCategory` — `Item/Category/CreativeLibrary`, DefaultAssetMap.
  - `ItemPlayerAnimations` — `Item/Animations`, DefaultAssetMap.
- Packet encoders:
  - `UpdateItemSoundSets`
  - `UpdateItemQualities`
  - `UpdateItemReticles`
  - `UpdateItemCategories`
  - `UpdateItemPlayerAnimations`
- Item encoder updates:
  - `qualityIndex` from `Quality`/`qualityId`, with Java’s default fallback.
  - quality-driven `ItemEntity` override (`ItemQuality.itemEntityConfig`).
  - `itemSoundSetIndex` from `ItemSoundSetId`/`ISS_Default`.
  - `reticleIndex` from `Reticle`.
  - generated `ItemPlayerAnimations` ids for contained item states
    (`**<state>_PlayerAnimationsId` where Java generates them).
  - category ids/strings where the protocol and item packet use them.
- Golden tests for every new packet and for the previously divergent item
  fields (818 `itemEntity`, 61 `playerAnimationsId`, `qualityIndex`,
  `itemSoundSetIndex`, `reticleIndex`).

## Read first

- `crates/hytale-assets/src/packets/item.rs`
- `crates/hytale-assets/src/registry.rs`
- Java `Item.processConfig`/`toPacket` and the child-store generators.
- The T35 `index.json` entries for these stores.

## Acceptance

```sh
cargo test -p hytale-assets
cargo test --workspace
cargo clippy -p hytale-assets --all-targets -- -D warnings
cargo fmt -p hytale-assets -- --check
```

- The item packet still covers all 3971 Java keys.
- Quality/entity, sound, reticle and generated animation differences are either
  closed or documented as unresolved Java/dumper gaps.
- All new packets decode; counts/max ids match Java.
- P0 tests and the authenticated join stay green.

## Deviations

- `EntityStatType` is a module store outside the 54-store inventory; item
  utility/stat HUD resolution is part of T53, not this task.
- Tool/weapon/armor/glider/ability payloads remain deferred (M5).
