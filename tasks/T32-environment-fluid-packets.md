# T32 — Environment and Fluid config asset packets

**Status:** done (session 22)
**Package:** `hytale-assets`, `hytale-server`, `client-bot`
**Depends on:** T30 (config asset streaming), T28 (P0 packet encoders)
**Agent fit:** medium

## Goal

Stream the two remaining join-critical world-content stores whose payloads are
self-contained enough to encode now: `Environment` (61) and `Fluid` (83).
`BlockType`/`Item` remain a follow-up because their contained-asset ids and
payloads are much larger.

Java's tag indexes are assigned globally across all 54 stores in load order. The
Rust server already owns its block/item ids; it now owns its tag indexes too.
Every environment/fluid packet and the `ServerTags` map are built from one
`TagRegistry`, so the client always resolves the indexes consistently.

## Deliverables

- `hytale-assets::packets`:
  - `TagRegistry`: deterministic alphabetical indexes for every tag key, value
    and `key=value` composite found in the P0 documents; `ServerTags` map.
  - `ConfigAssets { packets, tags }` replacing the packet-only return.
  - `AssetRegistry::environment_packet` (`UpdateEnvironments`):
    id, water tint, tags, `rebuildMapGeometry = true`, `maxId = max + 1`.
  - `AssetRegistry::fluid_packet` (`UpdateFluids`):
    id, max level, textures with normalized weights, opacity, shader effects,
    light (Java's 3-digit nibble / 6-digit `/17` rule), particles, draw type,
    particle color, tags.
  - Config packet order: hitboxes, physical materials, fluids, environments.
- `WorldThread::config_asset_packets` returns the packets and tag map.
- `session::send_player_list` sends the matching non-empty `ServerTags`.
- Tests:
  - Environment/Fluid decode, counts (135/19), max ids, and all non-tag fields
    compared with the Java goldens;
  - every tag index used by either packet exists in `ServerTags`.

## Acceptance

```sh
cargo test -p hytale-assets --test asset_packets
cargo test -p client-bot --test auth_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The authenticated join reports config packets `[41, 87, 83, 61]`.
- Environment/fluid non-tag fields match the Java goldens.
- The bot completes the join and movement loop unchanged.

## Deviations

- Tag indexes are Rust-owned, not Java's global assignment order. This is the
  same policy as Rust-owned `BlockType`/`Item` ids; consistency is enforced by
  the `ServerTags` packet.
- `fluidFXIndex` and `blockSoundSetIndex` are 0 because the `FluidFX` and
  `BlockSoundSet` stores are not loaded or streamed yet.
- Environment `fluidParticles` is always `None`: the P0 pack has none, and the
  `FluidFX` indexes are unavailable.
- `Environment`/`Fluid` payloads are not byte-identical to Java because of the
  tag indexes and the two missing child-store indexes.
- `BlockType`/`Item` packets remain pending.

## Follow-ups

- Encode `BlockType` and `Item` from the Rust registry (goldens can validate
  structure/semantics; contained ids and tags stay Rust-owned).
- Add `FluidFX`, `BlockSoundSet` and `BlockParticleSet` so the fluid indexes and
  sound/particle references become real.
- Add `UpdateTagPatterns` for tag-pattern assets.
