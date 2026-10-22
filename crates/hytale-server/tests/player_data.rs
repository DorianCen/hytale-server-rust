//! Player-data CLI gate: copy the Java holder fixture into a world save.

use std::path::PathBuf;

use hytale_server::{run_player_data_tool, PlayerDataToolOptions};
use hytale_world::player_data::{PlayerData, PlayerStorage, Position, Rotation};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-server-player-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn copies_the_java_player_fixture_into_a_save() {
    let save_dir = temp_dir("fixture");
    let input = repo_root()
        .join("testdata/golden/server/player-data/12345678-1234-5678-9abc-def012345678.json");
    let report = run_player_data_tool(&PlayerDataToolOptions {
        input: Some(input),
        save_dir: save_dir.clone(),
        uuid: None,
        position: Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
        world_gen_id: 0,
    })
    .expect("copy the Java player");

    assert!(report.loaded_existing);
    assert_eq!(report.uuid, "12345678-1234-5678-9abc-def012345678");
    assert_eq!(report.component_count, 3);
    assert_eq!(report.position.x, 10.5);
    assert_eq!(report.position.y, 80.0);
    assert_eq!(report.position.z, -3.5);

    let storage = PlayerStorage::for_save_dir(&save_dir);
    let loaded = storage
        .load("12345678-1234-5678-9abc-def012345678".parse().unwrap())
        .expect("load")
        .expect("present");
    assert_eq!(loaded.world_gen_id(), Some(42));
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn creates_a_new_player_with_the_requested_uuid() {
    let save_dir = temp_dir("create");
    let report = run_player_data_tool(&PlayerDataToolOptions {
        input: None,
        save_dir: save_dir.clone(),
        uuid: Some("00000000-0000-0000-0000-000000000003".to_string()),
        position: Position {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        rotation: Rotation {
            pitch: 0.0,
            yaw: 0.5,
            roll: 0.0,
        },
        world_gen_id: 7,
    })
    .expect("create the player");

    assert!(!report.loaded_existing);
    let loaded = PlayerData::from_json(
        &std::fs::read_to_string(&report.path).expect("read the written file"),
    )
    .expect("parse");
    assert_eq!(loaded.world_gen_id(), Some(7));
    let transform = loaded.transform().expect("transform");
    assert_eq!(transform.position.x, 1.0);
    assert_eq!(transform.rotation.yaw, 0.5);
    std::fs::remove_dir_all(save_dir).ok();
}
