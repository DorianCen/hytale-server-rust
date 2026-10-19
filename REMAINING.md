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
