# T52d — Block health storage and fragility persistence

**Status:** done (session 41)
**Package:** `hytale-world`, `hytale-server`, `tools/java-dumper`
**Depends on:** T52 (T57/T57b consume the presentation seam)
**Agent fit:** medium

## Goal

Make T52's block-health/fragility state survive save/reload and remain
Java-readable by storing the real `BlockHealthSection` component instead of the
transient `World` map.

## Deliverables

- `hytale-world::chunk`:
  - add a per-section `BlockHealthSection` component alongside block/fluid/
    environment sections:
    - `Health`: `short block index -> {Health: f32, LastDamage: epoch millis}`;
    - `Fragility`: `short block index -> remaining seconds`;
    - transient pending deltas for T57/T57b replication.
  - implement Java `BlockHealthSection` version 0 BSON layout using Java's
    `Data` byte-array + `Short2ObjectMap`/`Short2FloatMap` codecs. Keep the
    component absent/empty when no damage exists so ordinary chunks are
    byte-identical.
  - read the legacy `BlockHealthChunk` version 2 component when present and
    migrate absolute block keys into section indices (T52 has not written it;
    old Java saves may contain it).
  - serialize/deserialize through the existing `ChunkCodec` so region files
    still pass the Java read-back gate.
- `World`:
  - move `damage_block`, `block_health`, `tick_block_health` and fragility
    countdown from the transient `BTreeMap` to the owning section;
  - clear health/fragility on block replace, respecting filler origin rules;
  - keep T52's 5 s / 0.1-s regeneration and Java's `BlockHealthSection`
    semantics.
- `BlockSimulation`:
  - emit pending health deltas separately from block changes;
  - expose them through `SimulationReport` for T57/T57b's
    `UpdateBlockDamage(s)` packets.
- `tools/java-dumper`:
  - add a `BlockHealthDumper`/probe that writes a Java chunk with damaged and
    fragile blocks and reads a Rust-written chunk back through the real
    `BlockHealthSection` codec.
- Tests:
  - Java fixture read: every damaged/fragile entry matches;
  - Rust round-trip and save/reload;
  - Rust write → Java `run-save-check` probe;
  - T52 damage/regeneration tests move to the section-backed implementation.

## Read first

- Java `modules/blockhealth/BlockHealth`, `BlockHealthSection`,
  `BlockHealthModule`, `BlockHealthSystems`, `BlockHealthChunk`
- `CoreServer/.../universe/world/chunk/section/BlockSection` (section component
  list and codec wiring)
- `crates/hytale-world/src/chunk/{column,storage}.rs`,
  `crates/hytale-world/src/chunk/section/mod.rs`,
  `crates/hytale-world/src/world.rs`
- `tools/java-dumper/run-save-check.sh`, `run-storage.sh`,
  `WorldStorageProbe`/`ChunkDumper` patterns
- `testdata/golden/server/storage/README.md` and the Java player/world fixtures

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
tools/java-dumper/run-save-check.sh <rust-save> <chunkX> <chunkZ> <blockX> <blockY> <blockZ> <expected-block>
```

- The Java-written health/fragility fixture is read exactly.
- Rust writes damaged/fragile state, reloads it and the Java probe reads the
  same health/last-damage/fragility values through `BlockHealthSection`.
- Ordinary undamaged chunks keep the existing Java byte layout (no extra
  component when empty).
- T52 simulation/regen and all storage gates remain green.

## Deviations to report or split

- `UpdateBlockDamage`/`UpdateBlockDamages` packet replication, fragility
  presentation and client block-health UI remain T57b; this task exposes pending
  deltas but does not send them.
- `BlockHealthChunk` writes/migration can be read-only if Java no longer writes
  it; document the decision.
- Any storage version bump must be justified against the Java codec and covered
  by the read-back probe.

## Result (session 41)

Implemented and verified against the real pack and the Java oracle. See
[`../PORTING_HISTORY.md`](../PORTING_HISTORY.md) session 41 for the changed
files and the remaining narrower deviations (sound playback, builtin module
lifecycle and client presentation stay with their later tasks).

**Remaining deviation:** the committed Java fixture is the real
`BlockHealthSection.CODEC` document, and Rust save/reload is covered, but the
Java whole-save probe (`run-save-check.sh`) does not yet read health/fragility
back from a Rust-written region. Extend `WorldStorageProbe` with the health
component before using this as a full reverse storage gate.
