# T57b — Presentation follow-ups

**Status:** done (session 49; residual presentation gaps deferred)
**Package:** `hytale-assets`, `hytale-server`, `tools/client-bot`
**Depends on:** T57
**Agent fit:** low/medium

## Goal

Close the client-presentation gaps that T57 deliberately deferred. T57 landed
block-health packets, runtime-entity spawns and the minimal world-map marker
path; this packet owns the remaining packet/config/UI parity. Content producers
for NPCs, projectiles and weather remain with T58/T59/T60, but their
presentation and asset-packet contract lives here.

## Deliverables

- Block interaction presentation:
  - Partial-hit block-health feedback and the client breaking interaction loop
    (damage/delta updates while a block is being broken, instead of the current
    full-health-then-replace break).
  - Fragility/health clearing and presentation around placed/broken blocks.
- Runtime entity lifecycle:
  - Transform/animation/despawn broadcasts for item, projectile and NPC runtime
    entities; stable network ids already exist from T57.
  - Item entity payloads with durability, quality and metadata
    (`ItemWithAllMetadata`), not just id/quantity.
  - A visible-entity registry so spawn/despawn/update filtering follows the
    viewer's chunk view as entities move.
- Config-burst asset packets:
  - Encode and load `UpdateEntityStatTypes`, `UpdateEntityEffects`,
    `UpdateProjectileConfigs` and `UpdateWeathers` with the existing
    dependency-ordered config cache.
  - Golden/semantic tests against the Java asset inventory; update
    `--validate-assets` store counts.
- World map/UI:
  - Map chunk image generation (`UpdateWorldMap.chunks`) and unload/removal.
  - Discoverable marker reveal state per player (block/POI/player markers) and
    `UpdateWorldMap` add/remove diffs; compass/player tracking.
  - The minimal HUD/window packets needed by M5 scenarios (container/bench
    windows and block-health display); no full UI framework.
- Weather/effects presentation:
  - Weather particles/screen effects/audio and the editor preview lock.
  - Particle/sound playback needed by the existing projectile/block-health
    consumers.
- Client-bot coverage for every new packet path and an ignored scripted
  loopback for partial-hit breaking, entity updates and map markers.

## Acceptance

```sh
./tools/test.sh fast
./tools/test.sh assets
./tools/test.sh slow
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A partial block hit sends the expected damage/delta and the client can finish
  the break; fragility is cleared on replacement.
- Runtime item/projectile/NPC entities spawn, update and despawn for viewers;
  item entities carry their full metadata.
- Every new config packet decodes and appears once in the cached join burst in
  dependency order.
- Map images/markers and reveal state match the server state in a scripted
  join.
- Existing v1, T56 and T57 gates stay green.

## Landed (session 48 — first slice)

- `hytale-assets` encodes `UpdateEntityStatTypes` (id 72),
  `UpdateEntityEffects` (id 51), `UpdateProjectileConfigs` (id 85) and
  `UpdateWeathers` (id 47) from the loaded stores, resolves effect removal and
  projectile sound ids, and includes them in the cached config burst between
  `ItemPlayerAnimations` and `Item` (21 packets total).
- `--validate-assets` validates/decode-checks all four packets; the
  config-order tests and client-bot config recognition cover the new ids.
- Item entity spawn presentation now carries the full stack payload
  (`Durability`, `MaxDurability`, `Quality`, `OverrideDroppedItemAnimation`,
  `Metadata`) from `DropItemStack`, and `runtime_entity_updates` maps it into
  `ItemWithAllMetadata`.
- Partial-hit block breaking: `SessionOptions.block_break_damage` (default
  `1.0`, preserving one-hit breaks) applies block health per left click; a
  partial hit queues one immediate `UpdateBlockDamage` packet and keeps the
  block until health reaches zero. The immediate delta is consumed so the next
  simulation tick does not replay it. The door loopback exercises a `0.5` hit.
- Other-player map markers: joins, 10 Hz movement and leaves now update
  `UpdateWorldMap` with `Player.png` markers carrying the player UUID/name.
  Markers are global (not view-filtered), matching the Java player marker
  provider. The player registry unit test covers join/move/remove.
- Marker discovery/reveal: discoverable `BlockMapMarker`s within 32 blocks are
  added on join/tick and the id set persists through the player save
  (`RustRevealedMarkers`, a typed placeholder for Java `PlayerWorldData`).
  Non-discoverable markers are always visible; previously revealed markers are
  restored on reconnect.
- Runtime entity lifecycle: `WorldThread` queues `RuntimeEntityEvent`
  Spawned/Updated/Despawned values for prefab, item and explicit despawn
  commands; `update_entity_transform` moves an entity and queues an update.
  Sessions drain the queue and broadcast spawn/update/removal to viewers; the
  join snapshot consumes existing events so entities are not spawned twice.
- New `crates/hytale-assets/tests/entity_packets.rs` gates the stat/effect/
  projectile/weather payloads; `players` unit tests gate the full item payload.

**Still open after session 48:** map chunk images and HUD/window packets. Projectile
physics/visual subfields and the full weather document
(clouds/colors/particles/fog) are mapped through the core fields only. The
stat/effect slice maps the core fields; application effects, model overrides,
min/max stat particles and full damage-calculator presentation remain part of
the T59 producer work.

## Landed (session 49 — second slice)

- Map chunk images: `hytale-server::map_image` renders one 96×96 palette image
  per loaded chunk from the legacy heightmap, column tint, `BlockType`
  tint/particle colours and top fluid layer, with height shading and water
  blending. `WorldThread::map_chunk_image` caches images per chunk and
  invalidates them on block/fluid/prefab changes.
- Joins and view changes send `UpdateWorldMap.chunks` for every loaded chunk in
  the session view and `MapChunk { image: None }` for images that leave it.
  Images are batched (32 per packet) so large views stay under `MAX_FRAME`.
- Block container windows: `hytale-world::containers` persists per-block
  `SimpleItemContainer` inventories in `block-containers.json`; container
  capacity/block-item data comes from `BlockEntity.Components.ItemContainerBlock`
  and `BlockType.container_item`. A right-click `OpenContainer` sends
  `SetPage(Bench)` + `OpenWindow(Container)` with the live `InventorySection`
  and switches the block to its `OpenWindow` state; `CloseWindow`/teardown send
  the close packets and restore `CloseWindow`.
- `hytale-world` state variants preserve block components and container
  inventories, so opening/closing a chest no longer drops its contents.
- `client-bot` records `UpdateWorldMap.chunks`, `SetPage`, `OpenWindow`,
  `UpdateWindow` and `CloseWindow`, and the ignored container loopback verifies
  the window/page/image payloads. New unit/loopback gates:
  `hytale-world::containers`, `world_thread::{map_chunk_images,block_containers}`,
  `map_image::tests` and `block_interactions::container_use_...`.
- `GameReport`/`GameLoopReport` expose `map_chunks`, `map_chunks_unloaded`,
  `windows_opened` and `windows_closed`.

**Residual gaps (deferred):** T60b closed bench-specific crafting windows and
container `MoveItemStack` transactions after this packet. Still open and left
with the owning systems: window distance validation, map-image refresh on block
edits instead of only on view changes, and byte-identical map images (the
generator follows the Java palette/silhouette contract; see `GOLDEN_DATA.md`).
Particles/sounds and full weather presentation remain with T59/T60 and later.
The T57b stat/effect/projectile/weather config packets and item metadata are
landed.

## Ownership split

- **T57b:** packet/API/encoder/UI presentation and scripted client coverage.
- **T59:** stat/effect/damage calculators, combat producers and projectile/NPC
  content that feeds T57b's packets.
- **T60:** selected builtin module runtime, weather content and map/UI modules.
- **T62:** packet-rate and spawn-abuse limits for the new broadcasts.

## Follow-ups

- Full window/page/UI framework, editor/builder tools and machinima remain
  T61/later.
- Multi-server map/UI state and external map services remain out of scope.
