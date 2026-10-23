# REMAINING.md — Deferred scope (CoreServer rewrite)

This file records what the Rust server deliberately does **not** implement in
v1 and where each deferred item should land. It is a scope guard, not a bug list.
The v1 contract is in [`plan.md`](plan.md) §3.

## 1. Gameplay systems

Deferred to M5:

- NPC AI, pathfinding, flocking, spawning and combat (`NPC/`, 1,051 files).
- Player combat, damage, stats, status effects, stamina.
- Crafting, recipes, item quality, drops, shops, rewards.
- Projectiles, mounts, parkour, mantling, crouch-slide, safety roll.
- Full inventory containers/transactions/UI.
- Interaction graph beyond basic block place/break.
- Block physics/ticking/fluids beyond the minimum needed for a stable world.
- Instances, encounters, portals, adventure plugins.
- World map UI, markers, live map rendering.

Deferred to M6:

- `builtin/` plugins (62 manifests, ~249k lines).
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
- Live OAuth device/browser flow, session-service registration and JWKS refresh:
  deferred. The current profile validates the same EdDSA JWT claims and
  `cnf.x5t#S256` certificate binding against a locally configured Ed25519 key,
  so the handshake and claim checks are real but the authority is local.

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
- Prefab entities and block components (ECS state) — prefab spawner needs them
  in M5; v1 only pastes blocks/fluids/environments.
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
