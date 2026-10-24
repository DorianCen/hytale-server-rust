# T44 — Self-spawn equipment and cosmetics from the player save

**Status:** done (session 32)
**Package:** `hytale-server`, `hytale-world`, `client-bot`
**Depends on:** T42
**Agent fit:** medium

## Goal

Stop sending empty model/skin/equipment arrays in the joining player's self
`EntityUpdates`. Decode the player save and the auth skin, resolve item ids
against the registry, and emit Java-compatible defaults when a component is
missing.

## Deliverables

- Player-save decoding for:
  - `Model` (`PersistentModel` → `ModelReference`: id, scale, static, random
    attachments);
  - `PlayerSkin` (optional saved component; the live Java skin comes from auth);
  - `ArmorInventory`, `HotbarInventory`, `UtilityInventory`;
  - legacy `Inventory` (`Armor`/`HotBar`/`Utility` + active slots) fallback.
- Typed `PlayerAppearance` with armor slots, right/left held items and item-id
  validation (`resolve_items`).
- `WorldThread::player_appearance()` returning the loaded player's resolved
  appearance.
- Self-spawn `EntityUpdates` now carries:
  - `ModelUpdate` from the saved model reference (or `None`);
  - `PlayerSkinUpdate` from the insecure handshake skin, falling back to a
    saved `PlayerSkin` component;
  - `EquipmentUpdate.armorIds` with four slots and `""` for empty;
  - right/left hand ids, using Java `BlockType.EMPTY_KEY` (`"Empty"`) when
    empty.
- `client-bot` decodes the self-spawn component values, supports an insecure
  handshake skin, and asserts the defaults/equipment.
- Java golden fixture for a saved model and full armor/hotbar/utility set,
  regenerated and checked by the real `EntityStore.REGISTRY`.

## Read first

- `crates/hytale-world/src/player_data.rs`
- `crates/hytale-server/src/world_thread.rs`, `session.rs`
- `tools/java-dumper/src/main/java/com/hypixel/hytale/tools/golden/WorldPlayerDumper.java`
- Java `PersistentModel.java`, `InventoryComponent.java`, `Inventory.java`,
  `InventoryUtils.createEquipmentUpdate`, `PlayerSystems.PlayerAddedSystem`
- Protocol `ModelUpdate`/`PlayerSkinUpdate`/`EquipmentUpdate`

## Acceptance

```sh
cargo test -p hytale-world
cargo test -p hytale-server
cargo test -p client-bot --test dev_join -- --ignored
cargo test -p client-bot --test auth_join -- --ignored
cargo test -p client-bot --test multi_join -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
tools/java-dumper/run-player-data.sh --check
tools/java-dumper/run-player-data.sh --probe-equipment <rust-written-player.json>
```

- A saved player with equipment sends those armor ids and active items.
- An empty/default save sends four empty armor slots, `"Empty"` hands, no
  model/skin.
- Unknown item ids are dropped and do not fail the join.
- The Java player fixture reads and writes back through the real codec.

## Deviations

- The protocol `Model` is built from the saved `ModelReference` (asset id,
  scale, Java's unknown texture). Full `ModelAsset` expansion (geometry path,
  textures, animation sets, attachments, hitboxes) is not ported yet; it needs
  an asset-store task. The client still receives a non-empty model payload when
  the save names one.
- Skin is taken from `InsecurePlayerOptions.skin` first because that is where
  Java carries it. A saved `PlayerSkin` component is a Rust-side fallback; the
  local JWT profile has no cosmetic claims yet.
- The world thread loads one player at startup (the T42 model), so every
  session on that world currently receives that player's appearance. Per-session
  player data is T45/T47 work.
- Armor visibility settings from `WorldGameplayConfig.PlayerConfig` are not
  applied to self-spawn yet; the raw armor ids are sent.

## Follow-ups

- Full `ModelAsset` → protocol `Model` packet mapping (geometry, animation sets,
  attachments, hitbox, particles/trails) for both self and other players.
- Per-session player load/save and identity binding.
- Armor visibility and held-item updates after gameplay starts (T46/T47).
- Sheet inventory/HUD packets remain M5.
