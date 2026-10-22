# Player data fixture

`12345678-1234-5678-9abc-def012345678.json` is written by the real Java
`EntityStore.REGISTRY.serialize(holder)` and `BsonUtil.writeDocument` — the same
path `DiskPlayerStorageProvider.DiskPlayerStorage.save` uses for
`<world>/players/<uuid>.json`.

Regenerate and verify determinism:

```sh
tools/java-dumper/run-player-data.sh
tools/java-dumper/run-player-data.sh --check
```

`WorldPlayerDumper` registers the component ids from `EntityModule.setup()` for
the ported core set (`UUID`, `Transform`, `WorldGenId`) and builds a holder with
fixed values. No full server boot is needed; the entity store codecs are static.

Values:

| Component | Fields |
|---|---|
| `UUID` | `12345678-1234-5678-9abc-def012345678` |
| `Transform` | position `(10.5, 80.0, -3.5)`, rotation `Pitch=0.25`, `Yaw=1.5707964`, `Roll=-0.125` |
| `WorldGenId` | `42` |

The Rust gate is `crates/hytale-world/tests/player_data.rs`: it checks every
component, round-trips the document, and verifies that unknown components (a
full Java player save has many) survive a Rust save. `hytale-server`'s
`tests/player_data.rs` covers the CLI copy/create path.

Reverse check (Rust writes, Java reads):

```sh
rm -rf target/player-smoke
cargo run -q -p hytale-server -- player-data \
    --input testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json \
    --save target/player-smoke
tools/java-dumper/run-player-data.sh --probe \
    target/player-smoke/players/12345678-1234-5678-9abc-def012345678.json
```

The probe deserializes the Rust-written document with the real
`EntityStore.REGISTRY`, checks all three components, then re-serializes the
holder and checks that the `Components` map still contains them.

Do not edit the JSON by hand.
