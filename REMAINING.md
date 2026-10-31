# REMAINING.md — Deferred scope (CoreServer rewrite)

This file records what the Rust server deliberately does **not** implement in
v1 and where each deferred item should land. It is a scope guard, not a bug list.
The v1 contract is the M4 exit in [`plan.md`](plan.md); later deferred scope is
tracked by M5–M7 there. Phase-2 child-store work (T35–T39) is **not** deferred;
it is part of M2 and listed in [`tasks/README.md`](tasks/README.md).

## 1. Gameplay systems

Deferred to M5/M6:

- NPC AI, pathfinding, flocking, spawning and combat (`NPC/`, 1,008 files).
  T58 lands the role/flock document catalogs, Java direction tables, block-grid
  A*, `PathFollower`, pursue/evade/rotate steering, group steering,
  view-cone perception and resolved role expressions; T59 adds the world spawn
  configurations/time/moon/flock selection, role combat profiles, Java damage
  ranges and a scripted `NpcActor`; T59b adds the live `WorldThread` NPC
  store, spawn/despawn/damage/tick commands, persistence, session ticking,
  movement-mode spawn-position search, environment/role selection,
  density/expected-count accounting, explicit population ticks and
  loaded-section environment segment discovery, real `BlockSet` floor
  resolution, `SpawnFluidTag` resolution, spawn `LightRanges` and beacon/
  spawn-marker catalogs and `--spawn-npc` CLI coverage; the final T59b slice
  adds automatic environment enumeration (`--auto-population`), NPC-group
  spawn suppression, live marker/beacon controllers with persisted cooldowns
  and the client-bot spawn/chase/despawn scenario. T59c adds the
  `ActionAttack`/`DamageEntity` combat path, the complete
  `Server/EncounterManager` catalog/evaluator, boss-bar/audio/weather/portal
  seams and live world-thread encounter commands. T60b adds the role
  instruction graph, blackboard, Walk/Fly/Dive motion controllers, a
  `bevy_ecs` actor/blackboard/motion schedule and target-bound encounter
  actions (invulnerability/role/release/effect). Some rarer builder
  actions/conditions and deeper instance lifecycle policy remain follow-ups. T60 lands
  the builtin module runtime for the selected block/weather/NPC modules.
- Player combat, damage, stats, status effects, stamina beyond T53's core:
  T53 lands the stat/effect/damage models, the Java asset fixture and the
  `EntityGameplay` component seam; T57 lands block-health/runtime-entity/map
  presentation and T59 resolves NPC combat profiles/damage. T60b adds
  per-session player health, NPC→player damage, stat-index-keyed
  `EntityStatsUpdate` replication (including per-entity NPC snapshots),
  real effect/DoT ticking with `EntityEffectsUpdate`, health persistence and
  automatic respawn. Condition evaluation, death-screen/configured-spawn
  policy and the remaining effect/condition packets remain follow-ups; T59c
  lands the NPC damage-calculator/`DamageEffects` action graph.
- Crafting beyond T60b's item/salvage recipe catalog, resource-type inputs,
  bench windows, atomic transactions and bench timer queue: knowledge/memories
  gating, bench upgrades/categories and item quality/shops/rewards.
- Projectiles beyond T55's deterministic core (legacy flight, block/entity hit,
  bounce/stick/TTL, new-config seam): density/drag/buoyancy/rolling and
  `GroundFollow` physics, predicted/pierce/DirectionalHitBox projectiles,
  explosions, particles/sounds and projectile spawn/despawn packets (T59).
- Mounts, parkour, mantling, crouch-slide and safety roll beyond T55's
  slide/roll/mantle state machine and rider/seat seam: world mount entities,
  traversal animation/anti-cheat and the builtin parkour checkpoints
  (T58/T59/T60/T62).
- Inventory windows/UI, block containers and item pickup/drop transactions
  beyond the T49 item-stack/container/transaction core and T56's persisted item
  entities (hotbar/utility/armor/storage/tools/backpack, active slots,
  held-item consumption and save/reconnect are landed; fixed-item simulation
  drops spawn runtime item entities, block-container inventories persist and the
  minimal `OpenWindow`/`CloseWindow` round trip is landed by T57b; T60b adds
  player↔window `MoveItemStack` routing and bench windows. Weighted drop lists
  and the remaining transaction/UI edge cases remain follow-ups).
- Interaction graph beyond the T50 block `Use`/door/state core: item and entity
  roots, selectors, effects/sounds, cooldowns and client-chain sync remain
  follow-ups; container and bench windows are landed by T57b/T60b.
- Block physics/ticking/fluids beyond T52/T52b/T52c: sound playback remains
  deferred. T60 lands the builtin `BlockTick`/`RandomTick`/`BlockPhysics`/
  `FallingBlocks`/`Fluid` module lifecycle and switches; weather-driven fire
  remains deferred. T57
  landed the block-health load/delta packets and fragility clearing; T57b adds
  partial-hit breaking and the window/map-image presentation. T52b/d
  cover partial-hitbox fluid coverage, waterlogging, finite/fire tickers,
  disabled tickers,
  `MaxSupportDistance` chains, filler/self-face support, light/fluid gates,
  weighted placement, scheduled growth and `BlockHealthSection` storage.
  Item-entity drops are T56; map marker presentation is T57.
- Player movement beyond the T51 authority core and T55 traversal state: the
  server validates finite/world bounds, resolves solid-block collision
  (sweep + one-block step), sends correction teleports and exposes a
  deterministic gravity/jump/fly/climb/slide/roll integrator with a mantle
  helper. Per-packet speed/teleport anti-cheat remains T62; fluids, triggers,
  world mount entities and entity collisions remain T52/T56/T58.
- Instance/portal transports and adventure plugins. T59c lands the encounter
  evaluator and portal-timer seam; T60b lands the portal-type/instance catalog,
  target-world creation/selection, saved-player transfer and the live session
  handoff. Portal return points restore the source transform and strip
  ephemeral items, entry counts enforce `AllowedEntryCount`, and
  `close_instance_if_empty` honors `CloseWhenEmpty`; deeper instance
  removal/lifecycle policy and adventure plugins remain deferred.
- Full world map UI/image provider and live map rendering beyond T57's settings,
  `BlockMapMarker` points of interest and T57b's per-chunk palette images
  (later).
- Weather presentation beyond T54's state/forecast/transition core: particles/
  screen effects/audio and the editor preview lock (later); the
  `UpdateWeathers` config packet is landed by T57b.

Deferred to M6:

- The remaining `builtin/` plugins (54 of 62) beyond T60's selected
  `BlockTick`/`RandomTick`/`FallingBlocks`/`BlockPhysics`/`Fluid`/`Weather`/
  `NPC`/`EncounterManager` set. `builtin/Crafting` bench windows and the
  remaining adventure plugins stay deferred.
- `builtin/HytaleGenerator` (density/assignment worldgen, 783 files) — separate
  engine, not part of the classic `WorldGenerator` port.
- Editor, AssetEditor, BuilderTools, machinima, voice, cosmetics.

## 2. Java plugin compatibility

- The Rust server does not load Java plugins or transform bytecode.
- Asset-only mods (packs with `manifest.json` and assets) are supported.
- Code plugins use a Rust-native API. A WASM or dynamic-library ABI is a later
  decision.
- A JVM sidecar/bridge is explicitly out of scope; adding it requires a scope
  change.

## 3. Storage providers

- v1 target: vanilla `IndexedStorage` only.
- RocksDB storage provider: deferred. Add an importer/migration tool only if a
  real deployment needs it.
- Storage-format migrations from older Hytale versions: deferred until the v1
  read/write path is stable.

## 4. Auth and services

- Telemetry, Sentry, discovery registration, update checks and live-config
  service integration: deferred.
- Singleplayer integration (client-spawned server, child sessions): deferred to
  M5/M6; the auth trait keeps the door open.
- Hosting-provider external session mode: deferred unless a user needs it.
- Live OAuth device flow, game-session creation and session-service JWKS:
  implemented in `hytale-auth::live` (`--auth-mode authenticated`), including
  `/game-session/new`, `/server-join/auth-grant`, `/server-join/auth-token`,
  live EdDSA/JWKS validation and the `cnf.x5t#S256` client-certificate binding.
  Still deferred: the browser PKCE flow, encrypted credential persistence,
  automatic token-refresh scheduling, and hosting-provider external-session
  mode. Until persistence lands, restarting the server requires a new device
  authorization (or `HYTALE_SERVER_SESSION_TOKEN` +
  `HYTALE_SERVER_IDENTITY_TOKEN`).

## 5. UI and client-facing extras

- Full window/page/UI system.
- World map, compass, markers, zone discovery UI.
- Machinima/camera sequences and voice chat.
- Server player list UI.
- Cosmetics, emotes, skin rendering beyond the join payload.

## 6. Worldgen integration gaps carried over

The worldgen port intentionally skipped these; CoreServer needs some of them.

- `WorldGenerator/builtin/worldgen/modifier` (34 files) and `ModifyEvents`
  dispatch. Needed for custom cave/cover/fluid/layer/prefab/tint content and
  ore modifiers. Port in M3 or make the modifier seam explicit, then port.
- Prefab entities and block components are landed by T56 as runtime stores
  with JSON sidecars. The full Java `EntityStore`/`ChunkStore` ECS BSON codec
  (and therefore Java-readable runtime entity saves) remains deferred; the
  sidecars preserve the original `Components` documents.
- `.lpf` binary prefab cache — not needed for v1; JSON is the source of truth.
- `BiomeDataSystem` player zone/biome tracking — needed for map/UI in M5.
- World map UI (the offline PNG renderer in `worldgen-cli` is enough for tests).

## 7. Operational features

- Hot reload of Rust code plugins.
- Asset file watcher / live reload (asset packs may get a watcher early if
  cheap; not required for v1).
- Multi-server clustering, cross-server transfer, referrals beyond the basic
  packet.
- Metrics endpoint, profiling UI, admin web UI.
- Windows/macOS packaging and service installers.

## 8. Performance work

v1 must be correct first. Deferred until the vertical slice works:

- SIMD/chunk meshing or rendering-side optimizations (server has no rendering).
- Chunk packet batching beyond the basic channel stream.
- Zero-copy asset loading.
- Storage compaction beyond `IndexedStorageFile`'s own behavior.
- Cross-world chunk cache sharing.

## 9. Non-goals

- Bit-exact parity with a *live* server's schedule-dependent block id order.
- Java ECS API parity (`Holder`, `Ref`, `ComponentRegistry` as public API).
- Binary compatibility with Java plugin jars.
- Reproducing Java class files, package names, or reflection behavior.
- Porting the 15k-line Java `Codec` library.

## 10. Real-client trace constraints (T48)

These were observed in a real-client log captured against a local server and are
encoded in `testdata/golden/server/trace/v1-lifecycle.jsonl` /
`tools/client-bot/tests/trace_fixture.rs`:

- `ViewRadius` (packet id `32`) must be written on the **Chunks** stream after
  `ClientReady`; writing it on the `Default` stream disconnects the client during
  loading with `Packet ViewRadius (ID 32) is not valid on channel Default`.
- The client handles the config asset burst and `UpdateTranslations` before
  `JoinWorld`; `JoinWorld` is processed in the `Playing` stage.
- `localhost` resolves to `::1` first on many hosts. A server bound only to
  `127.0.0.1`/`0.0.0.0` never sees the client's Initial packets and the client
  reports a 20-second `QUIC handshake failed` with no server trace records.
  `serve` now binds the other loopback family by default; a wildcard IPv4 bind
  needs `--bind [::]:PORT` (or the client must dial `127.0.0.1`).
- The initial chunk snapshot is generated before it is streamed, so a cold
  world with a large `--view-radius` can exceed the client's load timeout.
  Prefer radius 0-2 for the first join; a warmed world reconnects quickly.
- A release client now authenticates and enters `SettingUp`; it still fails in
  `WorldLoadFinished` while config-asset stores are incomplete. That is a
  separate asset-packet gap tracked by `tasks/T66-live-client-assets.md`, not
  an authentication failure.
- The setup flow observed by the client is `AuthGrant` → `ServerAuthToken` →
  `RequiredAssets` → config asset handlers → `JoinWorld`.
- A default online client expects the live Hytale session authority. The local
  EdDSA profile is intentionally offline/dev-only until the live session
  provider adapter lands; the trace runbook documents the dev/insecure profile.
- A full automated vanilla-client run (connect/spawn/move/edit/disconnect/
  reconnect) is not available in this environment. `serve --trace` and
  `testdata/golden/server/trace/README.md` are the capture seam; replace the
  bot baseline when a client capture can be made.
