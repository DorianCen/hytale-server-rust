//! T60b: player effects, damage-over-time, death and respawn.

use std::path::PathBuf;

use hytale_protocol::generated::common::EffectOp;
use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};

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
        "hytale-player-effects-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn burn_ticks_damage_and_death_respawns() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("burn");
    let (thread, _start) = WorldThread::spawn(WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    ))
    .expect("world thread");
    while thread.tick().expect("tick").remaining > 0 {}

    let init = thread
        .ensure_player_gameplay(7, None)
        .expect("player stats");
    assert!(!init.is_empty());
    let max_health = thread
        .player_health(7)
        .expect("health")
        .expect("player exists");

    thread
        .apply_player_effect(7, "Burn", None)
        .expect("apply Burn");
    let added = thread
        .take_player_effect_updates(7)
        .expect("effect updates");
    assert!(
        added.iter().any(|update| update.r#type == EffectOp::Add),
        "{added:?}"
    );

    let effect_updates = thread.tick_player_gameplay(7, 3.1).expect("tick gameplay");
    assert!(
        effect_updates
            .iter()
            .any(|update| update.r#type == EffectOp::Remove),
        "{effect_updates:?}"
    );
    let health = thread
        .player_health(7)
        .expect("health")
        .expect("player exists");
    assert!(health < max_health, "Burn should deal DoT: {health}");

    let result = thread
        .damage_player(7, 10_000.0, "Physical")
        .expect("damage")
        .expect("player exists");
    assert!(result.died);
    assert_eq!(thread.player_health(7).expect("health"), Some(0.0));
    assert!(thread.respawn_player(7).expect("respawn"));
    let health = thread
        .player_health(7)
        .expect("health")
        .expect("player exists");
    assert!(
        (health - max_health).abs() < 1e-4,
        "{health} != {max_health}"
    );
    assert!(
        thread
            .take_player_effect_updates(7)
            .expect("effect updates")
            .is_empty(),
        "respawn clears active effects"
    );

    thread.remove_player_gameplay(7).expect("remove player");
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
