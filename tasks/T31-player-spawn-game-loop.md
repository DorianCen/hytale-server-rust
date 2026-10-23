# T31 — Player add/spawn, entity tracker and the game packet loop

**Status:** planned
**Package:** `hytale-server`, `hytale-world`, `hytale-entities`, `client-bot`
**Depends on:** T30 (Game transition), T23 (world thread)
**Agent fit:** low/medium

## Goal

Complete the Java `World.addPlayer` → `PlayerSystems.sendPlayerSelf` path and
the live game loop. Session 20 already sends `AddToServerPlayerList` and a
minimal self `EntityUpdates` (model/skin/equipment defaults + the saved
transform); this task adds the real model/skin/equipment payloads, time and
gameplay config, the `readyForGameplay` teleport exchange, and movement-driven
view updates.

## Deliverables

- Populate the self-spawn `ModelUpdate`/`PlayerSkinUpdate`/`EquipmentUpdate`
  from the player save (or the Java-derived empty forms where no data exists).
- World time (`UpdateTime`/`UpdateTimeSettings`) and `WorldGameplayConfig`
  from the world config.
- `ClientReady(readyForGameplay)` handling and a `ClientTeleport`/ack exchange.
- A game packet loop that decodes movement and view changes, calls
  `WorldThread::set_view`, and streams incremental updates.
- `client-bot --auth-join` verifies the time/gameplay packets and a view update
  after a movement packet.

## Acceptance

```sh
cargo test -p hytale-server
cargo test -p client-bot --test auth_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

- The bot reports time and gameplay packets and then a view update after a
  movement packet.
- The self entity carries the same model/skin/equipment ids a Java server
  sends for a player with no custom cosmetics (or a traced payload).
- No packet is sent before its Java-order predecessor (player list before
  entity spawn, entity spawn before `readyForGameplay` teleport).

## Open questions

- The exact `EquipmentUpdate` payload needs a packet trace from a vanilla
  client; the Java `InventoryUtils.createEquipmentUpdate` path should be
  ported or stubbed with an empty update.
- `ClientTeleport` ack tracking is currently out of scope; if the client requires
  an ack before gameplay input, add `TeleportAck`.
