# Next steps — open work only (post-port backlog)

**Status:** the port is feature complete and bit-exact against the current
golden data (`tests/chunks_golden.rs`: 21 chunks / 21,504 columns, every
block / fluid / environment / priority byte). The golden-regeneration gate
(`tools/java-dumper/check.sh`), the procedural seams and the server adapter
(`crates/worldgen-server`) have landed; nothing below is required for the
parity claim — the list is ordered by how much it strengthens or extends it.

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

## 1. Verify the registry ids against a real server (highest value, unfinished)

- **Why:** the block / fluid / environment ids in
  `testdata/golden/registries.json` are pinned to the dumper's single-threaded
  asset-load order. Chunks are bit-exact *through that id mapping*; a different
  real-server load order would shift the numeric ids (names/structure would stay
  correct).
- **How:** boot a vanilla server with the same `Assets/Assets.zip`, dump
  `BlockType` / `Fluid` / `Environment` `getAssetMap()` ids (reuse the
  `RegistryDumper` logic), and diff against the committed dump. If the order
  differs, regenerate from the real server and re-run `tests/chunks_golden.rs`.
- **Done when:** the id-order caveat can be removed from `REMAINING.md` §6 and
  the `registries.json` `meta` block.

## 2. Visual-regression artefacts (optional)

- `worldgen-cli render-map` already renders height / zone / biome PNGs. Add a
  small golden-image test (fixed seed + region, pixel diff) for mask / biome /
  cave-slice views. This is a usability nicety — numeric parity is already
  proven bit-exactly.

## 3. Performance baseline (optional)

- Record release-mode numbers (`worldgen-cli bench`, `cargo bench` for
  `prefab_weights` and `cache`) on the reference machine; tune the worker count
  and note the per-worker world-load cost. Always benchmark `--release` —
  debug builds are ~20× slower on prefab-heavy chunks.

## 4. Deferred by design — do not implement unless scope changes

- Modifier/event plugin system, prefab entities + block components/ECS
  serialization, CoreServer plugin lifecycle, world-map UI, `.lpf` binary
  format. All documented in [`REMAINING.md`](REMAINING.md); these are
  intentional exclusions, not gaps.
