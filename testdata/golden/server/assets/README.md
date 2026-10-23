# Asset-store golden data (P0 + child stores)

Reference output of the real Java CoreServer asset pipeline for the phase-2
store set: P0, P1, and the P2/P3 dependency roots that feed their indexes.
Produced by `com.hypixel.hytale.tools.golden.AssetDumper`; regenerate, never
hand-edit.

## Regenerate

Prerequisites: JDK 25, Maven 3.10+, the read-only Java source tree under
`.hytale-source/` (compiled once; `run.sh` rebuilds stale class trees), and the
base-game pack at `Assets/Assets.zip` (3.7 GB, not part of the source tree).

```sh
# write the default phase-2 set into testdata/golden/server/assets/
tools/java-dumper/run-assets.sh

# write an arbitrary directory
tools/java-dumper/run-assets.sh /tmp/assets-golden

# determinism gate: two isolated runs + diff against this directory
tools/java-dumper/run-assets.sh --check

# narrower selections (for local investigation)
tools/java-dumper/run-assets.sh --priority P0 /tmp/assets-p0
tools/java-dumper/run-assets.sh --priority P0,P1 --closure /tmp/assets-p01
tools/java-dumper/run-assets.sh --stores FluidFX,ParticleSystem --closure /tmp/assets-fx
```

`ASSET_PACK=<path>` overrides the pack (relative to the repository root or
absolute). `DUMPER_PARALLELISM` defaults to `1`; the committed data is the
single-threaded decode. The dumper boots the vanilla pipeline exactly like the
registry dump: `Options.parse([])`, inert `HytaleServer`/`Universe` stubs, the
`AssetRegistryLoader` static initializer, the plugin-module stores and codec
variants, the pack's `CommonAssetsIndex.hashes`, and `Assets/Assets.zip` mounted
as a zip `FileSystem`.

### Store selection

Without a selector, the dumper emits:

- every P0 and P1 store;
- the explicit P2/P3 roots `FluidFX`, `ItemSoundSet`, `SoundEvent`,
  `TagPattern`, `ParticleSystem`, `ParticleSpawner`, `ItemReticleConfig`,
  `ItemPlayerAnimations`;
- the transitive `loadsAfter` closure of those extra roots
  (`AudioCategory`, `AudioState`, for example).

`--priority P0` reproduces the old P0-only set. `--stores A,B --closure` selects
explicit stores and their `loadsAfter` dependencies. The closure follows the
`loadsAfter` edges in `tools/asset-golden-dumper/asset-stores.json`; names that
are not part of the 54-store inventory (for example `EntityStatType`) are
recorded as `meta.missingDependencies`, never invented.

## Files

| File | Contents | Size |
|---|---|---:|
| `index.json` | meta + per-store keys, numeric ids, counts, packet metadata | 1,405,521 B |
| `BlockBoundingBoxes.bin` | `UpdateBlockHitboxes` init payload | 12,079 B |
| `BlockType.bin` | `UpdateBlockTypes` init payload | 5,872,115 B |
| `Environment.bin` | `UpdateEnvironments` init payload | 7,514 B |
| `Fluid.bin` | `UpdateFluids` init payload | 5,580 B |
| `Item.bin` | `UpdateItems` init payload | 2,628,479 B |
| `PhysicalMaterial.bin` | `UpdatePhysicalMaterials` init payload | 325 B |
| `BlockBreakingDecal.bin` | `UpdateBlockBreakingDecals` init payload | 1,253 B |
| `BlockParticleSet.bin` | `UpdateBlockParticleSets` init payload | 5,132 B |
| `BlockSoundSet.bin` | `UpdateBlockSoundSets` init payload | 3,112 B |
| `ItemCategory.bin` | `UpdateItemCategories` init payload | 6,570 B |
| `ItemQuality.bin` | `UpdateItemQualities` init payload | 3,545 B |
| `AudioCategory.bin` | `UpdateAudioCategories` init payload | 6,144 B |
| `AudioState.bin` | `UpdateAudioStates` init payload | 1,378 B |
| `FluidFX.bin` | `UpdateFluidFX` init payload | 426 B |
| `ItemSoundSet.bin` | `UpdateItemSoundSets` init payload | 1,529 B |
| `ParticleSpawner.bin` | `UpdateParticleSpawners` init payload | 1,887,288 B |
| `ParticleSystem.bin` | `UpdateParticleSystems` init payload | 517,716 B |
| `SoundEvent.bin` | `UpdateSoundEvents` init payload | 759,615 B |
| `TagPattern.bin` | `UpdateTagPatterns` init payload | 7,289 B |
| `ItemPlayerAnimations.bin` | `UpdateItemPlayerAnimations` init payload | 1,509,718 B |
| `ItemReticleConfig.bin` | `UpdateItemReticles` init payload | 758 B |

Total: 13,237,565 B of payloads (12.6 MiB) plus the 1.4 MiB index.
`BlockTypeListAsset`, `PrefabListAsset`, `GameplayConfig` and `ModelAsset` have
no packet generator in the Java server, so they only appear in `index.json`.
`BlockSet` has a generator, but it needs a server module that the asset-only boot
does not install; its index entry records `packetError` and has no `.bin`.

The phase-2 regeneration re-baselined the `BlockType`, `Environment`, `Fluid`
and `Item` payload hashes. Their sizes are unchanged and the Rust semantic gates
still pass; the committed hashes follow the canonical single-worker output.

## Payload format

`<Store>.bin` is the **exact uncompressed packet payload** of
`packetGenerator.generateInitPacket(assetMap, assets)`:

```java
Packet packet = generator.generateInitPacket(assetMap, assets);
byte[] payload = new byte[packet.computeSize()];
packet.serialize(MemorySegment.ofArray(payload), 0);
```

There is **no framing** (no packet-id/length header) and **no zstd
compression**. `packetId` / `packetSize` in `index.json` come from the packet
object (`packet.getId()` / `packet.computeSize()`). To frame it like the server,
prefix the 8-byte frame header and apply the packet's `IS_COMPRESSED` policy.

### Deterministic map order

The vanilla generators build several protocol dictionaries with **enum keys**
in fastutil hash maps (e.g. `ItemBase.interactions` keyed by `InteractionType`,
`BlockType.support` keyed by `BlockNeighbor`). An enum's `hashCode` is its
identity hash, so their iteration order differs between JVM runs. Before
serialization the dumper walks the generated `com.hypixel.hytale.protocol`
object graph and replaces every `java.util.Map` with a `LinkedHashMap` sorted by
a deterministic key order (enum: class name then ordinal; number: numeric;
string: lexicographic). The **entries and values are unchanged** — only the
dictionary order is canonicalized — and `computeSize`/`serialize` are still the
packet object's own path. Without this step `BlockType.bin` and `Item.bin`
differ between runs.

## index.json schema

```text
meta:
  producer, inventory, storeFilter, selectedStoreCount, pack, packFormat,
  packKey, protocolVersion, protocolCrc, protocolBuildNumber, parallelism,
  boot, payload, indexOrder, missingPackets, packetErrors,
  missingDependencies, loadFailures, deterministic
stores[]:                       (in selected-store inventory order)
  store, class, path, map, keyType, priority, packKey
  count                         loaded asset count (includes preloaded assets)
  maxId                         next index of indexed maps (absent for DefaultAssetMap)
  ids                           { key: numeric id }, sorted by key (indexed maps only)
  keys                          sorted key array
  packetGenerator, packetClass, packetId, packetSize
  bin, sha256                   empty for stores without a packet generator
  packetError                   only when the generator needs an un-booted server module
  loadFailures                  only when decode failures were recorded
```

Numeric ids are the ids of the store's `AssetMap` (`BlockTypeAssetMap` /
`IndexedLookupTableAssetMap`), i.e. the ids the server sends. They match
`../registries.json` exactly for the P0 stores (6135 blocks, 19 fluids, 135
environments).

## Known gaps (recorded in `index.json`)

The boot is the same canonical one as the registry dump, so it inherits its
gaps:

- `PrefabListAsset` fails to decode: `PrefabReference.processPrefabPath` calls
  `PrefabStore.getAssetPrefabsPath()` → `AssetModule.getBaseAssetPack()`, and
  the dumper does not boot `AssetModule`, so every list throws
  `NullPointerException: Cannot invoke "AssetModule.getBaseAssetPack()" because
  the return value of "AssetModule.get()" is null`. Consequently the one block
  type that references such a list (`ExamplePrefabPlacementBlock_Tree`) is
  missing from `BlockType` (as in `registries.json`). The store count is 0.
- `GameplayConfig` count is 0: its `Default` asset fails validation because
  `CameraEffects.DamageEffects` references `Damage_Fall`, and the camera-effect
  child store is not registered in the canonical boot.
- `BlockSet` (80 assets) has no payload: `BlockSetPacketGenerator` calls
  `BlockSetModule.getInstance().getBlockSets()`, and the asset-only boot does
  not install that module. The index entry is complete and carries
  `packetError`.
- Interaction/camera child stores report decode failures because their plugin
  codec variants are not registered.
- 37 `Item` assets fail to decode for the same reason (their interaction
  references are unresolved); the item packet contains the 3971 loaded items.

`loadFailures.failedPaths` are relative to the pack root; no absolute path or
timestamp is written anywhere.

## Rust consumers

- `crates/hytale-assets/tests/registry.rs` checks the file-backed store ids and
  block semantics against `index.json` / `registries.json`.
- `crates/hytale-assets/tests/asset_packets.rs` encodes
  `PhysicalMaterial.bin` and `BlockBoundingBoxes.bin` from the real registry and
  compares them byte-for-byte. Maps use ascending integer keys, matching
  `AssetDumper.canonicalizeMaps`; `Hitbox` components are double→float.
- The same test encodes `Environment.bin` and `Fluid.bin` and compares every
  non-tag/non-child-index field. Java's tag indexes are global across all 54
  stores; the Rust registry assigns its own deterministic indexes and publishes
  them in `ServerTags`, so tag arrays intentionally differ.
- `crates/hytale-assets/tests/block_item_packets.rs` decodes `BlockType.bin` and
  `Item.bin` and compares the implemented fields by block name/item key. Block
  ids, group ids and tag indexes are Rust-owned, so id references are
  normalized through the Rust registries; the item map uses string keys and is
  compared directly (`blockId` by block name).
- `crates/hytale-assets/tests/child_store_packets.rs` decodes every new
  child-store payload and verifies each `index.json` size/hash against the
  committed file.
- T36/T37/T38 consume the new child-store payloads and replace the temporary
  `0` child indexes (`BlockSoundSet`, `FluidFX`, `ItemSoundSet`, `ItemQuality`,
  `ItemReticleConfig`, `TagPattern`).
