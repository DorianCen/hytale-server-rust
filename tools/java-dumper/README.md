# java-dumper

Standalone Maven project that emits deterministic golden data from the original Java
Hytale world-generation code. See [`../../testdata/golden/README.md`](../../testdata/golden/README.md)
for the format and the porting contract.

## How it works

The Java sources under `.hytale-source/` are read-only reference material. The upstream
modules `Math`, `ProceduralLibrary` and `WorldGenerator` were compiled once already, so
this project does not depend on them through the parent reactor. Instead
[`run.sh`](run.sh):

1. packages each upstream `target/classes` tree into `lib/hytale-<Module>.jar`
   (Maven's `system` scope requires a file, not a directory);
2. compiles this project against those jars plus `gson` and `joml`;
3. runs `com.hypixel.hytale.tools.golden.GoldenDumper` with `testdata/golden/` as its
   output directory.

If an upstream `target/classes` tree is missing, the script compiles that module first
via `mvn -pl <Module> -am compile`.

The **registry dump** additionally boots the real CoreServer asset pipeline, so the
upstream `CoreServer` module (and its dependencies `AssetStore`, `Codec`, `Common`,
`Component`, `Protocol`, `Logger`, `Event`, `SneakyThrow`, `Registry`, `Storage`,
`Unsafe`, `FunctionalInterfaces`, `Metrics` and the vendored `ConcurrentFastUtil`) plus
the built-in plugins that register the block tick / farming codec variants
(`builtin/BlockTick`, `builtin/RandomTick`, `builtin/FallingBlocks`,
`builtin/adventure/Farming`, `builtin/adventure/WorldLocationCondition`) are packaged
into jars as well.

## Run

```sh
tools/java-dumper/run.sh            # writes testdata/golden/
tools/java-dumper/run.sh /tmp/dump  # writes an arbitrary directory
```

Requires JDK 25 and Maven 3.10+. The build tries `mvn -o` first and falls back to an
online build if the local repository is incomplete. A module whose class tree
still contains an Eclipse "Unresolved compilation problem" stub (for example
after a partially failed upstream build) is deleted and recompiled before it is
packaged.

The registry dump needs the base-game asset pack at `Assets/Assets.zip` (the script
resolves it relative to the repository root — the pack is not part of the source tree).
`GoldenDumper` writes `registries.json` before the world-mask dumps because the asset
store iteration order depends on class-initialization order; moving it later changes
block ids and invalidates the committed chunk goldens.

## Asset-store dump (P0)

`AssetDumper` reuses the registry boot and writes the P0 asset packets to
`testdata/golden/server/assets/`:

```sh
tools/java-dumper/run-assets.sh                     # write the committed directory
tools/java-dumper/run-assets.sh /tmp/assets-golden   # arbitrary output directory
tools/java-dumper/run-assets.sh --check             # two isolated runs + diff
```

`ASSET_PACK=<path>` overrides the pack (relative to the repository root or
absolute). See
[`../asset-golden-dumper/README.md`](../asset-golden-dumper/README.md) and
[`../../testdata/golden/server/assets/README.md`](../../testdata/golden/server/assets/README.md)
for the schema, sizes and the deterministic map-order canonicalization.

## Chunk-packet dump

`ChunkPacketDumper` boots the same real Default-world generator as `ChunkDumper`
for seed `0` at chunks `(0,0)` and `(4,-5)` and writes the real, uncompressed
`SetChunk` (per section), `SetColumn`, `SetChunkTintmap`,
`SetChunkEnvironments` (per section) and non-empty `SetFluids` payloads. It
calls the server's own packet builders (`BlockSection.getCachedChunkPacket`,
`BlockChunk.getCachedColumnPacket/getCachedTintsPacket`,
`EnvironmentSection.getCachedPacket`, `FluidSection.getCachedPacket`).

```sh
tools/java-dumper/run-chunks.sh                     # write testdata/golden/server/chunks-pkts/
tools/java-dumper/run-chunks.sh /tmp/chunks-golden   # arbitrary output directory
tools/java-dumper/run-chunks.sh --check             # two isolated runs + diff
```

See
[`../../testdata/golden/server/chunks-pkts/README.md`](../../testdata/golden/server/chunks-pkts/README.md)
for the manifest schema, packet list and `sentAtInit` semantics.

## World-storage dump

`StorageDumper` builds the same seed-0 chunks through the server's runtime chunk
model, serializes the `ChunkStore` holder with the real BSON component codec, and
writes the bytes into Java `IndexedStorageFile` region files:

```sh
tools/java-dumper/run-storage.sh                     # write testdata/golden/server/storage/java-world/
tools/java-dumper/run-storage.sh /tmp/storage-golden  # arbitrary output directory
tools/java-dumper/run-storage.sh --check              # two isolated runs + diff
```

See
[`../../testdata/golden/server/storage/README.md`](../../testdata/golden/server/storage/README.md)
for the layout and the Rust gate.

## Rust save reload check

`WorldStorageProbe` is the reverse storage gate: it reads a **Rust-written**
region with the real Java `IndexedStorageFile`, `BsonUtil.readFromBuffer`,
`ChunkStore.REGISTRY`, `ChunkColumn`/`BlockSection` and `BlockType` asset map,
then checks one block by name.

```sh
rm -rf target/headless-smoke
cargo run -q -p hytale-server -- headless --save target/headless-smoke --chunk 4,-5
cargo run -q -p hytale-server -- load-chunk --save target/headless-smoke --chunk 4,-5 \
    --set-block 129,250,-159,4
tools/java-dumper/run-reload-check.sh target/headless-smoke 4 -5 129 250 -159 Debug_Test_Block
```

The script prints `java reload check: ok` and exits non-zero when the region,
the blob, the component or the expected block name is wrong. It writes no
fixture; the check runs against a fresh temporary save. `WorldStorageProbe` and
`StorageDumper` share `ChunkStoreBootstrap`, which installs the minimal
`AssetModule`/`LegacyModule` singletons and registers the real chunk components
on `ChunkStore.REGISTRY`.

`run.sh` forwards `DUMPER_EXTRA_ARGS` to the entry point, so wrapper scripts can
pass tool-specific arguments without a new run script.

## World config dump and probe

`WorldConfigDumper` writes the per-world `config.json` with the real
`WorldConfig` codec (version 4) and boots the asset stores its static
initialiser needs. The `probe` mode is the reverse gate: it loads a Rust-written
config with the real codec and checks the expected fields.

```sh
tools/java-dumper/run-world-config.sh                       # write testdata/golden/server/world-config/
tools/java-dumper/run-world-config.sh /tmp/world-config     # arbitrary output directory
tools/java-dumper/run-world-config.sh --check               # two isolated runs + diff

cargo run -q -p hytale-server -- world-config \
    --input testdata/golden/server/world-config/config.json --save target/world-config-smoke
tools/java-dumper/run-world-config.sh --probe target/world-config-smoke/config.json
```

The dumper registers the provider codecs that `Universe` normally installs
(Global spawn, Void world gen, Disabled world map, default chunk/resource
storage). The probe strips `GameplayConfig` before the codec decode because this
harness cannot load the gameplay-config assets; the key is checked from the
parsed document instead. See
[`../../testdata/golden/server/world-config/README.md`](../../testdata/golden/server/world-config/README.md).

## Player data dump and probe

`WorldPlayerDumper` writes a real `EntityStore` holder document with the same
path `DiskPlayerStorageProvider` uses (`EntityStore.REGISTRY.serialize` +
`BsonUtil.writeDocument`). The `probe` mode is the reverse gate: it deserializes
a Rust-written player file with the real registry and checks the core
components.

```sh
tools/java-dumper/run-player-data.sh                    # write testdata/golden/server/player-data/
tools/java-dumper/run-player-data.sh /tmp/player-data   # arbitrary output directory
tools/java-dumper/run-player-data.sh --check            # two isolated runs + diff

cargo run -q -p hytale-server -- player-data \
    --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
    --save target/player-smoke
tools/java-dumper/run-player-data.sh --probe \
    target/player-smoke/players/12345678-1234-5678-9abc-def012345678.json
```

The dumper registers the `EntityModule.setup()` ids for the ported core set
(`UUID`, `Transform`, `WorldGenId`). Unknown components are preserved by the Rust
holder, so a full Java player save round-trips before the gameplay component
codecs land. See
[`../../testdata/golden/server/player-data/README.md`](../../testdata/golden/server/player-data/README.md).

## Whole-save check

`run-save-check.sh` runs the config, player and chunk probes against one
Rust-written save:

```sh
tools/java-dumper/run-save-check.sh target/save-smoke 4 -5 129 250 -159 Debug_Test_Block
```

It uses `config.json`, the first `players/*.json` and the region file for the
given chunk. The Rust side is `WorldSave` / `hytale-server world-boot`.

## Live registry snapshot

`RegistrySnapshot` writes the block / fluid / environment maps of the
**currently loaded** JVM in the same schema as `registries.json`. A server
calls it after its asset load (including mods):

```java
RegistrySnapshot.write(Path.of("run/worldgen-registries.json"));
```

The class reads the static CoreServer maps; it does not boot anything. For a
standalone exercise of the writer (booting the stubs first):

```sh
DUMPER_MAIN=com.hypixel.hytale.tools.golden.RegistrySnapshot \
  tools/java-dumper/run.sh /tmp/live-registries
```

See [`../../done/LIVE_SNAPSHOT.md`](../../done/LIVE_SNAPSHOT.md) for the full
integration and validation workflow (`worldgen-cli registries --compare`).

## Determinism check

```sh
tools/java-dumper/check.sh
```

Runs `run.sh` twice into isolated temporary directories, fails on any diff,
and then diffs the fresh output against the committed `testdata/golden/` so a
stale or hand-edited golden file is caught. This is the gate referenced in
`NEXT_STEPS.md` and `ARCHITECTURE.md` §6.

The canonical dump runs the asset pipeline with `common.parallelism=1`
(`DUMPER_PARALLELISM=1`, the default). The chunk dump uses the vanilla
provider label `SeedString("ChunkGenerator")`; `DUMPER_LOADER_SEED=<label>`
overrides it (e.g. to reproduce the earlier per-seed convention). `run.sh` accepts
`DUMPER_PARALLELISM=<n>` for the registry-order investigation: with parallel
decode, a few sibling block ids (and `Lava`/`Tar`) swap between runs — see
the `meta` block of `registries.json` and `REMAINING.md` §6.

## Layout

- `pom.xml` – build and dependencies.
- `run.sh` – build + run entry point.
- `run-assets.sh` – build + run entry point for the P0 asset-store dump
  (wraps `run.sh` with `AssetDumper`; `--check` runs the determinism gate).
- `run-chunks.sh` – build + run entry point for the chunk-packet dump
  (wraps `run.sh` with `ChunkPacketDumper`; `--check` runs the determinism gate).
- `run-storage.sh` – build + run entry point for the Java world-storage dump
  (wraps `run.sh` with `StorageDumper`; `--check` runs the determinism gate).
- `run-reload-check.sh` – build + run entry point for the reverse storage gate
  (wraps `run.sh` with `WorldStorageProbe`; no fixture is written).
- `run-world-config.sh` – build + run entry point for the world config fixture
  and probe (wraps `run.sh` with `WorldConfigDumper`; `--check` runs the
  determinism gate).
- `run-player-data.sh` – build + run entry point for the player holder fixture
  and probe (wraps `run.sh` with `WorldPlayerDumper`; `--check` runs the
  determinism gate).
- `run-save-check.sh` – whole-save gate: config, player and chunk probes against
  one save directory (no fixture is written).
- `src/main/java/com/hypixel/hytale/tools/golden/GoldenDumper.java` – the dumper
  (math/noise dumps).
- `src/main/java/com/hypixel/hytale/tools/golden/Phase2Dumper.java` – the Phase-2
  dumps (`zoom.json`, `climate.json`): the Default world mask and the procedural
  climate mask, loaded through the real Java loaders.
- `src/main/java/com/hypixel/hytale/tools/golden/RegistryDumper.java` – the registry
  dump (`registries.json`): the real block / fluid / environment registries of the
  base-game asset pack, produced by running the vanilla CoreServer asset pipeline
  (`AssetRegistryLoader`) over `Assets/Assets.zip`.
- `src/main/java/com/hypixel/hytale/tools/golden/AssetDumper.java` – the P0
  asset-store dump (`testdata/golden/server/assets/`): the real
  `generateInitPacket` payloads (uncompressed, unframed) and the key/id index,
  booting the same pipeline as `RegistryDumper`.
- `src/main/java/com/hypixel/hytale/tools/golden/ChunkPacketDumper.java` – the
  chunk-packet dump (`testdata/golden/server/chunks-pkts/`): real Java packets
  for seed 0 chunks `(0,0)` and `(4,-5)` from the same world boot as
  `ChunkDumper`.
- `src/main/java/com/hypixel/hytale/tools/golden/StorageDumper.java` – the Java
  world save (`testdata/golden/server/storage/java-world/`): real BSON chunk
  documents written into `IndexedStorageFile` region files.
- `src/main/java/com/hypixel/hytale/tools/golden/WorldStorageProbe.java` – the
  Rust save reload check: deserializes a Rust-written region with the real
  Java chunk codecs and resolves one block through the `BlockType` map.
- `src/main/java/com/hypixel/hytale/tools/golden/WorldConfigDumper.java` – the
  world config fixture writer and reverse probe: real `WorldConfig.CODEC` v4
  save/load plus the expected-field checks.
- `src/main/java/com/hypixel/hytale/tools/golden/WorldPlayerDumper.java` – the
  player holder fixture writer and reverse probe: real `EntityStore.REGISTRY`
  serialization plus the core-component checks.
- `src/main/java/com/hypixel/hytale/tools/golden/ChunkStoreBootstrap.java` –
  shared minimal `AssetModule`/`LegacyModule` boot and `ChunkStore` component
  registration used by `StorageDumper` and `WorldStorageProbe`.
- `src/main/java/com/hypixel/hytale/tools/golden/LoaderDumper.java` – the loader-chain
  dump (`loader.json`): the real Default-world `ChunkGeneratorJsonLoader` walk
  (zone requirements, seed chains, caves, biome containers) for the fixed seeds.
- `src/main/java/com/hypixel/hytale/tools/golden/ChunkDumper.java` – the full chunk
  dump (`chunks.json`): the real `ChunkGeneratorExecution.execute` for the fixed
  seed/chunk grid, run-length encoded per column. `ChunkProbe.java` runs only this
  dump for fast iteration
  (`java -cp <cp> com.hypixel.hytale.tools.golden.ChunkProbe <outDir> <worldDir> <repoRoot>`).
- `lib/` and `target/` – generated build output, not source.

The upstream `ProceduralLibrary`/`WorldGenerator` classes reference `fastutil` and
`sentry`; both are resolved from the local Maven repository and added to the runtime
classpath by `run.sh`.
