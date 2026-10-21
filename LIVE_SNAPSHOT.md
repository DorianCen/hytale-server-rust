# Live registry snapshot — integration guide

The Rust world generator resolves block / fluid / environment **names** through
a registry snapshot. The committed golden dump
(`testdata/golden/registries.json`) is the canonical reference for the parity
tests, but it is not a live server's id assignment: the vanilla asset pipeline
decodes assets in parallel and assigns a few sibling ids in decode-scheduling
order (measured: sibling block ids such as `2591 ↔ 2592`, and `Lava`/`Tar`
`13 ↔ 14`; see `REMAINING.md` §6).

A server that wants the generated ids to match **its own** asset maps must
hand the port a snapshot taken from the running server.

## 1. Java server: write the snapshot at boot

`tools/java-dumper` contains `RegistrySnapshot`, a small exporter that reads
the already-loaded static CoreServer maps (it does not boot anything):

```java
import com.hypixel.hytale.tools.golden.RegistrySnapshot;
import java.nio.file.Path;

// after AssetRegistryLoader.loadAssets(...) has completed, including mods:
RegistrySnapshot.write(Path.of("run/worldgen-registries.json"));
```

It writes the same schema as the golden dump:

| section | fields the Rust loader reads |
|---|---|
| `blocks.assets[]` | `name`, `index`, `drawType`, `drawTypeId`, `opacity`, optional `support` (`{down,up}` per `RotationTuple`) |
| `fluids.assets[]` | `name`, `index`, `maxFluidLevel` |
| `environments.assets[]` | `name`, `index` |

The `meta` block is informational and ignored by Rust.

The class depends on `Phase2Dumper`'s JSON writer (both in the dumper
package), so either put `tools/java-dumper/target/classes` (or the built jar)
on the server classpath, or copy those two classes into the server/plugin
source tree. It only uses public CoreServer APIs (`BlockType`, `Fluid`,
`Environment`) plus that writer.

A live snapshot also **closes the known golden gap**: the server has the
prefab-list store loaded, so `ExamplePrefabPlacementBlock_Tree` is present
(6,136 blocks instead of 6,135 for the vanilla pack).

### Standalone (no server) exercise

The same writer can boot the pipeline in-process over `Assets/Assets.zip`,
which is useful for testing the export/read round trip:

```sh
DUMPER_MAIN=com.hypixel.hytale.tools.golden.RegistrySnapshot \
  tools/java-dumper/run.sh /tmp/live-registries
# writes /tmp/live-registries/registries.json
```

`DUMPER_PARALLELISM=<n>` controls the common fork-join pool (default 1). Use a
parallel value to reproduce the scheduling-dependent ids; this standalone path
is *not* a live server, only the in-server call is.

## 2. Rust side: consume the snapshot

```sh
worldgen-cli generate --world testdata/Default \
  --registries run/worldgen-registries.json ...
```

or, when embedding the service:

```rust
let options = WorldGenOptions {
    registries_path: Some("run/worldgen-registries.json".into()),
    ..Default::default()
};
let service = WorldGenService::new(config, options)?;
```

`ChunkGeneratorJsonLoader::with_registries_path` is the loader-level hook;
`ChunkGenerator::get_registries()` exposes the loaded snapshot to the
executions and to the worker pool.

A future Rust server does not need this file: build the registries directly
with the public constructors (`BlockRegistry::with_names`,
`FluidRegistry::with_names`, `EnvironmentRegistry::new`,
`Registries::with_environments`) from its own asset pipeline and pass them to
the generator/service.

## 3. Validate / compare

```sh
worldgen-cli registries \
  --path run/worldgen-registries.json \
  --compare testdata/golden/registries.json
```

Output (example run; the swap count varies with scheduling):

```text
run/worldgen-registries.json: 6135 blocks, 19 fluids, 135 environments
testdata/golden/registries.json: 6135 blocks, 19 fluids, 135 environments
blocks: 12 id swaps, 0 metadata differences, 0 only in the first snapshot, 0 only in testdata/golden/registries.json
fluids: 0 id swaps, 0 max-level differences, 0 only in the first snapshot, 0 only in testdata/golden/registries.json
environments: 0 id swaps, 0 only in the first snapshot, 0 only in testdata/golden/registries.json
structurally identical (names and block/fluid metadata match)
```

Pass the **server** snapshot as `--path` and the golden dump as `--compare`:
extra entries in the first file (mods) are tolerated, entries the server is
missing are structural.

Id swaps are expected and do **not** fail (they are the scheduling effect).
A different name set, a different `drawType`/`opacity`/`support`, or a
different `maxFluidLevel` is structural and exits non-zero — that means the
snapshot came from a different pack/code, not just a different scheduling.

## 4. Shortcut: single-threaded vanilla server

If the server is vanilla (no mods) and you want to keep using the committed
dump, start it with

```sh
-Djava.util.concurrent.ForkJoinPool.common.parallelism=1
```

and verify with the exporter + `worldgen-cli registries --compare`. If the
comparison is structurally identical (and the ids match the dump), the
committed snapshot is authoritative for that setup. Mods that add block types
always require their own snapshot.

## 5. What this does not solve

- It does not port the asset pipeline to Rust; it assumes an existing live
  asset boot (Java today, a future Rust server's own pipeline later).
- It does not make the *golden* chunk ids match a parallel server; it makes
  the port use the server's ids. The parity tests keep using the canonical
  single-threaded dump.
- It does not add the prefab entities/components that are deferred in
  `REMAINING.md`; the snapshot only covers the registries.
