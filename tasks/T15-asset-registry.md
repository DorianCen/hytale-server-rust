# T15 — P0 asset registry and typed stores

**Status:** done (session 5)
**Package:** `hytale-assets`, `hytale-world`
**Depends on:** T08 (store inventory), T09 (pack + `Parent` resolver)
**Agent fit:** medium

## Goal

Load the P0 asset stores from an unmodified pack, resolve `Parent` chains with
the Java `AssetBuilderCodec` rule, assign numeric ids once, and expose the typed
fields the world generator and chunk heightmap need. This replaces the
golden-registry stand-in on the runtime path; the committed dumps remain the
test oracle.

## Inputs

- `Assets/Assets.zip` / `Assets/extracted` (base-game pack).
- `tools/asset-golden-dumper/asset-stores.json` (P0 store paths).
- `testdata/golden/server/assets/index.json` (Java ids for the simple stores).
- `testdata/golden/registries.json` (Java block/fluid/environment semantics).

## Deliverables

- `hytale-assets::registry`:
  - `AssetPack::list_in_storage_order` and `read_many` (one zip open per store).
  - `MergeHook::Shallow` / `merge_shallow` for Java field replacement.
  - P0 store loader (`AssetRegistry::load_p0`), preloaded assets, deterministic
    ids, contained block-type extraction from item documents, item-state and
    nested `State.Definitions` keys.
  - Typed `BlockTypeAsset` (draw type, opacity, material, support with all 64
    rotation probes) and `FluidAsset` (max level).
- `hytale-world::registry::registries_from_assets`: asset registry → the
  `worldgen-model` registry seam.
- Tests:
  - fixture pack for loading, parent inheritance, item/state extraction and
    support defaults;
  - real-pack gates: `BlockBoundingBoxes`/`Fluid`/`Environment`/
    `PhysicalMaterial` ids match `index.json`; every golden block's draw type,
    opacity and 64 support probes match `registries.json`; no golden block key
    is missing.

## Acceptance

```sh
cargo test -p hytale-assets
cargo test -p hytale-world
cargo clippy -p hytale-assets -p hytale-world --all-targets -- -D warnings
cargo fmt --all -- --check
```

- No golden file is read by the runtime loader.
- No Java source is modified.
- The only block keys beyond the golden set are documented Java load-failure
  extras (`ExamplePrefabPlacementBlock_Tree`, two `PortalKey_*` blocks).

## Follow-ups

- P1 stores and the 39 asset packet encoders.
- Java-exact `BlockType`/`Item` packet bytes need the Java contained-asset
  `HashMap` load order; the Rust snapshot currently owns its own order after
  the preloads and standalone block files.
- Wire the bridge into the headless server boot and the chunk heightmap path.
