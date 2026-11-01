//! T57b: entity stat/effect config packet encoders.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_protocol::generated::assets::{
    UpdateEntityEffects, UpdateEntityStatTypes, UpdateProjectileConfigs, UpdateWeathers,
};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::Reader;

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

#[test]
fn entity_stat_and_effect_packets_decode_and_carry_the_store() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_phase2(&pack).expect("load phase 2");
    let config = registry.config_assets().expect("encode config assets");

    let stat_bytes = config
        .packets
        .iter()
        .find(|packet| packet.id == UpdateEntityStatTypes::ID)
        .expect("UpdateEntityStatTypes");
    let stats = UpdateEntityStatTypes::decode(&mut Reader::new(&stat_bytes.payload))
        .expect("decode stat types");
    let types = stats.types.expect("stat map");
    assert_eq!(stats.max_id, 13);
    assert_eq!(types.len(), 13);
    let health = types
        .values()
        .find(|stat| stat.id.as_deref() == Some("Health"))
        .expect("Health stat");
    assert_eq!(health.max, 100.0);
    assert_eq!(health.value, 100.0);

    let effect_bytes = config
        .packets
        .iter()
        .find(|packet| packet.id == UpdateEntityEffects::ID)
        .expect("UpdateEntityEffects");
    let effects = UpdateEntityEffects::decode(&mut Reader::new(&effect_bytes.payload))
        .expect("decode effects");
    let effects_map = effects.entity_effects.expect("effect map");
    assert_eq!(effects.max_id, 260);
    assert_eq!(effects_map.len(), 260);
    assert!(effects_map
        .values()
        .any(|effect| effect.id.as_deref() == Some("Burn")));

    let projectile_bytes = config
        .packets
        .iter()
        .find(|packet| packet.id == UpdateProjectileConfigs::ID)
        .expect("UpdateProjectileConfigs");
    let projectiles = UpdateProjectileConfigs::decode(&mut Reader::new(&projectile_bytes.payload))
        .expect("decode projectile configs");
    let projectiles = projectiles.configs.expect("projectile map");
    assert_eq!(projectiles.len(), 188);
    assert!(projectiles.values().any(|config| {
        config.launch_force > 0.0
            && config
                .model
                .as_ref()
                .and_then(|model| model.asset_id.as_deref())
                .is_some()
    }));

    let weather_bytes = config
        .packets
        .iter()
        .find(|packet| packet.id == UpdateWeathers::ID)
        .expect("UpdateWeathers");
    let weathers =
        UpdateWeathers::decode(&mut Reader::new(&weather_bytes.payload)).expect("decode weathers");
    let weathers = weathers.weathers.expect("weather map");
    assert!(!weathers.is_empty());
    assert!(weathers
        .values()
        .any(|weather| weather.id.as_deref() == Some("Blood_Moon")));
}

#[test]
fn default_player_model_carries_eye_height_and_animations() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_phase2(&pack).expect("load phase 2");
    let model = registry.player_model.expect("default player model");
    assert_eq!(model.asset_id.as_deref(), Some("Player"));
    assert_eq!(model.path.as_deref(), Some("Characters/Player.blockymodel"));
    assert!(model.eye_height > 0.0, "eye height drives the camera");
    assert!(model.hitbox.is_some());
    let animation_sets = model.animation_sets.expect("animation sets");
    assert!(animation_sets.contains_key("StepRun"));
}
