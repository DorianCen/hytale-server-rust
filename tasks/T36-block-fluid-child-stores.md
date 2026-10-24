# T36 — Block/fluid child stores and packet encoders

**Status:** done (session 26)
**Package:** `hytale-assets`
**Depends on:** T35
**Agent fit:** medium

## Goal

Load and stream the stores that resolve `BlockType`, `Fluid` and `Environment`
child indexes, so those packets carry real ids instead of `0`.

## Deliverables

- Add P1/child store specs and collections to `hytale-assets::registry` for:
  - `BlockBreakingDecal` — `Item/Block/BreakingDecals`, DefaultAssetMap.
  - `BlockParticleSet` — `Item/Block/Particles`, DefaultAssetMap.
  - `BlockSoundSet` — `Item/Block/Sounds`, IndexedLookupTableAssetMap.
  - `FluidFX` — `Item/Block/FluidFX`, IndexedLookupTableAssetMap.
  - `SoundEvent` — `Audio/SoundEvents`, IndexedLookupTableAssetMap.
  - Plus the T35 dependency closure needed to decode them.
- Keep the P0 `AssetRegistry::load_p0` API working; add a clearly named
  extended loader (for example `load_phase2`/`load_all`) rather than silently
  changing every caller.
- Use Java-compatible id assignment and parent rules for each store; reproduce
  preloads/code-defined assets where Java has them.
- Packet encoders in `hytale-assets::packets` for:
  - `UpdateBlockBreakingDecals`
  - `UpdateBlockParticleSets`
  - `UpdateBlockSoundSets`
  - `UpdateFluidFX`
  - `UpdateSoundEvents`
- Resolve into the existing packets:
  - `BlockType.blockParticleSetId` stays a string; `blockSoundSetIndex` becomes
    the real `BlockSoundSet` id.
  - `Fluid.fluidFxIndex` and `Fluid.blockSoundSetIndex` become real ids.
  - Environment `fluidParticles` becomes `Some(...)` when the pack defines it.
- Golden tests in `crates/hytale-assets/tests/`:
  - byte comparison where ids match Java;
  - semantic/key comparison where Rust-owned ids differ;
  - every emitted child packet decodes with the generated codec;
  - counts and max ids match `index.json`.

## Acceptance

```sh
cargo test -p hytale-assets
cargo test --workspace
cargo clippy -p hytale-assets --all-targets -- -D warnings
cargo fmt -p hytale-assets -- --check
```

- All new packets decode.
- `blockSoundSetIndex`, `fluidFxIndex`, `blockSoundSetIndex` and environment
  fluid particles resolve for every asset that has them in Java.
- P0 tests and the join still pass.

## Deviations

- Stores outside the selected closure stay deferred (P2/P3 backlog).
- Child indexes remain Rust-owned when the Java id order is not reproducible;
  publish consistency through the packet + `ServerTags`/index registries.
- `AudioCategory`/`AudioState` are loaded as `SoundEvent` dependencies but have
  no init packet in this task.
- `AudioCategoryDuckingRule.targetAudioCategoryIndex` is emitted as `NOT_FOUND`
  to match the canonical dumper payload; Java resolves it in a runtime
  `LoadedAssetsEvent` that the asset-only boot does not run.
- The `ConcurrentHashMap` wave-order model was refined to include resize-time
  partial reversals. This reproduces the two colliding `AudioCategory` ids and
  also affects the Rust-owned `BlockType` order beyond the fixed preload
  prefix; P0 registry/golden tests remain green.

## Follow-ups

- T39 wires the five child packets into the join in dependency order.
- Add `AudioCategory`/`AudioState` streaming if a later client path needs them.
- Resolve ducking targets at audio-runtime load time rather than in the pure
  asset encoder.
