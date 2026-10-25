# Testing tiers

`cargo test --workspace` is the correctness gate, not the inner loop. Most of
its wall time is asset-backed integration tests that load `Assets/Assets.zip`
and boot a `WorldThread`/world generator. Use the tiers below while developing.

## Commands

```sh
./tools/test.sh unit          # in-crate unit tests, no asset pack
./tools/test.sh fast          # unit + integration, asset-backed tests skipped
./tools/test.sh integration   # all non-E2E integration tests
./tools/test.sh assets        # fast asset/golden packages
./tools/test.sh slow          # worldgen/world-boot/server generation suites
./tools/test.sh e2e           # ignored loopback/auth/trace tests, serial
./tools/test.sh all           # full workspace gate
```

Extra arguments are forwarded to cargo:

```sh
./tools/test.sh unit -- --nocapture
./tools/test.sh e2e -- --nocapture
```

## What each tier means

| Tier | Contents | Assets / network |
|---|---|---|
| `unit` | `#[cfg(test)]` modules in every crate (`cargo test --workspace --lib`) | No real asset pack |
| `fast` | unit tests plus non-E2E integration tests, with asset-backed tests skipped | No |
| `integration` | every non-`#[ignore]` integration test | Yes, when available |
| `assets` | fast asset/golden packages: `hytale-assets`, `hytale-entities`, `hytale-interactions`, `hytale-world` | Yes |
| `slow` | worldgen generation and world-boot suites: `worldgen-hytale`, `worldgen-cli`, `worldgen-server`, `hytale-server` | Yes |
| `e2e` | the `#[ignore]`d QUIC loopback, auth join, bot and trace-replay tests, one test thread | Yes + loopback UDP |
| `all` | the current release gate (`cargo test --workspace`) | Yes |

`integration` is the combined non-E2E gate (roughly `assets + slow`); use the
split tiers when you only need one side. `HYTALE_TEST_ASSETS=0` makes
asset-backed tests return early instead of loading the pack. `fast` forces
that; the other tiers default it to `1`, but keep an explicit value if you set
one:

```sh
HYTALE_TEST_ASSETS=0 ./tools/test.sh integration
```

Tests that need an unavailable asset pack also skip (the existing
`assets_available()` convention), so CI without the pack remains green. These
skips are early returns, so libtest still counts them as passed; run with
`-- --nocapture` to see the skip messages.

### Shared asset context

`tools/test.sh` sets `HYTALE_SHARED_ASSETS=1`. The first `WorldGenerator` in a
test process decodes the phase-2 registry, interaction catalog and config
burst once; later worlds in the same test binary reuse them. This is the
largest win for suites that boot several `WorldThread`s.

```sh
HYTALE_SHARED_ASSETS=0 cargo test -p hytale-server --test world_thread  # per-world path
HYTALE_SHARED_ASSETS=1 cargo test -p hytale-server --test world_thread  # shared path
```

Measured locally: `cargo test -p hytale-server` dropped from ~457 s to ~349 s
with sharing enabled. The setting is opt-in for normal runs, so production
behavior is unchanged.

## Measured baseline (local, debug profile)

These are whole-package runs from this workspace, not a full `cargo test
--workspace` sample:

| Command | Wall time |
|---|---|
| `./tools/test.sh unit` | ~1 s |
| `./tools/test.sh fast` (plain cargo) | ~16–21 s |
| `./tools/test.sh fast` (nextest) | ~8 s test time |
| `cargo test -p client-bot` (E2E ignored) | ~3 s |
| `cargo test -p hytale-assets` | ~97 s |
| `cargo test -p hytale-world` | ~78 s |
| `cargo test -p worldgen-cli` | ~106 s |
| `cargo test -p worldgen-hytale` | ~275 s |
| `cargo test -p hytale-server` | ~457 s |
| `HYTALE_SHARED_ASSETS=1 cargo test -p hytale-server` | ~349 s |
| `./tools/test.sh assets` (nextest, fast asset/golden) | ~4.5 min |
| `./tools/test.sh slow` (nextest, worldgen/world-boot) | ~12 min |

The full `cargo test --workspace` is the sum of the remaining asset-backed
suites plus build time, which is why it is unsuitable as an inner loop. The
asset-backed side is now split: `assets` is the quick packet/golden validation
(~4.5 min), and `slow` is the worldgen generation/world-boot gate (~12 min).
The latter still contains genuinely expensive generation tests; it is a
pre-merge/release check, not the daily edit/build loop.

The dominant costs are repeated full asset-registry loads/repeated
`WorldThread`/world-generator boots, and the genuinely expensive worldgen
generation tests. `fast` avoids the former and skips the latter.

## E2E tests

E2E tests are already `#[ignore]`d. They remain compiled by `cargo test`, but
only run with `--ignored`:

```sh
./tools/test.sh e2e
# equivalent to:
cargo test --workspace -- --ignored --test-threads=1
```

The serial thread count is intentional: the loopback tests bind UDP ports and
several boot full worlds.

## cargo-nextest (optional, auto-detected)

`cargo-nextest` runs test *binaries* in parallel (plain `cargo test` runs them
sequentially) and can enforce per-test timeouts. `tools/test.sh` auto-detects
it; plain cargo is the fallback.

```sh
cargo install cargo-nextest --locked
./tools/test.sh fast          # uses nextest automatically
HYTALE_TEST_RUNNER=cargo ./tools/test.sh fast   # force plain cargo
```

The `unit`, `fast`, `integration`, `assets` and `e2e` tiers map to the profiles
in [`.config/nextest.toml`](.config/nextest.toml). `all` deliberately stays on
`cargo test` because nextest does not run doc tests.

## CI

CI runs the quick tier (`./tools/test.sh fast`). Without the asset pack most
asset-backed tests would skip anyway; `fast` makes that explicit and keeps the
job fast even if a runner ever has the pack. A future split can add:

1. asset/golden job on a runner with the pack (`./tools/test.sh assets`);
2. serial E2E job for trusted branches/nightly (`./tools/test.sh e2e`).
