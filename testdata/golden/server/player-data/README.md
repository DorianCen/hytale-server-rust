# Player data fixtures

Both files are written by the real Java `EntityStore.REGISTRY.serialize(holder)`
and `BsonUtil.writeDocument` — the same path
`DiskPlayerStorageProvider.DiskPlayerStorage.save` uses for
`<world>/players/<uuid>.json`.

Regenerate and verify determinism:

```sh
tools/java-dumper/run-player-data.sh
tools/java-dumper/run-player-data.sh --check
```

`WorldPlayerDumper` registers the component ids from `EntityModule.setup()` and
builds holders with fixed values. No full server boot is needed; the entity
store codecs are static. Item stacks are decoded through `ItemStack.CODEC`
instead of the item asset store, so the dumper does not need the asset pipeline.

## `12345678-1234-5678-9abc-def012345678.json` — core holder

| Component | Fields |
|---|---|
| `UUID` | `12345678-1234-5678-9abc-def012345678` |
| `Transform` | position `(10.5, 80.0, -3.5)`, rotation `Pitch=0.25`, `Yaw=1.5707964`, `Roll=-0.125` |
| `WorldGenId` | `42` |

## `22345678-1234-5678-9abc-def012345678.json` — model and equipment

The same transform/world-gen fields plus:

| Component | Fields |
|---|---|
| `Model` | `PersistentModel` → `ModelReference(Id=Player, Scale=1.0, Static=false)` |
| `ArmorInventory` | capacity 4; `[0]=Armor_Adamantite_Head`, `[1]=Armor_Adamantite_Chest`, `[2]=Armor_Adamantite_Hands`, `[3]=Armor_Adamantite_Legs` |
| `HotbarInventory` | capacity 9; `[0]=Weapon_Longsword_Adamantite`, `ActiveSlot=0` |
| `UtilityInventory` | capacity 4; `[0]=Potion_Health` (quantity 3), `ActiveSlot=0` |

This fixture is the oracle for the Rust inventory-component reader
(`ArmorInventory`/`HotbarInventory`/`UtilityInventory`, plus the legacy
`Inventory` fallback) and the self-spawn equipment mapping.

The Rust gates are `crates/hytale-world/tests/player_data.rs` and the
`player_data` unit tests in `crates/hytale-world/src/player_data.rs`.
`hytale-server`'s `tests/player_data.rs` covers the CLI copy/create path and
`tests/world_thread.rs` checks equipment loading plus unknown-item fallback.

## Reverse check (Rust writes, Java reads)

Core fixture:

```sh
rm -rf target/player-smoke
cargo run -q -p hytale-server -- player-data \
    --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
    --save target/player-smoke
tools/java-dumper/run-player-data.sh --probe \
    target/player-smoke/players/12345678-1234-5678-9abc-def012345678.json
```

Equipment fixture:

```sh
rm -rf target/player-equipment-smoke
cargo run -q -p hytale-server -- player-data \
    --input testdata/golden/server/player-data/22345678-1234-5678-9abc-def012345678.json \
    --save target/player-equipment-smoke
tools/java-dumper/run-player-data.sh --probe-equipment \
    target/player-equipment-smoke/players/22345678-1234-5678-9abc-def012345678.json
```

The probes deserialize the Rust-written document with the real
`EntityStore.REGISTRY`, check the components, then re-serialize the holder and
check that the `Components` map still contains them.

Do not edit the JSON by hand.
