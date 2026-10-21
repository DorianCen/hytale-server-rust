# Live registry snapshot plugin

A tiny Hytale server plugin that writes the **live** block / fluid /
environment registries in the `registries.json` schema used by the Rust world
generator. It dumps in `start()`, after the server has loaded its assets
(including mods) and before the first world is generated.

Use it when the Rust generator must use the server's own numeric ids. The
canonical golden dump is single-threaded and a real server decodes assets in
parallel, so a few sibling ids (and `Lava`/`Tar`) are assigned in
scheduling-dependent order. See [`../../LIVE_SNAPSHOT.md`](../../LIVE_SNAPSHOT.md).

## Build

```sh
tools/live-snapshot-plugin/build.sh
# or: tools/live-snapshot-plugin/build.sh /path/to/HytaleServer.jar
```

The server jar defaults to the launcher install path
(`~/.var/app/com.hypixel.HytaleLauncher/.../Server/HytaleServer.jar`); use
`HYTALE_SERVER_JAR` or the first argument otherwise.

## Run

Put `target/RegistrySnapshotPlugin.jar` into a server `mods/` directory and
set the output property:

```sh
cd /path/to/server-run-dir
mkdir -p mods
cp /path/to/RegistrySnapshotPlugin.jar mods/
java \
  -Dworldgen.snapshot.output=/path/to/worldgen-registries.json \
  -Dworldgen.snapshot.shutdown=true \
  -jar /path/to/HytaleServer.jar \
  --assets /path/to/Assets.zip --bare --auth-mode offline --disable-sentry
```

Properties:

| property | meaning |
|---|---|
| `worldgen.snapshot.output` | target JSON file (required; no file written when absent) |
| `worldgen.snapshot.shutdown` | `true` stops the server ~1 s after writing (headless capture) |

`--bare --auth-mode offline` is enough to load the asset pipeline and start
the plugin manager; the core `PermissionsModule` currently logs an unrelated
`mod_error` in bare mode, but the snapshot is written first. In a normal
server boot the plugin's shutdown hook exits cleanly.

## Validate the result

```sh
worldgen-cli registries \
  --path /path/to/worldgen-registries.json \
  --compare testdata/golden/registries.json
```

A real boot of the 2026-10-21 server produced **6,138 blocks / 19 fluids /
135 environments** against the golden **6,135 / 19 / 135**: the three extra
blocks are the prefab-list-dependent `ExamplePrefabPlacementBlock_Tree` and
the two portal keys (the golden dump's known gap), the name/metadata sets are
identical, and 6,130 block ids differ only by the expected scheduling shifts.

Generating the seven seed-0 reference chunks with that snapshot and comparing
block/fluid **names** and tint against the Java golden produced **0
differences over 2,293,760 voxels**.
