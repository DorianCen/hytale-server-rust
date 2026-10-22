//! World config (`config.json`) compatibility with the Java `WorldConfig`
//! codec.

use std::path::PathBuf;

use hytale_world::world_config::{
    Box2d, WorldConfig, WorldUuid, DEFAULT_GAMEPLAY_CONFIG, WORLD_CONFIG_VERSION,
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/golden/server/world-config/config.json")
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-world-config-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn reads_the_java_world_config_fixture() {
    let config = WorldConfig::load(fixture()).expect("the Java fixture loads");

    assert_eq!(config.version, WORLD_CONFIG_VERSION);
    assert_eq!(
        config.uuid,
        "12345678-1234-5678-9abc-def012345678".parse().unwrap()
    );
    assert_eq!(config.display_name.as_deref(), Some("Rust Golden World"));
    assert_eq!(config.seed, 1_234_567_890_123);
    assert_eq!(config.game_time, "2026-10-22T12:34:56Z");
    assert_eq!(config.game_mode.as_deref(), Some("Adventure"));
    assert_eq!(config.gameplay_config_id(), DEFAULT_GAMEPLAY_CONFIG);
    assert!(!config.is_ticking);
    assert!(config.is_block_ticking);
    assert!(config.is_pvp_enabled);
    assert!(!config.is_fall_damage_enabled);
    assert!(config.is_game_time_paused);
    assert!(!config.is_spawning_npc);
    assert!(config.is_block_spawners_enabled);
    assert!(!config.is_spawn_markers_enabled);
    assert!(config.is_all_npc_frozen);
    assert!(!config.is_saving_players);
    assert!(config.is_saving_chunks);
    assert!(!config.save_new_chunks);
    assert!(!config.is_unloading_chunks);
    assert!(!config.is_compass_updating);
    assert!(config.is_objective_markers_enabled);
    assert_eq!(
        config.disabled_fluid_tickers.iter().collect::<Vec<_>>(),
        vec!["Fluid=Water"]
    );

    assert_eq!(config.world_gen.type_id(), Some("Void"));
    assert_eq!(config.world_gen.get_str("Tint"), Some("#5b9e28"));
    assert_eq!(
        config.world_gen.get_str("Environment"),
        Some("Env_Zone1_Plains")
    );
    assert_eq!(config.world_map.type_id(), Some("Disabled"));
    assert_eq!(config.chunk_storage.type_id(), Some("Hytale"));
    assert_eq!(config.resource_storage.type_id(), Some("Hytale"));

    let spawn = config.spawn_provider.as_ref().expect("spawn provider");
    assert_eq!(spawn.type_id(), Some("Global"));
    let spawn_point = spawn.get("SpawnPoint").expect("spawn point");
    assert_eq!(spawn_point["X"].as_f64(), Some(10.5));
    assert_eq!(spawn_point["Y"].as_f64(), Some(80.0));
    assert_eq!(spawn_point["Z"].as_f64(), Some(-3.5));

    assert_eq!(
        config.chunk_config.pregenerate_region,
        Some(Box2d {
            min: hytale_world::world_config::Vector2d {
                x: -512.0,
                y: -512.0
            },
            max: hytale_world::world_config::Vector2d { x: 512.0, y: 512.0 },
        })
    );
    assert_eq!(
        config.chunk_config.keep_loaded_region,
        Some(Box2d {
            min: hytale_world::world_config::Vector2d { x: -64.0, y: -64.0 },
            max: hytale_world::world_config::Vector2d { x: 64.0, y: 64.0 },
        })
    );

    assert_eq!(config.client_effects.sun_height_percent, 75.0);
    assert_eq!(config.client_effects.sun_angle_degrees, 71.6197280883789);
    assert_eq!(config.client_effects.bloom_power, 4.0);
    assert!(config.required_plugins.is_empty());
    assert!(config.plugin.is_empty());
    assert!(config.extra.is_empty());
}

#[test]
fn round_trips_the_java_fixture_without_losing_fields() {
    let config = WorldConfig::load(fixture()).expect("load");
    let text = config.to_json_pretty().expect("serialize");
    let reloaded = WorldConfig::from_json(&text).expect("parse");
    assert_eq!(config, reloaded);
}

#[test]
fn preserves_unknown_fields() {
    let source = r#"{
        "Version": 4,
        "UUID": {"$binary": "EjRWeBI0VniavN7wEjRWeA==", "$type": "04"},
        "Seed": 1,
        "FutureSection": {"Enabled": true, "Count": 3}
    }"#;
    let config = WorldConfig::from_json(source).expect("parse");
    assert_eq!(config.extra.len(), 1);
    assert_eq!(config.extra["FutureSection"]["Count"], 3);

    let text = config.to_json_pretty().expect("serialize");
    let reloaded = WorldConfig::from_json(&text).expect("reparse");
    assert_eq!(reloaded.extra["FutureSection"]["Enabled"], true);
}

#[test]
fn saves_atomically_and_keeps_a_backup() {
    let dir = temp_dir("save");
    let path = WorldConfig::path_in(&dir);
    let uuid: WorldUuid = "00000000-0000-0000-0000-000000000002".parse().unwrap();
    let mut config = WorldConfig::new(uuid, 42, Some("Test World".to_string()));
    config.save(&path).expect("first save");
    assert!(path.is_file());

    config.seed = 43;
    config.save(&path).expect("second save");
    assert!(path.with_extension("json.bak").is_file());

    let loaded = WorldConfig::load(&path).expect("reload");
    assert_eq!(loaded.seed, 43);
    assert_eq!(loaded.display_name.as_deref(), Some("Test World"));
    std::fs::remove_dir_all(dir).ok();
}
