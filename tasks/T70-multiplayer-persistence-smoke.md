# T70 — Multi-player and persistence smoke

**Status:** done (session 97)
**Package:** `hytale-server`, `tools/client-bot`, docs
**Depends on:** T45, T47, T49, T56, T59b, T69
**Agent fit:** low/medium

## Goal

Prove the combined multi-player and persistence path end to end with two real
loopback clients: both see each other's held items and a shared NPC, edits and
disconnects reach the peer, a reconnect restores the saved player, and a full
world-thread restart restores the edited blocks, both player documents and the
NPC runtime entity.

This is the smoke gate called out in
[`NEXT_STEPS.md`](../NEXT_STEPS.md) after the single-client block loop and the
focused T45/T47/T49/T56/T59b tests. It intentionally does not add a new load
generator or change the wire/asset contract.

## Deliverables

- New ignored loopback test
  `tools/client-bot/tests/multiplayer_persistence.rs`:
  - two clients join one shared `PlayerRegistry`/`WorldThread`;
  - both observe the shared passive NPC and each other's active held item;
  - the second client's block placement and break reach the first client;
  - `/save`, graceful disconnect and player/entity removal are observed;
  - reconnect restores the saved transform, inventory stack and connected peer;
  - the same save is reopened after `WorldThread::shutdown`, and the placed block,
    broken block, both player snapshots and the NPC entity are still present;
  - a fresh client joins the restarted world and receives the restored transform
    and full chunk snapshot.
- `client-bot` entity observation now exposes the remote equipment
  `right_hand`/`left_hand` ids, with a unit regression for the packet decoder.
- Task board, plan, next-steps and porting-history entries.

## Read first

- `crates/hytale-server/src/session.rs` (join inventory/broadcast order,
  `broadcast_held_items`, block interaction outcomes)
- `crates/hytale-server/src/players.rs` (`update_equipment`,
  `equipment_entity_updates`, runtime entity broadcasts)
- `crates/hytale-server/src/world_thread.rs` (`player_snapshot`,
  `spawn_npc`, `entities`, save/shutdown)
- `tools/client-bot/src/lib.rs` (`ObservedEntityUpdate`, `record_observed_entities`)
- `tools/client-bot/tests/multi_join.rs`, `inventory_join.rs`, `npc_scenario.rs`

## Acceptance

```sh
cargo test -p client-bot --lib
cargo test -p client-bot --test multiplayer_persistence -- --ignored
./tools/test.sh fast
cargo clippy -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
```

- Two clients see each other, their held items and one shared NPC.
- A peer edit and a peer disconnect are observed by the remaining client.
- Reconnect restores the saved transform/inventory; no duplicate session result
  is produced.
- A world restart preserves the edited blocks, both players and the NPC.
- A fresh client can join the restarted save and receives the restored player.

## Verification (session 97)

```sh
cargo test -p client-bot --lib
# 3 passed

cargo test -p client-bot --test multiplayer_persistence -- --ignored --nocapture
# 1 passed (115.87 s, asset-backed debug run)

./tools/test.sh fast
# 1619 passed, 29 skipped

cargo clippy -p client-bot --all-targets -- -D warnings
cargo fmt --all -- --check
# both clean
```

## Deviations

- The test drives one `WorldThread` plus `ServerEndpointBuilder` with a shared
  `PlayerRegistry` instead of the `serve`/universe listener. It needs a
  deterministic flat test area, a controlled NPC position and direct thread
  handles to restart the same save. The listener path remains covered by
  `multi_join`.
- A passive `Frog_Green` is used for the shared NPC. The aggressive `Rat` used by
  the NPC scenario can kill players during the long observation windows and
  makes inventory/reconnect assertions timing-dependent.
- The post-restart client asserts the restored transform and a full chunk
  snapshot. NPC persistence is asserted through the world thread's entity store
  because the passive critter can wander out of the radius-0 view before the
  later join; client-observed NPC visibility is covered in the first phase.
- Saved inventory after restart is asserted through `player_snapshot`; the join
  intentionally sends an empty inventory packet before the saved stacks, and the
  pre-restart reconnect already proves client-visible inventory restoration.
- The test is `#[ignore]`d because it needs the base-game pack and loopback UDP.

## Follow-ups

- A concurrent load generator (10/50/100 players) remains a dedicated T63/T64
  boundary item.
- Universe-level restart and multi-world handoff smoke can reuse this scenario
  once the listener test needs the same deterministic setup.
