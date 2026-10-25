# T57 — UI basics and block-health presentation

**Status:** done (session 47)
**Package:** `hytale-world`, `hytale-server`, `tools/client-bot`
**Depends on:** T46, T53, T56
**Agent fit:** low/medium

## Goal

Make the server state from T52d/T56 visible to a connected client: replicate
block health changes and chunk-load damage snapshots, spawn the persisted
runtime entities (prefab markers and item drops), and send the minimal world-map
settings/marker state needed by the M5 scenarios.

This is the first presentation slice. Full stat/effect/projectile config asset
packets, map-image generation, discovery/reveal state and per-entity animation
remain later work.

## Deliverables

- `hytale-world`:
  - `BlockHealthSection` exposes its damaged entries.
  - `World::block_health_entries` / `World::block_health_entries_in_chunk`
    return absolute `(x, y, z, health)` entries.
  - Replacing a block clears its health, fragility and pending delta.
- `hytale-server`:
  - `WorldThread::block_damage_packets` and
    `block_damage_packets_for_chunk` build `UpdateBlockDamage`/
    `UpdateBlockDamages` grouped by 32³ section (Java index/absolute health).
  - The chunk streamer writes a section's damage snapshot after its chunk
    packets (Java's future load packet ordering).
  - The simulation loop broadcasts health regeneration/damage deltas through
    `PlayerRegistry` to viewers.
  - `PlayerRegistry` can broadcast runtime-entity spawns; prefab/item entities
    in view are spawned after the join chunk snapshot.
  - `DropItemStack` spawns a persisted item entity and broadcasts it; fixed
    simulation drops expose their spawned entity ids and broadcast those.
  - Join sends `UpdateWorldMapSettings` with the Java chunk-map defaults and an
    `UpdateWorldMap` marker batch built from `BlockMapMarker` block components.
- `tools/client-bot`: observe `UpdateBlockDamage`/`UpdateBlockDamages` on the
  chunks stream and world-map markers on the default stream.
- Tests: packet builder tests, block-health load/delta ordering, runtime
  entity packets, map marker conversion, and the existing loopback gates.

## Acceptance

```sh
./tools/test.sh fast
cargo test -p hytale-world
cargo test -p hytale-server
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A damaged block produces a section packet with the Java index, absolute
  health and delta; regeneration produces the same form with a positive delta.
- A joining client receives the damaged entries after the chunk packets.
- Item drops and prefab entities spawn as `EntityUpdates` with transform and
  model/item components; item ids are stable across save/reopen.
- A `BlockMapMarker` component becomes a world-map marker with its name, icon,
  icon size, compass icon and block position.
- Existing v1 and T56 gates stay green.

## Deviations to report

- No map chunk images are generated; `UpdateWorldMap.chunks` stays empty. Only
  block-marker points of interest are streamed; T57b owns map images.
- Discoverable markers are sent without per-player reveal state; T57b owns the
  discovery/reveal mechanic.
- Runtime entities are static presentation entities: spawn packets are sent,
  but no animation/transform tick is produced until T57b/T59.
- The full Java `UpdateEntityStatTypes`/`UpdateEntityEffects`/
  `UpdateProjectileConfigs`/`UpdateWeathers` asset packets stay with T57b/T60.
- Full stat/effect/damage presentation, particles/sounds and client HUD windows
  remain T57b/T59/T60.

## Landed (session 47)

- `hytale-world`: `BlockHealthSection::entries`/`clear`, world-level health
  entry snapshots, and block replacement clearing health/fragility/deltas.
- `hytale-server`: `BlockDamagePacket` builders group entries by section with
  the Java block index and absolute health; the chunk streamer writes the load
  snapshot after each chunk; simulation deltas and regeneration broadcast to
  viewers.
- Runtime presentation: join sends prefab/item entities after the chunk
  snapshot; `DropItemStack` and fixed simulation drops spawn persisted item
  entities and broadcast them; runtime network ids use a disjoint offset.
- World map: join sends `UpdateWorldMapSettings` with the chunk-map scales and
  an `UpdateWorldMap` marker batch built from `BlockMapMarker` components.
- `client-bot` observes block damage, map settings/markers and runtime entity
  spawns; the ignored dev-join loopback now places `Hunt.Generic.Site` and
  verifies the marker plus three runtime entity spawns.
- The nextest-only prefab-store race was fixed by giving each test process its
  own temporary prefab root.

## Follow-ups

- T57b owns partial-hit breaking, entity lifecycle/metadata, the remaining
  config packets and map images/discovery.
- T58/T59 add NPC/combat producers; T60 ports the selected world-map/weather
  modules; T62 adds packet-rate limits.
