//! T60: disabled builtin modules do no world-thread work.

use std::path::PathBuf;

use hytale_server::{
    BuiltinRuntime, GeneratorOptions, WorldThread, WorldThreadError, WorldThreadOptions,
};
use hytale_world::collision::Vec3;
use hytale_world::player_data::{PlayerTransform, Position, Rotation};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-builtin-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn disabled_modules_skip_simulation_npc_weather_and_encounter_work() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("all-off");
    let mut options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    options.builtin = BuiltinRuntime::none();
    let (thread, _start) = WorldThread::spawn(options).expect("world thread");
    while thread.tick().expect("tick").remaining > 0 {}

    let dirt = thread
        .block_id_for_item("Soil_Dirt")
        .expect("block lookup")
        .expect("Soil_Dirt is a block");
    let bush = thread
        .block_id_for_item("Plant_Bush")
        .expect("block lookup")
        .expect("Plant_Bush is a block");
    thread.set_block(4, 200, 4, dirt).expect("dirt");
    thread.set_block(4, 201, 4, bush).expect("bush");
    let report = thread.simulate(0.05).expect("simulation tick");
    assert_eq!(report.block_ticks, 0, "{report:?}");
    assert_eq!(report.random_ticks, 0, "{report:?}");
    assert_eq!(report.falling_moves, 0, "{report:?}");
    assert_eq!(report.fluid_ticks, 0, "{report:?}");
    assert!(report.block_changes.is_empty(), "{report:?}");
    assert_eq!(thread.get_block(4, 201, 4).expect("bush"), Some(bush));

    let error = thread
        .spawn_npc(
            "Rat",
            PlayerTransform::new(
                Position {
                    x: 4.0,
                    y: 202.0,
                    z: 4.0,
                },
                Rotation {
                    pitch: 0.0,
                    yaw: 0.0,
                    roll: 0.0,
                },
            ),
        )
        .expect_err("the NPC module is disabled");
    assert!(
        matches!(error, WorldThreadError::Thread(ref message) if message.contains("disabled")),
        "{error:?}"
    );

    let runtime = thread.tick_npcs().expect("npc tick");
    assert_eq!(runtime.ticked, 0);
    assert_eq!(runtime.moved, 0);
    let spawns = thread.tick_npc_spawns().expect("npc spawn tick");
    assert_eq!(spawns.spawned, 0);
    let marker_error = thread
        .add_spawn_marker("marker", "Bat", Vec3::new(4.0, 210.0, 4.0))
        .expect_err("the NPC module is disabled");
    assert!(
        matches!(marker_error, WorldThreadError::Thread(ref message) if message.contains("disabled")),
        "{marker_error:?}"
    );
    let beacon_error = thread
        .add_beacon("beacon", "Bat", Vec3::new(4.0, 210.0, 4.0))
        .expect_err("the NPC module is disabled");
    assert!(
        matches!(beacon_error, WorldThreadError::Thread(ref message) if message.contains("disabled")),
        "{beacon_error:?}"
    );
    let markers = thread.tick_spawn_markers(0.05, 0.05).expect("marker tick");
    assert_eq!(markers.marker_spawns, 0);
    let beacons = thread.tick_beacons(0.05, 0.05).expect("beacon tick");
    assert_eq!(beacons.beacon_spawns, 0);
    assert!(thread.tick_encounters().expect("encounter tick").is_empty());
    assert_eq!(thread.weather_at(4, 210, 4).expect("weather"), None);

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
