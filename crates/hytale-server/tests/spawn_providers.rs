//! T75: listener worlds start without a synthetic player and resolve provider
//! spawn points per joining UUID.

use std::path::PathBuf;

use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::world_config::{ConfigObject, WorldConfig, WorldUuid};
use serde_json::json;

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
        "hytale-spawn-provider-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn generator() -> GeneratorOptions {
    GeneratorOptions {
        assets_pack: repo_root().join("Assets/Assets.zip"),
        assets_root: repo_root().join("Assets/extracted"),
        world_dir: repo_root().join("testdata/Default"),
        workers: 1,
    }
}

fn write_config(save_dir: &std::path::Path, provider: serde_json::Value) {
    let uuid: WorldUuid = "10000000-0000-0000-0000-000000000075"
        .parse()
        .expect("world uuid");
    let mut config = WorldConfig::new(uuid, 7, Some("Spawn Gate".to_string()));
    config.spawn_provider =
        Some(serde_json::from_value::<ConfigObject>(provider).expect("provider"));
    config
        .save(WorldConfig::path_in(save_dir))
        .expect("write config");
}

fn start(save_dir: &std::path::Path) -> (WorldThread, hytale_server::WorldStartReport) {
    let mut options = WorldThreadOptions::with_defaults(generator(), save_dir);
    options.seed = 7;
    options.display_name = Some("Spawn Gate".to_string());
    options.player_uuid = None;
    options.create_default_player = false;
    options.spawn_search = true;
    WorldThread::spawn(options).expect("world thread")
}

#[test]
fn listener_boot_creates_no_player_and_resolves_global_spawn() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("global");
    write_config(
        &save_dir,
        json!({
            "Id": "Global",
            "SpawnPoint": {"X": 8.5, "Y": 120, "Z": 8.5}
        }),
    );
    let (thread, start) = start(&save_dir);
    assert!(!start.player_created);
    assert!(start.player_uuid.is_empty(), "no synthetic player UUID");
    let players = save_dir.join("players");
    assert!(
        !players.is_dir()
            || std::fs::read_dir(&players)
                .expect("players")
                .next()
                .is_none(),
        "boot must not write a player document"
    );

    let first: WorldUuid = "20000000-0000-0000-0000-000000000075"
        .parse()
        .expect("first player");
    let second: WorldUuid = "30000000-0000-0000-0000-000000000075"
        .parse()
        .expect("second player");
    for uuid in [first, second] {
        let snapshot = thread.player_snapshot(uuid).expect("player snapshot");
        assert_eq!(snapshot.transform.position.x, 8.5);
        assert_eq!(snapshot.transform.position.y, 120.0);
        assert_eq!(snapshot.transform.position.z, 8.5);
        assert!(players.join(format!("{uuid}.json")).is_file());
        assert_eq!(
            thread.spawn_point(uuid).expect("provider spawn"),
            Some(snapshot.transform)
        );
    }
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn individual_provider_gives_two_players_distinct_stable_spawns() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("individual");
    write_config(
        &save_dir,
        json!({
            "Id": "Individual",
            "SpawnPoints": [
                {"X": 8.5, "Y": 120, "Z": 8.5},
                {"X": 12.5, "Y": 120, "Z": 12.5}
            ]
        }),
    );
    let (thread, _start) = start(&save_dir);
    let candidates = [
        "40000000-0000-0000-0000-000000000075",
        "50000000-0000-0000-0000-000000000075",
        "60000000-0000-0000-0000-000000000075",
        "70000000-0000-0000-0000-000000000075",
    ];
    let first: WorldUuid = candidates[0].parse().expect("first player");
    let first_point = thread.player_snapshot(first).expect("first").transform;
    let again = thread.player_snapshot(first).expect("again").transform;
    assert_eq!(first_point, again, "per-UUID selection is deterministic");
    assert_eq!(first_point.position.y, 120.0);
    assert!([8.5, 12.5].contains(&first_point.position.x));
    let second_point = candidates[1..]
        .iter()
        .filter_map(|uuid| {
            let uuid: WorldUuid = uuid.parse().ok()?;
            let point = thread.player_snapshot(uuid).expect("candidate").transform;
            (point.position.x != first_point.position.x).then_some(point)
        })
        .next()
        .expect("a second UUID selects the other configured spawn");
    assert_eq!(second_point.position.y, 120.0);
    assert!([8.5, 12.5].contains(&second_point.position.x));
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
