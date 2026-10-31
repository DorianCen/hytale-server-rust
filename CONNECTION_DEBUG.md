# Connection-debug session handoff — 2026-10-31 (fourth run)

## Objective

Debug the release-client connection to the Rust server. **Only the user runs the
Hytale client.** The agent must never launch it.

## Current status

- Authentication, setup, assets, world load and `JoinWorld` all succeed now.
- The client enters `GameInstance.StartJoiningWorld()` and waits for chunks.
- The server streams 81 columns (810 `SetChunk`, 810
  `SetChunkEnvironments`, 81 `SetColumn`), then the client stops receiving and
  the QUIC connection times out after 30s.
- Cause: sections with no stored data were sent as `SetChunk` with
  `data = null`. Java's `ChunkTracker` marks those with `SetAbsentChunks`;
  without it the client "sees a never-arriving neighbour and stalls meshing" and
  never sends `ClientReady(readyForGameplay)`.
- Fixed by emitting `SetAbsentChunks` for data-less sections in the live chunk
  stream. Also restored Java's missing join packets: `UpdateSunSettings`,
  `UpdatePostFxSettings`, and world-map settings before the chunk batch.
- The fix has **not yet been confirmed by a rerun**.

## Evidence

Client log (ends mid-load, no exception):

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

## Fixes landed

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
  - `initial_for_client` / `full_for_client` replace `SetChunk` with
    `SetAbsentChunks` for sections whose `data` is `None`; the raw `initial` /
    `full` batches stay byte-identical to the Java dumper fixtures.
  - Regression: `chunk::stream::tests::client_batches_mark_absent_sections`.
- `crates/hytale-server/src/session.rs` / `world_thread.rs`
  - Send the world's `UpdateSunSettings` and `UpdatePostFxSettings` (Java
    `onFinishPlayerJoining`) and move `UpdateWorldMapSettings` before the chunk
    snapshot.
  - `WorldThread::client_effects()` exposes the world config's `ClientEffects`.

After the containment fix only 2 references remain unresolved (two root
interaction names used inside an interaction array); they are sent as
`NOT_FOUND` and the client skips them. A key-level walk of all 5,758 interaction
nodes found no cycles.

## Verification evidence

```text
./tools/test.sh fast
Summary [ 19.665s] 1554 tests run: 1554 passed, 24 skipped

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

1. Rerun the release client against this commit.
2. It should finish chunk loading, send `ClientReady(readyForGameplay)`, and
   receive `ClientTeleport`.
3. If it still stalls, capture `target/trace/RealTest.jsonl` and the client log;
   the trace now includes `SetAbsentChunks` and the sun/post-FX packets, so the
   next gap can be compared against Java's join burst.
4. Known follow-up: the saved player transform is still `(0, 0, 0)`; once the
   client joins, add a real surface spawn (Java's spawn provider) so the player
   does not start inside the ground.
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
