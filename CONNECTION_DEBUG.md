# Connection-debug session handoff — 2026-11-01 (fourteenth run)

## Objective

Debug the release-client connection to the Rust server. **Only the user runs the
Hytale client.** The agent must never launch it.

## Current status

- **The client now joins successfully.** The eighth run (`6053350`) fixed the
  chunk-screen stall by pacing the snapshot.
- Ninth run (`2026-11-01_10-10-25_client.log`) loaded the world, entered
  `InGame`, and showed terrain. Three follow-up problems were found:
  1. The player spawned at `(0, 0, 0)`, inside the terrain.
  2. No HUD was visible.
  3. The client disconnected after ~30 s with no server packets, although the
     server process stayed up.
- Root cause of the 30 s disconnect: the first game-loop tick blocked for ~30 s
  on lazily loaded NPC catalogs (`tick_npc_spawns` 23.3 s,
  `tick_spawn_markers` 6.6 s). The client's `ClientReady(ready_for_gameplay)`,
  `RequestNoClip` and movement packets sat unread until after the QUIC idle
  timeout. The catalogs are now preloaded at world boot; the preload itself was
  then reduced from ~32 s to ~0.3 s by reading each catalog with a single zip
  open (`AssetPack::read_many`) instead of reopening the 3.7 GB pack per file.
- The HUD was missing because Java's
  `PlayerHudManagerSystems.InitializeSystem` sends
  `UpdateVisibleHudComponents(DEFAULT_HUD_COMPONENTS)` when the player entity is
  added; the Rust server never sent it. It now does, after the stat map.
- The spawn is now resolved one block above the highest non-air block of the
  spawn column when the saved transform is absent or inside terrain. The
  ninth-run client still spawned inside the ground because the surface fix only
  ran for the world-boot player; `player_snapshot` now applies it to the
  **authenticated** player on every join.
- The client sends no `MouseInteraction` packets while embedded in terrain, and
  its local collision blocks movement. Surface spawn should unblock both. It
  also receives an empty inventory, so number-key item switching has nothing to
  select; middle-mouse pick-block needs a valid block target.
- The tenth run exposed that the origin column is a **one-block ledge at a
  cliff**: heights around `(0,0)` are `0,0,107,110,111` (x = -2..2). Picking the
  highest block of that column left the player unable to move. The spawn
  resolver now loads a 3x3 chunk area and picks the nearest column with a
  two-block air gap and neighbours within one block of its ground height.
- The tenth run then disconnected with 1505 `ClientMovement` and 1281
  `ClientTeleport` packets. The trace's `ServerDisconnect` hash decodes to
  **"packet rate limit exceeded"**. Movement corrections are now rate-limited
  to 10/s, and the default inbound limits were raised to 2048/s with a 4096
  burst.
- The eleventh run joined and could move, but the camera sat at foot level and
  the client logged `No animation with id StepRun on entity with model ` (empty
  model). The self-spawn `ModelUpdate` only carried `assetId`; Java's
  `Model.toPacket` also sends `path`, `texture`, `eyeHeight`, the animation sets
  and the hitbox. Without them the client has no eye offset or animation sets,
  so the camera stays at the entity origin and the interaction raycast starts
  at the feet (which also breaks block targeting). The full
  `Server/Models/Human/Player.json` packet is now sent.
- The twelfth run still had no first-person hand and crashed with a
  `TargetInvocationException` wrapping a client `NullReferenceException` when
  the player tried to hit a block. The model packet was still missing the
  model's `Camera` (first-person offset), `DetailBoxes`, `DefaultAttachments`
  and gradients. Those are now included too. The crash address
  (`0x6f3e45`) is in the client's reflection-invoked interaction/rendering
  path; the missing camera/attachment data is the leading suspect.
- No `MouseInteraction` packets reached the server before the crash, so the
  failure is client-side before the interaction is sent.
- The thirteenth run crashed again after moving for ~1.3 s, with the same
  stack. The server sent no packet immediately before the crash (only the
  initial `ClientTeleport` and `UpdateTime`), so the failure is local to the
  client's movement/animation/stat code. The self-spawn was still missing the
  `MovementStates`, `EntityEffects` and `EntityStats` components; the client's
  stat map warnings (`No EntityStatValue found`) show those were applied
  late/empty. The self-spawn now follows Java
  `PlayerSystems.sendPlayerSelf` order (Model, Skin, Equipment, Transform,
  effects, stats) and adds a default `MovementStates` component.
- The empty-hand interaction chain was incomplete: `UpdateUnarmedInteractions`
  only carried `SwapFrom`. Java's `UnarmedInteractions.Empty` also defines
  `Primary`, `Use`, `Pick` and `Ability3`; without them the client never starts
  block-break, use or pick interactions. The registry now loads the inline roots
  from `Server/Item/Unarmed/Interactions/Empty.json` and the packet maps every
  type to its root-interaction index.
- Server-side commands are still not implemented: `ChatMessage` is ignored and
  no `CommandTreeSync` is sent. This is a separate feature from the join path.

## Evidence

Sixth-run client log:

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_23-21-54_client.log
```

Seventh run:

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_23-38-17_client.log
```

The decisive new lines are immediately after `Received JoinWorldPacket`:

```text
Received JoinWorldPacket
All rendered chunks were discarded.
All rendered chunks were discarded.
GameInstance.OnSetupComplete()
GameInstance.StartJoiningWorld()
```

The same warning appears in older failure logs only during teardown, so it is
the client's world-clear path. `ProcessJoinWorldPacket: FadeInOut is True`
confirms the server asked for a transition on a fresh join.

## Earlier evidence

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_21-34-06_client.log
```

Last lines:

```text
Stage SettingUp -> Playing took 119ms
Loading world: PrepareRootInterations: 0ms
PrepareAmbienceFX: 0ms
Loading world: PrepareInterations: 0ms
PrepareBlockOverlayAtlas: 0ms
Loading world: PrepareWorldMap: 2ms
```

Crash dump (`coredumpctl info 533403`):

- `SIGSEGV` on the main thread, `RSP` exactly at the stack guard page.
- The crashing stack repeats `HytaleClient + 0xa561fd` hundreds of times, i.e.
  unbounded recursion, not a nil dereference.
- Walking the stack out of the recursion locates `HytaleClient + 0xa55e60`,
  whose string literals include:
  `Interaction index {0} is out of range or references an unpopulated slot; skipping malformed reference`.

Sentry envelope (fatal SIGSEGV at `2026-10-31T20:34:16.683Z`):

```text
.../UserData/.sentry-cache/SentryNative/419ac751-.../b47a0f16-....envelope
```

Its breadcrumbs also stop at `PrepareWorldMap`.

Server side:

```text
session error: connection handshake timed out
```

The trace ends at `WorldLoadFinished`; the client crashed before sending
`SetupFinalize`, so the server never enters the game stage.

Third run (after the interaction fix):

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_22-14-22_client.log
```

The client survives the load and then reports:

```text
ERROR|HytaleClient.Networking.PacketHandler|Exception when handling packet 22 WorldLoadFinished:
  System.NullReferenceException
   at HytaleClient!<BaseAddress>+0x9db009
   at HytaleClient!<BaseAddress>+0x9d98c8
   at HytaleClient!<BaseAddress>+0x5521eb
   ...
```

`0x9db009` is `movss 0x8(%rdi)` with `rdi` loaded from element offset `+0xa0`.
Disassembling the dispatcher shows the handler at `0x551da0` is the
`UpdateWeathers` path (it clones the Weather store), and the `+0xa0` object is a
24-byte `NearFar`-shaped object (`float Near`, `float Far`).

## Root cause

Java sends `Integer.MIN_VALUE` for an absent interaction reference:

- `AssetMapWithIndexes.NOT_FOUND = Integer.MIN_VALUE`
- `Interaction.getInteractionIdOrUnknown(null)` returns that sentinel.

The Rust encoders used `0` for absent references. Since `0` is a valid
interaction id, every absent `Next`/`Failed` became "go to interaction 0".
Interaction 0 has no `Next`, so its own encoded reference was `0` as well: a
one-node cycle the client resolves recursively.

## Root cause (third run)

`Weather.Fog` was `null`. Java's `Weather.toPacket()` always assigns:

```java
packet.fog = new NearFar(fogDistance[0], fogDistance[1]);
```

with `DEFAULT_FOG_DISTANCE = {-96, 1024}`. The Rust encoder only filled
`id`/`tagIndexes`/`stars`/`moons` and took `Weather::default()` for everything
else, so every weather carried `fog: null`. The client clones the weather store
on `WorldLoadFinished` and dereferences `Fog` without a null check.

Note `Creative_Hub` has no `FogDistance` of its own but inherits
`Forgotten_Temple` (`-32, 512`); the merged asset value is used.

Fourth run (after the Weather fix):

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_22-51-37_client.log
```

The client now reaches:

```text
Received JoinWorldPacket
GameInstance.OnSetupComplete()
ProcessJoinWorldPacket: FadeInOut is True
GameInstance.PrepareJoiningWorld()
GameInstance.StartJoiningWorld()
[Voice] World transition cleared playbacks and speaker state
```

and then only:

```text
ReceiveLoop: No packets received for 5.0s (total: 1862)
... (repeated)
Connection is closed after 1862 packets
Disconnected during loading (network): Connection timed out
```

Trace (`target/trace/RealTest.jsonl`) shows the server streamed:

```text
setColumn                81
SetChunk                 810
SetChunkEnvironments     810
UpdateWorldMapSettings     1
```

## Root cause (fourth run)

Sections with no stored data were sent as `SetChunk` with `data = null`.
Java's `ChunkTracker` deliberately marks such sections with `SetAbsentChunks`
instead (see its `markAbsent` comment: without it "the client sees a
never-arriving neighbour and stalls meshing of the section adjacent to a
column's terrain top / bottom"). The client therefore never finished its chunk
tracker, never sent `ClientReady(readyForGameplay)`, and the QUIC connection
idled out after 30 s.

Fifth run (after `SetAbsentChunks`):

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_23-08-54_client.log
```

The client still waits on the loading screen, then at 23:09:37 (32 s later)
processes the buffered setup packets and reports:

```text
Exception when handling packet 60 UpdateRecipes: Received UpdateRecipes at
Playing connection stage but expected it only during SettingUp.
StreamReassembler: failed to deserialize packet on channel Default
  Packet UpdateWorldMap (ID 241) is not valid on channel Default (expected WorldMap)
A Equipment packet had been received on the local player
Unhandled exception: Attempted to set character model without sending a model first
```

## Root cause (fifth run)

Three independent join-path divergences from Java:

1. `UpdateWorldMap` is a `WorldMap`-channel packet. Java opens **two**
   server-initiated unidirectional streams at setup, `Chunks` then `WorldMap`;
   the Rust server only opened `Chunks`, so `UpdateWorldMap` was written on the
   default stream and rejected.
2. `UpdateRecipes` is a Setup-stage asset packet (already sent in the config
   burst). The join path sent it a second time after `JoinWorld`, which the
   client rejects at `Playing`.
3. A brand-new player has no saved `Model` component, so `ModelUpdate.model`
   was null. An `Equipment` update then arrived first, and the client threw
   "Attempted to set character model without sending a model first".

Sixth run (`bc5a6a9`):

```text
/home/chome/.var/app/com.hypixel.HytaleLauncher/data/Hytale/UserData/Logs/2026-10-31_23-21-54_client.log
```

All previous errors are gone, but the client still sits on the chunk screen:

```text
GameInstance.StartJoiningWorld()
ReceiveLoop: No packets received for 5.0s (total: 1862)   (repeated)
Connection is closed after 1862 packets
```

The trace shows the full initial snapshot: 81 `SetColumn`, 395 `SetChunk`,
395 `SetChunkEnvironments`, 415 `SetAbsentChunks`. `ViewRadius` from the client
was decoded as `128` blocks (4 chunks), so 9x9 columns is the expected area.

## Root cause (sixth run)

The hybrid absent markers did not help because the chunks were not being
rejected: they were being **discarded after rendering**. The client's log shows
`All rendered chunks were discarded.` immediately after `JoinWorld`, and
`FadeInOut is True`.

## Root cause (seventh run)

Java's first-world add path in `Universe` is:

```java
world.addPlayer(playerRefComponent, null, false, false);
```

so `onSetupPlayerJoining` builds `new JoinWorld(false, false, uuid, 0, 9)` and
skips `ClearChunks` entirely. The Rust server sent `ClearChunks` and
`JoinWorld { clear_world: true, fade_in_out: true }`. The client interpreted the
fresh join as a world transition, rendered the incoming chunks, then discarded
them when it entered the new world. Its chunk tracker was left waiting for
sections that the server had already sent once and never resent.

## Root cause (ninth run)

The client log freezes at `total: 3202` packets after `GameInstance.OnWorldJoined`.
The server trace shows a 33.4 s gap between the last chunk/world-map packet and
the first `UpdateTime`, then it reads the buffered `RequestNoClip` and 70
`ClientMovement` packets at once. Instrumenting the game loop showed:

```text
[join-diag] tick_npc_spawns took 23.298948378s
[join-diag] tick_spawn_markers took 6.583950597s
```

Both commands load their catalogs lazily on first use. `RoleCatalog::load`,
`SpawnMarkerCatalog::load` and `BeaconCatalog::load` called `AssetPack::read` per
file, reopening the 3.7 GB zip each time. They now collect the paths and call
`read_many`, which opens the archive once; the whole preload dropped from 32.2 s
to 0.28 s.

## Root cause (eighth run)

The client's packet counter freezes at 1862 while the trace contains 2229
outbound packets. The 1862nd outbound packet is `seq 1867`, in the middle of the
chunk burst; the remaining 14 columns (about 364 packets) plus the world-map
updates and `UpdateTime` packets were never read. The client's chunk queue is
bounded, and the single-burst snapshot overruns it: the game thread waits for
columns that are still sitting behind the full queue, so the stream reader
never resumes.

Java avoids this by pacing: `ChunkTracker.MAX_SECTIONS_PER_TICK = 40`, sent once
per 30 TPS world tick (local connections allow 2560 sections/s, still capped at
40/tick). A non-cubic column has 10 sections, so four columns per 33 ms.

## Fixes landed

- `crates/hytale-server/src/world_thread.rs`
  - Preload `RoleCatalog`, `WorldSpawnCatalog`, `SpawnMarkerCatalog` and
    `BeaconCatalog` at world boot instead of on the first game-loop tick.
  - Resolve a safe surface spawn near the origin: load a 3x3 chunk area and pick
    the nearest column with a two-block air gap and flat-enough neighbours.
    Applied to the world-boot player and to every authenticated join near the
    spawn.
- `crates/hytale-server/src/session.rs`
  - Rate-limit movement corrections to at most one per 100 ms and ignore
    sub-0.05-block differences, so a stuck client cannot trigger a teleport
    flood.
  - Include the player `EntityEffectsUpdate`, `EntityStatsUpdate` (last, like
    Java) and a default `MovementStatesUpdate` in the self-spawn `EntityUpdates`.
- `crates/hytale-server/src/main.rs`
  - Raise the default inbound limits to 2048 packets/s and a 4096 burst.
- `crates/hytale-npc/src/role.rs` / `spawn_special.rs`
  - Load catalogs with one `AssetPack::read_many` call instead of reopening the
    zip for every JSON file.
- `crates/hytale-server/src/session.rs`
  - Send Java's default `UpdateVisibleHudComponents` after the player stat map.
- `crates/hytale-server/src/serve.rs`
  - Log `session accepted from ...` and a per-session end summary while the
    server is running.
- `crates/hytale-server/src/chunk_streamer.rs`
  - Initial snapshots are paced at four columns per 33 ms, matching Java's
    `MAX_SECTIONS_PER_TICK` budget. Incremental `ChunkLoaded` sends are still
    immediate.
- `crates/hytale-net/src/trace.rs`
  - Trace packet records now carry `ts_ms` (Unix milliseconds) so future runs
    can correlate server writes with client log timestamps.
- `crates/hytale-server/src/session.rs`
  - Fresh joins send `JoinWorld { clear_world: false, fade_in_out: false }` and
    no `ClearChunks`, matching Java `Universe.addPlayer(..., false, false)`.
  - World switches keep `ClearChunks + JoinWorld { clear_world: true }` but use
    `fade_in_out: false` (Java `transferPlayerAsync`).
- `tools/client-bot`
  - The bot accepts the server-opened `Chunks` stream only after sending
    `ClientReady`; a QUIC unidirectional stream is not observable by the peer
    until its first frame, which is `ViewRadius` (written after ready).
  - `dev_join` now reads the buffered `WorldMap` stream directly for markers.
- `crates/hytale-assets/src/packets/mod.rs`
  - `Weather.Fog` is now always populated from `FogDistance`, defaulting to
    Java's `{-96, 1024}`.
  - `Weather.ScreenEffect` is now forwarded as well.
  - Added `INTERACTION_NOT_FOUND` (`i32::MIN`) and used it for every interaction
    reference: `Next`, `Failed`, `Blocked`, `Click`/`Held`, the eight
    `MovementCondition` directions, `ForkInteractions`, ground/collision next,
    `Serial`/`Parallel` child arrays, root-interaction children and the unarmed
    `SwapFrom` binding.
  - `DamageEntity.Blocked` and the `MovementCondition` directions are now
    resolved from JSON instead of being hardcoded to `0`.
- `crates/hytale-assets/src/registry.rs` / `packets/mod.rs`
  - Load `Server/Item/Unarmed/Interactions/Empty.json`, register its inline
    `Primary`/`Use`/`Pick`/`Ability3` roots (and their contained interactions),
    and map every type in `UpdateUnarmedInteractions` to its root index.
  - Load `Server/Models/Human/Player.json` as the full protocol `Model`
    (`path`, `texture`, gradients, eye/crouch/sit/sleep offsets, animation
    sets, attachments, hitbox, first-person camera, detail boxes) and attach it
    to `PlayerAppearance`. Regression:
    `default_player_model_carries_eye_height_and_animations`.
- `crates/hytale-assets/src/registry.rs`
  - Inline objects in `Interactions` arrays are registered as contained
    interaction assets at any nesting depth, matching Java's
    `CHILD_ASSET_CODEC_ARRAY`. Previously only root-interaction arrays were
    registered, so many references collapsed to `0`.
- Regressions:
  - `crates/hytale-assets/tests/interaction_packets.rs`
    - `absent_interaction_references_use_not_found`
  - `crates/hytale-assets/tests/block_item_packets.rs`
    - `weather_fog_is_always_sent`
    - interaction dangling-id checks now allow the sentinel.
- `crates/hytale-world/src/chunk/stream.rs`
  - `initial_for_client` / `full_for_client` keep the raw
    `SetChunk`/`SetChunkEnvironments` pair and additionally emit
    `SetAbsentChunks` for sections whose `data` is `None`; the raw
    `initial`/`full` batches stay byte-identical to the Java dumper fixtures.
  - Regression: `chunk::stream::tests::client_batch_adds_absent_markers_for_data_less_sections`.
- `crates/hytale-server/src/session.rs` / `world_thread.rs`
  - Send the world's `UpdateSunSettings` and `UpdatePostFxSettings` (Java
    `onFinishPlayerJoining`) and move `UpdateWorldMapSettings` before the chunk
    snapshot.
  - `WorldThread::client_effects()` exposes the world config's `ClientEffects`.
- `crates/hytale-net/src/chunk_stream.rs` / `trace.rs`
  - Added `WorldMapStreamWriter` and a `WorldMap` trace stream. The server now
    opens `Chunks` then `WorldMap` (Java order), and every `UpdateWorldMap`
    write goes to the world-map stream.
- `crates/hytale-server/src/session.rs`
  - Removed the duplicate join-time `UpdateRecipes`; the Setup asset burst
    already carries it.
- `crates/hytale-server/src/players.rs`
  - `spawn_entity_updates` always sends a `ModelUpdate` with the default
    `Player` model when the saved appearance has none, so the following
    Equipment update is accepted.
  - Regression: `spawn_carries_a_default_model_for_a_new_player`.

After the containment fix only 2 references remain unresolved (two root
interaction names used inside an interaction array); they are sent as
`NOT_FOUND` and the client skips them. A key-level walk of all 5,758 interaction
nodes found no cycles.

## Verification evidence

```text
./tools/test.sh fast
Summary [ 19.552s] 1555 tests run: 1555 passed, 24 skipped

cargo test -p client-bot --test auth_join -- --ignored        → 2 passed (49 s)
(dev server + client-bot --observe 35: 35 UpdateTime, HUD packet, ClientTeleport, no gap)
cargo test -p client-bot --test dev_join -- --ignored         → 1 passed (45 s)
cargo test -p client-bot --test portal_session -- --ignored   → 1 passed (93 s)
(pacing keeps these loopback joins green)

HYTALE_TEST_ASSETS=1 cargo test -p hytale-assets
block_item_packets: 19 passed
interaction_packets: 1 passed
(all other hytale-assets suites passed)

cargo test -p hytale-world --test chunk_packets_golden   → passes
cargo clippy --workspace --all-targets -- -D warnings   → clean
cargo fmt --all -- --check                              → clean
--validate-assets                                       → 50 stores, failures: 0
```

## What to do next

1. Rerun the release client against this commit. It should join, spawn on the
   surface, show the HUD, and stay connected past 30 s.
2. If the HUD is still missing or the stat warnings persist, capture the client
   log; the next suspect is the order of the player `EntityStatsUpdate` versus
   the self-spawn `EquipmentUpdate`.
3. If anything still fails, capture `target/trace/RealTest.jsonl`; records
   include `ts_ms` for client/server timing correlation.
4. Known follow-up: the server is single-player and joins the world's stored
   player, not necessarily the authenticated UUID. Per-join player identity and
   a real Java `SpawnProvider` (including `Global`/`FitToHeightMap`) are still
   future work.
5. Do not speculate about protocol mismatch; protocol is proven identical.

## Commands

Create the world:

```bash
cargo run --release -p hytale-server -- universe --root target/universe create RealTest
cargo run --release -p hytale-server -- universe --root target/universe default RealTest
```

Launch:

```bash
cargo run --release -p hytale-server -- serve \
  --universe target/universe \
  --world RealTest \
  --auth-mode authenticated \
  --trace target/trace/RealTest.jsonl
```

Server errors are printed as `session error: ...`. The trace records packet
ids, channels, order and payload hashes.

## Important files

- `crates/hytale-assets/src/packets/mod.rs` — interaction encoders and
  `INTERACTION_NOT_FOUND`
- `crates/hytale-assets/src/registry.rs` / `packets/mod.rs`
  - Load `Server/Item/Unarmed/Interactions/Empty.json`, register its inline
    `Primary`/`Use`/`Pick`/`Ability3` roots (and their contained interactions),
    and map every type in `UpdateUnarmedInteractions` to its root index.
  - Load `Server/Models/Human/Player.json` as the full protocol `Model`
    (`path`, `texture`, gradients, eye/crouch/sit/sleep offsets, animation
    sets, attachments, hitbox, first-person camera, detail boxes) and attach it
    to `PlayerAppearance`. Regression:
    `default_player_model_carries_eye_height_and_animations`.
- `crates/hytale-assets/src/registry.rs` — contained interaction registration
- `crates/hytale-assets/tests/interaction_packets.rs` — new sentinel/self-loop
  regression
- `crates/hytale-assets/tests/block_item_packets.rs` — interaction graph
  materialization
- `crates/hytale-assets/src/registry.rs` / `packets/mod.rs`
  - Load `Server/Item/Unarmed/Interactions/Empty.json`, register its inline
    `Primary`/`Use`/`Pick`/`Ability3` roots (and their contained interactions),
    and map every type in `UpdateUnarmedInteractions` to its root index.
  - Load `Server/Models/Human/Player.json` as the full protocol `Model`
    (`path`, `texture`, gradients, eye/crouch/sit/sleep offsets, animation
    sets, attachments, hitbox, first-person camera, detail boxes) and attach it
    to `PlayerAppearance`. Regression:
    `default_player_model_carries_eye_height_and_animations`.
- `crates/hytale-assets/src/registry.rs` — block preloads and translations
- `crates/hytale-server/src/session.rs` — auth/setup/asset burst
- `tasks/T66-live-client-assets.md` — current T66 status
- `PORTING_HISTORY.md` — Session 73 entry
- `JAVA_VERSION_DIFF.md` — 0.7.2 vs 0.7.5 comparison

## Remaining T66 gaps

- Confirm this interaction-sentinel fix with the release client; T66 is **not
  complete** until the client remains in `Playing` through world load, spawn,
  movement and a block edit.
- Long-tail interaction variants still use the `Simple` base form: `Chaining`,
  the `MemoriesCondition` `Next` map, the `Charging`/`Wielding` next maps,
  `PlaceModeSelect`, `ChangeActiveSlot` details and similar.
- Other asset id spaces still encode "missing" as `0`; Java uses
  `NOT_FOUND`/`-1` for several of them, so the same class of bug may appear next.
