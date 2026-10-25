# Porting history — CoreServer rewrite, phase 2

Session 1–23 history is archived in
[`done/coreserver-vertical-slice/PORTING_HISTORY.md`](done/coreserver-vertical-slice/PORTING_HISTORY.md).
This file records phase-2 sessions only.

---

## Session 24 — Archive and phase-2 re-baseline (2026-10-24)

- **Goal:** close out the first vertical slice and leave a clean phase-2 plan
  and task board.
- **Landed:**
  - T34: archived the completed phase docs and tasks T01–T33 under
    [`done/coreserver-vertical-slice/`](done/coreserver-vertical-slice/README.md).
  - Updated `done/world-generation/README.md` to distinguish the two archives
    and point at the new root plan.
  - New root docs: `plan.md` (T34–T64 milestones), `NEXT_STEPS.md`,
    `ARCHITECTURE.md`, `PORTING.md`, `PORTING_HISTORY.md`, `REMAINING.md`,
    `GOLDEN_DATA.md`, `BENCHMARKS.md`, and `tasks/README.md`.
  - Created the M2/M3 task packets T35–T42; later milestone tasks remain
    roadmap-sized in `plan.md` until their milestone starts.
- **Verified:**
  - Markdown links in both `done/` trees resolve.
  - `cargo test --workspace`, clippy and fmt green (code unchanged).
- **Follow-ups:** begin T35 (child-store goldens), then T36–T39 and T40–T42 in
  parallel.

---

## Session 25 — Child-store golden dumper extension (2026-10-24)

- **Goal:** extend the Java asset oracle to the P1 stores and the P2/P3
  dependency roots needed by the phase-2 child indexes.
- **Landed:**
  - T35: `AssetDumper` now accepts `--priority`, `--stores` and `--closure`,
    preserves the default P0 set for `--priority P0`, and emits the phase-2
    default: P0 + P1 + `FluidFX`, `ItemSoundSet`, `SoundEvent`, `TagPattern`,
    `ParticleSystem`, `ParticleSpawner`, `ItemReticleConfig`,
    `ItemPlayerAnimations` and their `loadsAfter` closure.
  - `run-assets.sh` parses the same selector flags and still supports
    `--check` for the full committed set.
  - Committed 26 store entries / 21 payloads (~12.6 MiB) in
    `testdata/golden/server/assets/`: block decals/particles/sounds, item
    categories/qualities/sounds/animations/reticles, audio categories/states,
    FluidFX, particle systems/spawners, sound events and TagPattern.
  - `index.json` now records `selectedStoreCount`, dynamic `storeFilter`,
    `missingPackets`, `packetErrors`, `missingDependencies` and per-store
    `packetError`.
  - Per-store packet-generation failures are recorded instead of aborting the
    dump.
  - Updated the asset/Java dumper READMEs, the golden README, `GOLDEN_DATA.md`
    and the task board.
- **Verified:**
  - Two isolated default dumps are byte-identical.
  - `cargo test -p hytale-assets` and `cargo test --workspace` pass with the
    regenerated goldens.
  - Existing P0 payload sizes are unchanged; the four semantic P0 payloads were
    re-baselined to the canonical single-worker output.
- **Deviations:**
  - `BlockSet` (80 assets) has no payload: its generator needs
    `BlockSetModule`, which the asset-only boot does not install. The index entry
    carries `packetError`.
  - `GameplayConfig` has count 0 because `Default` fails validation on a missing
    camera effect; it has no packet generator.
  - `PrefabListAsset` remains empty (prefab store/AssetModule not booted).
  - `loadsAfter` dependencies outside the 54-store inventory are reported as
    `meta.missingDependencies` and not invented.
- **Follow-ups:**
  - T36 (block/fluid child stores), T37 (item child stores) and T38 (TagPattern)
    can now start in parallel; T39 wires them into the join.

---

## Session 26 — Block/fluid, item and tag-pattern child stores (2026-10-24)

- **Goal:** finish the M2 child-store work in parallel: block/fluid indexes, item
  indexes and `TagPattern`.
- **Landed:**
  - T36: `BlockBreakingDecal`, `BlockParticleSet`, `BlockSoundSet`, `FluidFX`
    and `SoundEvent` load through `AssetRegistry::load_phase2`, with
    `AudioCategory`/`AudioState` as `SoundEvent` dependencies. All five init
    packets are byte-identical to the Java goldens. `BlockType.blockSoundSetIndex`,
    `Fluid.fluidFxIndex`, `Fluid.blockSoundSetIndex` and
    `Environment.fluidParticles` resolve from the stores.
  - T37: `ItemSoundSet`, `ItemQuality`, `ItemReticleConfig`, `ItemCategory` and
    `ItemPlayerAnimations` load (181 animations = 160 files + 21 contained).
    All five init packets are byte-identical. `qualityIndex`, quality-driven
    `itemEntity`, `itemSoundSetIndex`, `reticleIndex` and `playerAnimationsId`
    now match Java for all 3,971 items, so the old residual list is empty.
  - T38: `TagPattern` loads 196 assets (11 files + 185 contained) with exact
    Java key/id parity. `UpdateTagPatterns` emits the same op trees and payload
    size as Java and is sent before `BlockType`/`Item`. `TagRegistry` includes
    pattern-only `Equals` tags. The server's `WorldGenerator` uses
    `load_phase2` so the join streams patterns.
  - Merged the three branches into one integration branch and reconciled the
    shared `AssetRegistry`, `packets/mod.rs`, preloads and `load_phase2`.
- **Verified:**
  - `cargo test -p hytale-assets` (all child-store suites).
  - `cargo test --workspace`.
  - `cargo clippy --workspace --all-targets -- -D warnings`.
  - `cargo fmt --all -- --check`.
- **Deviations:**
  - `UpdateTagPatterns` cannot be byte-compared because Rust owns tag indexes;
    op trees/size match and every index resolves in `ServerTags`.
  - `AudioCategoryDuckingRule.targetAudioCategoryIndex` stays `NOT_FOUND` to
    match the dumper; Java resolves it in a runtime load event.
  - `AudioCategory`/`AudioState` are loaded but not streamed.
  - `ItemReticleConfig.ServerEvents` indexes are deterministic but Rust-owned.
- **Follow-ups:**
  - T39 wires all new packets into the final config order, caches encoded
    `ConfigAssets`, and adds `--validate-assets`.

---

## Session 27 — Full config burst, cache and asset validation (2026-10-24)

- **Goal:** finish M2 by wiring the child packets into the join, encoding the
  config burst once, and validating the live asset registry.
- **Landed:**
  - `AssetRegistry::config_assets()` now emits 17 packets in dependency order:
    hitboxes, physical materials, sound events, block sound sets, FluidFX,
    Fluid, Environment, TagPattern, BlockType, block particles/decals, item
    sounds/qualities/reticles/categories/animations, then Item.
  - `WorldThread` encodes the burst once at world start and hands sessions an
    `Arc<ConfigAssets>`; the encode no longer runs per connection.
  - `hytale-server --validate-assets [--pack PATH]` loads every enabled store,
    reports counts/id ranges/packet sizes/tag counts, checks packet order,
    duplicate ids, decodability, determinism and that every asset/pattern tag
    index resolves in `ServerTags`, and exits non-zero on drift.
  - `client-bot` recognises all 17 config packet ids; the authenticated join
    asserts their exact dependency order.
  - Semantic tests now compare `fluidFXIndex`, `blockSoundSetIndex`,
    `fluidParticles` and the previously divergent item fields directly, removing
    the child-index exceptions.
  - Added `crates/hytale-server/tests/asset_validation.rs` and a world-thread
    cache test (`Arc::ptr_eq` across two config requests).
- **Verified:**
  - `cargo run -p hytale-server -- --validate-assets` → 21 stores, 17 packets,
    579 tags, 24,397 checked references, `validation: ok`.
  - `cargo test -p client-bot --test auth_join -- --ignored`.
  - `cargo test --workspace`.
  - `cargo clippy --workspace --all-targets -- -D warnings`.
  - `cargo fmt --all -- --check`.
- **Deviations:**
  - Validation checks the live registry/encoding rather than committed
    `index.json`, so it also works with mod packs; fixture comparison remains in
    the Rust tests.
  - The cache is per world thread until T41 gives the universe ownership of
    multiple worlds.
  - `AudioCategory`/`AudioState` remain dependency-only stores (no packets).
- **Follow-ups:**
  - T40 (`hytale-core`), T41 (universe/world lifecycle), T42 (persistent
    multi-session listener) start the M3 runtime track.

---

## Session 28 — `hytale-core` runtime services (2026-10-24)

- **Goal:** give the M3 runtime shared config, logging, task, event and module
  services without changing existing server behaviour.
- **Landed:**
  - `config`: `ServerConfig` with defaults (matching the current server name,
    MOTD, worker/player limits), optional JSON file, `HYTALE_CORE_*` environment
    overrides and typed CLI overrides. Precedence is defaults → file → env →
    CLI. Unknown JSON fields and unknown `HYTALE_CORE_*` keys are rejected.
  - `logging`: process-wide text/JSON tracing subscriber initialization.
  - `tasks`: cloneable `ShutdownToken`, named task spawning, `TaskScheduler`
    with graceful-deadline cancellation and force-abort, plus a `ShutdownReport`.
    Dropping a scheduler cancels the token and aborts owned tasks.
  - `events`: typed `EventBus` with deterministic subscriber order, synchronous
    callbacks and bounded async channels; dropping a `Subscription` unsubscribes,
    full channels drop with an accounting report.
  - `modules`: `ModuleManifest`/`ModuleRegistry` with duplicate, unknown and
    cycle detection and deterministic dependency-first resolution.
  - Added unit tests for config precedence/validation, event ordering and
    unsubscribe, shutdown/deadline behaviour and module registration.
- **Verified:**
  - `cargo test -p hytale-core` (23 tests).
  - `cargo clippy -p hytale-core --all-targets -- -D warnings`.
  - `cargo fmt -p hytale-core -- --check`.
  - `cargo test --workspace`.
- **Deviations:**
  - The binary is not switched to these services yet, so existing output is
    unchanged; T41/T42 compose them.
  - The module registry is a manifest/dependency seam only; lifecycle hooks are
    deferred to T60.
- **Follow-ups:**
  - T41 builds the universe/world lifecycle on the scheduler and config.
  - T42 uses the event bus and shutdown token for the persistent listener.

---

## Session 29 — Universe and world lifecycle (2026-10-24)

- **Goal:** own multiple worlds for the process lifetime instead of one scripted
  save.
- **Landed:**
  - `hytale-world::universe`: persistent `Universe` catalog with `universe.json`,
    create/list/resolve/default/save/delete/unload, name and UUID lookup, and
    missing/corrupt-world errors. Worlds remain normal `WorldSave` directories.
  - `WorldProviderKind` (`Default`/`Flat`/`Void`/`Dummy`) and the
    `WorldProvider` trait; `StaticWorldProvider` produces empty columns for the
    non-default stubs.
  - Player load/save seam on the universe (`load_or_create_player`,
    `save_player`).
  - `hytale-server::UniverseRuntime`: one live `WorldThread` per started world,
    create-and-start, start/stop, explicit save, save-on-unload and
    save-on-shutdown, with clean `WorldThread` teardown.
  - `hytale-server universe` CLI: create/list/show/save/delete/default with
    `--root`, `--seed` and `--provider`.
  - Tests: universe catalog/reopen/provider/player round-trips; a two-world
    runtime test starts both, edits both, saves and reloads them from disk.
- **Verified:**
  - `cargo test -p hytale-world`.
  - `cargo test -p hytale-server` (including the two-world lifecycle test).
  - Universe CLI smoke: create/list/default/show/delete.
  - Full workspace/clippy/fmt gates.
- **Deviations:**
  - Flat/Void/Dummy are empty-column stubs and `UniverseRuntime` only starts
    Default worlds until M5.
  - Each world thread still loads its own asset registry; T42 moves the shared
    registry/config cache to the process runtime.
  - Live player transform persistence remains T44/T47; T41 provides the
    load/save seam.
- **Follow-ups:**
  - T42 builds the persistent listener and multi-session runtime on
    `UniverseRuntime`.

---

## Session 30 — Persistent listener and multi-session runtime (2026-10-24)

- **Goal:** replace the one-connection demo with a process-lifetime listener
  over the universe runtime.
- **Landed:**
  - `hytale-server::serve`: one QUIC endpoint and one `UniverseRuntime`; the
    selected/default world starts once and every accepted connection gets an
    independent task running the existing join state machine.
  - Sessions share the world thread's cached `ConfigAssets` (no per-connection
    re-encode).
  - Connection limit with explicit `server full` rejection, peak-session
    accounting, idle timeout in the game loop, clean EOF/`ClientDisconnect`
    handling, and close-after-session.
  - Graceful shutdown: closes the endpoint, closes active connections, waits for
    session tasks up to `--shutdown-timeout`, aborts the remainder, then stops
    world threads and saves chunks/config.
  - `hytale-server serve` CLI with auth modes, world selection, view/tick
    settings, session limits/timeouts, and `--universe`.
  - `chunk-serve` remains the single-connection transport demo.
  - Added `client-bot` multi-join test: two concurrent clients, one disconnect,
    a third rejoin, and clean listener shutdown.
- **Verified:**
  - `cargo test -p client-bot --test multi_join -- --ignored`.
  - `cargo test -p client-bot --test auth_join -- --ignored`.
  - `cargo test -p hytale-server`.
  - `cargo test --workspace`, clippy and fmt.
- **Deviations:**
  - Sessions do not see each other yet (T45).
  - `serve` serves one selected/default world; multi-world session routing needs
    protocol work.
  - Each world thread still owns its asset registry; cross-world sharing remains
    an optimization.
- **Follow-ups:**
  - T43 starts M4 with the world clock and `GameplayConfig`.

---

## Session 31 — World clock and `GameplayConfig` (2026-10-24)

- **Goal:** replace protocol-default time/gameplay state with a real world
  clock and the loaded gameplay configuration.
- **Landed:**
  - T43: `hytale-world::world_clock` ports `WorldTimeResource`: Java's segmented
    real-time → game-time mapping (`DAYTIME_SECONDS`/`NIGHTTIME_SECONDS` over
    the configured durations), pause/resume, duration sanitisation, Java range
    wrap, and ISO-8601 `GameTime` parse/format (`0001-01-01T05:30:00Z` default).
  - `hytale-assets::gameplay` loads `Server/GameplayConfigs` as an internal
    `DefaultAssetMap` store (8 assets; parents resolved with the normal shallow
    merge). It models the protocol-visible world/combat/player fields, applies
    Java `GameplayConfig.DEFAULT` values for absent sections, resolves block
    tags through `ServerTags`, and emits `WorldGameplayConfig`.
  - `WorldThread` owns the clock and the gameplay packet. `Tick` advances by the
    wall-clock delta since the previous command; `WorldTime`, `Save` and
    `Shutdown` also settle the clock. `Save`/`Shutdown` write `GameTime` and
    `IsGameTimePaused` into `config.json`; duration overrides win over the
    gameplay asset's `World` values.
  - `WorldThread::{gameplay_config, set_time_paused, advance_time}` expose the
    join packet, pause seam and a deterministic test/debug tick.
  - Sessions send the loaded `WorldGameplayConfig` instead of the hardcoded
    packet, and broadcast `UpdateTime` once per real second while running;
    a pause change re-sends `UpdateTimeSettings`.
  - `client-bot` decodes `WorldGameplayConfig`/`UpdateTimeSettings`/initial
    `UpdateTime`, and `observe_time` collects live time packets for the
    loopback test.
  - `--validate-assets` now reports the internal `GameplayConfig` store (8
    assets) alongside the 21 streaming stores.
- **Verified:**
  - `cargo test -p hytale-world` (30 tests incl. 10 clock tests).
  - `cargo test -p hytale-assets` (gameplay parser/packet tests).
  - `cargo test -p hytale-server` (world-thread clock/save/reload test,
    asset validation store count).
  - `cargo test -p client-bot --test auth_join -- --ignored`
    (gameplay flags + two increasing `UpdateTime` packets).
  - `cargo test -p client-bot --test multi_join -- --ignored`.
  - `cargo run -p hytale-server -- --validate-assets` → 22 stores, 17 packets,
    579 tags, 24,397 references, `validation: ok`.
  - Full workspace/clippy/fmt gates.
- **Deviations:**
  - Java's canonical asset boot reports `GameplayConfig` count 0 because all
    eight files fail camera-effect validation. Rust reads them directly; a
    missing id or empty store falls back to `GameplayConfig.DEFAULT`.
  - Time packets are per-session writes from the shared clock until T45 adds a
    session registry/broadcast path.
  - `UpdateTimeSettings` is only re-sent on a pause change, matching Java's
    settings-change broadcast (joining players get it in the join burst).
- **Follow-ups:**
  - T44 gives the self-spawn packet real equipment/cosmetics.
  - T54 owns weather, sleep/day-time interpolation and moon-phase events.

---

## Session 32 — Self-spawn appearance and equipment (2026-10-24)

- **Goal:** replace the empty self-spawn model/skin/equipment components with
  data decoded from the player save and the auth handshake.
- **Landed:**
  - T44: `PlayerData::appearance` decodes the saved `Model`
    (`PersistentModel.ModelReference`: id, scale, static, random attachments), an
    optional `PlayerSkin` component, modern `ArmorInventory`/`HotbarInventory`/
    `UtilityInventory` containers (capacity, items, active slots) and the legacy
    `Inventory` (`Armor`/`HotBar`/`Utility` + active slot fields) fallback.
  - Added `PlayerAppearance` with four armor slots, right/left held items and
    `resolve_items`, which drops ids the asset registry does not know.
  - `WorldThread::player_appearance()` loads and resolves the player's saved
    appearance alongside the transform.
  - Self-spawn `EntityUpdates` now sends a `ModelUpdate` from the saved model
    reference, a `PlayerSkinUpdate` from the insecure
    `InsecurePlayerOptions.skin` (falling back to a saved skin), and an
    `EquipmentUpdate` with four armor ids and Java's `"Empty"` held-item values.
  - `client-bot` records the self-spawn model/skin/armor/hand values; the dev
    join advertises a test skin, and the authenticated join asserts the empty
    defaults.
  - Extended the Java `WorldPlayerDumper` with a second golden fixture
    (`22345678-…json`) containing the saved model and full armor/hotbar/utility
    set, plus `write-equipment`/`probe-equipment` modes. `run-player-data.sh`
    now writes, checks and probes both fixtures.
  - Rust reads the new Java fixture and the reverse Rust-write → Java
    `--probe-equipment` gate is green.
- **Verified:**
  - `cargo test -p hytale-world` (player-data unit/integration tests).
  - `cargo test -p hytale-server` (self-spawn mapping and equipment world-thread
    test).
  - `cargo test -p client-bot --test dev_join -- --ignored` (skin over the
    insecure handshake).
  - `cargo test -p client-bot --test auth_join -- --ignored`,
    `--test multi_join -- --ignored`.
  - `cargo test --workspace`, clippy and fmt.
  - `tools/java-dumper/run-player-data.sh --check` and
    `--probe-equipment`.
- **Deviations:**
  - `ModelUpdate` uses the saved reference id/scale and Java's unknown texture;
    full `ModelAsset` geometry/animation/attachment expansion is a follow-up.
  - Skin comes from the insecure handshake (as in Java); the local JWT profile
    has no cosmetic claims, and a saved `PlayerSkin` component is the fallback.
  - The world thread still loads one player; per-session player identity/save is
    T45/T47.
  - Armor visibility from `GameplayConfig.PlayerConfig` is not applied to the
    self-spawn payload yet.
- **Follow-ups:**
  - T45 adds other-player tracking and needs the same appearance/equipment
    payload for remote entities.
  - T46 can reuse the equipment item ids for held-item consistency.
  - Full `ModelAsset` resolution should land before the real-client gate if the
    vanilla client requires it.

---

## Session 33 — Other-player entity tracking and broadcast (2026-10-24)

- **Goal:** make multiple sessions on one world visible to each other.
- **Landed:**
  - T45: `hytale-server::players::PlayerRegistry`, owned once by the `serve`
    listener. It assigns unique network ids, stores UUID/username/world,
    transform, appearance, auth skin, view centre/radius and a per-session
    message queue, and tracks each viewer's visible entity set.
  - Join: the new client receives the complete `AddToServerPlayerList`
    snapshot; existing clients receive an add entry; visible entities spawn in
    both directions. The self spawn and `SetClientId` use the assigned network
    id.
  - Leave: the player-list entry is removed everywhere and the entity removal
    only goes to viewers that had it visible. The `PlayerSession` guard
    unregisters on every session exit/error/abort path.
  - Movement: `ClientMovement` updates the registry transform (10 Hz throttle,
    flushing the final position) and sends `TransformUpdate` only to viewers
    whose square chunk view contains the entity.
  - View changes: chunk and `ViewRadius` changes recompute visibility and emit
    spawns/removals for entities entering/leaving the view.
  - The session loop drains the queue every 50 ms and writes list/entity
    packets; `serve` shares one registry across accepted sessions. Single-
    session `serve_connection` still creates a private registry.
  - `client-bot`: decodes initial player-list UUIDs and post-join
    add/remove/entity events; supports movement-after-join, an observation
    window, self-disconnect and a keep-alive task for the setup/chunk streams.
  - The multi-client loopback test now covers mutual spawn, mutual movement,
    disconnect removal and a third rejoin with the still-connected player in
    its list.
- **Verified:**
  - `cargo test -p hytale-server` (registry unit tests + session mapping).
  - `cargo test -p client-bot --test multi_join -- --ignored` (spawn/move/
    disconnect/rejoin).
  - `cargo test -p client-bot --test auth_join -- --ignored` and
    `--test dev_join -- --ignored`.
  - `cargo test --workspace`, clippy and fmt.
- **Deviations:**
  - The world thread still loads one player appearance; all sessions share it
    until per-session player data lands in T47.
  - The server player list is global; entity spawn/update/removal is
    view-filtered with a square chunk radius.
  - Movement broadcasts are 10 Hz, and the registry queue is unbounded.
  - Remote look orientation shares the body rotation until a dedicated pose
    component exists.
- **Follow-ups:**
  - T46 applies block edits and can reuse the registry for interested sessions.
  - T47 should load/save per-session player data and persist live transforms.
  - T62 adds queue backpressure/rate limits.

---

## Session 34 — Block place/break (2026-10-24)

- **Goal:** complete the v1 edit loop: client interaction → world edit →
  broadcast → save.
- **Landed:**
  - T46: `MouseInteraction` is decoded in the session loop. Pressed left is a
    break and pressed right is a place; target/entity selection, reach (5 plus
    Java's 2-block buffer), held-item presence and non-block items are validated
    in a pure `parse_block_edit` helper. Invalid interactions are rejected
    without returning a session error.
  - `WorldThread` gained `get_block`, `place_block` (only into air) and
    `block_id_for_item` (item with a `BlockType` → block id). Edits use the
    existing `World::set_block` path, which marks the chunk dirty for the normal
    region save.
  - `PlayerMessage::BlockUpdate(ServerSetBlock)` and
    `PlayerRegistry::broadcast_block` fan a block edit to every viewer whose
    square chunk view contains it, including the editor. The session writes it
    on the chunks stream.
  - `client-bot` sends place/break `MouseInteraction` packets, reads
    `ServerSetBlock` from the chunk stream during observation, and the
    multi-client loopback now asserts an observer sees another client's
    placement and break.
  - Added world-thread persistence coverage: resolve `Rock_Stone`, reject a
    sword/non-block and unknown item, place/get, save, respawn and read the
    edited block back.
  - Session unit tests cover valid break/place parsing and release/out-of-reach/
    missing-item/middle-button rejection.
- **Verified:**
  - `cargo test -p hytale-server` (8 lib tests + world-thread block edit test).
  - `cargo test -p client-bot --test multi_join -- --ignored` (spawn, movement,
    block placement/break broadcast, disconnect, rejoin).
  - `cargo test -p client-bot --test auth_join -- --ignored` and
    `--test dev_join -- --ignored`.
  - `cargo test --workspace`, clippy and fmt.
  - `tools/java-dumper/run-save-check.sh` on a Rust-written save with an edited
    block (`Debug_Test_Block`) → `whole-save check: ok`.
- **Deviations:**
  - Placement uses the packet's `itemInHandId` but does not decrement a live
    inventory; full inventory transactions are T49.
  - Broadcast filler/rotation are `0`; per-item/creative reach, releases, mouse
    motion, block components and damage/particle feedback remain later tasks.
- **Follow-ups:**
  - T47 persists live transforms and restores edits on reconnect.
  - T49/T50 add inventory consumption and the interaction graph.
  - T52 adds block simulation and component updates.





---

## Session 35 — Disconnect, reconnect and teardown (2026-10-24)

- **Goal:** make the v1 lifecycle reliable: release every resource on leave,
  survive server/client disconnects, and restore player/world state on
  reconnect.
- **Landed:**
  - T47: session teardown now runs on every post-registration exit path. It
    persists the live `Transform` for the session UUID, removes the session's
    registered chunk view, and drops the `PlayerSession` so the registry
    broadcasts the player-list/entity removal.
  - `WorldThread` gained independently registered views (`ViewId`,
    `add_view`/`update_view`/`remove_view`). Loaded chunks are the union of all
    live views, so one player moving or leaving cannot unload another player's
    terrain. The first session view replaces the startup/CLI base view, and the
    last view leaving unloads session-owned chunks.
  - Added `player_snapshot(uuid)` and `save_player_transform(uuid, transform)`.
    They use the normal player storage path and preserve every other saved
    component, so reconnecting resumes the saved position/appearance.
  - `ClientDisconnect` is decoded and ends the game loop cleanly. The existing
    idle-timeout `ServerDisconnect`, transport-close and half-closed-stream
    paths now all flow through the same teardown.
  - `client-bot` records the self-spawn transform and sends a real
    `ClientDisconnect` while observing. The loopback test now covers join →
    move → edit → graceful disconnect → reconnect with the same UUID, asserting
    the saved transform and the edited block; a second test covers idle-timeout
    teardown and a later join on the same listener.
  - Added world-thread tests for view sharing/release and per-UUID transform
    save/reload across a world restart.
- **Verified:**
  - `cargo test -p hytale-server` (8 lib tests; 7 world-thread tests including
    registered views and player snapshot).
  - `cargo test -p client-bot --test multi_join -- --ignored` (reconnect
    transform/edit plus idle-timeout teardown).
  - `cargo test -p client-bot --test auth_join -- --ignored` and
    `--test dev_join -- --ignored`.
  - `cargo test --workspace`, clippy and fmt.
  - `tools/java-dumper/run-save-check.sh` on a fresh Rust-written save →
    `whole-save check: ok`.

- **Deviations:**
  - Only the `Transform` is saved at teardown; live inventory/held-item state is
    T49. Unknown/existing components are preserved as loaded.
  - Chunk ownership is per view, but chunk load/unload events are still one
    world-wide queue; per-view event fan-out is follow-up work.
  - Reconnecting while the old session is still registered creates a second
    player entry; duplicate-identity takeover is T62.
  - The server closes the QUIC connection after teardown instead of waiting for
    a client disconnect acknowledgement (no such packet exists).

- **Follow-ups:**
  - T48 verifies the lifecycle against a real-client trace.
  - T49 persists inventory and held-item state.
  - T62 adds rate limits/backpressure and duplicate-identity handling.
  - T63 soak-tests repeated join/leave cycles.
---

## Session 36 — Real-client trace and v1 gate (2026-10-24)

- **Goal:** close M4/v1 with a repeatable, committed packet trace and record the
  real client's packet/channel requirements.
- **Landed:**
  - T48: `hytale-net::trace` defines the JSONL trace schema (packet id/name,
    registry channel, setup/chunks stream, direction, per-session order, wire
    payload length and SHA-256), a `PacketTraceSink` on the framed and chunk
    streams, a per-session `TraceRecorder`, and `read_trace`/`write_trace`.
  - `hytale-server serve --trace <path>` captures every session and flushes on
    shutdown. No raw payload bytes are written.
  - `client-bot` gained `send_ready_for_gameplay` so the dev join exercises the
    teleport/time flow.
  - Added the live `trace_gate`: actor + observer joins, actor move/edit,
    disconnect and same-UUID reconnect/break, then compares a fresh server trace
    with the committed fixture as an ordered per-session subsequence (stable
    payload hashes included).
  - Added a fast `trace_fixture` test that validates the committed fixture
    against the generated packet registry and asserts the recorded config order,
    `ViewRadius` on `Chunks`, `JoinWorld`, `ClientTeleport`, player-list/entity
    events, chunk batches and `ServerSetBlock` placement/break.
  - The server-side trace of the real client log (not redistributed) identified
    the hard `ViewRadius (ID 32) must be on Chunks` rule; the fixture/gate now
    covers it. The real-client capture runbook is committed under
    `testdata/golden/server/trace/README.md`.
- **Verified:**
  - `cargo test -p hytale-net` (trace metadata round trip; raw payload not
    present in the file).
  - `cargo test -p client-bot --test trace_fixture` (default suite).
  - `cargo test -p client-bot --test trace_gate -- --ignored`.
  - `cargo test -p client-bot --test multi_join -- --ignored`, `auth_join` and
    `dev_join`.
  - `cargo test --workspace`, clippy and fmt.
  - `tools/java-dumper/run-save-check.sh` on the T47 Rust-written save still
    passes.
- **Deviations:**
  - The committed lifecycle fixture is a normalized server trace from the
    bot-driven loopback, because the vanilla-client capture cannot be
    redistributed or automated in this environment. `serve --trace` plus the
    runbook are the replacement seam.
  - The live gate is `#[ignore]`d (asset pack + loopback); the fixture/registry
    validation runs in CI/default.
  - The local EdDSA auth profile is not trusted by a default online client;
    live-session auth remains an adapter and is called out in `REMAINING.md`.
  - Dynamic payloads keep order/id/channel but drop hashes by design.
- **Follow-ups:**
  - T49 persists inventory/held-item state and extends the traced lifecycle.
  - Replace/augment the baseline with a real-client capture when available.
  - T62/T63 cover rate limits and repeated trace/soak runs.
---

## Session 37 — Inventory, containers and transactions (2026-10-24)

- **Goal:** replace appearance-only inventory decoding with real item stacks,
  containers and held-item gameplay state, and preserve it across save/reload.
- **Landed:**
  - T49: new `hytale-inventory` crate with a Java-codec-compatible `ItemStack`
    (`Id`, `Quantity`, `Durability`, `MaxDurability`, `QualityOverride`,
    `Metadata`, `OverrideDroppedItemAnimation`) and an `ItemRules` seam for
    asset-provided max stack/durability/quality.
  - `SimpleItemContainer` implements Java's partial-add, all-or-nothing-remove,
    target-slot move/merge/swap and clear transactions. `PlayerInventory` covers
    hotbar, utility, armor, storage, tools and backpack, active-slot clamping,
    modern `*Inventory` components and the legacy `Inventory` fallback.
  - `PlayerData` gained `inventory()`/`set_inventory()`; the world thread loads
    the inventory in `PlayerSnapshot` and saves transform + inventory together
    on teardown.
  - Sessions send the initial `UpdatePlayerInventory` plus hotbar/utility/tools
    `SetActiveSlot` packets after self-spawn. `SetActiveSlot` and
    `DropItemStack` are handled; held-item changes broadcast an equipment update
    to visible players.
  - Block placement uses the authoritative active hotbar stack, consumes one
    item and sends an inventory resync. The old packet-item fallback remains for
    sessions without a saved inventory, keeping T46's debug loopback green.
  - `client-bot` decodes `UpdatePlayerInventory`/`SetActiveSlot`; a new
    `inventory_join` loopback seeds `Rock_Stone ×5`, places one, reconnects and
    asserts the saved `×4` stack returns. The trace fixture now includes the
    inventory/active-slot packets.
  - Scripted crate tests cover merge/split/swap/drop/pickup, Java fixture
    component round-trip, legacy fallback and packet-section conversion. The
    world-thread test now round-trips transform + inventory across a restart.
- **Verified:**
  - `cargo test -p hytale-inventory` (9 scenarios + doc tests).
  - `cargo test -p hytale-world`, `cargo test -p hytale-server`.
  - `cargo test -p client-bot --test inventory_join -- --ignored`.
  - `cargo test -p client-bot --test multi_join`, `auth_join`, `dev_join`,
    `trace_gate` and `trace_fixture` (as applicable).
  - `cargo test --workspace`, clippy and fmt.
  - `tools/java-dumper/run-player-data.sh --probe` on the Rust-written player
    holder (inventory components included) → `java player data check: ok`.
  - The existing T46/T47 whole-save Java gate remains green; the T49 world is a
    generated universe world, so its config UUID is not the fixture config UUID
    the whole-save script assumes.
- **Deviations:**
  - Inventory windows, block containers, item entities and the
    `MoveItemStack`/`SmartMoveItemStack` server packet path are later gameplay
    work; the pure container transactions are implemented/tested.
  - `DropItemStack` removes/resyncs the stack without spawning a world item
    entity (T56).
  - Legacy/debug sessions with no saved inventory keep the T46 packet-item
    placement fallback.
  - The server sends all six inventory sections on join; saves only write
    present/non-empty components.
- **Follow-ups:**
  - T50/T56 wire block containers and dropped/runtime item entities.
  - T53 consumes item utility/stat metadata.
  - T57 adds inventory UI presentation.
---

## Session 38 — Interaction graph and block interactions (2026-10-24)

- **Goal:** load the Java interaction graph, walk root/chained/variable nodes and
  dispatch block `Use` interactions before falling back to T46 placement.
- **Landed:**
  - T50: new `hytale-interactions` crate. It loads `Item/Interactions` and
    `Item/RootInteractions` from the pack in one `read_many` pass, applies
    Java-style `Parent` inheritance, and exposes `InteractionCatalog`.
  - The executor walks `Condition`, `Replace`/interaction vars, `Serial`,
    `Parallel`, `Chaining`, `Repeat`, `Next`/`Failed` and inline nodes, emitting
    `WorldAction`s (`Door`, `ChangeState`, `ChangeBlock`, `OpenContainer`,
    `OpenBench`, `UseBlock`, `BreakBlock`, `PlaceBlock`).
  - `BlockInteractions` extracts the block's `Interactions`, `Bench`, `Container`
    and `State.Definitions` metadata; the current state is derived from the
    contained state block key.
  - `WorldThread` loads the catalog once, exposes `block_type_json`,
    `set_block_state` (preserving rotation/filler) and `interaction_catalog`.
  - Right-click now resolves the target block's `Use` root first. Door toggles
    and `ChangeState` maps are applied through `set_block_state` and broadcast as
    authoritative `ServerSetBlock`; containers/benches are handled UI actions
    (windows remain T57). If no block action handles the click, T46 placement
    runs unchanged, including T49 held-item consumption.
  - Added `block_uses`/`block_state_changes` session counters.
  - `client-bot` gained `use_block_after_join`; the new `block_interactions`
    loopback places a door, right-clicks it twice and asserts the place → open →
    close/change block-update sequence plus the two use/state counters.
  - Graph tests cover referenced/inline door nodes, condition failure paths,
    replacement vars/defaults, container roots, block metadata extraction and a
    real-pack catalog load.
- **Verified:**
  - `cargo test -p hytale-interactions` (6 tests + real pack load).
  - `cargo test -p client-bot --test block_interactions -- --ignored`.
  - `cargo test -p client-bot --test inventory_join`, `multi_join`,
    `trace_gate` and the existing joins.
  - `cargo test -p hytale-server`, `cargo test --workspace`, clippy and fmt.
- **Deviations:**
  - Selectors, effects/sounds, cooldowns, stats, projectiles, entity/weapon
    roots and client-chain sync remain T53/T55/T57.
  - Door handling is single-block and heuristic; double doors, fillers/hitbox
    updates, blocked checks, soft-block breaking and exact rotation matrices are
    not ported. `ChangeBlock` is parsed but not applied.
  - Containers/benches are detected and treated as handled, but window/UI
    packets are T57.
  - Right-click prioritises the block `Use` root over the held item's full
    `Secondary` item root.
- **Follow-ups:**
  - T51/T53/T55 add movement/combat/projectile consumers.
  - T56 adds block components/entity interaction hosts.
  - T57 adds container/bench UI windows.

---

## Session 39 — Movement physics and collision authority (2026-10-25)

- **Goal:** make the server validate/resolve player movement against the Java
  movement config and block collision geometry without fighting a normal client.
- **Landed:**
  - T51: new internal `MovementConfig` asset store
    (`Server/Entity/MovementConfig`) with every `MovementSettings` field, the
    optional `Fly` override and Java `DEFAULT_MOVEMENT` fallback.
    `GameplayConfig.Player.MovementConfig` selects the asset; the world sends
    it as `UpdateMovementSettings` (id `110`) during the join burst with the
    player model's mass (`68`) and drag (`0.5`).
  - `hytale-world::collision`: `Vec3`, `Aabb`, `BlockShape`,
    `BlockCollisionTable` and `WorldCollision`. Shapes come from the solid
    `BlockType` material plus its `HitboxType`/`BlockBoundingBoxes`
    (Java 64-slot rotation and `Box.normalize`), with `FillerBlockUtil` origin
    unpacking. Unloaded chunks are air for movement.
  - `hytale-entities::movement`: the Java player hitbox
    (`0.65 × 1.85 × 0.65`), Y→X→Z continuous AABB sweep with one-block step-up,
    overlap escape, exact passthrough for free targets, `MovementBounds`
    (finite, world limits, 512-block step) and a deterministic
    `MovementBody` integrator using `GRAVITY_ACCELERATION = 32`, jump,
    swim-jump, fly and climb values from the selected settings.
  - `hytale-server`: `ClientMovement` absolute/relative targets and directions
    use Java `ValidateUtil` semantics (non-finite disconnects), targets over
    the bounds are counted/ignored, collision resolutions are applied and
    corrected clients receive an authoritative `ClientTeleport` with an
    incrementing id. Movement counters (`movement_rejections`,
    `movement_corrections`, `movement_steps`) flow into `SessionSummary`.
  - `client-bot`: `movement_after_edits` sends a post-edit collision walk;
    `PlayerObservation` records server teleports; `GameJoinReport` records
    `UpdateMovementSettings`; new `movement_join` loopback places a wall, walks
    into it, and asserts the face correction plus zero corrections for the
    free walk. `multi_join` asserts normal movement is never corrected.
- **Verified:**
  - `cargo test -p hytale-entities`, `-p hytale-assets` (real `Default`
    movement config), `-p hytale-world` (real collision shapes) and
    `-p hytale-server` (world-thread wall/slab/settings tests).
  - `cargo test -p client-bot --test movement_join -- --ignored`, plus the
    existing `multi_join`, `inventory_join`, `block_interactions`,
    `auth_join` and `trace_gate` ignored suites.
  - `cargo test --workspace`, clippy `-D warnings` and fmt.
- **Deviations:**
  - Java keeps walking client-authoritative; the port validates and resolves
    the target instead of replaying client input. The free-target path is
    bit-for-bit so the loop has no rubber-banding.
  - Solid blocks only: fluids, trigger/damage boxes, ladders, mounts, traversal
    and entity collisions remain T52/T55/T58.
  - Unloaded chunks are air for movement (Java treats them as unknown/solid).
  - Per-packet speed/teleport anti-cheat is T62; the current bounds are finite,
    world-coordinate and 512-block step limits.
- **Follow-ups:**
  - T52 fluids/ticks/support; T55 projectiles/traversal; T58 NPC movement.
  - T62 real rate-limit and teleport/anti-cheat policy.

---

## Session 40 — Block simulation (2026-10-25)

- **Goal:** run scheduled/random ticks, support loss, falling blocks, fluid flow,
  block health and drops over loaded chunks without changing the Java-readable
  chunk/region layout.
- **Landed:**
  - T52: `BlockSection` now has the transient `scheduleTick` priority queue,
    `begin_tick` (pre-tick + copy/clear) and `finish_tick` applying Java's
    `BlockTickStrategy`. Nothing was added to the serialized section.
  - New `hytale-world::simulation` with asset-derived `SimulationTables`
    (`Support`/`Supporting`, tags from containing items, `TagId`, `FaceType`,
    `SupportDropType`, `MaxSupportDistance`, `FallingBlockSettings`,
    `Gathering`, random/scheduled procedures and default fluid tickers) and a
    deterministic `BlockSimulation` pass: changed-block scheduling, support
    removal (`Break`/`Destroy`/`Fall`), natural drops
    (`Physics`→`Breaking`→`Soft`→`Harvest`), one-cell falling-block movement,
    water/lava flow with demotion/collision conversion, `ChangeIntoBlock`/
    `SpreadTo`/`PlaceBlock` random ticks and transient `BlockHealth`
    regeneration.
  - `World` gained a tick counter, changed-block drain, rotation/filler/fluid
    accessors, `set_block_full`, `set_fluid`, `damage_block` and
    `block_health`; the block-health map is transient.
  - `WorldThread` gained `simulate(dt)`, `force_random_tick`, fluid read/write,
    `fluid_id`, `block_drops`, `damage_block` and `block_health` seams.
  - The server runs `simulate(0.05)` on every 50 ms loop tick and broadcasts
    `ServerSetBlock`/`ServerSetFluid` changes through the view-filtered player
    queues. Session summaries record simulation/block/fluid counters. Left-click
    break feeds the health map before replacing the block.
  - Tests cover synthetic support/drop/fall/fluid/health walkthroughs, real
    pack metadata for stone, grass, plant, water and falling barrels, and
    world-thread integration for all five behaviors.
- **Verified:**
  - `cargo test -p hytale-world` and `cargo test -p hytale-server` (including
    `tests/simulation.rs`).
  - `cargo test --workspace`, clippy `-D warnings` and fmt.
  - Existing `block_interactions`, `inventory_join`, `multi_join`, `trace_gate`
    and `auth_join` ignored suites.
- **Deviations:**
  - Fluid flow ignores partial-hitbox face coverage, waterlogging, fire/finite
    tickers, disabled-ticker config and sound events.
  - Support is direct-face only; `MaxSupportDistance`, filler-specific support,
    `SelfFaceType`, light-gated spread and tick suppression are not full ports.
  - Random tick ordering uses `FastRandom`, not Java `SplittableRandom`.
    `PlaceBlock` takes the first placement candidate.
  - Falling blocks are block movement, not ECS entities, with no impact
    break/explode effects.
  - Block health is transient (no `BlockHealthSection` save) and damage/fragility
    packets are T57. Drops are reported, not spawned as item entities (T56).
  - Builtin module lifecycle for the tick/fluid/falling plugins remains T60.
- **Follow-ups:**
  - T52b full fluid ticker parity (partial hitboxes, waterlogging, finite/fire,
    disabled tickers); T52c support/random-tick parity (support distance,
    filler/self-face, light/fluid gates, weighted placement); T52d Java
    `BlockHealthSection` storage/fragility. These are explicit tasks and do not
    block T53.
  - T56 item entities/drops; T57 damage/fragility presentation; T58 NPC block
    interaction; T60 builtin toggles.

---

## Session 41 — T52 parity follow-ups (2026-10-25)

- **Goal:** close the T52 fluid, support/random-tick and block-health parity
  gaps before T53.
- **Landed:**
  - T52b: `hytale-world::collision` exposes rotated hitbox boxes, union bounds,
    protrusion and filler offsets; `simulation` ports Java
    `blocksFluidFrom`/`isFullySolid`, waterlogging, `Default`/`Finite`/`Fire`
    tickers, ticker `Parent` inheritance, collision `PlaceFluid`/sound fields
    and the world `DisabledFluidTickers` set. `tools/java-dumper/run-fluid.sh`
    emits `testdata/golden/server/fluid/fluid-tickers.json` from the real
    `FluidTicker.blocksFluidFrom` and decoded tickers.
  - T52c: `MaxSupportDistance` chains, `SupportsRequiredFor`, filler/self-face
    rules, multi-cell protruding hitboxes, `WAITING_CHUNK`, `SpreadTo` light and
    fluid/revert gates, weighted `PlaceBlock`, `Basic/SplitChanceBlockGrowth`
    and the `TickSuppressionRegistry` seam. `WorldClock::sunlight_factor`
    matches Java and feeds the light gate. Random ticks use Java's stable
    once-per-cycle walk plus unstable picks.
  - T52d: new `chunk::section::block_health` with the Java
    `BlockHealthSection` v0 BSON layout, legacy `BlockHealthChunk` v2
    migration, section-backed damage/fragility/regeneration and
    `SimulationReport::health_deltas`. `tools/java-dumper/run-block-health.sh`
    emits the Java section fixture.
  - Tests: real-pack fluid edge cases, Java fluid fixture parity, support
    chain/disallowed/face rules, light/revert, weighted placement, health
    round-trip/save-reload and the Java health fixture.
- **Verified:**
  - `cargo test -p hytale-world`, `cargo test -p hytale-server --test simulation`.
  - `cargo clippy -p hytale-world -p hytale-server --all-targets -- -D warnings`.
  - `cargo fmt --all -- --check`.
  - Java probes: `run-fluid.sh` and `run-block-health.sh`.
- **Deviations:**
  - Fluid sound events are parsed but not played (presentation follows T57).
  - Fire rain/weather input is behind the seam and lands with T54.
  - Random-tick ordering is deterministic Rust, not Java `SplittableRandom`
    (per `GOLDEN_DATA.md`, schedule-dependent order is not a gate).
  - The `TickSuppressionRegistry` producer and builtin module lifecycle remain
    T60; this session provides the core suppression set and checks.
  - Legacy `BlockHealthChunk` is read/migrated; it is not written back.
- **Follow-ups:** T53; T56 item-entity drops; T57 damage/fragility packets and
  audio; T60 builtin module toggles.

---

## Session 42 — T53 start: entity gameplay core (in progress)

- **Goal:** begin entity stats, effects and combat basics.
- **Landed so far:**
  - `hytale-entities::stats`: `EntityStatType`, values, maps, static
    min/max modifiers, regeneration timers, reset behavior and pending update
    seam.
  - `hytale-entities::effects`: effect assets, active instances, overlap
    (`Ignore`/`Overwrite`/`Extend`), expiry, invulnerability and stat/damage
    modifier carrying.
  - `hytale-entities::damage`: damage causes/inheritance, resistance and
    outgoing modifiers, health-stat damage and death results.
  - `hytale-assets`: `Server/Entity/Stats` loaded into the P0 registry with
    `Unknown` at id 0; item `Utility.EntityStatsToClear` and
    `DisplayEntityStatsHUD` names now resolve through it.
- **Verified:**
  - `cargo test -p hytale-entities`.
  - `cargo test -p hytale-assets --test block_item_packets`.
  - `cargo clippy -p hytale-entities -p hytale-assets --all-targets -- -D warnings`.
- **Still open:** server/ECS component registration, condition evaluation,
  effect damage-over-time/calculators, damage/effect/stat packets, Java asset
  fixtures for effects/damage causes, and NPC/combat producers.

---

## Session 43 — T54 time and weather

- **Goal:** begin the time/weather milestone after T53's core.
- **Landed so far:**
  - `Server/Weathers` loaded into the P0 registry with Java's preloaded
    `Unknown` at id 0.
  - `hytale-world::weather`: per-environment 24-hour `WeatherForecasts`
    parsing, `WeatherForecastSeed`, Java weighted-selection epsilon rule,
    deterministic hourly selection, `ForcedWeather`, effective weather and
    environment lookup.
  - `WorldClock::current_hour`/`moon_phase` with the midday phase swap.
  - `WorldThread::weather_at`; sessions send `UpdateWeather` on join (0.5 s)
    and on effective-weather change (10 s).
  - Real-pack weather/environment tests, a forced-weather world-thread test and
    clock/weather unit tests.
- **Verified:**
  - `cargo test -p hytale-world -p hytale-server`.
  - `cargo clippy --workspace --all-targets -- -D warnings`.
  - `cargo fmt --all -- --check`.
- **Split follow-ups:** `UpdateWeathers` full asset-document parity/goldens and
  config-burst wiring, weather particles/screen effects/audio, forced-weather
  commands/editor preview and a live weather transition loopback test.
