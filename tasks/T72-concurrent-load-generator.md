# T72 — Concurrent player load generator

**Status:** proposed (filed session 99)
**Package:** `tools/client-bot`, `hytale-server`, docs
**Depends on:** T63, T70
**Agent fit:** medium

## Goal

Turn the existing two-client smoke into a repeatable 10/50/100-player load
harness and publish a baseline for join latency, tick/chunk throughput, queue
pressure, and server RSS. T63 covers one sequential soak session; this packet
covers concurrency against one listener.

## Deliverables

- Add a load mode to `client-bot` (CLI flags or a dedicated `load` binary):
  - `--players N` (10/50/100 presets plus arbitrary values);
  - deterministic per-bot UUID/username/start transform;
  - staggered joins, then a scripted move/chat/place/break/save window;
  - graceful disconnect and a final `/who`/player-list check.
- Collect per-run metrics:
  - join latency p50/p95/max;
  - accepted/rejected sessions and `server full`/duplicate counts;
  - chunk packets/frames/bytes, view changes and unloads;
  - tick rate or tick-duration samples where available;
  - resident/peak RSS from `hytale-server::memory`;
  - broadcast queue overflow/rate-limit/idle-timeout counts.
- Write a machine-readable report (JSON or TSV) under `target/perf/` and a
  short summary to stdout.
- Add an ignored loopback gate:
  - `cargo test -p client-bot --test load -- --ignored` with a small
    `HYTALE_LOAD_PLAYERS` value (default 10) so CI can exercise the harness
    cheaply;
  - document the manual 50/100 runs.
- Record the baseline and limits in `BENCHMARKS.md`, including hardware and the
  exact command.

## Read first

- `tools/client-bot/src/lib.rs` (`dev_join`, `observe_players`,
  `receive_chunk_stream`)
- `tools/client-bot/tests/multi_join.rs`
- `tools/client-bot/tests/multiplayer_persistence.rs`
  (`run_clients`/shared-registry helper)
- `crates/hytale-server/src/serve.rs` (listener, semaphore, `peak_connections`)
- `crates/hytale-server/src/players.rs` (queue capacity, overflow)
- `crates/hytale-server/src/memory.rs`
- `crates/hytale-server/tests/soak.rs`, `BENCHMARKS.md`

## Acceptance

```sh
cargo test -p client-bot --test load -- --ignored
HYTALE_LOAD_PLAYERS=50 cargo test -p client-bot --test load -- --ignored --nocapture
./tools/test.sh fast
cargo clippy -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

- 10, 50 and 100 bots join one listener without deadlock or queue-overflow
  false positives.
- The report contains join latency, connection counts and RSS.
- The default test remains fast when ignored; no new flaky default-suite test.

## Notes

- Do not commit 50/100-player timing thresholds that depend on developer
  hardware; record them as documentation and assert only correctness/absence of
  overflow in the cheap gate.
