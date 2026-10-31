# Connection-debug session handoff — 2026-10-31 (second run)

## Objective

Debug the release-client connection to the Rust server. **Only the user runs the
Hytale client.** The agent must never launch it.

## Current status

- The client authenticates, receives every config store, reaches `Playing`, and
  runs all world-load steps through `PrepareWorldMap`.
- It then dies with a **native SIGSEGV**, not a managed exception.
- The `Debug_Model` model-path warning is gone, so that fix was real but was not
  the crash.
- The crash is a runaway recursion in the client's interaction resolver. It was
  triggered by the server encoding "no interaction" as id `0` instead of Java's
  `AssetMapWithIndexes.NOT_FOUND` (`i32::MIN`).
- Interaction id `0` is the preloaded `*Change_Active_Slot`, which has no
  `Next`/`Failed`. The old payload therefore made node `0` point at itself and
  the client followed the chain until the stack was exhausted.

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

## Root cause

Java sends `Integer.MIN_VALUE` for an absent interaction reference:

- `AssetMapWithIndexes.NOT_FOUND = Integer.MIN_VALUE`
- `Interaction.getInteractionIdOrUnknown(null)` returns that sentinel.

The Rust encoders used `0` for absent references. Since `0` is a valid
interaction id, every absent `Next`/`Failed` became "go to interaction 0".
Interaction 0 has no `Next`, so its own encoded reference was `0` as well: a
one-node cycle the client resolves recursively.

## Fixes landed

- `crates/hytale-assets/src/packets/mod.rs`
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
  - `crates/hytale-assets/tests/block_item_packets.rs` now allows the sentinel
    while still rejecting dangling ids.

After the containment fix only 2 references remain unresolved (two root
interaction names used inside an interaction array); they are sent as
`NOT_FOUND` and the client skips them. A key-level walk of all 5,758 interaction
nodes found no cycles.

## Verification evidence

```text
./tools/test.sh fast
Summary [ 19.611s] 1552 tests run: 1552 passed, 24 skipped

HYTALE_TEST_ASSETS=1 cargo test -p hytale-assets
block_item_packets: 18 passed
interaction_packets: 1 passed
(all other hytale-assets suites passed)

cargo clippy --workspace --all-targets -- -D warnings   → clean
cargo fmt --all -- --check                              → clean
--validate-assets                                       → 50 stores, failures: 0
```

## What to do next

1. Rerun the release client against this commit.
2. If it passes `PrepareWorldMap`, the client should send `SetupFinalize`; then
   the server sends `ClearChunks` + `JoinWorld`. Report the next failing stage.
3. If it crashes again, capture the new client log, `coredumpctl info <pid>`,
   and the newest Sentry envelope. The next suspect is another asset id space
   that still uses `0` for "none" (block/effect/sound/item ids) or a long-tail
   interaction field listed below.
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
