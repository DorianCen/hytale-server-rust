# T63 — Performance, memory and soak

**Status:** done (session 68)
**Package:** `hytale-server`, `tools`, docs
**Depends on:** T48, T62
**Agent fit:** medium

## Goal

Publish an honest Rust CoreServer performance/memory baseline for boot, asset
decode, config-cache reuse, chunk streaming, world tick and save, and prove
that repeated joins/edits do not leak within a defined soak budget.

## Delivered

### Performance probe

`crates/hytale-server/examples/perf.rs` measures, in release mode:

- asset pack open, P0 decode, phase-2 decode and config-packet encode;
- world spawn and initial chunk/view plan;
- cold and warm config-asset cache (`Arc::ptr_eq` shared cache);
- tick + simulation throughput over a configurable batch;
- one out-of-view chunk load;
- join chunk-batch packet encode and zstd framing;
- block edit and save;
- resident/peak RSS at each stage.

It emits tab-separated `metric<TAB>value<TAB>unit` rows so results can be
archived without another benchmark dependency. `HYTALE_PERF_TICKS` and
`HYTALE_PERF_VIEW_RADIUS` select the workload.

`tools/perf.sh` records hardware/build metadata, runs the probe and then the
release soak, writing everything under `target/perf/<timestamp>.*`.

### Memory snapshots

`hytale-server::memory::{resident_bytes, peak_resident_bytes}` reads `VmRSS` and
`VmHWM` from `/proc/self/status` on Linux and returns `None` elsewhere. A unit
test verifies the supported-platform path and the portable fallback.

### Soak regression

`crates/hytale-server/tests/soak.rs` keeps one world thread alive and runs
`HYTALE_SOAK_CYCLES` cycles of:

1. register a unique player in a bounded registry queue;
2. place a block in the radius-1 view;
3. tick + simulate and drain events/broadcasts;
4. unregister the player;
5. save every 10 cycles.

It asserts that the registry is empty at the end, the radius-1 view holds at
most 9 chunks, edits are still readable, and resident memory growth stays under
`HYTALE_SOAK_RSS_LIMIT_MIB` (default 64 MiB).

### Stale config-cache regression

The asset-backed `config_assets_are_cached_per_world` test still asserted the
pre-T65 21-packet burst, so it only passed when the fast tier skipped assets.
T63 updates it to the current 47-packet release-client burst and its exact ids,
and the test now runs with assets.

### Published baseline

`BENCHMARKS.md` records the T63 hardware, build, commands, probe table, memory
figures and the 1,000-cycle soak result.

## Acceptance

```sh
tools/perf.sh
HYTALE_SOAK_CYCLES=1000 cargo test --release -p hytale-server --test soak -- --ignored --nocapture
HYTALE_TEST_ASSETS=1 cargo test -p hytale-server --test world_thread config_assets_are_cached_per_world
cargo test -p hytale-server --lib memory
./tools/test.sh fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Measured T63 numbers (AMD Ryzen 7 9800X3D, 64 GiB RAM, Linux 7.3.1):

- P0 decode 1.28 s; phase-2 decode 2.57 s; config encode 68.5 ms.
- World spawn 4.09 s; config-cache hit 0.009 ms.
- 200 tick+simulate passes at 104.8 ticks/s; one out-of-view chunk load 210 ms.
- Join chunk batch: 0.347 ms encode / 0.614 ms zstd framing.
- Save 10 chunks: 39.7 ms.
- 1,000 join/edit/save cycles: +417,792 B RSS, under the 64 MiB ceiling.

## Deviations

- A Java CoreServer baseline is not available in this workspace; the probe and
  report are ready for a side-by-side run with the same pack/seed, but the
  Java/Rust ratio remains pending.
- The probe is an example binary rather than criterion to avoid a multi-second
  asset boot inside the default test suite. Existing worldgen criterion benches
  are unchanged.
- Tick throughput includes one `simulate(0.05)` pass and is a batch guardrail,
  not a wall-clock TPS measurement.
- Per-player memory at high concurrency is not isolated; the soak exercises one
  sequential session at a time. Multi-player memory is a follow-up for a
  dedicated load harness (T63/T64 boundary).

## Follow-ups

- Run the same probe against the Java server on this host and fill the
  comparison column in `BENCHMARKS.md`.
- Add a concurrent 10/50/100-player load generator if multiplayer deployment
  sizing becomes a requirement.
- Move the chunk-per-region/storage microbenchmarks to criterion if they become
  hot-path work rather than infrastructure.
