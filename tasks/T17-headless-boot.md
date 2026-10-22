# T17 — Headless boot: generate one chunk and save a region blob

**Status:** done (session 7)
**Package:** `hytale-server`, `worldgen-hytale`, `worldgen-server`
**Depends on:** T16 (storage codec), T15 (asset registry)
**Agent fit:** medium

## Goal

Compose the first real server path: load the vanilla asset pack, generate one
chunk with the server's own registry ids, convert it to the runtime column, save
a Java-compatible region blob and verify the reload. No network or ECS layer
yet.

## Deliverables

- `ChunkGeneratorJsonLoader::with_registries` (in-memory snapshot).
- `WorldGenOptions.registries` so `worldgen-server` workers use the server
  registry instead of `registries.json`.
- `hytale-server::headless`:
  - asset pack → P0 `AssetRegistry` → `worldgen-model::Registries`;
  - `WorldGenChunkProvider` generation;
  - `ChunkColumn::from_generated_chunk_with_registries`;
  - `ChunkCodec` + `RegionFile` save;
  - reopen/decode verification.
- CLI: `hytale-server headless [--assets-pack] [--assets-root] [--world]
  [--save] [--seed] [--chunk x,z] [--workers]`.
- `ChunkCodec::decode_at(bytes, x, z)`.
- `crates/hytale-server/tests/headless.rs` (skips without the base-game pack).

## Acceptance

```sh
cargo run -p hytale-server -- headless --save target/headless-world
cargo test -p hytale-server
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The generated chunk uses the asset-registry ids for blocks, fluids and
  environments.
- The saved region blob reopens and decodes to the same blocks, fluids,
  environments, tint and heightmap.
- The existing chunk-packet and storage goldens stay green.

## Follow-ups

- Region → runtime world loading (not just the one-chunk verification).
- Player BSON, `World.json`/world config.
- Client-bot auth/setup/game and the packet-trace fixture.
