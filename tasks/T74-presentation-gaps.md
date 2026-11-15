# T74 — Presentation gaps

**Status:** done (session 103)
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

## Landed (session 103)

- **Map refresh:** the session tracks chunks touched by block/fluid broadcasts
  and re-renders those `MapChunk` images at the existing 1 s map cadence, even
  when the view center has not changed. Unload/view membership behaviour is
  unchanged. Map refreshes now use the active (possibly portal-switched) world
  thread rather than the join world.
- **Map stream:** client-bot accepts the dedicated server `WorldMap`
  unidirectional stream (`hytale_net::WorldMapStreamReader`) and decodes
  `UpdateWorldMap.chunks`, so map bytes and edit refreshes are observable in
  loopback tests.
- **Window validation:** both container and bench opens are guarded by Java
  `BlockWindow.MAX_DISTANCE` (7 blocks, block integer position). A live window
  is validated on each game-loop tick: moving out of range or replacing the
  block closes it. `window_open_rejections`, `window_distance_closes` and
  `windows_closed` are reported.
- **Weather document fields:** `UpdateWeathers` now carries clouds, sunlight
  damping/colors, sun/moon/gloom colors and scales, sky/fog colors, fog
  height/density timelines, screen-effect colors, color filters, water tints,
  weather particles and `FogOptions`, including Java's `rgba(#hex, alpha)`
  color form.
- **Scripted presentation:** new ignored `client-bot/tests/presentation.rs`
  forces a weather change, spawns `Example_Boss`, pings
  `Combat_Music_Encounter_Deepworks` and spawns the real
  `Bear_Voidtaken_Tentacle_Marker` role. It asserts the `UpdateWeather`
  transition, `SpawnParticleSystem`, `SetAudioState` and `PlaySoundEvent3D`
  packets on the client.
- **First-person/held-item:** no new release-client build/trace was available;
  the existing self-spawn equipment and held-item loopbacks remain the
  regression (see deviations).

## Verification (session 103)

```sh
cargo test -p client-bot --test block_interactions -- --ignored # 2 passed (52.91 s)
cargo test -p client-bot --test presentation -- --ignored       # 1 passed (62.20 s)
cargo test -p hytale-assets --test entity_packets               # 2 passed (23.32 s)
cargo test -p hytale-server --lib                               # 59 passed
./tools/test.sh fast                                            # 1650 passed, 33 skipped
./tools/test.sh assets                                          # 381 passed
./tools/test.sh slow                                            # pre-existing failures, see deviations
cargo clippy --workspace --all-targets -- -D warnings           # clean
cargo fmt --all -- --check                                      # clean
```

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

## Deviations

- No Java `MapImage` dumper/fixture exists for a fixed region, so byte parity
  remains a semantic gate. The exact open fields are the `ImageBuilder`
  quantisation/shading tuning; palette/bit-pack form and 96×96 layout match.
  `GOLDEN_DATA.md` records this and client-bot can now consume the map stream
  when a dumper is added.
- Opening a container/bench beyond reach is usually rejected earlier by the
  interaction reach check; the window-specific open counter is defensive. The
  loopback test covers the rejected use and the live out-of-range close.
- First-person/held-item drift requires a release-client trace; no new client
  build exposed a difference. Existing equipment/held-item loopbacks stay
  green.
- `./tools/test.sh slow` reports the same `world_boot`/world-thread failures on
  the parent commit `9869ca5` (for example `requested_chunks 0 != 1`), so they
  are pre-existing and unrelated to the presentation changes.
- Particles/audio now come from real encounter/role producers in the
  presentation loopback: `Bear_Voidtaken_Tentacle_Marker` emits
  `SpawnParticleSystem`, `Combat_Music_Encounter_Deepworks` emits
  `SetAudioState`, and `Example_Boss` emits `PlaySoundEvent3D`.

## Notes

- Golden-first: map-image parity and any new presentation payload need a Java
  dumper output before code invents expected bytes.
- Keep the full window/UI framework, editor/builder tools and machinima out of
  scope (T61).
