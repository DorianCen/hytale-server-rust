# CoreServer → Rust rewrite plan — phase 2 (session 24+)

This is the active plan for the work after the first vertical slice. The
completed slice is archived in
[`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md);
the worldgen port remains archived in
[`done/world-generation/`](done/world-generation/README.md).

The task board is [`tasks/README.md`](tasks/README.md). T43 (world clock and
`GameplayConfig`), T44 (self-spawn appearance/equipment), T45 (other-player
tracking), T46 (block place/break), T47 (disconnect, reconnect and session
teardown) and T48 (real-client trace and v1 gate) are done; M4/v1 is complete,
T49 (inventory containers/transactions), T50 (interaction graph and block
interactions), T51 (movement physics and collision authority), T52 (block
simulation) and its parity follow-ups T52b–T52d are done; M5 gameplay parity
continues with T53 (done), T54 (done), T55 (done), T56 (done) and T57
(done), then T58.

## 0. Baseline

Completed and not to be redone:

- M1: protocol codegen, framing, QUIC/mTLS, transport.
- M2 done: P0 asset loader, deterministic ids, common assets, the ten child
  stores/packets and `UpdateTagPatterns`; the final 21-packet config burst is
  cached once and `--validate-assets` is green.
- M3 done: `hytale-core` runtime services, the persistent universe catalog and
  provider seam, per-world `UniverseRuntime`, player load/save seam, the
  `universe` CLI, and the persistent multi-session `serve` listener with shared
  per-world config cache, connection limits, idle timeout and graceful shutdown.
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
| M3 | Runtime, universe, world lifecycle | T40–T42 | A persistent universe catalog and multi-session listener exist (one selected world per listener) |
| M4 | v1 client join, multiplayer, edits | T43–T48 | The v1 gate in `plan.md` is satisfied and trace-tested |
| M5 | Core gameplay parity | T49–T57b | Scripted gameplay scenarios match Java |
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

**Status:** done (session 27)

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

**Status:** done (session 26)
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

**Status:** done (session 26)
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

**Status:** done (session 26)
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

**Status:** done (session 27)
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

**Status:** done (session 30)

### T40 — `hytale-core` runtime

**Status:** done (session 28)
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

**Status:** done (session 29)
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

**Status:** done (session 30)
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

**Status:** done (session 36) — the T48 trace gate closes M4/v1.

### T43 — World clock and `GameplayConfig`

**Status:** done (session 31)
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

**Deviations**

- `GameplayConfig` loads as an internal Rust store (all 8 pack assets) while
  the Java canonical boot reports count 0 because the config files fail
  camera-effect validation. The world falls back to `GameplayConfig.DEFAULT`
  when the id is missing or the store is empty.
- `UpdateTime` is written by each session once per real second while time is
  running; a pause-state change also re-sends `UpdateTimeSettings`. There is no
  cross-session broadcast registry until T45, so each session writes its own
  copy from the shared world clock.
- `DayTime` interpolation/sleep commands and moon-phase events remain T54 and
  the command framework.

---

### T44 — Self-spawn equipment and cosmetics from the player save

**Status:** done (session 32)
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

**Deviations**

- The `ModelUpdate` payload is built from the saved `ModelReference` (asset id
  and scale, with Java's unknown texture) because the `ModelAsset` store and its
  geometry/animation expansion are not ported yet.
- The insecure handshake's `skin` is the primary cosmetic source (matching
  Java); a saved `PlayerSkin` component is a fallback. The local JWT profile
  does not carry cosmetic claims yet.
- The world thread still owns one loaded player; per-session identity and save
  are T45/T47.

---

### T45 — Other-player entity tracking and broadcast

**Status:** done (session 33)
**Package:** `hytale-server`, `client-bot`
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

**Deviations**

- The world thread still owns one loaded player appearance, so sessions share
  that appearance until per-session player data lands in T47.
- Entity traffic is filtered by the square chunk view; the server player list is
  global.
- Movement updates are throttled to 10 Hz and flushed after a stop.
- Player-message queues are unbounded; backpressure is T62.

---

### T46 — Block place/break

**Status:** done (session 34)
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

**Deviations**

- Placement resolves the packet's `itemInHandId` through the item/block
  registries; it does not decrement a live inventory yet (T49).
- Broadcasts use filler `0`/rotation `0`; block rotations, filler and block
  components are later gameplay work.
- Only pressed left/right mouse buttons are handled; releases, mouse motion and
  per-item/creative reach remain later interaction work.

---

### T47 — Disconnect, reconnect and session teardown

**Status:** done (session 35)
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

**Deviations**

- Teardown persists the live `Transform` (and preserves all other components);
  live inventory/held-item state is T49.
- Sessions own registered views and the loaded-chunk set is their union; the
  chunk event queue is still world-wide, so per-view event filtering is a
  follow-up. The startup/CLI base view is replaced by the first session view.
- Reconnect during a still-registered session creates a second player entry;
  duplicate-identity takeover is T62.
- The server closes the QUIC connection after `ServerDisconnect`/teardown rather
  than waiting for a client acknowledgement (Java has no such packet).

---

### T48 — Real-client trace and v1 compatibility gate

**Status:** done (session 36)
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

**Deviations**

- `serve --trace` records server-side packet ids/channels/order/hashes in
  `hytale-net::trace`; the committed fixture is the normalized bot lifecycle
  baseline because a vanilla-client capture cannot be redistributed/automated
  here. The real-client capture runbook is in
  `testdata/golden/server/trace/README.md`.
- The live gate is `#[ignore]`d (assets + loopback); the registry/fixture gate
  runs in the default workspace suite.
- The local EdDSA auth authority is not trusted by a default online vanilla
  client; live-session auth remains an adapter, not a packet-order gap.
- Dynamic payloads keep order/id/channel but intentionally drop hashes.

---

## M5 — Core gameplay parity

These are roadmap-sized work packets; create each `tasks/Txx-*.md` when the
milestone starts. Each task must name its Java oracle and scripted scenario.

### T49 — Inventory, containers and transactions

**Status:** done (session 37)
**Package:** new `hytale-inventory` (or `hytale-entities` modules) + server/world
**Goal:** item stacks, held item, inventory containers, pickup/drop, atomic
transactions and save integration.
**Acceptance:** scripted item move/split/merge/drop/pickup scenarios match Java;
save/reload preserves inventory.

**Deviations**

- The pure `hytale-inventory` crate implements item stacks, six player
  containers, active slots and atomic move/split/merge/swap/drop/pickup.
  Inventory windows, block containers, item entities and the full
  `MoveItemStack`/`SmartMoveItemStack` server path remain later gameplay work.
- The server consumes the authoritative held block on placement and persists
  inventory with the transform; a legacy session without a saved inventory
  keeps the T46 packet-item fallback.
- `DropItemStack` removes/resyncs the stack but does not spawn a world item
  entity until T56.

### T50 — Interaction graph and block interactions

**Status:** done (session 38)
**Package:** `hytale-interactions`, `hytale-server`, `client-bot`
**Goal:** root/interaction chains, interaction vars, block interaction dispatch,
doors/benches/containers as the first consumers.
**Acceptance:** scripted primary/secondary/use interactions match Java; block
edit path from T46 remains green.

**Deviations**

- The executor covers structural/variable/block/state nodes used by the first
  consumers; selectors, effects, sounds, cooldowns, projectiles, stats and
  client-chain sync remain T53/T55/T59.
- Door handling uses single-block state variants and a front heuristic; double
  doors, fillers/hitbox updates, blocked checks and exact rotations are not
  ported. `ChangeBlock` actions are parsed but not applied.
- Bench/container blocks are dispatched as handled UI actions; window packets
  are T59/T60.
- Right-click prioritises the target block's `Use` root over the held item's
  full `Secondary` item root.

### T51 — Player movement physics and collision authority

**Status:** done (session 39)
**Package:** `hytale-world`, `hytale-entities`, `hytale-assets`, `hytale-server`,
`client-bot`
**Goal:** server-authoritative movement, collision, gravity, step/climb basics,
anti-cheat bounds.
**Acceptance:** scripted walk/jump/fall/collide scenarios match Java within
documented tolerance; no rubber-banding in the bot loop.

**Deviations**

- Java does not simulate player walking; `ClientMovement` positions are
  client-authoritative. The port keeps that model, validates finite/world
  bounds, resolves collision and sends a correction only when the resolved
  position differs. `MovementBody` is the deterministic gravity/jump/fly/climb
  integrator for tests and later NPC work.
- Collision uses the solid `BlockMaterial` rule, Java 64-rotation hitboxes and
  filler origins. Fluids, trigger/damage boxes, mounts, traversal and entity
  collisions are T52/T55/T58.
- Unloaded chunks are air for movement (Java says unknown/solid) so a crossing
  into the next view chunk is not blocked.
- Per-packet speed anti-cheat and Java's teleport-ack queue are deferred to
  T62; the current bounds reject non-finite values, out-of-world coordinates and
  moves over 512 blocks, and correction teleports use an incrementing id.
- The one-block step height is the block-grid auto-jump ceiling; Java exposes
  slope/wall angles instead.

### T52 — Block simulation: ticks, fluids and support

**Status:** done (session 40; parity follow-ups T52b–T52d done in session 41)
**Package:** `hytale-world`, `hytale-server`
**Goal:** random/block ticks, fluid flow, support propagation, falling blocks,
block health and drops.
**Acceptance:** scripted tick/fluid/support scenarios match Java; regions
remain Java-readable.

**Deviations**

- The core `BlockSection` tick queue, support check, random-tick procedures,
  default fluid ticker, falling-block movement and transient block health are
  native. The builtin `BlockTick`/`RandomTick`/`BlockPhysics`/`FallingBlocks`/
  `Fluid` module lifecycle remains T60.
- T52b closes the fluid gaps: partial-hitbox face coverage, waterlogging,
  finite/fire tickers, disabled-ticker config and collision conversion.
- T52c closes support/random-tick gaps: `MaxSupportDistance` chains,
  filler/self-face rules, chunk-boundary waiting, light/fluid gates, weighted
  placement, scheduled growth and the tick-suppression seam.
- Random-tick ordering still uses a deterministic Rust RNG, not Java's
  `SplittableRandom`; `GOLDEN_DATA.md` does not gate schedule-dependent order.
- Falling blocks are block movement, not ECS entities, and have no impact
  break/explode effect.
- T52d stores health/fragility in the Java `BlockHealthSection` component and
  migrates the legacy `BlockHealthChunk`; T57 landed the damage/replication packets.
- Simulation drops are reported but item entities/inventory pickup are T56.

**Follow-ups**

- T52b (fluid ticker parity), T52c (support/random-tick parity) and T52d
  (block-health storage) are done; T57 consumes the pending health deltas.
- Sound-event playback and builtin module lifecycle stay with T60.

### T53 — Entity stats, effects and combat basics

**Status:** done (session 44)
**Package:** `hytale-entities`
**Goal:** entity stat maps, effects, damage causes, health/death, basic combat.
Also load the `EntityStatType` module store for item utility/stat HUD payloads.
**Acceptance:** scripted damage/effect/death scenarios match Java; item stat
indexes resolve.

**Deviations**

- The stat/effect/damage cores, the `EntityStatType`/`EntityEffect`/
  `DamageCause` stores and a Java entity-gameplay fixture are landed
  (`testdata/golden/server/entity/entity-gameplay.json`). The
  `EntityGameplay` component seam bundles stat/effect/death state for T55/T59; no world/NPC producer attaches it yet.
- Full condition evaluation, damage calculators, effect DoT, knockback and
  animations stay T55/T59.
- `UpdateEntityStatTypes`/`UpdateEntityEffects` config-burst parity and
  per-entity stat/effect updates stay T59.

### T54 — Time and weather

**Status:** done (session 43)
**Package:** `hytale-world`, `hytale-server`
**Goal:** weather state/assets and transitions; completed time integration.
**Acceptance:** scripted weather transition and time scenario match Java.

### T55 — Projectiles and traversal

**Status:** done (session 45)
**Package:** `hytale-entities`
**Goal:** projectile spawn/flight/hit, basic mounts, parkour/mantling/crouch-slide
as scoped in `REMAINING.md`.
**Acceptance:** scripted projectile hit/miss and traversal scenarios.

**Deviations**

- Legacy `Projectile` assets drive the scripted flight; the new
  `ProjectileConfig`/`StandardPhysics` density/drag/buoyancy/rolling and
  `GroundFollow` physics are loaded/decoded but simulated later.
- Slide/roll/mantle are deterministic server-side models of Java's
  client-authoritative movement states; mounts are a rider/seat seam, not world
  entities yet.
- Projectile spawn/despawn packets, explosions, particles/sounds, prediction,
  pierce hitboxes, mount entities, traversal animation and anti-cheat remain
  T58/T59/T62.

### T56 — Prefab spawner and runtime entities

**Status:** done (session 46)
**Package:** `worldgen-hytale`, `hytale-world`, `hytale-server`
**Goal:** prefab block/fluid/environment paste (already), prefab entities and
block components, spawn/despawn lifecycle.
**Acceptance:** prefab scenario matches Java; save/reload preserves entities.

**Deviations**

- Prefab entity and block-component documents are preserved as JSON and the
  runtime store persists them in `entities.json`/`block-components.json`; the
  full Java `EntityStore`/`ChunkStore` ECS codec is not ported.
- Paste uses forced writes and does not model technical flags, paste events or
  block-settings flags. Entity order is prefab-file order (Java groups by
  column/hash order).
- Item drops with weighted drop lists stay unresolved; fixed-item simulation
  drops become item entities. Runtime entity spawn presentation landed in T57;
  projectile spawn/despawn and dynamic transforms remain T59.

### T57 — UI basics and block health presentation

**Status:** done (session 47)
**Package:** `hytale-world`, `hytale-server`, `tools/client-bot`
**Goal:** the minimal UI/world-map/compass/marker packets needed by M5 scenarios;
block health/breaking presentation.
**Acceptance:** client-visible state matches the server state in scripted runs.

**Deviations**

- Block health uses `UpdateBlockDamage`/`UpdateBlockDamages` for load snapshots
  and simulation deltas; the client-side breaking animation and partial-hit
  interaction loop remain later combat work.
- The world map streams settings and `BlockMapMarker` points of interest, but
  not map chunk images; discoverable markers ignore per-player reveal state.
- Runtime entities are static spawn presentations (no transform/animation
  tick); item metadata/quality are not yet carried in the spawn packet.
- Full `UpdateEntityStatTypes`/`UpdateEntityEffects`/`UpdateProjectileConfigs`/
  `UpdateWeathers` config-burst packets, particles/sounds and HUD windows remain
  T57b/T59/T60.

### T57b — Presentation follow-ups

**Status:** in progress (session 49)
**Package:** `hytale-assets`, `hytale-server`, `tools/client-bot`
**Depends on:** T57
**Agent fit:** low/medium

**Goal:** close the client-presentation gaps T57 deferred: partial-hit block
breaking, runtime entity lifecycle/metadata, the remaining entity/projectile/
weather config packets, map images and discovery, and the minimal HUD/window
packets needed by M5 scenarios. Producers remain with T58/T59/T60; T57b owns
the packet/encoder/UI contract and scripted client coverage.

**Acceptance:** partial hits and fragility, entity spawn/update/despawn,
full item metadata, all new config packets in the join burst, map
images/discovery and the M5 HUD/window packets are visible in scripted runs;
existing v1/T56/T57 gates stay green.

**Landed (session 49):** chunk world-map images with join/view-change load and
unload, plus persisted block-container inventories and the
`SetPage(Bench)`/`OpenWindow(Container)`/`CloseWindow` window round trip.
Bench-specific crafting windows stay with the builtin Crafting module (T60);
container item transactions and map refresh on block edits are follow-ups.

**Follow-ups:** full UI framework, editor/builder tools, machinima and
multi-server map state stay T61/later.

---

## M6 — Builtins and NPC

### T58 — NPC core: AI, pathfinding and flocking

**Status:** in progress (session 50)
**Package:** new `hytale-npc`
**Goal:** NPC archetypes, navigation/pathfinding, flocking/steering, perception.
**Acceptance:** scripted NPC navigation/flocking scenarios match Java.

**Landed (session 50):** new `hytale-npc` crate with role/flock document
catalogs, Java `SearchDirections`/steering/view-cone ports, a block-grid A*,
`PathFollower` and the `NpcDumper` fixture. T58b adds the role builder
expression subset and reference/parameter resolution, so all 1,147 roles
resolve to usable archetype fields. The full builder graph, half-step
motion-controller A*, wander/avoidance/group forces and blackboard sensors are
T58c.

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
6. M5 starts only after T48 and closes with T57b; M6 starts after T57b.
   T58 (the new `hytale-npc` crate) may run in parallel in a separate worktree,
   but T59/T60 presentation work waits for T57b. M7 after M6.
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
