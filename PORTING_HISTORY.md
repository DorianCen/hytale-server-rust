# Porting history — CoreServer rewrite, phase 2

Session 1–23 history is archived in
[`done/coreserver-vertical-slice/PORTING_HISTORY.md`](done/coreserver-vertical-slice/PORTING_HISTORY.md).
This file records phase-2 sessions only.

---

## Session 24 — Archive and phase-2 re-baseline (2026-10-24)

- **Goal:** close out the first vertical slice and leave a clean phase-2 plan
  and task board.
- **Landed:**
  - T34: archived the completed phase docs and tasks T01–T33 under
    [`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md).
  - Updated `done/world-generation/README.md` to distinguish the two archives
    and point at the new root plan.
  - New root docs: `plan.md` (T34–T64 milestones), `NEXT_STEPS.md`,
    `ARCHITECTURE.md`, `PORTING.md`, `PORTING_HISTORY.md`, `REMAINING.md`,
    `GOLDEN_DATA.md`, `BENCHMARKS.md`, and `tasks/README.md`.
  - Created the M2/M3 task packets T35–T42; later milestone tasks remain
    roadmap-sized in `plan.md` until their milestone starts.
- **Verified:**
  - Markdown links in both `done/` trees resolve.
  - `cargo test --workspace`, clippy and fmt green (code unchanged).
- **Follow-ups:** begin T35 (child-store goldens), then T36–T39 and T40–T42 in
  parallel.

---

## Session 25 — Child-store golden dumper extension (2026-10-24)

- **Goal:** extend the Java asset oracle to the P1 stores and the P2/P3
  dependency roots needed by the phase-2 child indexes.
- **Landed:**
  - T35: `AssetDumper` now accepts `--priority`, `--stores` and `--closure`,
    preserves the default P0 set for `--priority P0`, and emits the phase-2
    default: P0 + P1 + `FluidFX`, `ItemSoundSet`, `SoundEvent`, `TagPattern`,
    `ParticleSystem`, `ParticleSpawner`, `ItemReticleConfig`,
    `ItemPlayerAnimations` and their `loadsAfter` closure.
  - `run-assets.sh` parses the same selector flags and still supports
    `--check` for the full committed set.
  - Committed 26 store entries / 21 payloads (~12.6 MiB) in
    `testdata/golden/server/assets/`: block decals/particles/sounds, item
    categories/qualities/sounds/animations/reticles, audio categories/states,
    FluidFX, particle systems/spawners, sound events and TagPattern.
  - `index.json` now records `selectedStoreCount`, dynamic `storeFilter`,
    `missingPackets`, `packetErrors`, `missingDependencies` and per-store
    `packetError`.
  - Per-store packet-generation failures are recorded instead of aborting the
    dump.
  - Updated the asset/Java dumper READMEs, the golden README, `GOLDEN_DATA.md`
    and the task board.
- **Verified:**
  - Two isolated default dumps are byte-identical.
  - `cargo test -p hytale-assets` and `cargo test --workspace` pass with the
    regenerated goldens.
  - Existing P0 payload sizes are unchanged; the four semantic P0 payloads were
    re-baselined to the canonical single-worker output.
- **Deviations:**
  - `BlockSet` (80 assets) has no payload: its generator needs
    `BlockSetModule`, which the asset-only boot does not install. The index entry
    carries `packetError`.
  - `GameplayConfig` has count 0 because `Default` fails validation on a missing
    camera effect; it has no packet generator.
  - `PrefabListAsset` remains empty (prefab store/AssetModule not booted).
  - `loadsAfter` dependencies outside the 54-store inventory are reported as
    `meta.missingDependencies` and not invented.
- **Follow-ups:**
  - T36 (block/fluid child stores), T37 (item child stores) and T38 (TagPattern)
    can now start in parallel; T39 wires them into the join.

---

## Session 26 — Block/fluid, item and tag-pattern child stores (2026-10-24)

- **Goal:** finish the M2 child-store work in parallel: block/fluid indexes, item
  indexes and `TagPattern`.
- **Landed:**
  - T36: `BlockBreakingDecal`, `BlockParticleSet`, `BlockSoundSet`, `FluidFX`
    and `SoundEvent` load through `AssetRegistry::load_phase2`, with
    `AudioCategory`/`AudioState` as `SoundEvent` dependencies. All five init
    packets are byte-identical to the Java goldens. `BlockType.blockSoundSetIndex`,
    `Fluid.fluidFxIndex`, `Fluid.blockSoundSetIndex` and
    `Environment.fluidParticles` resolve from the stores.
  - T37: `ItemSoundSet`, `ItemQuality`, `ItemReticleConfig`, `ItemCategory` and
    `ItemPlayerAnimations` load (181 animations = 160 files + 21 contained).
    All five init packets are byte-identical. `qualityIndex`, quality-driven
    `itemEntity`, `itemSoundSetIndex`, `reticleIndex` and `playerAnimationsId`
    now match Java for all 3,971 items, so the old residual list is empty.
  - T38: `TagPattern` loads 196 assets (11 files + 185 contained) with exact
    Java key/id parity. `UpdateTagPatterns` emits the same op trees and payload
    size as Java and is sent before `BlockType`/`Item`. `TagRegistry` includes
    pattern-only `Equals` tags. The server's `WorldGenerator` uses
    `load_phase2` so the join streams patterns.
  - Merged the three branches into one integration branch and reconciled the
    shared `AssetRegistry`, `packets/mod.rs`, preloads and `load_phase2`.
- **Verified:**
  - `cargo test -p hytale-assets` (all child-store suites).
  - `cargo test --workspace`.
  - `cargo clippy --workspace --all-targets -- -D warnings`.
  - `cargo fmt --all -- --check`.
- **Deviations:**
  - `UpdateTagPatterns` cannot be byte-compared because Rust owns tag indexes;
    op trees/size match and every index resolves in `ServerTags`.
  - `AudioCategoryDuckingRule.targetAudioCategoryIndex` stays `NOT_FOUND` to
    match the dumper; Java resolves it in a runtime load event.
  - `AudioCategory`/`AudioState` are loaded but not streamed.
  - `ItemReticleConfig.ServerEvents` indexes are deterministic but Rust-owned.
- **Follow-ups:**
  - T39 wires all new packets into the final config order, caches encoded
    `ConfigAssets`, and adds `--validate-assets`.

---

## Session 27 — Full config burst, cache and asset validation (2026-10-24)

- **Goal:** finish M2 by wiring the child packets into the join, encoding the
  config burst once, and validating the live asset registry.
- **Landed:**
  - `AssetRegistry::config_assets()` now emits 17 packets in dependency order:
    hitboxes, physical materials, sound events, block sound sets, FluidFX,
    Fluid, Environment, TagPattern, BlockType, block particles/decals, item
    sounds/qualities/reticles/categories/animations, then Item.
  - `WorldThread` encodes the burst once at world start and hands sessions an
    `Arc<ConfigAssets>`; the encode no longer runs per connection.
  - `hytale-server --validate-assets [--pack PATH]` loads every enabled store,
    reports counts/id ranges/packet sizes/tag counts, checks packet order,
    duplicate ids, decodability, determinism and that every asset/pattern tag
    index resolves in `ServerTags`, and exits non-zero on drift.
  - `client-bot` recognises all 17 config packet ids; the authenticated join
    asserts their exact dependency order.
  - Semantic tests now compare `fluidFXIndex`, `blockSoundSetIndex`,
    `fluidParticles` and the previously divergent item fields directly, removing
    the child-index exceptions.
  - Added `crates/hytale-server/tests/asset_validation.rs` and a world-thread
    cache test (`Arc::ptr_eq` across two config requests).
- **Verified:**
  - `cargo run -p hytale-server -- --validate-assets` → 21 stores, 17 packets,
    579 tags, 24,397 checked references, `validation: ok`.
  - `cargo test -p client-bot --test auth_join -- --ignored`.
  - `cargo test --workspace`.
  - `cargo clippy --workspace --all-targets -- -D warnings`.
  - `cargo fmt --all -- --check`.
- **Deviations:**
  - Validation checks the live registry/encoding rather than committed
    `index.json`, so it also works with mod packs; fixture comparison remains in
    the Rust tests.
  - The cache is per world thread until T41 gives the universe ownership of
    multiple worlds.
  - `AudioCategory`/`AudioState` remain dependency-only stores (no packets).
- **Follow-ups:**
  - T40 (`hytale-core`), T41 (universe/world lifecycle), T42 (persistent
    multi-session listener) start the M3 runtime track.


