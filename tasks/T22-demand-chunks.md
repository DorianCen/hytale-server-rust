# T22 — Generate missing chunks and demand-load around the player

**Status:** done (session 12)
**Package:** `hytale-world`, `hytale-server`, `worldgen-hytale`
**Depends on:** T21 (complete saved-world boot)
**Agent fit:** medium

## Goal

Make the saved-world boot load the chunks the player needs: load stored chunks,
generate the missing ones, and save them. Extract the worldgen boot so `headless`
and `world-boot` share one setup.

## Deliverables

- `hytale-world::world`:
  - `WorldSave::ensure_chunks(codec, coordinates, generate)` loads in-memory,
    then stored, then generated chunks; generated columns are inserted dirty.
  - `DemandReport` (`requested`/`already_loaded`/`loaded`/`generated`).
  - `chunk_coordinates_around(center_x, center_z, radius)`.
- `hytale-server::generation::WorldGenerator`:
  - shared assets → P0 registry → worldgen registries → worker pool boot;
  - `generate(seed, x, z) -> ChunkColumn` and `shutdown`;
  - `headless` refactored to use it.
- `hytale-server world-boot` gained `--assets-root`, `--world`, `--workers` and
  `--view-radius` (default `0` = the player's chunk).
  - It derives the player chunk from the loaded transform, unions stored chunk
    coordinates with the demand square, loads/generates them, then edits/saves.
  - Report now includes requested/loaded/generated counts and the radius.
- Tests:
  - `hytale-world/tests/world_save.rs` demand pass (stored vs generated, second
    pass all from disk);
  - `hytale-server/tests/world_boot.rs` generates the player chunk, then reloads
    it on the second boot;
  - `hytale-server/tests/headless.rs` boots with demand loading and saves both
    the stored and generated chunks.
- Worldgen fidelity fix: `PrefabPatternGenerator::get_displacement` now uses
  `seed.wrapping_add(seed_offset)`, matching Java's wrapping `int` addition. The
  demand gate hit an overflow panic with the Java config seed cast to `int`.

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
    --view-radius 1 --set-block 129,250,-159,4
tools/java-dumper/run-save-check.sh target/save-smoke 4 -5 129 250 -159 Debug_Test_Block
```

- `world-boot` reports 1 stored chunk, 9 generated chunks around the player and
  10 saved chunks (the edit plus the generated set).
- The whole-save Java gate still passes.
- The generated region reads in Java:
  `run-reload-check.sh target/save-smoke 0 0 0 64 0 Empty`.
- Existing worldgen goldens stay green (`cargo test -p worldgen-hytale
  -p worldgen-model`).

## Deviations

- `view_radius` is a square chunk radius; there is no view-distance shape,
  priority queue or cancellation yet.
- The boot still loads every stored chunk, even outside the radius.
- Demand loading is synchronous on the calling thread; the live world thread
  will move it behind the tick loop.
- Generated chunks are marked dirty and saved immediately by the boot.
- The config `Seed` is a Java `long`, but `worldgen-server` takes an `i32`; the
  boot casts it (`long as int`), matching Java's worldgen API.

## Follow-ups

- Move demand loading onto the world thread with a priority queue and
  cancellation.
- Add a proper view-distance tracker for the join path.
- Stream the loaded/generated chunks to a client (chunk packets).
- Client-bot auth/setup/game with the packet-trace fixture.
