# T74 — Presentation gaps

**Status:** proposed (filed session 99)
**Package:** `hytale-assets`, `hytale-server`, `hytale-world`,
`tools/client-bot`
**Depends on:** T57b, T60b, T66
**Agent fit:** low/medium

## Goal

Close the presentation gaps T57b deferred and T66 did not need: map-image
refresh/byte parity, window-distance validation, particle/audio/weather
presentation, and first-person/held-item polish if a new client build exposes it.

## Current gaps

- `UpdateWorldMap.chunks` is refreshed on join/view changes, not after block
  edits, so the map can lag the world.
- `map_image.rs` follows the Java palette/silhouette contract but is not proven
  byte-identical; `GOLDEN_DATA.md` records the open parity gap.
- Container/bench windows are not validated against Java's distance/reach rules
  on open or after movement.
- Particle/sound/weather packets and config exist, but scripted presentation
  coverage is partial (particles/audio around encounters/projectiles, full
  weather document fields).
- First-person/held-item presentation has only the release-client smoke; there
  is no regression if a new client build exposes a model/animation drift.

## Deliverables

- Map refresh:
  - invalidate and resend the affected `MapChunk` image after block/fluid edits
    (throttled), in addition to view changes;
  - keep unload/removal behavior;
  - add a client-bot assertion that an edit refreshes the map image.
- Byte parity:
  - extend the Java dumper to emit map images for a small fixed region (or
    document the existing fixture) and compare Rust bytes/palette/silhouette;
  - do not invent a golden if the dumper cannot produce one.
- Window validation:
  - enforce Java's distance rule when opening/using a container/bench and close
    windows that move out of range;
  - count rejected opens and cover the door/bench loopbacks.
- Particle/audio/weather:
  - add scripted packet assertions for existing encounter/projectile/weather
    producers;
  - map the remaining weather document fields needed for presentation.
- First-person/held-item:
  - record a new trace if a release-client build exposes a model/held-item
    difference; keep changes behind packet fields that already exist.

## Read first

- `crates/hytale-server/src/map_image.rs`
- `crates/hytale-server/src/session.rs` (`send_world_map_images`,
  `open_container_window`, `open_bench_window`, `close_container_window`)
- `crates/hytale-server/src/players.rs`
- `crates/hytale-assets/src/packets/{mod,child}.rs`
- `crates/hytale-world/src/weather.rs`
- `tools/client-bot/tests/{dev_join,block_interactions,npc_scenario}.rs`
- `GOLDEN_DATA.md`, `tasks/T57b-presentation-followups.md`
- Java `CoreServer/.../world/worldmap/**`, `modules/entity/**`,
  `modules/interaction/interaction/config/**` and the window handlers

## Acceptance

```sh
./tools/test.sh fast
./tools/test.sh assets
./tools/test.sh slow
cargo test -p client-bot --test block_interactions -- --ignored
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- A block edit refreshes the corresponding map image; unloads still work.
- Map bytes match the Java fixture where the dumper provides one; any remaining
  difference is documented with the exact field.
- Container/bench opens outside the Java distance are rejected and live windows
  close when out of range.
- Particle/audio/weather and held-item checks have scripted or trace-backed
  coverage.

## Notes

- Golden-first: map-image parity and any new presentation payload need a Java
  dumper output before code invents expected bytes.
- Keep the full window/UI framework, editor/builder tools and machinima out of
  scope (T61).
