# T72 — Concurrent player load generator

**Status:** done (session 101)
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

## Landed (session 101)

- `client_bot::load` drives N deterministic bots against one listener with a
  configurable stagger, join timeout and workload window. Each bot joins,
  moves, chats, runs `/who` (bot 0 also `/save`), places and breaks a block,
  disconnects gracefully and contributes to a JSON report.
- The report records join p50/p95/max, joined/failed, accepted/rejected/server
  full, duplicate identities, client-teardown vs unexpected session errors,
  chunk packets/frames/bytes, chunks/unloads/view changes, block edits/rejects,
  chat/commands, queue overflow/rate-limit/idle-timeout counts, simulation
  ticks and effective tick rate, and resident/peak RSS.
- `client-bot --load --players N` starts an embedded loopback listener (release
  baseline commands in `BENCHMARKS.md`); `--external --host` targets an existing
  listener. Reports default to `target/perf/load-<players>.json`.
- New ignored gate `tools/client-bot/tests/load.rs` runs 10 bots by default
  (`HYTALE_LOAD_PLAYERS` overrides) and asserts full joins, no rejected/queued
  overflow, chunk counters, edits, ticks, RSS and a decodable report.
- The listener now completes QUIC handshakes on a dedicated accept task. The
  previous `select!` loop dropped concurrent handshakes whenever the 100 ms
  shutdown tick won, which the 10-bot gate exposed as half the joins failing.
- `SessionSummary` exposes chunk packets/frames/bytes, view changes and block
  edit/use counters for the load report.

## Verification (session 101)

```sh
cargo test -p client-bot --lib                                  # 6 passed
cargo test -p client-bot --test load -- --ignored               # 1 passed
cargo test -p client-bot --test block_interactions -- --ignored # 2 passed
./tools/test.sh fast                                            # see below
cargo clippy -p hytale-server -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

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
