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

## Dumper (T11, extended in T35)

The dumper lives in the existing Java golden-dumper project (it reuses the
`RegistryDumper` boot of the vanilla asset pipeline):

- entry point:
  `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/AssetDumper.java`
- wrapper: `tools/java-dumper/run-assets.sh`
- output: `testdata/golden/server/assets/` (schema and sizes in its
  [`README.md`](../../testdata/golden/server/assets/README.md))

```sh
tools/java-dumper/run-assets.sh                     # default phase-2 set
tools/java-dumper/run-assets.sh /tmp/assets-golden   # arbitrary output directory
tools/java-dumper/run-assets.sh --check              # two runs + diff committed

# narrower selections
tools/java-dumper/run-assets.sh --priority P0 /tmp/assets-p0
tools/java-dumper/run-assets.sh --priority P0,P1 --closure /tmp/assets-p01
tools/java-dumper/run-assets.sh --stores FluidFX,ParticleSystem --closure /tmp/assets-fx
```

Prerequisites: JDK 25, Maven 3.10+, the compiled `.hytale-source/` tree, and
`Assets/Assets.zip`. `ASSET_PACK=<path>` overrides the pack (relative to the
repository root or absolute). `DUMPER_PARALLELISM=1` is the canonical setting
and the default.

The default selection is P0 + P1 + the extra roots `FluidFX`, `ItemSoundSet`,
`SoundEvent`, `TagPattern`, `ParticleSystem`, `ParticleSpawner`,
`ItemReticleConfig`, `ItemPlayerAnimations`, plus the transitive `loadsAfter`
closure of those roots. `--priority P0` reproduces the old P0-only output.
`--closure` follows the `loadsBefore`/`loadsAfter` edges in
`tools/asset-golden-dumper/asset-stores.json`; dependencies outside the 54-store
inventory are recorded in `index.json` as `meta.missingDependencies`.

The dumper boots the real `AssetRegistryLoader` (single-threaded decode), calls
each selected store's `generateInitPacket(assetMap, assets)` and writes:

```text
testdata/golden/server/assets/
  index.json            per-store keys, numeric ids, counts, packet id/size, sha256
  <Store>.bin           exact uncompressed packet payload (computeSize + serialize)
```

Stores without a packet generator (`BlockTypeListAsset`, `PrefabListAsset`,
`GameplayConfig`, `ModelAsset`) only appear in `index.json`. `BlockSet` has a
generator but needs `BlockSetModule`, which the canonical asset-only boot does
not install; its index entry carries `packetError` and no `.bin`.

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

`GameplayConfig` has no loaded assets in the canonical boot: `Default` fails
validation because `CameraEffects.DamageEffects` references `Damage_Fall` and
the camera-effect store is not registered. `BlockSet` loads 80 assets but its
packet generator needs `BlockSetModule`, which the asset-only boot does not
install; the index entry carries `packetError` and no `.bin`. Both gaps are
reported rather than worked around with fake data.

Full-pack runs are not part of the fast `cargo test` tier. The committed
inventory above is enough for store ordering, schema coverage, and the M2 work
queue.
