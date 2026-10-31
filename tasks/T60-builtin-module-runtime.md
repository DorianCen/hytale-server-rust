# T60 — Builtin module runtime and selected modules

**Status:** done (session 63)
**Package:** `crates/hytale-builtin`, `hytale-core`, `hytale-world`, `hytale-server`
**Depends on:** T58, T59
**Agent fit:** low

## Goal

Replace the deferred builtin module seam with a Rust-native manifest/lifecycle
runtime, and select the `builtin/` modules whose gameplay systems already landed
in M5/M6. Each selected module is feature-gated, starts in Java dependency
order, and exposes a runtime switch that gates the corresponding native system.
The acceptance gate is a parity test per enabled module plus an integration test
proving a disabled module does no world-thread work.

## Deliverables

- New `hytale-builtin` crate:
  - `PluginManifest`/`PluginIdentifier`/`AuthorInfo` with the Java field names
    (`Group`, `Name`, `Version`, `Description`, `Authors`, `Website`, `Main`,
    `ServerVersion`, `Dependencies`, `OptionalDependencies`, `LoadBefore`,
    `DisabledByDefault`, `IncludesAssetPack`, `SubPlugins`).
  - `BuiltinModule` trait with `setup`/`start`/`shutdown` hooks and a
    `BuiltinHost` lifecycle using `hytale_core::ModuleRegistry` for
    dependency-first ordering/cycle detection. States mirror `PluginState`.
  - `StartOptions` selection: manifest defaults (all selected modules are
    default-enabled), explicit `--module` requests and `--disable-module`
    removals. A module whose dependency is disabled fails closed instead of
    running without it.
  - Feature-gated selected modules: `BlockTick`, `RandomTick`, `FallingBlocks`,
    `BlockPhysics`, `Fluid`, `Weather`, `NPC`, `EncounterManager`.
  - `BuiltinRuntime` switches serialized for boot/config use.
- `hytale-world::simulation::SimulationStages`: scheduled/random/falling/
  physics/fluid stages. `BlockSimulation::tick` skips a disabled stage before
  walking chunks or queueing work.
- `hytale-server` wiring:
  - `WorldThreadOptions.builtin`, `WorldBootOptions.builtin`,
    `ServeOptions.builtin`, `UniverseRuntime::with_builtin`.
  - CLI `--list-modules`, `--module <Group:Name>`, `--disable-module <Group:Name>`
    with a host start/shutdown around every command.
  - World-thread gates for NPC spawn/tick, encounter tick and weather queries.
- Golden data: `testdata/golden/builtin/manifests.json` with the eight selected
  reference manifests.

## Acceptance

```sh
cargo test -p hytale-builtin
cargo test -p hytale-builtin --no-default-features
cargo test -p hytale-world --lib simulation
cargo test -p hytale-server --test builtin_runtime
./tools/test.sh fast
cargo clippy -p hytale-builtin --all-targets --all-features -- -D warnings
cargo clippy -p hytale-server --all-targets -- -D warnings
cargo clippy -p hytale-world --all-targets -- -D warnings
cargo fmt --all -- --check
```

- `crates/hytale-builtin/tests/parity.rs` has one named parity test per module;
  each compares the compiled manifest to the golden and checks the feature
  presence.
- The default start order is asserted against the Java dependency edges:
  `BlockTick` before `BlockPhysics`/`Fluid`, `FallingBlocks` before
  `BlockPhysics`, `NPC`/`Weather` before `EncounterManager`.
- Disabling `BlockTick` must fail with `DisabledDependency` because `Fluid`
  and `BlockPhysics` depend on it.
- `hytale-server/tests/builtin_runtime.rs` boots with
  `BuiltinRuntime::none()` and asserts zero block/random/falling/fluid work,
  NPC spawn refusal, empty NPC/encounter ticks and `weather_at == None`.
- `--no-default-features` compiles and registers no modules; `--list-modules`
  prints the eight manifest rows.

## Deviations

- The selected modules are lifecycle/runtime adapters over the native systems
  landed by T52/T54/T58/T59. The Java `PluginBase` registry objects (asset/
  command/event/entity registries) are not recreated; Rust crates already own
  those seams.
- Core-only dependencies (`Hytale:LegacyModule`, `Hytale:EntityModule`,
  `Hytale:BlockTypeModule`, …) remain in the parity manifests but are treated
  as server-provided during ordering, because the Rust core has no separate
  plugin for them.
- `--disable-module` fails closed when a selected dependent needs the module.
  Java's `PluginManager` disables dependents; the CLI error is stricter and
  easier to diagnose.
- Weather/encounter/NPC presentation remains the T57b/T59c baseline; T60 only
  gates their native producers.
- The `slow` nextest profile now allows three 180 s periods because the
  pre-existing `npc_spawn_control` persistence test takes ~373 s locally; it
  passed on the rerun.

## Verified (session 63)

- `cargo test -p hytale-builtin`: 14 lib + 14 parity tests + 1 doc test pass.
- `cargo test -p hytale-builtin --no-default-features`: 15 parity tests pass
  and no modules are registered.
- `cargo test -p hytale-world`: all crate tests pass, including the new
  disabled-stage tests.
- `cargo test -p hytale-server --test builtin_runtime`: passes with the real
  asset pack (32.8 s).
- `./tools/test.sh fast`: 1,441 passed, 17 skipped.
- `./tools/test.sh slow`: 810 passed, 2 skipped.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` are clean.
- The golden manifests compare byte-for-field equal to the eight reference
  `manifest.json` files.

## Follow-ups

- `builtin/Crafting` bench windows, container `MoveItemStack` transactions and
  weighted drop-list production.
- Full builder graph, half-step motion-controller A*, wander/avoidance,
  blackboard sensors and the NPC ECS (T61+ scope).
- Deferred encounter spawner/particle/sound actions and instance/portal
  transports.
- Player health/combat and per-entity stat/effect/damage packets.
- A `cargo` feature matrix job that builds the server with each individual
  builtin feature disabled.
