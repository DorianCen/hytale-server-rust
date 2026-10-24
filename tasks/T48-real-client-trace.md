# T48 — Real-client trace and v1 compatibility gate

**Status:** done (session 36)
**Package:** `hytale-net`, `hytale-server`, `client-bot`, `testdata/golden/server`, docs
**Depends on:** T47
**Agent fit:** medium

## Goal

Prove the M4 lifecycle with a committed, replayable packet trace instead of only
asserting bot-side state, and record what a real client requires. The trace
stores packet ids/channels/order/payload hashes only — never proprietary payload
bytes.

## Deliverables

- `hytale-net::trace`:
  - JSONL trace schema (`TraceHeader`/`TraceRecord`), SHA-256 payload hashes and
    a `PacketTraceSink` hook on both framed streams;
  - `TraceRecorder`/`TraceSession` for per-session capture and `read_trace`/
    `write_trace` helpers.
- `hytale-server serve --trace <path>`:
  - records inbound/outbound setup packets and outbound chunk-stream packets;
  - assigns a trace session id per accepted connection and flushes on shutdown.
- `client-bot`:
  - `send_ready_for_gameplay` option so the dev join exercises the teleport flow;
  - lifecycle trace gate: join → move → edit → disconnect → reconnect with the
    same UUID, while a second client observes;
  - fast registry/fixture validation that runs without the asset pack.
- `testdata/golden/server/trace/`:
  - committed normalized `v1-lifecycle.jsonl` fixture;
  - README with format, provenance, regeneration command and the real-client
    capture runbook.
- Docs: `REMAINING.md` records the real-client packet/channel constraints found
  in the captured client log; `PORTING_HISTORY.md` records the gate.

## Read first

- `crates/hytale-net/src/frame_stream.rs`, `chunk_stream.rs`, `trace.rs`
- `crates/hytale-server/src/serve.rs`, `session.rs`, `main.rs`
- `tools/client-bot/src/lib.rs`, `tests/multi_join.rs`
- `testdata/golden/server/README.md`
- Real client log observation (not redistributed): setup stage ordering and the
  `ViewRadius (ID 32)` channel error during loading.

## Acceptance

```sh
cargo test -p hytale-net
cargo test -p client-bot --test trace_fixture
cargo test -p client-bot --test trace_gate -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The committed fixture validates against the generated packet registry in the
  default test path.
- The live gate captures a fresh trace and matches it as an ordered per-session
  subsequence, including stable payload hashes.
- `ViewRadius` is asserted on the `Chunks` stream (real-client constraint).
- The trace file contains only ids/names/channels/directions/lengths/hashes.

## Deviations

- The committed fixture is a normalized lifecycle trace produced by the
  `client-bot` loopback, because a vanilla client capture cannot be committed or
  automated in this environment. The server-side `--trace` capture path and the
  runbook are the real-client seam; replacing the baseline is a one-command
  refresh once a client run is available.
- The live gate is `#[ignore]`d like the existing world/loopback tests because it
  needs the base-game pack and UDP; the fast fixture test stays in the default
  workspace suite.
- The local EdDSA auth profile is not trusted by a default online vanilla client
  (it expects the live session authority). The runbook covers the dev/insecure
  profile and points at the live session adapter as the remaining auth scope,
  not a protocol gap.
- Dynamic payloads (`UpdateTime`, entity transforms, player-list entries) keep
  their order/id/channel but deliberately drop hashes so the gate is not
  time/identity dependent.

## Follow-ups

- Replace/augment `v1-lifecycle.jsonl` with a captured vanilla-client run when a
  client automation environment is available.
- T49 adds inventory state to the traced lifecycle.
- T62 hardens trace/rate-limit handling; T63 soak-tests repeated traces.
