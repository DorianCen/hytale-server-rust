# T56 — Prefab spawner and runtime entities

**Status:** done (session 46)
**Package:** `worldgen-hytale`, `hytale-world`, `hytale-server`
**Depends on:** T45, T50 (plus the prefab parser from the worldgen port)
**Agent fit:** low/medium

## Goal

Finish the prefab path the world generator deliberately left stubbed: parse
prefab entities and block components, paste a prefab into a live world, and own
a persisted runtime entity/block-component store. The same store is the landing
point for the item and projectile entities deferred by T49/T52/T55. Client
presentation packets stay in T57/T57b.

The Java oracles are `BsonPrefabBufferDeserializer` (entity holders and
`components` state), `PrefabUtil.paste` (rotation, forced block writes, filler
components, entity position rotation) and `BlockComponentSection` /
`EntityStore` persistence. The Rust port keeps the same *values* as JSON on
disk instead of porting the full ECS codec (documented deviation).

## Deliverables

- `worldgen-hytale`:
  - `PrefabBufferBlockEntry.components` carries the prefab block's
    `"components"` document instead of only counting it.
  - `PrefabEntity` (local `Transform` position/rotation + the full `Components`
    document) is parsed from `"entities"` and baked with the prefab anchor.
  - `PrefabBuffer::entities` and an accessor iteration that applies the prefab
    rotation to entity positions.
  - A real-pack test over `Hunt/Generic/Site.prefab.json` (10 blocks, 3
    entities, one `BlockMapMarker` component).
- `hytale-world`:
  - `runtime::{RuntimeEntityStore, RuntimeEntity, RuntimeEntityKind,
    BlockComponentStore}` with stable ids, spawn/despawn, radius/coord queries
    and dirty tracking.
  - JSON persistence in the world save root (`entities.json`,
    `block-components.json`), versioned and atomically written; `WorldSave`
    loads/saves it with the chunks.
  - `World::{spawn_entity, despawn_entity, entity, entities,
    set_block_component, block_component, remove_block_component}`; replacing a
    block removes its component, matching Java block-entity replacement.
  - `prefab::paste_prefab` applies rotated blocks/fluids/fillers/support values,
    records block components, rotates prefab entity positions by the prefab
    rotation and spawns them through the runtime store.
- `hytale-server`:
  - `WorldGenerator` owns the asset-pack `PrefabStore` (same registries as the
    generator) and exposes name lookup.
  - `WorldThread::place_prefab(name, x, y, z, rotation)` loads the prefab's
    chunks, pastes it and returns a report.
  - `WorldThread::{entities, despawn_entity, spawn_item_entity}` and
    `WorldThread::{block_component, block_components}` for tests/follow-ups.
  - Simulation drops create item entities in the runtime store.
  - Save/shutdown persists runtime entities and block components.
- Tests: real `Hunt.Generic.Site` paste (blocks, component, 3 entities, rotation),
  save/reopen persistence, block replacement clearing components, despawn and
  simulation-drop item entities.

## Acceptance

```sh
cargo test -p worldgen-hytale
cargo test -p hytale-world
cargo test -p hytale-server
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A prefab with entities and block components pastes at an origin/rotation and
  each block/entity lands at the Java-rotated position.
- `save` then a fresh `WorldSave::open`/`WorldThread` restores the same runtime
  entities and block components.
- Despawning removes an entity and persists.
- Existing v1 join/edit/chunk gates stay green.

## Deviations to report

- The full Java `EntityStore`/`ChunkStore` ECS BSON codec is not ported; runtime
  entities and block components are persisted as versioned JSON that preserves
  the original `Components` documents. Chunk regions stay Java-readable and do
  not embed the sidecar state.
- Prefab entity spawn order is file order (Java groups entities by prefab
  column/hash order). Identity is the Rust runtime id; the prefab's saved UUID
  remains a component value.
- Paste uses force semantics (`Flags.FORCE`/`ANCHOR_BLOCK` off): technical
  placement checks, block-settings flags, events and `NO_ENTITIES` are not
  modelled.
- No spawn/despawn/entity packets yet; T57 adds spawn presentation and T57b owns
  transform/despawn/metadata.

## Landed (session 46)

- `worldgen-hytale`: `PrefabBufferBlockEntry.components` keeps the prefab block
  `"components"` document; `PrefabEntity` keeps the anchor-adjusted transform
  and full `Components`; `PrefabBufferAccessor::for_each_entity` applies the
  prefab rotation. The real-pack `Hunt.Generic.Site` test gates 10 blocks,
  3 entities and the `Hunt_Trap` `BlockMapMarker` component.
- `hytale-world`: new `runtime` module (entity/block-component stores, stable
  ids, radius queries, versioned atomic JSON) and `prefab::paste_prefab`
  (rotated blocks/fluids/fillers/support, block components, rotated entity
  spawn). `WorldSave` loads/saves the sidecars; block replacement removes the
  component.
- `hytale-server`: `WorldGenerator` owns the asset-pack `PrefabStore`;
  `WorldThread` gains `place_prefab`, `entities`, `despawn_entity`,
  `spawn_item_entity`, `block_component` and `block_components`. Simulation
  fixed-item drops become item entities.
- Tests: real `Hunt.Generic.Site` paste/rotation/component/entity, save+reopen,
  despawn persistence, item-entity persistence and simulation drops.

## Follow-ups

- T57 sends spawn/block-health presentation; T57b owns entity transform/despawn/metadata.
- T58/T59 produce NPC entities and feed target boxes into projectiles.
- Weighted drop-list resolution for simulation drops stays with the drop-list
  producer (T57/T60).
- T62 adds spawn-rate/entity-count limits and rate limiting.
- Port the full Java `EntityStore`/`ChunkStore` codec if Java-readable runtime
  entity saves become a requirement; the current sidecars are JSON.
