# T46 — Block place/break

**Status:** done (session 34)
**Package:** `hytale-server`, `hytale-world`, `client-bot`
**Depends on:** T42
**Agent fit:** medium

## Goal

Complete the v1 edit loop: decode the client's mouse interaction, validate the
target and held item, apply the edit on the world thread, broadcast the
authoritative block to interested clients and persist it through the normal
region save path.

## Deliverables

- `hytale-server::session`:
  - decode `MouseInteraction` on the default stream;
  - treat pressed left as break and pressed right as place;
  - validate target presence, entity-vs-block target, reach (5 + Java's 2-block
    buffer), and held-item presence;
  - reject invalid interactions without returning a session error.
- `WorldThread`:
  - `get_block` for loaded chunks;
  - `place_block` that only fills air;
  - `block_id_for_item` resolving an item id with a `BlockType` to the block id;
  - edits mark the chunk dirty through the existing `World::set_block` path.
- `PlayerRegistry`:
  - `PlayerMessage::BlockUpdate(ServerSetBlock)` written on the chunks stream;
  - `broadcast_block` sends to every viewer whose square chunk view contains the
    block, including the editor.
- `client-bot`:
  - sends `MouseInteraction` place/break packets;
  - reads `ServerSetBlock` on the chunk stream during observation;
  - multi-client loopback asserts one client sees another's placement and break.
- Tests:
  - valid/invalid interaction parsing (release, reach, missing item, unsupported
    button);
  - item → block id resolution and non-block rejection;
  - place/get/save/reload through `WorldThread`;
  - two-client block placement/break broadcast;
  - existing Java whole-save reload gate.

## Read first

- `crates/hytale-server/src/session.rs`, `world_thread.rs`, `players.rs`
- `crates/hytale-world/src/world.rs`, `chunk/column.rs`
- `tools/client-bot/src/lib.rs`, `tests/multi_join.rs`
- Java `GamePacketHandler.handleMouseInteraction`, `InteractionModule`,
  `InteractionValidation`, `ChunkSystems` (`ServerSetBlock`),
  `Item.hasBlockType`

## Acceptance

```sh
cargo test -p hytale-server
cargo test -p client-bot --test multi_join -- --ignored
cargo test -p client-bot --test auth_join -- --ignored
cargo test -p client-bot --test dev_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
tools/java-dumper/run-save-check.sh <rust-written-save> ...
```

- A bot places and breaks a block; another client sees both changes.
- The edit survives save/reload and the Java reload gate.
- Invalid targets/ids/reach are rejected without killing the session.

## Deviations

- Block placement uses the packet's `itemInHandId` and resolves it against the
  item/block registries. There is no live inventory decrement yet; full
  inventory consistency remains T49.
- The edit broadcast uses filler `0` and rotation `0`; rotations, filler and
  block-component updates are later gameplay work.
- The server does not yet emit block damage/breaking particles or sounds.
- Only the pressed mouse-button state is handled (releases and mouse motion are
  ignored).
- Reach uses the Adventure default (5) plus Java's 2-block buffer; per-item
  `UseDistance` and creative reach are not loaded yet.

## Follow-ups

- T47 saves the live player transform and reconnects with the edited world.
- T49 ports the inventory and held-item transactions so placing consumes items.
- T50 dispatches the full interaction graph (use/primary/secondary, block
  components, doors/benches/containers).
- T52 adds block ticks/fluids/support; block entity components follow.
