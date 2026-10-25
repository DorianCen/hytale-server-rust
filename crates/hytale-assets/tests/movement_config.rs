//! Real-pack `MovementConfig` and `GameplayConfig.Player.MovementConfig` load.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry, MovementConfigAsset};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

#[test]
fn the_default_movement_config_matches_the_pack_document() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    let default = registry
        .movement_configs
        .get("Default")
        .expect("the Default movement config");
    assert_eq!(default.value.settings.base_speed, 5.5);
    assert_eq!(default.value.settings.jump_force, 11.8);
    assert_eq!(default.value.settings.climb_up_sprint_speed, 0.07);
    assert_eq!(default.value.settings.max_speed_multiplier, 15.0);
    assert_eq!(
        default.value.settings.forward_sprint_speed_multiplier,
        1.273
    );
    assert_eq!(default.value.settings.max_wall_angle_degrees, 15.0);
    assert_eq!(default.value.fly, None);

    let gameplay = registry
        .gameplay_configs
        .get("Default")
        .expect("the Default gameplay config");
    assert_eq!(gameplay.value.player.movement_config, "Default");
}

#[test]
fn the_java_fallback_is_used_when_the_asset_is_missing() {
    let fallback = MovementConfigAsset::default();
    assert_eq!(fallback.settings.base_speed, 5.5);
    assert_eq!(fallback.settings.jump_force, 11.8);
    assert_eq!(fallback.settings.max_speed_multiplier, 1000.0);
    assert_eq!(fallback.fly, None);
}
