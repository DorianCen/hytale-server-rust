# REMAINING.md — Deferred scope

The Rust rewrite targets **world generation only** (terrain, caves, prefabs, world layout)
with bit-exact parity against the Java `WorldGenerator`. The following Java functionality
is explicitly **deferred** and not ported:

## 1. Modifier / event plugin system
- Source: `com.hypixel.hytale.builtin.worldgen.modifier.**`
  (`content/{cave,common,cover,fluid,layer,prefab,tint}`, `event`, `op`).
- The `BlockPopulator`-driven content pipeline that registers named block modifiers
  (`"!Rock_Bedrock"` style mask entries, tint ops, fluid ops) is replaced by the direct
  Rust populators; a pluggable modifier trait is kept at the
  `BlockPriorityChunk` boundary so this can be re-added later.
- The cave loaders dispatch the Java `ModifyEvents.CaveTypes` and
  `ModifyEvents.CaveCovers` events; the Rust port skips the dispatch (the
  cave configuration of the JSON file is loaded unchanged).

## 2. Entity serialization from prefabs
- Prefab `.prefab.json` entity lists and `PrefabPasteUtil` entity spawning are stubbed.
- Only blocks/fluids/environments from prefabs are pasted.
- The same deferral is mirrored on the golden-dumper side:
  `tools/java-dumper` carries a **shadowed copy** of the CoreServer
  `BsonPrefabBufferDeserializer`
  (`tools/java-dumper/src/main/java/com/hypixel/hytale/server/core/prefab/
  selection/buffer/BsonPrefabBufferDeserializer.java`, loaded because the
  dumper's `target/classes` precedes the module jars on its classpath)
  whose two patches skip the entity decode AND the block "components"
  state decode — the vanilla decode of both fails on the entity component
  codecs of the builtin plugins (the Camera plugin's `CameraShake` …),
  which the dumper boot does not load. The chunk-level golden dump
  (`testdata/golden/chunks.json`) therefore covers exactly the Rust parity
  target: blocks, fluids, environments, tint, priorities — no entities, no
  components, no BlockPhysics support values.
- The prefab **components** (the ECS state of a prefab block) are stubbed as
  well: the `.prefab.json` reader counts them (`PrefabJsonStats`) but does
  not deserialize them.
- The **prefab store** (the resolution of a prefab *name* into a prefab
  buffer) is ported (Phase 5a): `prefab::prefab_store::PrefabStore` resolves
  the names (the dotted names and the `".*"` wildcards) into the prefab
  files and loads the `.prefab.json` files into a `PrefabBuffer`.

## 3. CoreServer / ECS integration
- `Holder<ChunkStore>`, chunk entity model, network serialization, and the
  `HytaleWorldGenProvider` plugin lifecycle are not ported.
- The boundary is the minimal `trait WorldGen` returning `GeneratedChunk`.

## 4. World map UI
- Source: `com.hypixel.hytale.server.worldgen.map` (live map rendering) — not ported.
  Offline PNG rendering in `worldgen-cli` covers verification needs.

## 5. `.lpf` prefab cache format
- JSON (`.prefab.json`) is the source of truth. The binary `.lpf` cache format is only
  investigated if profiling shows prefab parsing is a bottleneck.
- The base-game asset pack ships no `.lpf` file, so nothing is lost by the
  current port (the `PrefabStore` still resolves the `.lpf` sibling first,
  exactly like the Java `PrefabBufferUtil.loadBuffer`).

## 6. BlockType definition assets — FOUND (the registries are now golden data)
- The 3.7 GB base-game asset pack **has been provided** and is extracted under
  `Assets/` (git-ignored — the zip plus the extracted tree are several GB of
  game content). It fills the world-side gaps that were open before:
  - `Assets/extracted/Server/Prefabs` — ~9.3k `*.prefab.json` files, i.e. the
    `PrefabStore: ASSETS` store the Default world references.
  - `Assets/extracted/Server/Environments` — `Default.json` plus `Dungeons/`,
    `Legacy/`, `Unique/`, `Zone0` … `Zone4`.
  - `Assets/extracted/Server/Item/Block/Fluids` — the 17 fluid item assets.
  - `Assets/extracted/Common/TintGradients` — the tint gradient assets.
  - `Assets/extracted/Server/BlockTypeList/*.json` — the block type **name lists**
    (`Rock.json`, `Soils.json`, `Fluids.json`, …).
  - `Assets/extracted/Server/World/Default` — byte-identical to `testdata/Default`,
    so the world-side assets are complete.
- **THE BLOCK DEFINITIONS WERE NOT MISSING.** They are the `"BlockType"`
  objects **embedded in the item assets** (`Server/Item/Items/**`, 3,960 files,
  3,178 of which carry a `"BlockType"`):
  - `Server/Item/Items/Rock/Aqua/Rock_Aqua.json` contains a `"BlockType": {…}`
    object next to the item's own `"Parent": "Rock_Stone"`.
  - `Item.CODEC` registers that object in the **BlockType asset store** as a
    *child* asset via
    `ContainedAssetCodec(BlockType.class, BlockType.CODEC, Mode.INHERIT_ID_AND_PARENT)`
    (`AssetStore/…/codec/ContainedAssetCodec.java`): the block id is inherited
    from the container item's key (`Rock_Aqua`) and the parent is inherited from
    the item's `"Parent"` (`Rock_Stone`), so the blocks form a parent chain.
  - The numeric ids are handed out by `BlockTypeAssetMap.putAll0` in
    **asset-load order** (the four hard-coded `preLoadAssets` — `Empty`,
    `Unknown`, `Debug_Cube`, `Debug_Model` — are ids 0…3), so the ids can only
    be obtained by running the real asset pipeline.
- **The registries are now golden data.** `tools/java-dumper`
  (`RegistryDumper.java`) boots the vanilla CoreServer asset pipeline
  (`AssetRegistryLoader`) over `Assets/Assets.zip` (mounted as a zip
  `FileSystem`, exactly like the vanilla server) and dumps
  `testdata/golden/registries.json`:
  - `blocks` — 6,135 block types: `{ name, index, drawType, drawTypeId,
    opacity, unknown }` plus the per-`RotationTuple` `support` map
    (`containsKey(DOWN/UP)`, absent for the Java `null` support map) and the
    `preLoadOrder` / `emptyId` / `unknownId` / `debugCubeId` / `debugModelId`
    constants,
  - `fluids` — 19 fluids: `{ name, index, maxFluidLevel }`,
  - `environments` — 135 environments: `{ name, index }` (the world generator
    resolves the environment names of the biome environment containers and of
    the prefab contents through `Environment.getAssetMap().getIndex(name)`),
  - a `meta` block documenting the boot, the load-order strategy and the
    remaining gap.
- The Rust side loads the dump through `worldgen_model::registry::Registries::
  from_golden` (serde) — it fills the `BlockRegistry` (index → draw type) and
  the `FluidRegistry` (index → max fluid level) exactly as dumped; the Rust
  side never re-derives the ordering. `Registries::empty()` is unchanged and
  stays the default until the loader subtask wires the dump into the world
  generator.
- **The remaining open question is the load order.** The dump pins the ids to
  the order this dumper observed (the zip entry order of the pack, with a
  single-threaded decode — see the `meta` block of the dump). They are only
  identical to a real server's ids when the real server loads the same pack in
  the same order; a real-server dump has to be compared before the ids are
  treated as authoritative.
- **Known gap:** the prefab list assets (`Server/PrefabList`) cannot be
  decoded without the prefab store, so the single block type that references
  such a list (`ExamplePrefabPlacementBlock_Tree`) is missing from the dump.
  The other stores the registries do not depend on (interaction, particle and
  weather) also report load failures for the same reason — they do not
  influence the block, fluid and environment ids.
- **Observable consequences while the registry is empty** (both are *correct*
  Java semantics for a missing asset, but they differ from the Java generator
  that has the pack):
  - `WaterPopulator` skips every block, because
    `BlockType.getAssetMap().getAsset(id)` is `null` → **no water is written
    at all**.
  - `ChunkGeneratorExecution.setFluid(x, y, z, type, fluid)` defaults the
    fluid level to `8` instead of the level of the real fluid.
- **Consequence for the parity plan**: the chunk-level golden verification now
  has the block/fluid ids available; it is no longer blocked on the pack. The
  noise, the terrain thresholds, the layer/cover geometry and the random draw
  order are verified independently; the block/fluid **ids** and the fluid
  **levels** come from the golden dump.
