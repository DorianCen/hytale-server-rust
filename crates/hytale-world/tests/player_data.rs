//! Player save compatibility with Java's `DiskPlayerStorageProvider` and the
//! `EntityStore` holder codec.

use std::path::PathBuf;

use hytale_world::player_data::{PlayerData, PlayerStorage, PlayerTransform, Position, Rotation};
use hytale_world::world_config::WorldUuid;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json")
}

fn uuid() -> WorldUuid {
    "12345678-1234-5678-9abc-def012345678".parse().unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-player-data-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn reads_the_java_player_fixture() {
    let data = PlayerData::from_json(&std::fs::read_to_string(fixture()).unwrap()).expect("load");

    assert_eq!(data.uuid(), Some(uuid()));
    assert_eq!(data.world_gen_id(), Some(42));
    assert_eq!(data.component_count(), 3);
    assert_eq!(
        data.component_names().collect::<Vec<_>>(),
        vec!["WorldGenId", "Transform", "UUID"]
    );

    let transform = data.transform().expect("Transform component");
    assert_eq!(
        transform,
        PlayerTransform::new(
            Position {
                x: 10.5,
                y: 80.0,
                z: -3.5
            },
            Rotation {
                pitch: 0.25,
                yaw: 1.570_796_4,
                roll: -0.125
            }
        )
    );
}

#[test]
fn round_trips_without_losing_components() {
    let data = PlayerData::from_json(&std::fs::read_to_string(fixture()).unwrap()).expect("load");
    let text = data.to_json_pretty().expect("serialize");
    assert_eq!(PlayerData::from_json(&text).expect("reparse"), data);

    // Unknown components (a full Java save has many) survive a Rust save.
    let mut extended = data.clone();
    extended.insert_component("Inventory", serde_json::json!({"Items": [], "Version": 3}));
    let text = extended.to_json_pretty().expect("serialize");
    let reloaded = PlayerData::from_json(&text).expect("reparse");
    assert_eq!(reloaded.component("Inventory").unwrap()["Version"], 3);
    assert_eq!(reloaded.uuid(), Some(uuid()));
}

#[test]
fn storage_saves_lists_and_removes_players() {
    let dir = temp_dir("storage");
    let storage = PlayerStorage::for_save_dir(&dir);
    let data = PlayerData::from_json(&std::fs::read_to_string(fixture()).unwrap()).expect("load");

    storage.save(uuid(), &data).expect("save");
    assert_eq!(
        storage.path_for(uuid()),
        dir.join("players/12345678-1234-5678-9abc-def012345678.json")
    );
    assert_eq!(storage.players().expect("list"), vec![uuid()]);
    assert_eq!(storage.load(uuid()).expect("load"), Some(data.clone()));

    storage.save(uuid(), &data).expect("second save");
    assert!(storage
        .path_for(uuid())
        .with_extension("json.bak")
        .is_file());

    assert!(storage.remove(uuid()).expect("remove"));
    assert!(!storage.remove(uuid()).expect("remove missing"));
    assert!(storage.load(uuid()).expect("load missing").is_none());
    assert!(storage.players().expect("list").is_empty());
    std::fs::remove_dir_all(dir).ok();
}
