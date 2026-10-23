# T39 — Asset wiring, config cache and `--validate-assets`

**Status:** planned (session 25+)
**Package:** `hytale-assets`, `hytale-server`, `client-bot`
**Depends on:** T36, T37, T38
**Agent fit:** medium

## Goal

Make the full config packet set part of the join, cache it for multi-connection
use, and expose an asset validation command.

## Deliverables

- Final dependency order. The exact order is constraint-driven; every index
  reference must arrive before its consumer. Expected shape:
  `BlockBoundingBoxes` → `PhysicalMaterial` → `SoundEvent`/`BlockSoundSet` →
  `FluidFX` → `Fluid` → `Environment` → `TagPattern` → `BlockType` →
  `BlockParticleSet`/decals → `ItemSoundSet`/`ItemQuality`/`ItemCategory` →
  `Item`.
- Add the new packets to `AssetRegistry::config_assets()` and the session setup
  burst.
- Cache encoded `ConfigAssets` once per loaded registry and share it across
  sessions (`Arc`/`OnceLock` at the world/universe level); no per-connection
  8–10 MiB re-encode.
- Add `hytale-server --validate-assets [--pack PATH]`:
  - loads every enabled store;
  - reports counts, failures, ids, tag indexes and encoded packet sizes;
  - exits non-zero on validation drift.
- Update `client-bot` packet recognition and the auth loopback expectations.
- Re-run the semantic goldens, remove closed residual exceptions and update the
  documented known differences.

## Acceptance

```sh
cargo test -p hytale-assets
cargo test -p client-bot --test auth_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run -p hytale-server -- --validate-assets
```

- The authenticated join sends all enabled config packets in dependency order.
- Asset validation is deterministic and reports no failures for the default
  pack.
- Config packets are encoded once per process/world.
- All P0 tests and the teleport/movement loop still pass.

## Deviations

- P2/P3 stores outside the selected closure remain deferred.
- The validation command does not regenerate goldens; it compares against the
  committed expectations.
