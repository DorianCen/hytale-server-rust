//! World-config CLI gate: load the Java fixture, write it into a save
//! directory, and create deterministic defaults for a new world.

use std::path::PathBuf;

use hytale_server::{run_world_config_tool, WorldConfigToolOptions};
use hytale_world::world_config::WorldConfig;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-server-config-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn loads_the_java_fixture_and_writes_a_save_config() {
    let save_dir = temp_dir("fixture");
    let input = repo_root().join("testdata/golden/server/world-config/config.json");
    let report = run_world_config_tool(&WorldConfigToolOptions {
        input: Some(input),
        save_dir: save_dir.clone(),
        seed: 0,
        display_name: None,
        uuid: None,
    })
    .expect("copy the Java config");

    assert!(report.loaded_existing);
    assert_eq!(report.uuid, "12345678-1234-5678-9abc-def012345678");
    assert_eq!(report.seed, 1_234_567_890_123);
    let written = WorldConfig::load(report.path).expect("reload");
    assert_eq!(written.display_name.as_deref(), Some("Rust Golden World"));
    assert_eq!(written.world_gen.type_id(), Some("Void"));
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn new_worlds_get_a_stable_uuid() {
    let first_dir = temp_dir("first");
    let second_dir = temp_dir("second");
    let first = run_world_config_tool(&WorldConfigToolOptions {
        input: None,
        save_dir: first_dir.clone(),
        seed: 99,
        display_name: Some("Headless".to_string()),
        uuid: None,
    })
    .expect("create the first config");
    let second = run_world_config_tool(&WorldConfigToolOptions {
        input: None,
        save_dir: second_dir.clone(),
        seed: 99,
        display_name: Some("Headless".to_string()),
        uuid: None,
    })
    .expect("create the second config");

    assert!(!first.loaded_existing);
    assert_eq!(first.uuid, second.uuid);
    assert_eq!(first.seed, 99);
    assert_eq!(first.display_name.as_deref(), Some("Headless"));
    std::fs::remove_dir_all(first_dir).ok();
    std::fs::remove_dir_all(second_dir).ok();
}
