# Next steps — open work only (post-port backlog)

**Status:** the port is feature complete and bit-exact against the current
golden data (`tests/chunks_golden.rs`: 21 chunks / 21,504 columns, every
block / fluid / environment / priority byte). The golden-regeneration gate
(`tools/java-dumper/check.sh`), the procedural seams, the server adapter
(`crates/worldgen-server`), the render goldens and the release-mode
benchmark baseline (`BENCHMARKS.md`) have landed; nothing below is required
for the parity claim.

**Context for the next session:**
- Completed work, root causes and per-phase notes: [`PORTING_HISTORY.md`](PORTING_HISTORY.md)
  (archived; **do not read unless debugging or verifying a regression**).
- Architecture / Java→Rust mapping: [`ARCHITECTURE.md`](ARCHITECTURE.md).
- Deliberately deferred scope: [`REMAINING.md`](REMAINING.md).
- Golden-data formats / regeneration: [`testdata/golden/README.md`](testdata/golden/README.md).

## Regression gate (must stay green)

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
# Optional, needs the 3.7 GB asset pack (Assets/Assets.zip + Assets/extracted):
tools/java-dumper/run.sh          # regenerate testdata/golden/
tools/java-dumper/check.sh        # two isolated runs + diff against the committed dump
```

---

## 1. Feed a live server registry snapshot (highest value, unfinished)

- **Finding (2026-10-21):** the registry ids are *not* stable in the vanilla
  pipeline. With `DUMPER_PARALLELISM=16`, two dumps of the same pack differed
  in a few swapped sibling block ids (e.g. 2591 ↔ 2592) and `Lava`/`Tar`
  (13 ↔ 14); the chunk/loader dumps differed only through those ids. The
  canonical single-threaded dump (`registries.json`) remains the reference
  for the parity tests, but no "real-server order" can be reproduced.
- **Why it matters:** the Java generator resolves names through the live
  `BlockType` / `Fluid` / `Environment` maps at generation time. A server
  must therefore generate chunks against *its own* snapshot, not the dump.
- **How:** at server boot, write the live maps in the `registries.json`
  schema (the dumper's `RegistryDumper` writer is the reference) and pass the
  file through `WorldGenOptions::registries_path` (CLI `--registries`); the
  loader already threads it into `ChunkGenerator::get_registries()`. A
  real-server dump also lets us diff the expected sibling swaps and confirm
  that names/structure are identical.
- **Done when:** the `worldgen-server` adapter documentation describes the
  snapshot handoff and a real server boot has been generated against its own
  maps (chunk ids consistent with that boot). No attempt should be made to
  find one universal id order — the `meta` block documents why.

## 2. Deferred by design — do not implement unless scope changes

- Modifier/event plugin system, prefab entities + block components/ECS
  serialization, CoreServer plugin lifecycle, world-map UI, `.lpf` binary
  format. All documented in [`REMAINING.md`](REMAINING.md); these are
  intentional exclusions, not gaps.
