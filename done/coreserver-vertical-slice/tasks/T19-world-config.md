# T19 — World config load/save and the Java config gate

**Status:** done (session 9)
**Package:** `hytale-world`, `hytale-server`, `tools/java-dumper`
**Depends on:** T18 (runtime world storage)
**Agent fit:** medium

## Goal

Load and write the per-world `config.json` (`WorldConfig` v4) with the real Java
codec as the oracle. The chunk regions and the world config are the two halves of
a saved world; this task lands the metadata half.

## Deliverables

- `hytale-world::world_config`:
  - `WorldConfig` with the v4 fields in codec order: UUID, display name, seed,
    providers, chunk config, flags, game time, client effects, gameplay config,
    disabled fluid tickers and plugin sections.
  - `WorldUuid` encoded as Java's strict extended JSON
    (`{"$binary":"<base64>","$type":"04"}`).
  - Provider sections (`WorldGen`, `WorldMap`, `ChunkStorage`,
    `ResourceStorage`, `SpawnProvider`) kept as ordered JSON objects.
  - Unknown fields preserved via `extra`, so a newer Java config survives a
    Rust load/save cycle.
  - `WorldConfig::load`/`save` with Java's pretty JSON, `.bak` copy and atomic
    temp-file rename; `WorldConfig::path_in(save_dir)`.
- `hytale-server::world_config_tool`:
  - load an existing config (`--input`) or Java-default a new one;
  - write `<save>/config.json`;
  - deterministic UUID derived from seed + name for headless worlds.
- CLI: `hytale-server world-config [--input] [--save] [--seed] [--name]
  [--uuid]`.
- Java oracle:
  - `WorldConfigDumper write <path>` constructs a config with known values and
    saves it through the real `WorldConfig.save`.
  - `WorldConfigDumper probe <path>` loads a Rust-written document with the real
    codec and checks every expected field.
  - `tools/java-dumper/run-world-config.sh` (`--check` determinism, `--probe`).
- Fixture: `testdata/golden/server/world-config/config.json` + README.
- Tests:
  - `crates/hytale-world/tests/world_config.rs` — fixture fields, lossless
    round-trip, unknown-field preservation, atomic save/backup.
  - `crates/hytale-server/tests/world_config.rs` — fixture copy and stable
    default UUIDs.

## Acceptance

```sh
tools/java-dumper/run-world-config.sh --check
cargo run -q -p hytale-server -- world-config \
    --input testdata/golden/server/world-config/config.json \
    --save target/world-config-smoke
tools/java-dumper/run-world-config.sh --probe target/world-config-smoke/config.json
cargo test -p hytale-world --test world_config
cargo test -p hytale-server --test world_config
```

- The Java dumper is deterministic and matches the committed fixture.
- The Rust-written config loads in the real Java `WorldConfig` codec with the
  same UUID, seed, name, game time, flags and gameplay config.
- Unknown fields survive load → save → reload.

## Deviations

- `GameplayConfig` is validated from the parsed document and stripped before the
  codec decode in the probe. The harness cannot load the gameplay-config assets
  (they depend on the camera-effect store), so the codec validator for that one
  key cannot run there. A production Java boot validates it normally.
- `DisabledFluidTickers` uses `BTreeSet`, so Rust writes a deterministic sorted
  array; Java uses a `HashSet`.
- Provider payloads are not decoded into typed structs. The world config owns
  `Type`/`Id` and the common fields; provider-specific options stay as JSON.
- The headless chunk boot does not write or read `config.json` yet. The CLI does;
  wiring the boot to it is the follow-up.
- New headless worlds get a UUID derived from seed + name instead of Java's
  random UUID, so repeated runs are comparable.

## Follow-ups

- Player BSON (`DiskPlayerStorageProvider`, `EntityStore.REGISTRY`).
- Wire `WorldConfig` into the headless boot and `World` lifecycle (seed, spawn
  provider, world-gen settings, save flags).
- Decode provider payloads when the provider system lands.
- `WorldMapConfig`, death overrides and crash recovery stay opaque until their
  consumers are ported.
