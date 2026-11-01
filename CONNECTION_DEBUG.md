# Connection-debug session handoff — 2026-11-01 (eighth run)

## Objective

Debug the release-client connection to the Rust server. **Only the user runs the
Hytale client.** The agent must never launch it.

## Current status

- Authentication, setup, assets, world load and `JoinWorld` all succeed.
- Seventh run (`bff4877`) confirmed the first-join flags are now correct
  (`ProcessJoinWorldPacket: FadeInOut is False`, no `ClearChunks`), but the
  client still stalls on the chunk screen.
- Eighth run fixes the remaining divergence: the whole 9x9 snapshot was sent in
  one burst. The client read 1862 of the 2229 outbound packets and then stopped
  reading; the missing 367 packets are the tail of the chunk burst. That is the
  client's bounded chunk queue filling up: it waits for the still-buffered
  columns while the stream reader is blocked on the full queue.
- Java's `ChunkTracker` sends at most `MAX_SECTIONS_PER_TICK` (40) sections per
  30 TPS world tick. The Rust snapshot now paces four columns (40 sections) per
  33 ms tick. This fix has **not yet been confirmed by a client rerun**.
- The two `All rendered chunks were discarded.` warnings also occur in the
  seventh run *before* `Received JoinWorldPacket`; they are part of the client's
  world-preparation/teardown path, not the chunk rejection.

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
Summary [ 19.561s] 1555 tests run: 1555 passed, 24 skipped

cargo test -p client-bot --test auth_join -- --ignored        → 2 passed
cargo test -p client-bot --test dev_join -- --ignored         → 1 passed
cargo test -p client-bot --test portal_session -- --ignored   → 1 passed
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

1. Rerun the release client against this commit. The paced stream should let
   the client drain its chunk queue, send `ClientReady(readyForGameplay)`,
   receive `ClientTeleport`, and show the world.
2. If anything still fails, capture `target/trace/RealTest.jsonl`; records now
   include `ts_ms`, so server writes can be matched against the client log
   timestamps. World-map packets show `"stream":"world_map"` and data-less
   sections appear as `SetAbsentChunks`.
3. Known follow-up: the saved player transform is still `(0, 0, 0)`. Add a real
   surface spawn (Java spawn provider) so the player does not start inside the
   ground.
4. Do not speculate about protocol mismatch; protocol is proven identical.

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
- `crates/hytale-assets/src/registry.rs` — contained interaction registration
- `crates/hytale-assets/tests/interaction_packets.rs` — new sentinel/self-loop
  regression
- `crates/hytale-assets/tests/block_item_packets.rs` — interaction graph
  materialization
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
