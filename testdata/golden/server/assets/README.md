# Asset-store golden data (P0)

Reference output of the real Java CoreServer asset pipeline for the **P0**
stores of [`tools/asset-golden-dumper/asset-stores.json`](../../../../tools/asset-golden-dumper/asset-stores.json).
Produced by `com.hypixel.hytale.tools.golden.AssetDumper`; regenerate, never
hand-edit.

## Regenerate

Prerequisites: JDK 25, Maven 3.10+, the read-only Java source tree under
`.hytale-source/` (compiled once; `run.sh` rebuilds stale class trees), and the
base-game pack at `Assets/Assets.zip` (3.7 GB, not part of the source tree).

```sh
# write testdata/golden/server/assets/
tools/java-dumper/run-assets.sh

# write an arbitrary directory
tools/java-dumper/run-assets.sh /tmp/assets-golden

# determinism gate: two isolated runs + diff against this directory
tools/java-dumper/run-assets.sh --check
```

`ASSET_PACK=<path>` overrides the pack (relative to the repository root or
absolute). `DUMPER_PARALLELISM` defaults to `1`; the committed data is the
single-threaded decode. The dumper boots the vanilla pipeline exactly like the
registry dump: `Options.parse([])`, inert `HytaleServer`/`Universe` stubs, the
`AssetRegistryLoader` static initializer, the plugin-module stores and codec
variants, the pack's `CommonAssetsIndex.hashes`, and `Assets/Assets.zip` mounted
as a zip `FileSystem`.

## Files

| File | Contents | Size |
|---|---|---:|
| `index.json` | meta + per-store keys, numeric ids, counts, packet metadata | 1,081,229 B |
| `BlockBoundingBoxes.bin` | `UpdateBlockHitboxes` init payload | 12,079 B |
| `BlockType.bin` | `UpdateBlockTypes` init payload | 5,872,115 B |
| `Environment.bin` | `UpdateEnvironments` init payload | 7,514 B |
| `Fluid.bin` | `UpdateFluids` init payload | 5,580 B |
| `Item.bin` | `UpdateItems` init payload | 2,628,479 B |
| `PhysicalMaterial.bin` | `UpdatePhysicalMaterials` init payload | 325 B |

Total: 9,607,321 B (9.2 MiB). `BlockTypeListAsset` and `PrefabListAsset` have
no packet generator in the Java server, so they have no `.bin`; their keys are
in `index.json`.

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
  producer, inventory, storeFilter, pack, packFormat, packKey
  protocolVersion, protocolCrc, protocolBuildNumber   (ProtocolSettings)
  parallelism, boot, payload, indexOrder, missingPackets, loadFailures,
  deterministic
stores[]:                       (in P0 inventory order)
  store, class, path, map, keyType, priority, packKey
  count                         loaded asset count (includes preloaded assets)
  maxId                         next index of indexed maps (absent for DefaultAssetMap)
  ids                           { key: numeric id }, sorted by key (indexed maps only)
  keys                          sorted key array
  packetGenerator, packetClass, packetId, packetSize
  bin, sha256                   empty for stores without a packet generator
  loadFailures                  only when decode failures were recorded
```

Numeric ids are the ids of the store's `AssetMap` (`BlockTypeAssetMap` /
`IndexedLookupTableAssetMap`), i.e. the ids the server sends. They match
`../registries.json` exactly (6135 blocks, 19 fluids, 135 environments).

## Known gaps (recorded in `index.json`)

The boot is the same canonical one as the registry dump, so it inherits its
gaps:

- `PrefabListAsset` (78 lists) fails to decode: `PrefabReference.processPrefabPath`
  calls `PrefabStore.getAssetPrefabsPath()` → `AssetModule.getBaseAssetPack()`,
  and the dumper does not boot `AssetModule`, so every list throws
  `NullPointerException: Cannot invoke "AssetModule.getBaseAssetPack()" because
  the return value of "AssetModule.get()" is null`. Consequently the one block
  type that references such a list (`ExamplePrefabPlacementBlock_Tree`) is
  missing from `BlockType` (as in `registries.json`).
- Non-P0 child stores (the interaction and camera stores) report decode
  failures because their plugin codec variants are not registered. They do not
  change the block/fluid/environment ids.
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
  them in `ServerTags`, so tag arrays intentionally differ. `fluidFXIndex` and
  `blockSoundSetIndex` are 0 until those child stores are streamed.

`BlockType`/`Item` packets are still pending; they use Rust-owned contained-asset
ids and the same Rust-owned tag registry.
