//! World-thread movement authority: collision resolution against placed blocks.

use std::path::PathBuf;

use hytale_protocol::generated::common::FlyMode;
use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::collision::Vec3;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-movement-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn world(name: &str) -> Option<(WorldThread, PathBuf)> {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let save_dir = temp_dir(name);
    let options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, _start) = WorldThread::spawn(options).expect("spawn the world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    Some((thread, save_dir))
}

#[test]
fn movement_settings_use_the_gameplay_config_and_player_physics() {
    let Some((thread, save_dir)) = world("settings") else {
        return;
    };
    let settings = thread.movement_settings().expect("movement settings");
    assert_eq!(settings.base_speed, 5.5);
    assert_eq!(settings.jump_force, 11.8);
    assert_eq!(settings.mass, 68.0);
    assert_eq!(settings.drag_coefficient, 0.5);
    assert_eq!(settings.fly, FlyMode::Disabled);
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_wall_clamps_the_resolved_position_and_a_slab_steps_up() {
    let Some((thread, save_dir)) = world("collision") else {
        return;
    };
    let stone = thread
        .block_id_for_item("Rock_Stone")
        .expect("stone lookup")
        .expect("Rock_Stone is a block");
    let slab = thread
        .block_id_for_item("Rock_Stone_Brick_Half")
        .expect("slab lookup")
        .expect("the half slab is a block");

    // Two blocks high at x=5 so the one-block step cannot clear it.
    thread.set_block(5, 200, 4, stone).expect("wall lower");
    thread.set_block(5, 201, 4, stone).expect("wall upper");
    let from = Vec3::new(4.5, 200.0, 4.5);
    let to = Vec3::new(7.5, 200.0, 4.5);
    let resolved = thread
        .resolve_movement(from, to, false, true)
        .expect("resolve wall");
    assert!(resolved.corrected);
    assert!(
        (resolved.position.x - (5.0 - 0.325)).abs() < 1e-4,
        "{resolved:?}"
    );
    assert!((resolved.position.z - 4.5).abs() < 1e-9, "{resolved:?}");

    // A half slab at the destination is stepped onto.
    thread.set_block(5, 200, 8, slab).expect("slab");
    let resolved = thread
        .resolve_movement(
            Vec3::new(4.5, 200.0, 8.5),
            Vec3::new(5.5, 200.0, 8.5),
            false,
            true,
        )
        .expect("resolve slab");
    assert!(resolved.stepped, "{resolved:?}");
    assert!((resolved.position.y - 200.5).abs() < 1e-4, "{resolved:?}");

    // A free target stays bit-for-bit identical (no rubber-banding).
    let free = Vec3::new(12.25, 200.0, 12.25);
    let resolved = thread
        .resolve_movement(Vec3::new(10.0, 200.0, 10.0), free, false, true)
        .expect("resolve free");
    assert!(!resolved.corrected);
    assert_eq!(resolved.position, free);

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
