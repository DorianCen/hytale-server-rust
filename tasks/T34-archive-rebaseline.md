# T34 — Archive the completed vertical slice and re-baseline docs

**Status:** done (session 24)
**Package:** docs, `done/`, `tasks/`
**Depends on:** T33
**Agent fit:** high

## Goal

Close out the first vertical slice (sessions 1–23, T01–T33) and leave phase-2
docs and a task board that describe only the remaining work.

## Deliverables

- `done/coreserver-vertical-slice/` with the phase-1 snapshots:
  `plan.md`, `NEXT_STEPS.md`, `ARCHITECTURE.md`, `PORTING.md`,
  `PORTING_HISTORY.md`, `REMAINING.md`, `GOLDEN_DATA.md`, `BENCHMARKS.md`,
  `README.md`, and `tasks/` (T01–T33).
- Root phase-2 docs: `plan.md`, `NEXT_STEPS.md`, `ARCHITECTURE.md`,
  `PORTING.md`, `PORTING_HISTORY.md`, `REMAINING.md`, `GOLDEN_DATA.md`,
  `BENCHMARKS.md`, `tasks/README.md`.
- `done/world-generation/README.md` updated to point at both archives.
- Task files for the next milestone(s), starting with T35.

## Acceptance

- Markdown links in `done/` and the root docs resolve.
- `git status` shows only docs/task moves and additions.
- `cargo test --workspace`, clippy and fmt remain green (no code change).
