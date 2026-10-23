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
