//! Complete saved-world boot: config and player creation/loading.

use std::path::PathBuf;

use hytale_server::{run_world_boot, WorldBootOptions};
use hytale_world::player_data::PlayerStorage;
use hytale_world::world_config::WorldConfig;

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
        "hytale-server-boot-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn creates_then_reopens_a_config_and_player() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("boot");
    let options = WorldBootOptions {
        assets_pack: repo_root().join("Assets/Assets.zip"),
        save_dir: save_dir.clone(),
        seed: 5,
        display_name: Some("Boot Gate".to_string()),
        world_uuid: None,
        player_uuid: None,
        set_block: None,
    };

    let first = run_world_boot(&options).expect("first boot");
    assert!(first.config_created);
    assert!(first.player_created);
    assert_eq!(first.stored_chunks, 0);
    assert_eq!(first.seed, 5);
    assert_eq!(first.block_edit, None);
    assert!(save_dir.join("config.json").is_file());

    let config = WorldConfig::load(save_dir.join("config.json")).expect("load config");
    assert_eq!(config.seed, 5);
    assert_eq!(config.display_name.as_deref(), Some("Boot Gate"));
    assert_eq!(config.uuid.to_string(), first.world_uuid);

    let second = run_world_boot(&options).expect("second boot");
    assert!(!second.config_created);
    assert!(!second.player_created);
    assert_eq!(second.world_uuid, first.world_uuid);
    assert_eq!(second.player_uuid, first.player_uuid);
    assert_eq!(
        PlayerStorage::for_save_dir(&save_dir)
            .players()
            .expect("list players"),
        vec![first.player_uuid.parse().unwrap()]
    );

    std::fs::remove_dir_all(save_dir).ok();
}
