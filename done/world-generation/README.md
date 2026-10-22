# Archived worldgen port documentation

These files document the completed WorldGenerator → Rust port. They are kept
for reference and for the existing crates' design history, not as the current
plan.

The current CoreServer rewrite starts at [`../plan.md`](../plan.md) and
[`../NEXT_STEPS.md`](../NEXT_STEPS.md).

| File | Contents |
|---|---|
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Java class → Rust module mapping for worldgen |
| [`PORTING.md`](PORTING.md) | worldgen porting conventions |
| [`PORTING_HISTORY.md`](PORTING_HISTORY.md) | completed worldgen phases, root causes, regressions |
| [`REMAINING.md`](REMAINING.md) | worldgen deferred scope |
| [`BENCHMARKS.md`](BENCHMARKS.md) | release-mode worldgen baseline |
| [`LIVE_SNAPSHOT.md`](LIVE_SNAPSHOT.md) | live server registry snapshot workflow |
| [`NEXT_STEPS.md`](NEXT_STEPS.md) | final post-port backlog (closed) |
| [`plan.md`](plan.md) | original worldgen phase plan |

The worldgen regression gate still applies:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```
