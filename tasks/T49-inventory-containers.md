# T49 — Inventory, containers and transactions

**Status:** done (session 37)
**Package:** new `hytale-inventory`, `hytale-world`, `hytale-server`, `client-bot`
**Depends on:** T48
**Agent fit:** medium

## Goal

Replace the appearance-only inventory decoding with a real item-stack/container
model, wire the player's held item and active slots into gameplay, and preserve
inventory state through save/reconnect without regressing the v1 edit loop.

## Deliverables

- `hytale-inventory`:
  - `ItemStack` codec-compatible with Java (`Id`, `Quantity`, `Durability`,
    `MaxDurability`, `QualityOverride`, `Metadata`,
    `OverrideDroppedItemAnimation`);
  - `SimpleItemContainer` with merge/add/remove/clear and atomic target-slot
    move/split/swap semantics;
  - `PlayerInventory` for hotbar, utility, armor, storage, tools and backpack,
    including Java active-slot clamping and modern/legacy component loading.
- `hytale-world`:
  - `PlayerData::inventory()` / `set_inventory()` over the existing holder
    component map.
- `hytale-server`:
  - `PlayerSnapshot` carries the inventory;
  - teardown saves transform + inventory together;
  - initial `UpdatePlayerInventory` and hotbar/utility/tools `SetActiveSlot`
    packets on join;
  - `SetActiveSlot` handling and held-item equipment broadcasts;
  - block placement consumes one item from the authoritative active hotbar
    stack and sends an inventory update;
  - `DropItemStack` removes the requested quantity and resyncs the inventory.
- `client-bot`:
  - decodes `UpdatePlayerInventory`/`SetActiveSlot`;
  - inventory join test: seeded hotbar `Rock_Stone ×5`, place one, reconnect, and
    assert the saved `×4` inventory is restored.
- Tests:
  - scripted add/merge/split/swap/drop/pickup scenarios (Java container rules);
  - Java player fixture inventory round trip;
  - world-thread transform + inventory save/reload;
  - inventory join/consume/reconnect loopback.

## Read first

- Java `ItemStack.java`, `ItemContainer.java`, `SimpleItemContainer.java`,
  `InternalContainerUtilItemStack.java`, `InventoryComponent.java`,
  `ActiveSlotInventoryComponent.java`, `PlayerSystems.java`,
  `InventoryPacketHandler.java`
- `crates/hytale-world/src/player_data.rs`
- `crates/hytale-server/src/world_thread.rs`, `session.rs`, `players.rs`
- `tools/client-bot/src/lib.rs`, `tests/multi_join.rs`

## Acceptance

```sh
cargo test -p hytale-inventory
cargo test -p hytale-world
cargo test -p hytale-server
cargo test -p client-bot --test inventory_join -- --ignored
cargo test -p client-bot --test multi_join -- --ignored
cargo test -p client-bot --test auth_join -- --ignored
cargo test -p client-bot --test dev_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
tools/java-dumper/run-player-data.sh --probe <rust-written-player.json>
```

- Item move/split/merge/drop/pickup scenarios follow Java's container semantics.
- Placing a block consumes the authoritative held item.
- Inventory survives save/reconnect and the Java whole-save gate.

## Deviations

- Inventory windows, containers-as-block-entities, item entity spawn/pickup
  visuals and the full `MoveItemStack`/`SmartMoveItemStack` server packet path
  remain later gameplay work. The pure container transactions are implemented
  and tested; server integration covers active slots, held-item consumption,
  drop removal and save/reconnect.
- A `DropItemStack` removes the item and resyncs the client, but does not spawn
  a world item entity yet (T56 runtime entities).
- When a session has no saved inventory (legacy/debug joins), placement falls
  back to the packet's `itemInHandId` so the existing v1 edit loop stays green.
  A real saved inventory is authoritative.
- The server sends all six inventory sections on join even when a component was
  absent; Java accepts nullable/empty sections. Save writes only present or
  non-empty components.
- Quality override `-1` round trips through `Option<i32>`; item-asset max
  stack/durability is supplied by `ItemRules` implementations (the crate tests
  use a deterministic rules table).

## Follow-ups

- T50/T56 wires block containers and dropped/runtime item entities.
- T53 consumes item utility/stat metadata.
- T57 adds inventory UI presentation.
