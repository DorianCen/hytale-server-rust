# Tasks — CoreServer rewrite, phase 2

Each file is a self-contained work packet for one branch/worktree. Tasks are
sized so a fast subagent can finish them without loading the whole codebase into
context. The main agent owns sequencing and shared-file edits.

## Status board

| Task | Title | Depends on | Agent fit | Status |
|---|---|---|---|---|
| [T34](T34-archive-rebaseline.md) | Archive vertical slice and re-baseline docs | T33 | high | done (session 24) |
| [T35](T35-child-store-goldens.md) | Child-store golden dumper extension | T34 | medium | done (session 25) |
| [T36](T36-block-fluid-child-stores.md) | Block/fluid child stores + packets | T35 | medium | done (session 26) |
| [T37](T37-item-child-stores.md) | Item child stores + packets | T35 | medium | done (session 26) |
| [T38](T38-tag-patterns.md) | TagPattern + `UpdateTagPatterns` | T35 | medium | done (session 26) |
| [T39](T39-asset-wiring-validation.md) | Asset wiring, config cache, `--validate-assets` | T36, T37, T38 | medium | done (session 27) |
| [T40](T40-hytale-core-runtime.md) | `hytale-core` runtime | T34 | medium | done (session 28) |
| [T41](T41-universe-world-lifecycle.md) | Universe and world lifecycle | T40 | low/medium | done (session 29) |
| [T42](T42-persistent-listener.md) | Persistent listener and multi-session runtime | T39, T41 | low/medium | done (session 30) |
| [T43](T43-world-clock-gameplay-config.md) | World clock and `GameplayConfig` | T42 | medium | done (session 31) |
| [T44](T44-self-spawn-appearance.md) | Self-spawn equipment and cosmetics | T42 | medium | done (session 32) |
| [T45](T45-other-player-tracking.md) | Other-player entity tracking and broadcast | T42, T44 | low/medium | done (session 33) |
| [T46](T46-block-place-break.md) | Block place/break | T42 | medium | done (session 34) |
| [T47](T47-disconnect-reconnect.md) | Disconnect, reconnect and session teardown | T43–T46 | medium | done (session 35) |
| [T48](T48-real-client-trace.md) | Real-client trace and v1 gate | T47 | low/medium | done (session 36) |
| [T49](T49-inventory-containers.md) | Inventory, containers and transactions | T48 | low/medium | done (session 37) |
| [T50](T50-interaction-graph.md) | Interaction graph and block interactions | T49 | low/medium | done (session 38) |
| [T51](T51-movement-physics.md) | Player movement physics and collision | T48 | low/medium | done (session 39) |
| [T52](T52-block-simulation.md) | Block simulation: ticks, fluids, support | T49, T51 | low/medium | done (session 40) |
| [T52b](T52b-fluid-ticker-parity.md) | Fluid ticker parity follow-up | T52 | medium | done (session 41) |
| [T52c](T52c-support-random-tick-parity.md) | Support/random-tick parity follow-up | T52 | medium | done (session 41) |
| [T52d](T52d-block-health-storage.md) | Block health storage and fragility | T52 | medium | done (session 41) |
| [T53](T53-entity-stats-effects-combat.md) | Entity stats, effects and combat basics | T48 | low/medium | done (session 44) |
| [T54](T54-time-weather.md) | Time and weather | T43, T48 | medium | done (session 43) |
| [T55](T55-projectiles-traversal.md) | Projectiles and traversal | T51, T53 | low/medium | done (session 45) |
| [T56](T56-prefab-runtime-entities.md) | Prefab spawner and runtime entities | T45, T50 | low/medium | done (session 46) |
| [T57](T57-ui-block-health-presentation.md) | UI basics and block-health presentation | T46, T53 | low/medium | done (session 47) |
| [T57b](T57b-presentation-followups.md) | Presentation follow-ups | T57 | low/medium | in progress (session 49) |
| [T58](T58-npc-core.md) | NPC core: AI, pathfinding, flocking | T48, T57b | low/medium | done (session 50) |
| [T58b](T58b-npc-builder-expressions.md) | NPC builder expressions and resolved archetypes | T58 | medium | done (session 50) |
| [T58c](T58c-npc-world-grid-group.md) | NPC world grid and group steering | T58, T58b | medium | done (session 50) |
| [T59](T59-npc-spawning-combat.md) | NPC spawning, combat and encounters | T58 | low/medium | done (sessions 51–61; T59b runtime follow-ups) |
| [T59b](T59b-npc-runtime-spawning.md) | NPC runtime, spawn positions and beacons | T59 | low/medium | done (sessions 52–62) |
| [T59c](T59c-npc-combat-encounters.md) | Combat action graph and encounters | T59, T59b | low/medium | done (session 61) |
| [T60](T60-builtin-module-runtime.md) | Builtin module runtime and selected modules | T58, T59 | low/medium | done (session 63) |
| [T60b](T60b-followup-gameplay.md) | Player combat/stats, containers, crafting, encounters, portal/ECS | T60 | low | done (session 64) |
| [T65](T65-live-auth.md) | Live OAuth/session-service authentication for release clients | T48, T42 | medium | done (session 65; asset follow-up T66) |
| [T66](T66-live-client-assets.md) | Live-client config-asset completeness | T35, T39, T60b, T65 | medium | in progress (session 70; real-client smoke pending) |
| T61 | [Editor/builder/machinima/voice/cosmetics scope](T61-editor-builder-scope.md) | T60 | low | done (session 66) |
| T62 | [Security, fuzzing and rate limits](T62-security-fuzzing-rate-limits.md) | T48 | medium | done (session 67) |
| [T63](T63-performance-memory-soak.md) | Performance, memory and soak | T48 | medium | done (session 68) |
| T64 | [Packaging, docs and plugin/mod API](T64-packaging-docs-plugin-api.md) | T60 | medium | done (session 69) |

Task files for T43–T66 are created when their milestone starts; the milestone
scope is in [`../plan.md`](../plan.md). T65 closed live authentication; its
real-client asset follow-up is T66 (in progress; session 70 landed the item,
block and twenty-one store encoders, user-run client smoke still pending). T61 records the
editor/builder/machinima/voice/cosmetics scope: only cosmetic appearance
passthrough is in scope, editor clients are refused in every auth mode, and
`builtin/HytaleGenerator` stays a separate engine project. T62 lands the
session abuse limits (payload/rate/view/queue ceilings), duplicate-identity
refusal, deterministic protocol/asset fuzz targets and the `tools/security.sh`
dependency/fuzz gate. T63 publishes the release performance/memory baseline
for boot, assets, config cache, tick, chunk stream and save, adds the
1,000-cycle join/edit/save soak with an RSS ceiling, and records Java
boot/validation comparisons via `tools/java-baseline.sh`. T64 lands the
reproducible release archive, systemd deployment docs, the compiled-in Rust
plugin API/migration guide and the asset-mod merge workflow; Java plugin jars
and dynamic/Rust plugin loading remain explicitly out of scope. T52b–T52d closed the parity gaps
recorded by T52 in session 41. T53's gameplay core, Java entity-gameplay
fixture and T54's time/weather core are landed; T55 adds the projectile core
and traversal seam; T56 lands prefab entities/block components and the
persisted runtime entity store; T57 lands block-health replication, runtime entity spawns and
world-map basics. T57b is the M5 presentation follow-up: partial-hit block
interaction, entity lifecycle/metadata, the stat/effect/projectile/weather
config packets, map images/discovery and the minimal HUD/window packets. Its
second slice lands chunk map images and container windows; bench crafting
windows, container transactions and particle/weather presentation remain
T59/T60. T58
(NPC core) starts M6 after T57b; its first slice lands the role/flock catalogs,
Java direction tables, block-grid A*, path following, pursue/evade/rotate
steering and view-cone perception; T58b resolves the builder expression subset
and reference templates so all roles become usable archetypes; T58c adds
real-world walk/fly planning and group steering. T58 is done with the builder
graph and full motion-controller probes deferred to T59/T60. T59's first slice
lands the world spawn configuration/gates, weighted selection, role combat
profiles, damage ranges and a scripted NPC actor; the live runtime/spawn
position search are T59b; the encounter/action-graph layers are T59c (done). T59b's
first slice adds the live WorldThread NPC store, spawn/despawn/damage/tick
commands, persistence and session target publication; the second adds
movement-mode spawn-position search, environment/role selection and the
density/expected-count controller; the third adds automatic population ticks
through `set_npc_spawn_targets`/`tick_npc_spawns`; a fourth adds loaded-section
environment segment counting for population targets; a fifth resolves the real
`SpawnBlockSet` catalog to allowed floors; a sixth resolves `SpawnFluidTag`
through the fluid tag index, a seventh evaluates config `LightRanges` and an
eighth loads the beacon/spawn-marker catalogs with weighted selection and
ISO-8601 cooldowns; a ninth adds `--spawn-npc` to `world-boot`/`serve`; and
the final slice adds automatic environment enumeration, spawn suppression,
live marker/beacon controllers with persisted cooldowns, `--auto-population`
and the client-bot spawn/chase/despawn scenario. T60 lands the Rust-native
builtin module runtime (`hytale-builtin`) with the eight selected manifests,
feature-gated lifecycle ordering, runtime switches and the
`--module`/`--disable-module`/`--list-modules` CLI; disabled simulation, NPC,
encounter and weather modules do no world-thread work. T60b closes the first
gameplay follow-up slice: player health/stat replication, container
`MoveItemStack` transactions, bench windows and recipe delivery, encounter
spawn/particle/sound execution, portal/instance catalog plans and the NPC
builder/ECS storage seam.

## Rules

1. **One task per branch/worktree.** Never edit files owned by another task.
2. **Shared files are coordinated.** `Cargo.toml`, `Cargo.lock`, `plan.md`,
   `NEXT_STEPS.md`, `ARCHITECTURE.md` and this board are main-agent-owned unless
   the task explicitly lists them.
3. **Read only what the task lists.** Use `rg`/`find` for discovery, then read
   the matches. Do not open thousands of generated Java files.
4. **Golden-first.** If the task needs expected bytes and no fixture exists, stop
   and extend the Java dumper instead of inventing values.
5. **Generated code is write-only.** Edit the IR/generator, rerun, commit both.
6. **No broad refactors.** Finish the acceptance section; report follow-ups.
7. **Gate before handoff:**

   ```sh
   ./tools/test.sh fast
   cargo test -p <crate>
   cargo clippy -p <crate> --all-targets -- -D warnings
   cargo fmt -p <crate> -- --check
   ```

   For workspace-level tasks use `./tools/test.sh all` (the full asset-backed
   workspace gate) instead of `cargo test --workspace`. See
   [`../TESTING.md`](../TESTING.md) for the fast `assets`, slow `slow` and
   serial E2E tiers.
8. **Report:** files changed, commands run, results, deviations, follow-ups.
9. **Do not touch `done/`** except broken-link fixes; it is the archive.

## Subagent sizing guide

- **high**: isolated files, no Java reading beyond one small class, no network.
- **medium**: one Java class/generator or a bounded format port.
- **low**: multi-crate integration, TLS/QUIC behaviour, or debugging against a
  real client. Use the main agent or a stronger model.

If a task exceeds ~200k tokens of reading, split it. The task must name the
exact files so the subagent never has to guess.
