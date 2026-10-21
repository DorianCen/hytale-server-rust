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
- Live server registry snapshot: [`LIVE_SNAPSHOT.md`](LIVE_SNAPSHOT.md).

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
- **Exporter (landed):** `RegistrySnapshot.write(Path)` in
  `tools/java-dumper` dumps the currently loaded CoreServer maps in the
  `registries.json` schema; a standalone mode (which boots the stubs first)
  and `worldgen-cli registries --path ... --compare ...` were verified
  end-to-end. Full guide: [`LIVE_SNAPSHOT.md`](LIVE_SNAPSHOT.md).
- **Remaining:** call `RegistrySnapshot.write` from a real server boot (after
  `AssetRegistryLoader.loadAssets`, with the server's mods/overrides), then
  run `worldgen-cli registries --compare` to confirm that names/metadata are
  identical and only the expected sibling ids swap. Pass the file via
  `WorldGenOptions::registries_path` / CLI `--registries`.
- **Done when:** a real server boot has been generated against its own maps
  (chunk ids consistent with that boot). No attempt should be made to find
  one universal id order — the `meta` block documents why.

## 2. Deferred by design — do not implement unless scope changes

- Modifier/event plugin system, prefab entities + block components/ECS
  serialization, CoreServer plugin lifecycle, world-map UI, `.lpf` binary
  format. All documented in [`REMAINING.md`](REMAINING.md); these are
  intentional exclusions, not gaps.
