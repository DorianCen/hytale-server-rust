# T40 — `hytale-core` runtime

**Status:** done (session 28)
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
- The config file is JSON. Unknown JSON fields are rejected; unknown
  `HYTALE_CORE_*` environment keys are rejected; other environment variables are
  ignored.
- Logging initialization installs the global tracing subscriber but is not wired
  into the existing binaries in this task, so their output is unchanged.
- The task scheduler owns tasks in a `JoinSet`; dropping it cancels the token and
  aborts the tasks. Shutdown waits for cooperative cancellation up to the
  deadline, then force-aborts.
- The event bus serializes delivery through one mutex. Sync callbacks must not
  publish re-entrantly; async subscribers use bounded channels and report
  dropped events instead of blocking.

## Follow-ups

- T41/T42 compose these services into the universe/listener runtime and switch
  the binaries to `ServerConfig`/`init_logging`.
- Add module lifecycle/factory hooks when T60 starts implementing builtin
  modules; this task only resolves manifests and dependency order.

