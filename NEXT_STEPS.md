# Next steps — CoreServer rewrite, phase 2 (session 60)

**Status:** the first vertical slice is archived in
[`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md)
(tasks T01–T33, sessions 1–23). The phase-2 plan is [`plan.md`](plan.md) and
the task board is [`tasks/README.md`](tasks/README.md).

**Tasks landed:** **T53** (entity stats/effects/combat plus the Java
entity-gameplay fixture), **T54** (time and weather), **T55** (projectiles and
traversal), **T56** (prefab entities/block components plus the persisted
runtime entity store), **T57** (block-health replication, runtime entity
spawns and world-map basics) and **T58** (NPC core). **T59**'s first slice lands
the world spawn rules, role combat profiles, damage ranges and a scripted NPC
actor; **T59b** adds the live WorldThread NPC runtime, persistence, session
ticking, movement-mode spawn-position/environment selection, automatic
population ticks, block/fluid/light constraints and beacon/spawn-marker
catalogs plus `world-boot`/`serve --spawn-npc`. Automatic environment
enumeration, live beacon/marker controllers, spawn suppression and **T59c**
encounters remain.

## Current state

Landed and green:

- Protocol codegen, framing, QUIC/mTLS transport.
- P0 asset registry + deterministic ids; config packets `41`, `87`, `83`, `61`,
  `40`, `54`.
- T35 child-store goldens (26 stores, 21 payloads) with selector/closure support
  in `AssetDumper`; `BlockSet` and `GameplayConfig` gaps are recorded.
- T36 block/fluid child stores (`BlockBreakingDecal`, `BlockParticleSet`,
  `BlockSoundSet`, `FluidFX`, `SoundEvent` + audio deps); all five payloads are
  byte-identical to Java and `BlockType`/`Fluid`/`Environment` child indexes
  resolve.
- T37 item child stores (`ItemSoundSet`, `ItemQuality`, `ItemReticleConfig`,
  `ItemCategory`, `ItemPlayerAnimations`); all five payloads are byte-identical
  and the item quality/entity/sound/reticle/animation fields have zero residual
  differences over all 3,971 Java items.
- T38 `TagPattern` (196 assets, exact Java key/id parity) and
  `UpdateTagPatterns`; pattern tags join the deterministic `TagRegistry`.
- T39 final config order (21 packets after T57b), one encoded `ConfigAssets` shared by all
  sessions, `--validate-assets`, and client-bot packet recognition.
- T40 `hytale-core`: typed layered config, text/JSON logging init, async task
  scheduler with a shutdown deadline, ordered sync/async event bus, and the
  module-manifest registry seam.
- T41 universe/world lifecycle: persistent catalog (`universe.json`), name/UUID
  resolution, default world, create/load/save/delete, per-world
  `UniverseRuntime` threads, player load/save seam, and the `universe` CLI.
  Flat/Void/Dummy providers are empty-column stubs until M5.
- T42 persistent `serve` listener: one process-level universe/world thread,
  independent session tasks over the existing join state machine, shared cached
  config burst, connection limit, idle timeout, clean shutdown and the
  two-client disconnect/rejoin loopback test.
- T43 world clock and `GameplayConfig`: a Java-compatible day/night clock
  (segmented real-time rates, pause/resume, wrap and ISO-8601 game time), the
  internal eight-asset `GameplayConfig` store with tag-resolved
  `WorldGameplayConfig`, per-second `UpdateTime` broadcasts, and save/reload
  through `config.json`.
- T44 self-spawn appearance: saved `Model`/`PlayerSkin`/inventory decoding
  (modern components plus legacy `Inventory`), item-id validation with
  empty-slot fallback, the insecure handshake skin, and a Java equipment
  fixture checked by the real entity-store codec.
- T45 other-player tracking: one shared `PlayerRegistry` per served world,
  unique network ids, full player-list snapshots/add/remove, view-filtered
  entity spawn/movement/removal broadcasts, 10 Hz movement throttling, and a
  two-bot loopback test covering mutual spawn, movement, disconnect and rejoin.
- T46 block place/break: `MouseInteraction` left/right validation, item→block
  resolution, `WorldThread::{get_block,place_block,block_id_for_item}`, dirty
  chunk persistence, view-filtered `ServerSetBlock` broadcasts, and a two-bot
  placement/break loopback.
- T47 disconnect/reconnect/teardown: per-session registered views with shared
  loaded-chunk ownership, `ClientDisconnect` handling, per-UUID player snapshot
  and transform save on every exit path, registry/view/entity cleanup, a
  reconnect loopback that restores the saved transform and edited block, and an
  idle-timeout teardown test.
- T48 real-client trace/v1 gate: `hytale-net::trace` (JSONL packet
  ids/channels/order/hashes), `serve --trace`, a committed normalized
  join/move/edit/disconnect/reconnect fixture, a live trace replay gate, a fast
  registry fixture test, and the real-client capture runbook with the
  `ViewRadius`-on-Chunks constraint recorded.
- T49 inventory: new `hytale-inventory` crate with Java-compatible item stacks,
  six player containers, active slots and atomic move/split/merge/swap/drop/
  pickup; `PlayerData` inventory save/load; initial inventory/active-slot
  packets; authoritative held-item consumption on placement; inventory saves
  with the transform; a seeded hotbar loopback proving `×5` → place → `×4` →
  reconnect restores `×4`.
- T50 interactions: new `hytale-interactions` crate loading root/interaction
  assets with `Parent` inheritance, structural graph walking, interaction vars,
  block metadata extraction, door/state/container outcomes, and `WorldThread`
  state-variant changes. Right-click `Use` dispatch now toggles a placed door
  before falling back to placement; tests cover the graph, real pack load and a
  door loopback.
- T51 movement authority: internal `MovementConfig` loading with the Java
  defaults, `UpdateMovementSettings` on join, block collision shapes from
  `BlockType`/`BlockBoundingBoxes` (64 rotations + filler origins), the
  `hytale-entities::movement` AABB sweep/step/gravity integrator, bounded
  `ClientMovement` validation, authoritative collision corrections via
  `ClientTeleport`, and a ladder-free walk/fall/collide loopback that stays
  rubber-band free.
- T52 block simulation: `BlockSection` scheduled-tick queue, deterministic
  random ticks, support loss (`Break`/`Destroy`/`Fall`), one-cell falling
  blocks, water/lava flow, native gathering drops and block health; the server
  runs one simulation pass per 50 ms and broadcasts block/fluid changes.
- T52b fluid parity: partial-hitbox face coverage, waterlogging, finite/fire
  tickers, `DisabledFluidTickers`, collision conversion and Java ticker-field
  inheritance. A Java `FluidTickerProbe` fixture covers hitbox/ticker fields.
- T52c support/random-tick parity: `MaxSupportDistance` chains, filler/self-face
  rules, `WAITING_CHUNK`, `SpreadTo` light/fluid/revert gates, weighted
  `PlaceBlock`, scheduled chance growth and the tick-suppression seam.
- T52d block-health storage: per-section Java `BlockHealthSection` v0 BSON,
  legacy `BlockHealthChunk` migration, fragility countdown and pending health
  deltas for T57. A Java `BlockHealthDumper` fixture gates the codec.
- T53 (done): `hytale-entities` has the stat type/value/map core (clamp, static
  min/max modifiers, regeneration incl. `ClampAtZero`, reset behavior), active
  entity effects with overlap/expiry/invulnerability, and the damage-cause/
  resistance/outgoing/health/death core. `Server/Entity/Stats`,
  `Server/Entity/Effects` and `Server/Entity/Damage` load with Java ids; the
  `DamageCause` parent rule inherits only `DeathMessageKey`. Item
  `Utility.EntityStatsToClear`/`DisplayEntityStatsHUD` names resolve to
  deterministic stat ids. `EntityGameplay` is the component seam for T55/T57/
  T59c, and `testdata/golden/server/entity/entity-gameplay.json` gates all 13
  stats, 260 effects and 20 damage causes against Java. Client packets,
  conditions, damage calculators and DoT remain T59c.
- T54 (done): `Server/Weathers` loads with `Unknown` at id 0;
  environment forecasts are parsed from the real 24-hour tables;
  `hytale_world::weather` selects deterministic weather per hour, honors
  `ForcedWeather`, and exposes the effective index through `WorldThread`.
  `WorldClock` now has current-hour and moon-phase helpers. Sessions send
  `UpdateWeather` on join and when the effective weather changes. Full
  `UpdateWeathers` asset-document parity and presentation remain T60
  follow-ups.
- T55 (done): `hytale-entities::projectile` launches from the real legacy
  `Projectile` assets (86) with Java angle/offset math, gravity, terminal-speed
  cap, swept block/entity AABB collision, bounce/stick/rest, hit/miss/death
  timers, TTL and the T53 damage handoff. `ProjectileConfig` (188) and its
  `Standard`/`GroundFollow` physics load as the new-system seam. Movement gains
  the slide/roll/mantle traversal state and a `MountState` rider seam. The
  Java entity fixture now gates both projectile stores; `--validate-assets`
  reports 27 stores. Packets, explosions, prediction and new-physics
  density/drag remain T59c.
- T56 (done): prefab parsing now keeps block `components` and entity
  `Components`; `hytale-world::runtime` owns stable-id runtime entities and
  block components with versioned atomic JSON sidecars, and
  `prefab::paste_prefab` applies rotated blocks/fluids/fillers/support,
  components and entity spawns. `WorldThread` exposes `place_prefab`,
  entity/component queries and `spawn_item_entity`; fixed-item simulation drops
  become item entities. The real `Hunt.Generic.Site` paste (10 blocks,
  1 component, 3 entities) survives save/reopen. Entity/despawn packets,
  weighted drop lists and the full Java ECS codec remain follow-ups.
- T57 (done): block-health load snapshots ride the chunks stream after each
  chunk; simulation damage/regeneration broadcasts `UpdateBlockDamage(s)` with
  Java section indexes and absolute health; replacing a block clears its
  health/fragility. Joining clients receive prefab/item runtime entity spawns,
  `DropItemStack` creates persisted item entities, and the world map gets
  `UpdateWorldMapSettings` plus `BlockMapMarker` points of interest. The
  ignored dev-join loopback verifies the marker and three prefab entity spawns.
  Map chunk images, discovery/reveal, dynamic entity transforms and the
  remaining presentation packets are T57b.
- T57b (in progress, session 49): the M5 presentation follow-up now encodes
  `UpdateEntityStatTypes` (72), `UpdateEntityEffects` (51),
  `UpdateProjectileConfigs` (85) and `UpdateWeathers` (47) in the 21-packet
  cached config burst, carries the full item stack payload
  (durability/quality/metadata) in item entity spawns, and delivers runtime
  entity lifecycle events (spawn/update/despawn) to viewers. Partial-hit block
  breaking is supported through `SessionOptions.block_break_damage` (default
  1.0), other-player map markers follow joins/moves/leaves, and discoverable
  block markers are revealed/persisted. Session 49 adds 96×96 palette map
  images for the loaded view with `UpdateWorldMap.chunks` load/unload diffs,
  and persisted block-container inventories with the
  `SetPage(Bench)`/`OpenWindow(Container)`/`CloseWindow` round trip. Still
  open: bench crafting windows (T60 builtin), container item transactions,
  map refresh on block edits and particle/weather presentation.
  Producers stay with T58/T59/T60; T57b should land before M5 is declared fully
  closed.
- T58 (done, session 50): the new `hytale-npc` crate loads the real
  `Server/NPC/Roles` documents (1,147 unique keys) and all 8 `Server/NPC/Flocks`
  definitions, selects range/weighted flock sizes with injected RNG, filters
  blocked roles, and ports `Steering` plus pursue/evade/rotate. Navigation has
  the Java `SearchDirections` tables, a block-grid A* with Java node limits and
  a nearest-node fallback, `PathFollower` waypoint/rejection logic and the
  `isInViewCone` perception test. `NpcDumper` (`run-npc.sh`) gate the pure
  direction/steering/view-cone outputs. T58b adds the role builder expression
  subset, reference-chain/parameter merging and `ResolvedRole`; all 1,147 roles
  resolve, and the expression oracle matches Java `Expression`/`StdScope`. The
  full builder graph, half-step motion-controller A*, wander/avoidance/group
  forces and blackboard sensors move with the builtin NPC module (T59/T60).
  T58c adds `WorldGrid` real-world walk/fly planning and the Java
  `GroupSteeringAccumulator`; T58 is done. Spawning/combat/ECS start T59.
- T59 (in progress, sessions 51–59; T59b carries the runtime slices): `hytale-npc::spawn` loads all 98
  `Server/NPC/Spawn/World` documents (271 role entries), applies the Java
  scaled-day/moon gates, moon-phase weight modifiers, flock-minimum missing
  count and weighted selection. `WorldClock` gained the matching
  day-progress/scaled-time/range helpers. `hytale-npc::combat` resolves
  role combat fields and `InteractionVars` damage calculators, builds the real
  `EntityGameplay` stats, and drives a scripted `NpcActor`
  (target acquisition, A* chase, melee, damage, death). Role `Modify` blocks
  are now flattened, so variant health/attack fields resolve correctly. The
  `NpcDumper` fixture gates `DamageCalculator.computeDamageRange`.
- T59b (in progress, sessions 52–60): `WorldThread` owns a live NPC actor map with
  `spawn_npc`/`despawn_npc`/`damage_npc`/`set_npc_targets`/`tick_npcs`, rebuilds
  actors and health from `entities.json` on boot, writes health before save and
  turns movement/death into runtime entity events. Sessions publish registered
  player positions as targets and tick NPCs with the game loop. The second
  slice adds walk/wade/underwater/dive/fly spawn-position search,
  environment/role weighted selection and the density/expected-count
  controller. The third adds explicit population targets and
  `tick_npc_spawns` with session-loop wiring; the fourth discovers segment
  counts from loaded world sections; the fifth resolves the real
  `SpawnBlockSet` catalog to allowed floors, the sixth resolves `SpawnFluidTag`
  through the fluid tag index, the seventh evaluates `LightRanges`, the eighth
  loads beacon/spawn-marker catalogs and the ninth adds
  `world-boot`/`serve --spawn-npc`. Automatic environment enumeration, live
  beacon/marker controllers, spawn suppression and the client-bot scenario
  remain T59b/T59c.
- Runtime chunks, BSON chunk/world/player storage, world config, save boot,
  demand loading, world thread and Java-ordered chunk packets.
- Dev/insecure and local-EdDSA authenticated join; common/config asset
  streaming; Game transition; player list/self-spawn; time/gameplay packets;
  teleport exchange; persistent chunk stream and movement-driven view updates.

```sh
./tools/test.sh fast    # inner loop: unit + non-asset integration (~8–21 s here)
./tools/test.sh unit    # pure unit tests only
cargo test --workspace  # full release gate (asset-backed suites included)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

See [`TESTING.md`](TESTING.md) for the fast asset/golden (`assets`), slow
worldgen/world-boot (`slow`) and serial E2E tiers.

## Phase 2 sequence

1. **M2 — asset completion (done):** T35 → T36/T37/T38 → T39.
2. **M3 — runtime:** T40 (done) → T41 (done) → T42 (done; parallel with M2 after T34).
3. **M4 — v1:** T43 (done) → T44 (done) → T45 (done) → T46 (done) → T47 (done)
   → T48 (done). M4/v1 is closed.
4. **M5/M6/M7:** gameplay, builtins/NPC, hardening (see [`plan.md`](plan.md));
   T55 (projectiles/traversal), T56 (prefab/runtime entities), T57
   (block-health/runtime entity/world-map presentation) and T57b (presentation
   follow-ups) are done. M6 starts with T58 (NPC core), whose first slice and
   T58b builder-expression and T58c world-grid/group-steering slices are
   landed. T58 is done. T59's first slice (spawn configuration/selection,
   combat profiles/damage and a scripted actor) and T59b's runtime plus
   spawn-position/controller/population slices (WorldThread NPC
   store/persistence/ticking, movement-mode position search, environment
   selection, density accounting, explicit population ticks, loaded-section
   segment discovery, real `BlockSet` floor resolution, fluid-tag resolution,
   light ranges, beacon/marker catalogs and `--spawn-npc` CLI coverage) are
   landed; T59b's automatic environment enumeration, live beacon/marker
   controllers/suppression and the client-bot scenario, plus T59c (combat
   action graph and encounters), are next. T59/T60 presentation work waits for
   T57b.

## Try it

The phase-1 commands still work:

```sh
cargo run -p hytale-server -- --validate-assets
cargo run -p hytale-server -- universe --root target/universe create Alpha --seed 1
cargo run -p hytale-server -- universe --root target/universe list
cargo run -p hytale-server -- serve --universe target/universe --bind 127.0.0.1:5520 \
  --trace target/trace/v1.jsonl --spawn-npc Rat
cargo run -p hytale-server -- headless --save target/headless-world --chunk 4,-5
cargo run -p hytale-server -- world-boot --save target/headless-world \
  --view-radius 1 --chunks-per-tick 4 --set-block 129,250,-159,4 --spawn-npc Rat
tools/java-dumper/run-save-check.sh target/headless-world 4 -5 129 250 -159 Debug_Test_Block

# authenticated join (the server prints the generated PKCS#8 key):
cargo run -p hytale-server -- chunk-serve --handshake --auth-mode local \
  --save target/auth-join-smoke --bind 127.0.0.1:5531 --view-radius 0
cargo run -p client-bot -- --host 127.0.0.1:5531 --auth-join \
  --auth-key "<pkcs8 base64 printed by the server>" --username AuthBot \
  --view-radius 0 --asset-limit 2 --move-chunk 1,0 --timeout 60
```

## Working rules

- One task per branch/worktree; shared files (`Cargo.toml`, `plan.md`,
  `NEXT_STEPS.md`, `tasks/README.md`) are main-agent-owned.
- Golden-first: if a task needs expected bytes and no fixture exists, extend the
  Java dumper instead of inventing values.
- Generated code is write-only.
- No broad refactors; finish the task’s acceptance section and report
  follow-ups.
- Update [`PORTING_HISTORY.md`](PORTING_HISTORY.md) at the end of each session.
