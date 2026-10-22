# Next steps — open work only (post-port backlog)

**Status:** the port is feature complete and bit-exact against the golden data
(`tests/chunks_golden.rs`: 21 chunks / 21,504 columns, every block / fluid /
environment / priority byte). The golden chunk dump now uses the vanilla
provider's `SeedString("ChunkGenerator")` label, and a **live server boot** was
used to verify the registry-snapshot integration end-to-end.

**There is no open post-port work.** The deferred scope is documented in
[`REMAINING.md`](REMAINING.md) and should not be implemented unless the scope
changes.

**Context for the next session:**
- Completed work, root causes and per-phase notes: [`PORTING_HISTORY.md`](PORTING_HISTORY.md)
  (archived; **do not read unless debugging or verifying a regression**).
- Architecture / Java→Rust mapping: [`ARCHITECTURE.md`](ARCHITECTURE.md).
- Deliberately deferred scope: [`REMAINING.md`](REMAINING.md).
- Golden-data formats / regeneration: [`testdata/golden/README.md`](../testdata/golden/README.md).
- Live server registry snapshot: [`LIVE_SNAPSHOT.md`](LIVE_SNAPSHOT.md).
- Release-mode performance baseline: [`BENCHMARKS.md`](BENCHMARKS.md).

## Regression gate (must stay green)

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
# Optional, needs the 3.7 GB asset pack (Assets/Assets.zip + Assets/extracted):
tools/java-dumper/run.sh          # regenerate testdata/golden/
tools/java-dumper/check.sh        # two isolated runs + diff against the committed dump
```

The Java dumper's chunk dump uses `SeedString("ChunkGenerator")` (the vanilla
`HytaleWorldGenProvider` label). `DUMPER_LOADER_SEED=<label>` overrides it for
comparisons with older conventions.

## Deferred by design — do not implement unless scope changes

- Modifier/event plugin system, prefab entities + block components/ECS
  serialization, CoreServer plugin lifecycle, world-map UI, `.lpf` binary
  format. All documented in [`REMAINING.md`](REMAINING.md); these are
  intentional exclusions, not gaps.
