# T45 — Other-player entity tracking and broadcast

**Status:** done (session 33)
**Package:** `hytale-server`, `client-bot`
**Depends on:** T42, T44
**Agent fit:** low/medium

## Goal

Make multiplayer observable: players on the same world see each other spawn,
move, leave and appear in the server player list, with view-radius interest
filtering.

## Deliverables

- `hytale-server::players::PlayerRegistry`, one per served world:
  - assigns unique network ids;
  - stores UUID, username, world UUID, transform, appearance, auth skin, view
    centre/radius and an outbound message queue per session;
  - tracks each viewer's currently visible entity set.
- Join/leave:
  - the joining client receives the full `AddToServerPlayerList` snapshot;
  - existing players receive an `AddToServerPlayerList` entry;
  - visible players spawn in both directions;
  - leaving removes the entity where it was visible and removes the player-list
    entry everywhere.
- Movement and view changes:
  - `ClientMovement` updates the registry transform (100 ms throttle) and
    broadcasts `TransformUpdate` only to viewers that can see the entity;
  - chunk/view-radius changes recompute visibility and emit spawns/removals for
    entities entering/leaving the square chunk view.
- Session loop:
  - drains the per-session player-message queue and writes list/entity packets;
  - `SetClientId` uses the assigned network id;
  - the self spawn uses the same network id.
- Listener:
  - `serve` owns one shared `PlayerRegistry` for all accepted sessions.
- `client-bot`:
  - decodes the initial player-list UUIDs and post-join player-list/entity
    events;
  - can send a movement packet and observe for a window after joining;
  - can keep its setup stream alive and/or disconnect itself for visibility
    tests.
- Multi-client loopback test covers mutual spawn, mutual movement, disconnect
  removal and rejoin.

## Read first

- `crates/hytale-server/src/session.rs`, `serve.rs`, `world_thread.rs`
- `crates/hytale-server/src/players.rs` (new)
- `crates/hytale-world/src/player_data.rs`
- `tools/client-bot/src/lib.rs`, `tests/multi_join.rs`
- Java `EntityTrackerSystems`, `PlayerSystems.PlayerAddedSystem`,
  `InventoryUtils.createEquipmentUpdate`, `AddToServerPlayerList`

## Acceptance

```sh
cargo test -p hytale-server
cargo test -p client-bot --test multi_join -- --ignored
cargo test -p client-bot --test auth_join -- --ignored
cargo test -p client-bot --test dev_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Two clients see each other's spawn, movement and disconnect.
- No entity update leaks after a player leaves.
- Player list and self identity remain correct.
- Existing single-client and authenticated joins keep working.

## Deviations

- The registry stores the world thread's one loaded player appearance for every
  session; per-session player-save loading and appearance remains T47 work.
- Player list updates are global (not view filtered), matching Java. Entity
  spawn/update/removal is filtered by the square chunk view.
- Movement broadcasts are throttled to 10 Hz; a final stopped position is
  flushed on the next loop iteration after the 100 ms window.
- The registry queue is unbounded; per-session backpressure/rate limits remain
  T62 work.
- Remote entity transforms use the same body/look orientation, so look-only
  rotation is approximated until a dedicated pose component lands.

## Follow-ups

- T46 reuses the registry for block edit interest filtering and held-item
  consistency.
- T47 makes player data per session and persists live transforms on leave.
- T53 entity stats/effects and T54 time/weather can add more `ComponentUpdate`
  broadcasts to the same registry.
- A future optimization can batch entity updates per viewer instead of one
  `EntityUpdates` per change.
