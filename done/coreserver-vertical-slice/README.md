# Archived CoreServer vertical-slice documentation

These files document the completed first CoreServer vertical slice: protocol
generation and transport, the P0 asset pipeline, runtime chunk/world storage,
the QUIC chunk stream, the authenticated join, the `Game` transition, player
spawn, the teleport/movement loop, and the six self-contained config asset
packets (sessions 1–23, tasks T01–T33).

They are kept for reference and design history, not as the current plan. The
phase-2 plan starts at [`../../plan.md`](../../plan.md) and
[`../../NEXT_STEPS.md`](../../NEXT_STEPS.md); the new task board is
[`../../tasks/README.md`](../../tasks/README.md).

| File | Contents |
|---|---|
| [`plan.md`](plan.md) | Original CoreServer rewrite plan and milestones |
| [`NEXT_STEPS.md`](NEXT_STEPS.md) | Status at the end of session 23 |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Architecture and Java→Rust mapping for the slice |
| [`PORTING.md`](PORTING.md) | Porting conventions (snapshot; the root copy is living) |
| [`PORTING_HISTORY.md`](PORTING_HISTORY.md) | Session 1–23 history |
| [`REMAINING.md`](REMAINING.md) | Deferred scope recorded during the slice |
| [`GOLDEN_DATA.md`](GOLDEN_DATA.md) | Java oracle and fixture layout at session 23 |
| [`BENCHMARKS.md`](BENCHMARKS.md) | Benchmark plan at session 23 |
| [`tasks/`](tasks/) | Completed tasks T01–T33 |

Nothing in this directory is generated; do not regenerate or “fix up” the
history. Code, tools, fixtures and `testdata/` remain at the repository root.

The worldgen archive is next door:

- [`../world-generation/README.md`](../world-generation/README.md)
