# World config fixture

`config.json` is written by the real Java `WorldConfig` codec
(`WorldConfig.CODEC`, version 4) through `WorldConfig.save`, which is exactly what
`WorldConfigProvider.save` writes to `<world>/config.json`.

Regenerate and verify determinism:

```sh
tools/java-dumper/run-world-config.sh
tools/java-dumper/run-world-config.sh --check
```

The dumper boots the real asset stores because `WorldConfig`'s static
initialiser builds the gameplay-config defaults. It registers the provider
codecs that `Universe` normally registers at boot (Global spawn, Void worldgen,
Disabled world map, default chunk/resource storage).

Values:

| Field | Value |
|---|---|
| `Version` | 4 |
| `UUID` | `12345678-1234-5678-9abc-def012345678` |
| `DisplayName` | `Rust Golden World` |
| `Seed` | `1234567890123` |
| `WorldGen` | `Void` (`#5b9e28`, `Env_Zone1_Plains`) |
| `ChunkConfig` | pregenerate `-512..512`, keep loaded `-64..64` |
| `GameTime` | `2026-10-22T12:34:56Z` |
| `GameMode` | `Adventure` |
| `DisabledFluidTickers` | `["Fluid=Water"]` |

The Rust gate is `crates/hytale-world/tests/world_config.rs`: it asserts every
field above, then round-trips the document without losing fields (including
unknown ones). `crates/hytale-server/tests/world_config.rs` covers the CLI copy
path.

Reverse check (Rust writes, Java reads):

```sh
cargo run -q -p hytale-server -- world-config \
    --input testdata/golden/server/world-config/config.json \
    --save target/world-config-smoke
tools/java-dumper/run-world-config.sh --probe target/world-config-smoke/config.json
```

`WorldConfigDumper probe` loads the file with the real `WorldConfig` codec and
checks the UUID, seed, name, game time, flags and gameplay config. The
`GameplayConfig` key is checked from the parsed document and removed before the
codec decode: this harness cannot load the gameplay-config assets (they depend
on the camera-effect store), so the codec validator for that one key cannot run
here. A production Java boot validates it normally.

Do not edit `config.json` by hand.
