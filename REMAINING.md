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
- The generation boundary is `trait WorldGen` returning `GeneratedChunk`;
  `crates/worldgen-server` adds the server-facing `ChunkProvider` adapter
  (Java `IWorldGen.generate(seed, index, x, z, stillNeeded, priority)` →
  `WorldGenRequest`, `shutdown()` → the service drain). It still hands back
  the port's own `GeneratedChunk`, not a `Holder<ChunkStore>`.

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
  from_golden` (serde) — it fills the `BlockRegistry` (index → draw type,
  opacity, per-rotation support) and the `FluidRegistry` (index → max fluid
  level) exactly as dumped; the Rust side never re-derives the ordering.
  `ChunkGeneratorJsonLoader::load()` wires the dump into every generation path
  (`Registries::load()` or an explicit `with_registries_path(...)`); the
  `Registries::empty()` fallback remains only for unit tests that construct a
  `ChunkGeneratorExecution` directly.
- **The load order is schedule-dependent in the vanilla pipeline.** The dump
  pins the ids to the zip entry order of the pack with a single-threaded
  decode (see the `meta` block). That order is canonical for the golden
  tests, but it is **not** a real server's order: with the default parallel
  decode the point at which a parent-waiting asset is committed depends on
  the scheduling. Measured with `DUMPER_PARALLELISM=16`, two runs differed in
  a few swapped sibling block ids (e.g. 2591 ↔ 2592) and `Lava`/`Tar`
  (13 ↔ 14); the name → id *set* is stable, only sibling assignments swap.
  The Java world generator resolves names through the live asset maps at
  generation time, so a production server integration must hand the port its
  **own** registry snapshot (the same `registries.json` schema, via
  `with_registries_path` / `WorldGenOptions::registries_path`). A real-server
  snapshot has not been collected yet (no server runtime in this workspace);
  the caveat therefore stays open in `NEXT_STEPS.md` §1. Names, structure and
  the generation logic are unaffected.
- **Known gap:** the prefab list assets (`Server/PrefabList`) cannot be
  decoded without the prefab store, so the single block type that references
  such a list (`ExamplePrefabPlacementBlock_Tree`) is missing from the dump.
  The other stores the registries do not depend on (interaction, particle and
  weather) also report load failures for the same reason — they do not
  influence the block, fluid and environment ids.
- **Consequence for the parity plan**: the chunk-level golden verification now
  has the block/fluid ids available and is **complete** — `tests/chunks_golden.rs`
  compares all 21 chunks / 21,504 columns bit-exactly (blocks, rotations,
  fillers, fluids, fluid levels, environments and priority bytes), including
  water and the cave/prefab stages.
