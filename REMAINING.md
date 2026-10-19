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

## 2. Entity serialization from prefabs
- Prefab `.prefab.json` entity lists and `PrefabPasteUtil` entity spawning are stubbed.
- Only blocks/fluids/environments from prefabs are pasted.

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

## 6. BlockType definition assets are still missing (the rest of the pack arrived)
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
- **STILL MISSING**: the block-type **definition** assets (`Server/Item/Block/Blocks`).
  The pack ships only `Server/Item/Block/Fluids`; the shared source tree ships
  only `HytaleAssets/Server/Item/Block/Blocks/_Debug/Debug_Test_Block.json`.
  The individual block definitions (`Rock_Stone`, `Rock_Shale`, `Soil_*`, `Grass`,
  …) live in the base-game content pack and are **not** part of `.hytale-source/`.
  The shared source still provides the **schema** and the **name lists**:
  - `HytaleAssets/Schema/BlockType.json` — the JSON schema plus the
    `internalKeys` prefix list (`Empty`, `Unknown`, `Debug_Cube`, `Debug_Model`).
  - `HytaleAssets/Server/BlockTypeList/*.json` — the *block type list* assets
    (name lists such as `Fluids.json` = `["Fluid_Water", "Fluid_Lava", …]`).
  - `HytaleAssets/Server/Item/Block/Fluids/*.json` — the fluid assets.
- The Java world generator resolves the names **at load time** through
  `BlockType.getAssetMap().getIndex(name)` / `Fluid.getAssetMap().getIndex(name)`
  and **throws** when a name is unknown, e.g.
  - `loader/container/LayerContainerJsonLoader` — the `Layers.Default` filling
    block (`Rock_Stone`, `Rock_Shale`, …),
  - `loader/container/WaterContainerJsonLoader` — the water entry block/fluid
    (`Water_Source`, `Lava_Source`, `Rock_Ice`, …),
  - `loader/util/ResolvedBlockArrayJsonLoader`,
    `loader/cave/CaveNodeTypeJsonLoader`, `loader/cave/FluidLevelJsonLoader`,
    …
  Without the block definitions the Java generator therefore **cannot load the
  Default world at all** — the block ids (and with them the chunk-level golden
  parity) stay blocked on that pack (or on obtaining it from the game install).
- The Rust port therefore carries a **data-driven registry seam**
  (`worldgen-model::registry`) that the loader subtask fills:
  - `BlockRegistry` — id → `BlockTypeInfo { draw_type }`
    (Java `BlockTypeAssetMap.getAsset(index)` → `Option`, `null` on a miss),
  - `FluidRegistry` — id → max fluid level
    (Java `Fluid.getAssetMap().getAssetOrDefault(index, Fluid.EMPTY)
    .getMaxFluidLevel()`, the `Fluid.EMPTY` default being `8`),
  - `Registries::empty()` — the default until the assets are loaded.
- **Observable consequences while the registry is empty** (both are *correct*
  Java semantics for a missing asset, but they differ from the Java generator
  that has the pack):
  - `WaterPopulator` skips every block, because
    `BlockType.getAssetMap().getAsset(id)` is `null` → **no water is written
    at all**.
  - `ChunkGeneratorExecution.setFluid(x, y, z, type, fluid)` defaults the
    fluid level to `8` instead of the level of the real fluid.
- **Consequence for the parity plan**: the chunk-level golden verification is
  blocked on the block-definition pack (or on obtaining it). The noise, the terrain
  thresholds, the layer/cover geometry and the random draw order are
  verified independently; the block/fluid **ids** and the fluid **levels**
  are only as good as the registry that is loaded.
