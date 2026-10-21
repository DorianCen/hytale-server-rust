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
tools/java-dumper/run.sh
```

Requires JDK 25 and Maven 3.10+. The build tries `mvn -o` first and falls back to an
online build if the local repository is incomplete.

The registry dump needs the base-game asset pack at `Assets/Assets.zip` (the script
resolves it relative to the repository root — the pack is not part of the source tree).

## Layout

- `pom.xml` – build and dependencies.
- `run.sh` – build + run entry point.
- `src/main/java/com/hypixel/hytale/tools/golden/GoldenDumper.java` – the dumper
  (math/noise dumps).
- `src/main/java/com/hypixel/hytale/tools/golden/Phase2Dumper.java` – the Phase-2
  dumps (`zoom.json`, `climate.json`): the Default world mask and the procedural
  climate mask, loaded through the real Java loaders.
- `src/main/java/com/hypixel/hytale/tools/golden/RegistryDumper.java` – the registry
  dump (`registries.json`): the real block / fluid / environment registries of the
  base-game asset pack, produced by running the vanilla CoreServer asset pipeline
  (`AssetRegistryLoader`) over `Assets/Assets.zip`.
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
