# T21 — Complete saved-world boot and the whole-save Java gate

**Status:** done (session 11)
**Package:** `hytale-world`, `hytale-server`, `tools/java-dumper`
**Depends on:** T18 (chunks), T19 (config), T20 (players)
**Agent fit:** medium

## Goal

Compose the three parts of a saved world — `config.json`, `players/` and
`chunks/` — into one boot path, and add a single Java gate that verifies a
Rust-written save in all three places.

## Deliverables

- `hytale-world::world::WorldSave`:
  - opens a save root, loads `config.json` or creates it;
  - owns the runtime `World` (chunk storage) and `PlayerStorage`;
  - `load_or_create_player` uses the config spawn point when creating;
  - `save_player`, `save_config`, `save_all` (dirty chunks + config + flush).
- `WorldUuid::derive(seed, label)` moved into `world_config` (the config CLI now
  uses it too).
- `hytale-server::world_boot` and CLI
  `hytale-server world-boot [--assets-pack] [--save] [--seed] [--name] [--uuid]
  [--player-uuid] [--set-block x,y,z,id]`:
  - load/create config, load/create player;
  - load every stored chunk;
  - optional block edit;
  - save dirty chunks + config, re-save the player holder.
- `crates/hytale-world/tests/world_save.rs`: create config+player+chunk, reopen,
  assert all three; spawn point and existing-config cases.
- `crates/hytale-server/tests/world_boot.rs`: config/player create then reopen.
- `crates/hytale-server/tests/headless.rs` extended with the composed boot after
  generation and mutation.
- Java whole-save gate: `tools/java-dumper/run-save-check.sh <save> <chunkX>
  <chunkZ> <blockX> <blockY> <blockZ> <expected-block>` runs the existing config,
  player and chunk probes against one save.

## Acceptance

```sh
rm -rf target/save-smoke
cargo run -q -p hytale-server -- headless --save target/save-smoke --chunk 4,-5
cargo run -q -p hytale-server -- world-config \
    --input testdata/golden/server/world-config/config.json --save target/save-smoke
cargo run -q -p hytale-server -- player-data \
    --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
    --save target/save-smoke
cargo run -q -p hytale-server -- world-boot --save target/save-smoke \
    --set-block 129,250,-159,4
tools/java-dumper/run-save-check.sh target/save-smoke 4 -5 129 250 -159 Debug_Test_Block
```

- `world-boot` reports one stored/loaded/saved chunk, the loaded config and the
  loaded player, and applies the block edit.
- The Java gate prints `java world config check: ok`, `java player data check:
  ok`, `java reload check: ok` and `whole-save check: ok`.
- Workspace tests, clippy and fmt are green.

## Deviations

- `world-boot` loads stored chunks; it does not generate missing ones yet
  (`headless` remains the generation path).
- It loads every stored chunk instead of demand-loading around a player.
- A new config has no spawn provider, so a newly created player starts at the
  origin; when the config has a `Global`/`SpawnPoint`, only the position is used
  for the player transform (rotation starts at zero).
- A newly created player has `WorldGenId = 0`; gameplay components are not
  hydrated on boot.
- `headless` still writes the chunk directly and does not create config/player;
  the fixture setup uses the dedicated config/player CLIs before `world-boot`.

## Follow-ups

- Generate missing chunks on boot and wire the `WorldGenRuntime`.
- Demand-load chunks around the player view radius.
- Hydrate `Player`/inventory/settings/skin components when the join path lands.
- Client-bot auth/setup/game with the packet-trace fixture.
