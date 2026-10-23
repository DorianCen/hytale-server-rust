# CoreServer → Rust rewrite plan — phase 2 (session 24+)

This is the active plan for the work after the first vertical slice. The
completed slice is archived in
[`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md);
the worldgen port remains archived in
[`done/world-generation/`](done/world-generation/README.md).

The task board is [`tasks/README.md`](tasks/README.md). The immediate next tasks
are **T36, T37 and T38** (block/fluid child stores, item child stores, and
`TagPattern`), which can run in parallel; T39 wires them into the join.

## 0. Baseline

Completed and not to be redone:

- M1: protocol codegen, framing, QUIC/mTLS, transport.
- M2 partial: P0 asset loader, deterministic ids, common assets, and the six
  self-contained config packets (`41`, `87`, `83`, `61`, `40`, `54`).
- M3 partial: runtime chunks, BSON region/world/player storage, world config,
  save boot, demand loading, world thread, chunk packets.
- M4 partial: dev/insecure join, local EdDSA auth, asset streaming, Game
  transition, player list/self-spawn, time/gameplay packets, teleport exchange,
  persistent chunk stream and movement-driven view updates.
- M0 of this phase: T34 archived the slice and re-baselined these docs.

Current gates (must stay green throughout):

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

The v1 gate remains the M4 exit below: a matching vanilla client connects,
spawns, sees terrain, moves, edits blocks, disconnects, reconnects, and the world
persists.

---

## 1. Milestones at a glance

| Milestone | Purpose | Tasks | Exit |
|---|---|---|---|
| M0 | Archive the completed slice and re-baseline the docs | T34 | Archive is self-contained; root docs describe phase 2 |
| M2 | Finish the asset foundation | T35–T39 | All P0/P1 asset packets and child indexes are golden-checked; `--validate-assets` passes |
| M3 | Runtime, universe, world lifecycle | T40–T42 | A persistent multi-world/multi-connection server runtime exists |
| M4 | v1 client join, multiplayer, edits | T43–T48 | The v1 gate in `plan.md` is satisfied and trace-tested |
| M5 | Core gameplay parity | T49–T57 | Scripted gameplay scenarios match Java |
| M6 | Builtins and NPC | T58–T61 | Selected builtin modules and NPC systems run with parity tests |
| M7 | Production hardening | T62–T64 | Security, performance, packaging, plugin API and soak gates pass |

Dependency spine:

```text
T34
 ├─ T35 → T36 ─┐
 ├─ T35 → T37 ─┼─ T39
 └─ T35 → T38 ─┘
 └─ T40 → T41 → T42 → T43 → T44 → T45 → T46 → T47 → T48
                                                   └─ M5 → M6 → M7
```

Asset work (T35–T39) and runtime work (T40–T42) can run in parallel after T34.
T43–T47 all land against the persistent runtime from T42.

---

## M0 — Archive and re-baseline

### T34 — Archive the completed vertical slice

**Status:** done (session 24)
**Package:** docs, `done/`, `tasks/`
**Depends on:** T33 (done)
**Agent fit:** high

Completed in [`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md).

**Goal**

Move the completed slice’s documentation and task list into
`done/coreserver-vertical-slice/`, leaving fresh root docs for phase 2.

**Deliverables**

- Create `done/coreserver-vertical-slice/` with:
  - `README.md` archive index (mirror `done/world-generation/README.md`).
  - `plan.md`, `NEXT_STEPS.md`, `ARCHITECTURE.md`, `PORTING_HISTORY.md`,
    `PORTING.md`, `REMAINING.md`, `GOLDEN_DATA.md`, `BENCHMARKS.md` snapshots.
  - `tasks/` containing T01–T33 exactly as completed.
- Use `git mv` so history is preserved. Do not copy code, fixtures, tools, or
  testdata.
- On the archive README, state:
  - what the slice covers (protocol → join → assets → chunks → auth → spawn);
  - that `PORTING.md` is a snapshot while the root copy remains the living
    convention guide;
  - which testdata and Java tools still apply.
- Update `done/world-generation/README.md` so its “current rewrite” links point
  at the new root docs, and add a link to the new archive.
- Create new root docs for phase 2:
  - `plan.md`: this milestone/task plan (adapted from this file).
  - `NEXT_STEPS.md`: status, gates, “start at T35”.
  - `PORTING_HISTORY.md`: empty phase-2 log starting at session 24.
  - `ARCHITECTURE.md`: current architecture with phase-1 details retained,
    phase-2 sections marked pending.
  - `REMAINING.md`: deferred scope, carried forward with milestone references.
  - `GOLDEN_DATA.md`: oracle rules plus new child-store fixture layout.
  - `BENCHMARKS.md`: measurement plan; reset “no numbers yet” only if true.
  - `tasks/README.md`: task board for T34–T64; individual task files created as
    each milestone starts.
- Keep `PORTING.md` at the root as the living guide; archive a snapshot.

**Acceptance**

```sh
find done/coreserver-vertical-slice -maxdepth 2 -type f | sort
rg -n '\]\([^)]*\.md\)' done/coreserver-vertical-slice done/world-generation
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- No Markdown link in either `done/` tree is broken.
- `git status` shows only documentation/task moves.
- Root `NEXT_STEPS.md` names T35 as the next task.

---

## M2 — Asset foundation completion

### T35 — Child-store golden dumper extension

**Status:** done (session 25)
**Package:** `tools/asset-golden-dumper`, `testdata/golden/server/assets`, Java dumper
**Depends on:** T34
**Agent fit:** medium

**Goal**

Extend the Java oracle from P0 to the P1 stores and the P2 stores that feed
their indexes, so child-store packet work is golden-first.

**Deliverables**

- Add a documented store selector to `tools/asset-golden-dumper` (for example
  `--priority P0,P1 --closure`) that dumps a store plus its dependency closure.
- Dump, with packet metadata and deterministic map ordering:
  - P1: `BlockBreakingDecal`, `BlockParticleSet`, `BlockSet`, `BlockSoundSet`,
    `ItemCategory`, `ItemQuality`.
  - Selected P2: `FluidFX`, `ItemSoundSet`, `SoundEvent`, `TagPattern`,
    `ParticleSystem`, `ParticleSpawner`, `ItemReticleConfig`,
    `ItemPlayerAnimations`.
  - Non-packet dependencies needed to decode the above (for example audio
    categories/sets) may be dumped as `index.json` entries without `.bin`.
- Add `<Store>.bin` for every selected store with a packet generator and keep
  `index.json` schema-compatible.
- Record in `testdata/golden/server/assets/README.md` how to regenerate and
  which stores are included.

**Acceptance**

```sh
tools/java-dumper/run-assets.sh --check
# two isolated runs are byte-identical
```

- New payloads decode with the generated Rust codecs.
- Existing P0 `.bin` files and `index.json` entries are unchanged.
- Store counts and `packetId`/`packetSize` match the Java inventory.

---

### T36 — Block/fluid child stores and packet encoders

**Package:** `hytale-assets`
**Depends on:** T35
**Agent fit:** medium

**Goal**

Load and stream the stores that resolve `BlockType`, `Fluid` and `Environment`
child indexes, so those packets can carry real ids instead of `0`.

**Deliverables**

- Registry collections for `BlockBreakingDecal`, `BlockParticleSet`,
  `BlockSoundSet`, `FluidFX`, `SoundEvent`, plus their decode dependencies.
- Java-compatible id assignment for file-backed stores and the parent rules
  those stores use; preloads where Java has code-defined entries.
- Packet encoders for the generated packet types of those stores
  (breaking decals, block particles, block sounds, fluid FX, sound events).
- Deterministic tag/index handling for any tag references in those stores.
- Golden tests: byte comparison where ids match Java, semantic comparison where
  Rust-owned ids/child indexes differ.

**Acceptance**

```sh
cargo test -p hytale-assets
cargo test --workspace
```

- Store counts/ids match the Java index.
- Every emitted packet decodes.
- `BlockType.blockSoundSetIndex`, `Fluid.fluidFxIndex`,
  `Fluid.blockSoundSetIndex` resolve to real ids where Java has them.
- Environment `fluidParticles` is populated when the pack defines it.

---

### T37 — Item child stores and packet encoders

**Package:** `hytale-assets`
**Depends on:** T35
**Agent fit:** medium

**Goal**

Resolve the item-side child stores: quality, sound set, reticle, categories and
generated player animations.

**Deliverables**

- Registry collections for `ItemSoundSet`, `ItemQuality`, `ItemReticleConfig`,
  `ItemCategory`, `ItemPlayerAnimations`.
- Packet encoders for the stores that have generators.
- Item encoder updates:
  - `qualityIndex` and quality-driven `ItemEntity`;
  - `itemSoundSetIndex`;
  - `reticleIndex`;
  - generated `ItemPlayerAnimations` ids for contained item states;
  - category ids where the protocol wants them.
- Golden tests for each new packet and for the previously divergent item fields.

**Acceptance**

```sh
cargo test -p hytale-assets
cargo test --workspace
```

- The 818 `itemEntity`, 61 `playerAnimationsId` and `qualityIndex` differences
  are explained by real stores or closed.
- Item packet keys still cover all 3971 Java items.

---

### T38 — TagPattern store and `UpdateTagPatterns`

**Package:** `hytale-assets`, `hytale-server`
**Depends on:** T35
**Agent fit:** medium

**Goal**

Stream tag patterns and make pattern-expanded tags part of the same Rust-owned
tag registry as `ServerTags`.

**Deliverables**

- `TagPattern` registry, parent handling and id assignment.
- Java pattern expansion into tag indexes; assets that use a pattern see the
  expanded indexes in `BlockType`/`Item`/`Fluid`/`Environment`.
- `UpdateTagPatterns` encoder and ordering before the block/item packets.
- Golden test with the Java pattern payload and `ServerTags` consistency.

**Acceptance**

- Every tag index emitted by any asset packet resolves through `ServerTags`.
- Pattern store packet decodes and matches the Java payload semantically.
- No index drift between `config_assets()` calls.

---

### T39 — Wire child packets, cache config assets, add `--validate-assets`

**Package:** `hytale-assets`, `hytale-server`, `client-bot`
**Depends on:** T36, T37, T38
**Agent fit:** medium

**Goal**

Make the full config packet set part of the join, cache it for multi-connection
use, and expose an asset validation command.

**Deliverables**

- Final dependency order, roughly:
  `BlockBoundingBoxes` → `PhysicalMaterial` → `SoundEvent`/`BlockSoundSet` →
  `FluidFX` → `Fluid` → `Environment` → `TagPattern` → `BlockType` →
  `BlockParticleSet` / decals → `ItemSoundSet`/`ItemQuality`/`ItemCategory` →
  `Item`.
  Exact order is constraint-driven: every index reference arrives before its
  consumer.
- Cache encoded `ConfigAssets` once per loaded registry and share it across
  sessions (no per-connection 8–10 MB re-encode).
- Add `hytale-server --validate-assets [--pack PATH]`:
  - loads every enabled store;
  - reports counts, failures, ids, tag indexes and encoded packet sizes;
  - exits non-zero on validation drift.
- Update `client-bot` for the new packet ids and config-order test.
- Re-run the semantic goldens and update the residual list; remove the
  exceptions that closed.

**Acceptance**

```sh
cargo test -p hytale-assets
cargo test -p client-bot --test auth_join -- --ignored
cargo test --workspace
cargo run -p hytale-server -- --validate-assets
```

- The auth join sends all P0/P1 config packets in dependency order.
- Asset validation is deterministic and reports no failures for the default
  pack.
- Config packet encoding happens once per process/world.

---

## M3 — Runtime, universe and world lifecycle

### T40 — `hytale-core` runtime

**Package:** `hytale-core`
**Depends on:** T34
**Agent fit:** medium

**Goal**

Create the shared runtime services every later task needs.

**Deliverables**

- Config loading with typed defaults and environment/CLI overrides.
- Structured logging/tracing initialization.
- Async task scheduler/shutdown token.
- Typed event bus with per-subscriber ordering.
- Module manifest/registry seam (feature-gated modules can register).
- Unit tests for config precedence, event ordering and graceful shutdown.

**Acceptance**

- `cargo test -p hytale-core` passes.
- Existing binaries boot with defaults unchanged.
- No global mutable singletons beyond process-level handles.

---

### T41 — Universe and world lifecycle

**Package:** `hytale-world`, `hytale-server`, `worldgen-server`
**Depends on:** T40
**Agent fit:** low/medium

**Goal**

Own multiple worlds for the process lifetime instead of one scripted save.

**Deliverables**

- `Universe`: create/load/list/unload/save worlds; default world selection;
  world UUID/name resolution.
- Per-world `WorldThread` lifecycle, spawn point, view registry and save-on-
  unload/shutdown.
- World provider seam: `Default`, `Flat`, `Void`, `Dummy` (at minimum Default;
  others may return empty chunks until M5).
- Player save load/save on world join/leave.
- CLI: a small `world`/`universe` surface for create/load/list/save/delete used
  by tests and operations.

**Acceptance**

- Two worlds can be created, edited and saved concurrently, then reloaded.
- Java save/read gates still pass.
- Shutdown flushes dirty chunks and player data once.
- A missing/corrupt world fails closed with a clear error.

---

### T42 — Persistent listener and multi-session runtime

**Package:** `hytale-server`, `hytale-net`, `client-bot`
**Depends on:** T41, T39 (for cached config packets)
**Agent fit:** low/medium

**Goal**

Replace the one-connection demo with the real server loop.

**Deliverables**

- `hytale-server serve` (or equivalent) that owns the QUIC listener, universe
  and shared asset registry for the process lifetime.
- One independent session task per connection, with the existing join state
  machine, auth modes and view tracking.
- Connection limits, idle/timeout handling, clean disconnect and graceful
  shutdown.
- Shared/cached config assets; per-session tag/config handoff.
- Keep `chunk-serve` as a debug transport demo or fold it into `serve --demo`.
- Loopback test with two or more clients joining the same world concurrently.

**Acceptance**

```sh
cargo test -p hytale-server
cargo test -p client-bot --test auth_join -- --ignored
```

- Two bots join concurrently, each receives its own 21-chunk view.
- Disconnecting one does not unload the other’s chunks or stop the listener.
- A new connection can join after a disconnect without restarting the process.
- Shutdown closes sessions and flushes world state.

---

## M4 — v1 client join, multiplayer and edits

### T43 — World clock and `GameplayConfig`

**Package:** `hytale-server`, `hytale-world`, `hytale-assets`
**Depends on:** T42
**Agent fit:** medium

**Goal**

Make time advance and stop using protocol defaults for gameplay rules.

**Deliverables**

- World clock state derived from `WorldConfig` day/night durations and pause.
- Tick progression, periodic `UpdateTime`, pause/resume behavior.
- Load `GameplayConfig` (internal store; no packet) and encode
  `WorldGameplayConfig` from it with protocol-default fallback.
- Tests for clock wrap, pause, duration overrides and config fallback.

**Acceptance**

- A client sees time advance after joining and after an hour rollover.
- Gameplay flags match the loaded config.
- Clock state survives save/reload via the world config/time source.

---

### T44 — Self-spawn equipment and cosmetics from the player save

**Package:** `hytale-server`, `hytale-world`
**Depends on:** T42
**Goal:** replace the empty self-spawn model/skin/equipment payloads.

**Deliverables**

- Decode/preserve the player save components needed for appearance and
  equipment (model, skin, equipment, selected item).
- Build the self `EntityUpdates` components from those values.
- Safe defaults when a component is absent or an item id is unknown.
- Java player-save read-back remains green.

**Acceptance**

- A saved player with equipment spawns with those items/components.
- An empty/default save still spawns with Java-compatible defaults.
- Unknown ids fall back without failing the join.

---

### T45 — Other-player entity tracking and broadcast

**Package:** `hytale-server`, `hytale-entities`
**Depends on:** T42, T44
**Goal:** make multiplayer observable.

**Deliverables**

- Per-world player entity registry (id, transform, appearance, view).
- Add/remove/update broadcasts as players join, leave, move or change view.
- Player list updates for all sessions.
- Movement throttling/interest filtering by view radius.
- Loopback test with two bots seeing each other’s spawn and movement.

**Acceptance**

- Two clients see each other spawn, move and disconnect.
- No entity update leaks after a player leaves.
- Player list and self identity remain correct.

---

### T46 — Block place/break

**Package:** `hytale-server`, `hytale-world`
**Depends on:** T42
**Goal:** complete the v1 edit loop.

**Deliverables**

- Decode client block place/break packets (or the equivalent interaction
  packets used by the client).
- Validate target, block id and reach; apply through `WorldThread`.
- Mark chunks dirty, stream block/section updates to interested sessions.
- Persist the edit through the existing region save path.
- Minimal inventory/held-item consistency needed by the edit flow (do not port
  the full inventory yet).

**Acceptance**

- A bot places and breaks a block; another client sees the change.
- The edit survives save/reload and the Java reload gate.
- Invalid targets/ids are rejected without killing the session.

---

### T47 — Disconnect, reconnect and session teardown

**Package:** `hytale-server`, `client-bot`
**Depends on:** T43–T46
**Goal:** make the v1 lifecycle reliable.

**Deliverables**

- Graceful `ClientDisconnect` and server-initiated disconnect paths.
- Session teardown: chunk view unload, player save, entity removal, broadcast.
- Reconnect with the same identity and saved position/state.
- Timeouts and half-closed QUIC stream handling.
- Loopback test: join → move → edit → disconnect → reconnect.

**Acceptance**

- No task/thread/memory leak across repeated join/leave cycles.
- Reconnect restores saved transform and world edits.
- A failed disconnect does not block the world thread or listener.

---

### T48 — Real-client trace and v1 compatibility gate

**Package:** `tools/client-bot`, `testdata/golden/server`, docs
**Depends on:** T47
**Goal:** prove the join against a real client trace, not only the bot.

**Deliverables**

- A trace-capture note/tool for the real client (packet id, channel, order,
  payload hashes; no proprietary payloads beyond what the project already
  commits).
- A committed trace fixture or a documented manual runbook if capture cannot be
  redistributed.
- A gate test that replays the trace order/state transitions against the
  server.
- Update `REMAINING.md`/`PORTING_HISTORY.md` with any client-required packets
  discovered.

**Acceptance**

- A vanilla client connects, spawns, sees terrain and another player, moves,
  edits blocks, disconnects and reconnects.
- The recorded trace gate is repeatable in CI (or the manual runbook is
  verified and linked).
- This closes `plan.md` M4 / v1.

---

## M5 — Core gameplay parity

These are roadmap-sized work packets; create each `tasks/Txx-*.md` when the
milestone starts. Each task must name its Java oracle and scripted scenario.

### T49 — Inventory, containers and transactions

**Package:** new `hytale-inventory` (or `hytale-entities` modules) + server/world
**Goal:** item stacks, held item, inventory containers, pickup/drop, atomic
transactions and save integration.
**Acceptance:** scripted item move/split/merge/drop/pickup scenarios match Java;
save/reload preserves inventory.

### T50 — Interaction graph and block interactions

**Package:** `hytale-entities`, `hytale-world`, `hytale-server`
**Goal:** root/interaction chains, interaction vars, block interaction dispatch,
doors/benches/containers as the first consumers.
**Acceptance:** scripted primary/secondary/use interactions match Java; block
edit path from T46 remains green.

### T51 — Player movement physics and collision authority

**Package:** `hytale-world`, `hytale-entities`
**Goal:** server-authoritative movement, collision, gravity, step/climb basics,
anti-cheat bounds.
**Acceptance:** scripted walk/jump/fall/collide scenarios match Java within
documented tolerance; no rubber-banding in the bot loop.

### T52 — Block simulation: ticks, fluids and support

**Package:** `hytale-world`, `hytale-entities`
**Goal:** random/block ticks, fluid flow, support propagation, falling blocks,
block health and drops.
**Acceptance:** scripted tick/fluid/support scenarios match Java; regions
remain Java-readable.

### T53 — Entity stats, effects and combat basics

**Package:** `hytale-entities`
**Goal:** entity stat maps, effects, damage causes, health/death, basic combat.
Also load the `EntityStatType` module store for item utility/stat HUD payloads.
**Acceptance:** scripted damage/effect/death scenarios match Java; item stat
indexes resolve.

### T54 — Time and weather

**Package:** `hytale-world`, `hytale-server`
**Goal:** weather state/assets and transitions; completed time integration.
**Acceptance:** scripted weather transition and time scenario match Java.

### T55 — Projectiles and traversal

**Package:** `hytale-entities`
**Goal:** projectile spawn/flight/hit, basic mounts, parkour/mantling/crouch-slide
as scoped in `REMAINING.md`.
**Acceptance:** scripted projectile hit/miss and traversal scenarios.

### T56 — Prefab spawner and runtime entities

**Package:** `hytale-world`, `hytale-entities`
**Goal:** prefab block/fluid/environment paste (already), prefab entities and
block components, spawn/despawn lifecycle.
**Acceptance:** prefab scenario matches Java; save/reload preserves entities.

### T57 — UI basics and block health presentation

**Package:** `hytale-server`, protocol
**Goal:** the minimal UI/world-map/compass/marker packets needed by M5 scenarios;
block health/breaking presentation.
**Acceptance:** client-visible state matches the server state in scripted runs.

---

## M6 — Builtins and NPC

### T58 — NPC core: AI, pathfinding and flocking

**Package:** new `hytale-npc`
**Goal:** NPC archetypes, navigation/pathfinding, flocking/steering, perception.
**Acceptance:** scripted NPC navigation/flocking scenarios match Java.

### T59 — NPC spawning, combat and encounters

**Package:** `hytale-npc`, `hytale-world`
**Goal:** spawn rules, combat, encounters/instances/portals.
**Acceptance:** scripted encounter/combat scenario matches Java.

### T60 — Builtin module runtime and selected modules

**Package:** `hytale-builtin` feature crates
**Goal:** module manifest/lifecycle and the selected `builtin/` modules needed
by M5/M6 (not all 62 at once).
**Acceptance:** each enabled module has a parity test; disabled modules add no
runtime cost.

### T61 — Editor, builder tools, machinima, voice, cosmetics; HytaleGenerator

**Package:** separate/deferred
**Goal:** scope explicitly after M6; keep `builtin/HytaleGenerator` as a separate
engine project.
**Acceptance:** documented scope decision and, if implemented, parity smoke
tests.

---

## M7 — Production hardening

### T62 — Security, fuzzing and rate limits

- Protocol/asset fuzzing, malformed packet handling, auth failure paths, rate
  limits, resource ceilings, dependency audit.
- Acceptance: fuzz targets run clean for the defined budget; abuse tests fail
  closed.

### T63 — Performance, memory and soak

- Benchmark boot, asset decode, config packet cache, chunk stream, world tick and
  save; memory ceilings; long-running soak with repeated joins/edits.
- Acceptance: published numbers against Java where comparable; no leaks in the
  soak budget.

### T64 — Packaging, docs and plugin/mod API

- Cross-platform release builds, service/package docs, asset-mod compatibility
  docs, Rust plugin API and migration guide.
- Acceptance: reproducible release artifact; documented plugin/asset-mod
  workflow; no Java-plugin compatibility claims.

---

## 2. Sequencing rules

1. T34 must land before creating phase-2 task files or touching root docs.
2. T35 must land before T36–T38: child-store work is golden-first.
3. T39 depends on T36–T38; do not wire partial indexes into the join.
4. T40–T42 can run in parallel with T35–T39 after T34, but T42 needs T39’s
   cached config packets.
5. T43–T47 are serialized by shared session/world state; T48 is the final M4
   gate.
6. M5 starts only after T48; M6 after M5; M7 after M6.
7. Every task keeps the workspace gates green before handoff.

## 3. Definition of done for phase 2

- M2 exit: all P0/P1 asset packets are golden-checked, child indexes are real,
  `--validate-assets` is deterministic, and the config cache is shared.
- M3 exit: the universe owns multiple worlds and the listener owns multiple
  sessions for the process lifetime.
- M4 exit: the `plan.md` v1 gate passes with a real-client trace.
- M5/M6 exit: the scripted parity suites for the enabled gameplay/NPC/builtin
  scope pass.
- M7 exit: security, soak, performance and packaging gates pass.
- `done/coreserver-vertical-slice/` remains a self-contained archive; new root
  docs and `tasks/` describe only remaining work.

## 4. Task-file template for phase 2

Each `tasks/Txx-*.md` should use the existing format:

```markdown
# Txx — Title

**Status:** planned | in progress | done (session N)
**Package:** crates/tools
**Depends on:** Tyy, Tzz
**Agent fit:** high | medium | low

## Goal
One paragraph tying the task to the milestone exit.

## Deliverables
- Concrete files/types/commands.

## Acceptance
```sh
cargo test -p <crate>
cargo clippy -p <crate> --all-targets -- -D warnings
cargo fmt -p <crate> -- --check
```
- Observable results and golden/Java gates.

## Deviations
- Filled in when the task lands.

## Follow-ups
- Explicitly out of scope for this task.
```
