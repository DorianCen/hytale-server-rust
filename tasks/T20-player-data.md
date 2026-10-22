# T20 — Player save BSON and the Java player gate

**Status:** done (session 10)
**Package:** `hytale-world`, `hytale-server`, `tools/java-dumper`
**Depends on:** T19 (world config)
**Agent fit:** medium

## Goal

Read and write `DiskPlayerStorageProvider` player files (`<world>/players/<uuid>.json`)
with the real Java `EntityStore` holder codec as the oracle. This is the last
part of a saved world after the chunk regions (T18) and world config (T19).

## Deliverables

- `hytale-world::player_data`:
  - `PlayerData`: the `{"Components": {...}}` holder document, with ordered
    component preservation.
  - Typed core components: `UUID` (extended JSON), `Transform` (position +
    rotation in Java's radians), and `WorldGenId`.
  - Unknown components stay as JSON and survive load → save → reload, so a full
    Java player save round-trips before inventory/gameplay components are ported.
  - `PlayerStorage`: `<save>/players/<uuid>.json` load/save/remove/list,
    Java-style pretty JSON, `.bak` copy and atomic rename.
- `hytale-server::player_data_tool` and CLI
  `hytale-server player-data [--input] [--save] [--uuid] [--x --y --z]
  [--pitch --yaw --roll] [--world-gen-id]`.
- Java oracle:
  - `WorldPlayerDumper` registers the `EntityModule.setup()` ids for the core
    components, builds a holder, serializes it with
    `EntityStore.REGISTRY.serialize`, and writes it with `BsonUtil.writeDocument`.
  - `probe` mode deserializes a Rust-written file with the real registry, checks
    all components, and re-serializes the holder.
  - `tools/java-dumper/run-player-data.sh` (`--check`, `--probe`).
- Fixture `testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json`
  + README.
- Tests:
  - `crates/hytale-world/tests/player_data.rs` — fixture fields, lossless
    round-trip, unknown components, storage save/list/remove/backup.
  - `crates/hytale-server/tests/player_data.rs` — CLI copy and create paths.

## Acceptance

```sh
tools/java-dumper/run-player-data.sh --check
rm -rf target/player-smoke
cargo run -q -p hytale-server -- player-data \
    --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
    --save target/player-smoke
tools/java-dumper/run-player-data.sh --probe \
    target/player-smoke/players/12345678-1234-5678-9abc-def012345678.json
cargo test -p hytale-world --test player_data
cargo test -p hytale-server --test player_data
```

- The Java dumper is deterministic and matches the committed fixture.
- The Rust-written player file loads in the real `EntityStore` codec with the
  same UUID, transform and world-gen id, and re-serializes with the same
  `Components`.
- Unknown components survive load → save → reload.

## Deviations

- Only `UUID`, `Transform` and `WorldGenId` are typed. `Player`, inventory,
  stats, settings and skin components are preserved as opaque JSON until their
  systems are ported.
- The fixture is a minimal holder, not a full join state. Java accepts the
  missing components; a production spawn adds defaults.
- `PlayerStorage::players` sorts UUID strings for deterministic listing rather
  than mirroring a filesystem directory order.
- The world boot does not load/save players yet; the CLI is the only caller.

## Follow-ups

- Wire `WorldConfig` + player load into a world boot and the join path.
- Port the `Player`/inventory/settings/skin component codecs as gameplay lands.
- Generate-missing-chunks path on top of `World`.
- Client-bot auth/setup/game with the packet-trace fixture.
