# T31 — Player add/spawn, entity tracker and the game packet loop

**Status:** done (session 21)
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

## Deviations

- The self-spawn uses Java's empty model/skin/equipment forms; cosmetics and
  inventory components are not decoded from the player save yet.
- `WorldGameplayConfig` uses the protocol defaults with block breaking,
  gathering and placement enabled; the `GameplayConfig` asset is not loaded.
- World time sends one initial `UpdateTimeSettings`/`UpdateTime` at the
  configured durations and pause flag, but the clock does not advance yet.
- `ClientMovement` only drives the view centre; there is no physics, collision
  or anti-cheat. The teleport ack is accepted by id without position validation.
- The world tick runs inline on the connection task; chunk generation can block
  the async loop while a new view loads.
- World map settings, other players and entity tracking are still absent.

## Open questions

- The exact `EquipmentUpdate`/cosmetics payload needs a packet trace from a
  vanilla client.
- Whether the client requires `UpdateWorldMapSettings` before accepting gameplay
  input needs a real-client trace.
