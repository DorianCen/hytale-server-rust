# T40 — `hytale-core` runtime

**Status:** planned (session 25+)
**Package:** `hytale-core`
**Depends on:** T34
**Agent fit:** medium

## Goal

Create the shared runtime services every later phase-2 task needs, without
changing existing server behaviour.

## Deliverables

- Typed configuration loading:
  - defaults in code;
  - optional server config file;
  - environment/CLI overrides;
  - unknown-field policy documented.
- Structured logging/tracing initialization.
- Async task scheduler and shutdown token:
  - spawn/named tasks;
  - graceful shutdown with a deadline;
  - no detached task leaks.
- Typed event bus:
  - per-subscriber ordering;
  - synchronous and async subscriber seams;
  - unsubscribe/drop behaviour.
- Module manifest/registry seam for feature-gated builtin modules.
- Unit tests for config precedence, event ordering, shutdown and module
  registration.

## Read first

- `plan.md` M3 and `ARCHITECTURE.md` runtime sections.
- Existing `hytale-server::session`/`WorldThread` task and shutdown patterns.
- Java `CoreServer` config/logging/event entry points for observable behaviour.

## Acceptance

```sh
cargo test -p hytale-core
cargo clippy -p hytale-core --all-targets -- -D warnings
cargo fmt -p hytale-core -- --check
cargo test --workspace
```

- Existing binaries boot with the same defaults.
- Config precedence and event ordering are tested.
- A shutdown token stops all registered tasks within the deadline.

## Deviations

- Full Java module discovery/classloading is out of scope; this is the Rust
  manifest/registry seam.
