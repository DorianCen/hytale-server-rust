//! T59b: automatic environment enumeration, spawn suppression, live
//! marker/beacon controllers and persisted cooldowns.

use std::path::{Path, PathBuf};

use hytale_npc::spawn_position::SpawnMovementMode;
use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::collision::Vec3;

const ENVIRONMENT: &str = "Env_Portal_Goblin_Cave_Deep";

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
        "hytale-npc-spawn-control-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn options(save_dir: PathBuf) -> WorldThreadOptions {
    WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir,
    )
}

fn open(save_dir: &Path) -> WorldThread {
    let (thread, _start) =
        WorldThread::spawn(options(save_dir.to_path_buf())).expect("world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    thread
}

fn platform(thread: &WorldThread) {
    thread.load_chunk(0, 0).expect("load platform chunk");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass is a block");
    for x in 0..=10 {
        for z in 0..=10 {
            thread.set_block(x, 80, z, soil).expect("floor");
            for y in 81..=84 {
                thread.set_block(x, y, z, 0).expect("clear headroom");
            }
            thread
                .set_block_environment(x, 81, z, ENVIRONMENT)
                .expect("environment");
        }
    }
}

fn environment_npcs(thread: &WorldThread) -> usize {
    thread
        .entities()
        .expect("entities")
        .iter()
        .filter(|entity| {
            entity
                .components
                .get("Npc")
                .and_then(|npc| npc.get("Environment"))
                .and_then(serde_json::Value::as_str)
                == Some(ENVIRONMENT)
        })
        .count()
}

#[test]
fn automatic_population_discovers_environments_and_survives_reopen() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("auto");
    let thread = open(&save_dir);
    platform(&thread);
    thread
        .set_npc_auto_population(true)
        .expect("auto population");

    let first = thread.tick_npc_spawns().expect("auto population tick");
    assert!(first.auto_environments >= 1, "{first:?}");
    assert!(first.spawned >= 1, "{first:?}");
    let populated = environment_npcs(&thread);
    assert!(populated >= 1, "the platform environment spawned an NPC");

    // A second tick must not grow the same environment beyond its expected
    // count. Recomputing `actual` from live tagged NPCs also handles deaths.
    let second = thread.tick_npc_spawns().expect("saturated tick");
    assert_eq!(environment_npcs(&thread), populated, "{second:?}");

    thread.save().expect("save");
    let saved_entities = thread.entities().expect("entities").len();
    thread.shutdown().expect("shutdown");

    let thread = open(&save_dir);
    thread.load_chunk(0, 0).expect("reload platform chunk");
    thread
        .set_npc_auto_population(true)
        .expect("auto population");
    thread.tick_npc_spawns().expect("reloaded population tick");
    assert_eq!(
        environment_npcs(&thread),
        populated,
        "the persisted Npc.Environment tag keeps the controller at its target"
    );
    assert_eq!(thread.entities().expect("entities").len(), saved_entities);
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn spawn_suppression_blocks_world_spawns_until_removed() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("suppression");
    let thread = open(&save_dir);
    platform(&thread);
    let center = Vec3::new(5.5, 81.0, 5.5);

    thread
        .add_spawn_suppressor(1, "Suppression_Portal_Goblin_Rat", center)
        .expect("add suppressor");
    assert_eq!(
        thread.spawn_suppressors().expect("suppressors"),
        vec![(1, "Suppression_Portal_Goblin_Rat".to_string(), center)]
    );
    assert!(thread
        .spawn_npc_near("Rat", center, 4, SpawnMovementMode::Walk)
        .expect("suppressed spawn")
        .is_none());
    assert!(thread
        .spawn_npc_in_environment(ENVIRONMENT, center, 4, SpawnMovementMode::Walk)
        .expect("suppressed environment spawn")
        .is_none());

    thread
        .set_npc_auto_population(true)
        .expect("auto population");
    thread
        .tick_npc_spawns()
        .expect("suppressed population tick");
    // Other loaded/generated ENVIRONMENT sections may still spawn outside the
    // zone; no NPC may be created inside the suppressor's radius.
    for entity in thread.entities().expect("entities") {
        if entity.kind != hytale_world::runtime::RuntimeEntityKind::Npc {
            continue;
        }
        let dx = entity.transform.position.x - center.x;
        let dy = entity.transform.position.y - center.y;
        let dz = entity.transform.position.z - center.z;
        assert!(
            dx * dx + dy * dy + dz * dz > 15.0 * 15.0,
            "suppressed NPC spawned inside the zone: {entity:?}"
        );
    }

    // The suppressor map survives save/reopen and still rejects spawns.
    thread.save().expect("save suppression state");
    thread.shutdown().expect("shutdown");
    let thread = open(&save_dir);
    thread.load_chunk(0, 0).expect("reload platform chunk");
    assert_eq!(
        thread.spawn_suppressors().expect("restored suppressors"),
        vec![(1, "Suppression_Portal_Goblin_Rat".to_string(), center)]
    );
    assert!(thread
        .spawn_npc_near("Rat", center, 4, SpawnMovementMode::Walk)
        .expect("restored suppressed spawn")
        .is_none());

    assert!(thread
        .remove_spawn_suppressor(1)
        .expect("remove suppressor"));
    let spawned = thread
        .spawn_npc_near("Rat", center, 4, SpawnMovementMode::Walk)
        .expect("unsuppressed spawn")
        .expect("Rat spawns after suppression is removed");
    assert_eq!(spawned.components["Npc"]["Role"].as_str(), Some("Rat"));

    // Repeated spawn/despawn must not leak runtime entities or actors.
    let entities = thread.entities().expect("entities").len();
    let actors = thread.npc_actors().expect("actors").len();
    for _ in 0..16 {
        let entity = thread
            .spawn_npc_near("Rat", center, 4, SpawnMovementMode::Walk)
            .expect("spawn loop")
            .expect("platform stays walkable");
        thread.despawn_npc(entity.id).expect("despawn loop");
    }
    assert_eq!(thread.entities().expect("entities").len(), entities);
    assert_eq!(thread.npc_actors().expect("actors").len(), actors);

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn live_markers_and_beacons_persist_their_cooldowns() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("special");
    let thread = open(&save_dir);
    platform(&thread);

    thread
        .add_spawn_marker("bat", "Bat", Vec3::new(4.5, 81.0, 4.5))
        .expect("add Bat marker");
    let bat = thread
        .trigger_spawn_marker("bat")
        .expect("trigger Bat")
        .expect("Bat spawned");
    assert_eq!(bat.components["Npc"]["Role"].as_str(), Some("Bat"));
    assert!(thread
        .trigger_spawn_marker("bat")
        .expect("cooldown trigger")
        .is_none());

    thread
        .add_beacon("rat", "Edible_Rat", Vec3::new(6.5, 81.0, 6.5))
        .expect("add beacon");
    let beacon_report = thread.tick_beacons(1.0, 1.0).expect("beacon tick");
    assert!(beacon_report.beacon_spawns >= 1, "{beacon_report:?}");
    assert!(
        thread
            .tick_spawn_markers(1.0, 1.0)
            .expect("marker tick")
            .marker_spawns
            == 0
    );

    let states = thread.spawn_special_states().expect("states");
    assert!(states.iter().any(|(id, key, state)| {
        id == "marker:bat" && key == "Bat" && state["Cooldown"].as_f64().unwrap_or(0.0) > 0.0
    }));
    assert!(states.iter().any(|(id, key, state)| {
        id == "beacon:rat" && key == "Edible_Rat" && state["Spawned"].as_i64().unwrap_or(0) >= 1
    }));

    thread.save().expect("save");
    thread.shutdown().expect("shutdown");

    let thread = open(&save_dir);
    let states = thread.spawn_special_states().expect("restored states");
    assert_eq!(states.len(), 2, "{states:?}");
    assert!(thread
        .trigger_spawn_marker("bat")
        .expect("restored marker")
        .is_none());
    assert_eq!(
        thread
            .tick_beacons(1.0, 1.0)
            .expect("restored beacon")
            .beacon_spawns,
        0
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
