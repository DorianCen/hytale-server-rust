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

## Run

```sh
tools/java-dumper/run.sh
```

Requires JDK 25 and Maven 3.10+. The build tries `mvn -o` first and falls back to an
online build if the local repository is incomplete.

## Layout

- `pom.xml` – build and dependencies.
- `run.sh` – build + run entry point.
- `src/main/java/com/hypixel/hytale/tools/golden/GoldenDumper.java` – the dumper
  (math/noise dumps).
- `src/main/java/com/hypixel/hytale/tools/golden/Phase2Dumper.java` – the Phase-2
  dumps (`zoom.json`, `climate.json`): the Default world mask and the procedural
  climate mask, loaded through the real Java loaders.
- `lib/` and `target/` – generated build output, not source.

The upstream `ProceduralLibrary`/`WorldGenerator` classes reference `fastutil` and
`sentry`; both are resolved from the local Maven repository and added to the runtime
classpath by `run.sh`.
