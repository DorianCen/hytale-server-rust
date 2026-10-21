# Asset golden dumper

This directory owns the Java-side asset oracle for the CoreServer rewrite.

## Current contents (T08)

- `extract_stores.py` — reads `AssetRegistryLoader.java` and writes the two
  inventory files.
- `asset-stores.json` — one entry per `HytaleAssetStore.builder(...)` call:
  class, asset path, key type, map type, codec, packet generator, load edges,
  preloaded assets, schema, priority, extension, and notes.
- `asset-stores.md` — the same inventory grouped by P0–P3 for review.

Regenerate both files with:

```sh
python3 tools/asset-golden-dumper/extract_stores.py
```

The script is deterministic: it walks the registrations in source order and
sorts the output by priority then class name. It never modifies
`.hytale-source/`.

## Dumper (T11)

The dumper lives in the existing Java golden-dumper project (it reuses the
`RegistryDumper` boot of the vanilla asset pipeline):

- entry point:
  `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/AssetDumper.java`
- wrapper: `tools/java-dumper/run-assets.sh`
- output: `testdata/golden/server/assets/` (schema and sizes in its
  [`README.md`](../../testdata/golden/server/assets/README.md))

```sh
tools/java-dumper/run-assets.sh                     # write the committed directory
tools/java-dumper/run-assets.sh /tmp/assets-golden   # arbitrary output directory
tools/java-dumper/run-assets.sh --check             # two runs + diff against the committed data
```

Prerequisites: JDK 25, Maven 3.10+, the compiled `.hytale-source/` tree, and
`Assets/Assets.zip`. `ASSET_PACK=<path>` overrides the pack (relative to the
repository root or absolute). `DUMPER_PARALLELISM=1` is the canonical setting
and the default.

The dumper reads the `priority: "P0"` entries of `asset-stores.json`, boots the
real `AssetRegistryLoader` (single-threaded decode), calls each store's
`generateInitPacket(assetMap, assets)` and writes:

```text
testdata/golden/server/assets/
  index.json            per-store keys, numeric ids, counts, packet id/size, sha256
  <Store>.bin           exact uncompressed packet payload (computeSize + serialize)
```

`BlockTypeListAsset` and `PrefabListAsset` have no packet generator in the Java
server, so they only appear in `index.json`. The P0 output is 9.2 MiB total
(`BlockType.bin` 5.7 MiB, `Item.bin` 2.6 MiB); it is committed because the
existing worldgen goldens are of the same order of magnitude.

### Determinism

Two runs must be byte-identical. Besides pinning the common fork-join pool to a
single worker, the dumper hands the assets to the generators in sorted key
order and canonicalizes the order of every `java.util.Map` in the generated
protocol object graph before serialization. The generators use enum-keyed
fastutil maps whose iteration order is identity-hash based, so without that
canonicalization `BlockType.bin` and `Item.bin` change between JVM runs. The
map entries and values are unchanged; only dictionary order is normalized.

### Known gaps

`PrefabListAsset` references the prefab store, which the dumper does not boot:
`PrefabReference.processPrefabPath` → `PrefabStore.getAssetPrefabsPath` →
`AssetModule.getBaseAssetPack()` throws a `NullPointerException` because
`AssetModule.get()` is null. All 78 lists fail to decode and
`ExamplePrefabPlacementBlock_Tree` is missing from `BlockType` (same as
`registries.json`). Non-P0 child stores (interaction, camera) and 37 item assets
report decode failures for missing plugin codec variants. They are recorded per
store in `index.json` (`loadFailures`) with pack-relative paths; no bytes are
invented.

Full-pack runs are not part of the fast `cargo test` tier. The committed
inventory above is enough for store ordering, schema coverage, and the M2 work
queue.
