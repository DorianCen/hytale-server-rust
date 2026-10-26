//! T59b: live NPC runtime entities in the world thread.

use std::path::{Path, PathBuf};

use hytale_npc::combat::NpcTarget;
use hytale_server::world_thread::RuntimeEntityEvent;
use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::player_data::{Position, Rotation};
use hytale_world::runtime::RuntimeEntityKind;

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
        "hytale-npc-runtime-{}-{nonce}-{name}",
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

fn transform(position: [f64; 3], yaw: f32) -> hytale_world::player_data::PlayerTransform {
    hytale_world::player_data::PlayerTransform::new(
        Position {
            x: position[0],
            y: position[1],
            z: position[2],
        },
        Rotation {
            pitch: 0.0,
            yaw,
            roll: 0.0,
        },
    )
}

#[test]
fn npc_entities_persist_health_and_despawn_on_death() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("persist");
    let thread = open(&save_dir);
    let entity = thread
        .spawn_npc("Rat", transform([0.5, 80.0, 0.5], 0.0))
        .expect("spawn Rat");
    assert_eq!(entity.kind, RuntimeEntityKind::Npc);
    assert_eq!(
        entity.components["Npc"]["Role"].as_str(),
        Some("Rat"),
        "role component"
    );
    assert_eq!(entity.components["Npc"]["Health"].as_f64(), Some(10.0));
    assert_eq!(
        entity.components["Model"]["Model"]["Id"].as_str(),
        Some("Rat")
    );
    assert!(thread
        .runtime_entity_events()
        .expect("events")
        .iter()
        .any(|event| matches!(event, RuntimeEntityEvent::Spawned(id) if *id == entity.id)));

    let result = thread
        .damage_npc(entity.id, 4.0, "Physical")
        .expect("damage")
        .expect("the actor exists");
    assert!((result.remaining_health - 6.0).abs() < 1e-6);
    assert!(!result.died);
    assert_eq!(
        thread
            .entity(entity.id)
            .expect("entity")
            .expect("entity exists")
            .components["Npc"]["Health"]
            .as_f64(),
        Some(6.0)
    );

    thread.save().expect("save");
    thread.shutdown().expect("shutdown");

    let thread = open(&save_dir);
    let actors = thread.npc_actors().expect("reloaded actors");
    assert_eq!(actors.len(), 1);
    let (id, actor) = &actors[0];
    assert_eq!(*id, entity.id);
    assert!((actor.health() - 6.0).abs() < 1e-6);
    assert_eq!(thread.entities().expect("entities").len(), 1);

    let result = thread
        .damage_npc(entity.id, 6.0, "Physical")
        .expect("lethal damage")
        .expect("the actor exists");
    assert!(result.died);
    assert!(thread
        .runtime_entity_events()
        .expect("events")
        .iter()
        .any(|event| matches!(
            event,
            RuntimeEntityEvent::Despawned { id: removed, .. } if *removed == entity.id
        )));
    assert!(thread.entities().expect("entities").is_empty());
    thread.shutdown().expect("shutdown reloaded world");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn npcs_attack_a_target_through_the_runtime_tick() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("attack");
    let thread = open(&save_dir);
    let attacker = thread
        .spawn_npc("Rat", transform([0.5, 80.0, 0.5], 0.0))
        .expect("spawn attacker");
    // A player target id uses the high bit, so missing damage targets are not
    // mistaken for runtime NPC ids.
    thread
        .set_npc_targets(vec![NpcTarget {
            id: hytale_server::players::PLAYER_NPC_TARGET_OFFSET | 7,
            position: hytale_world::collision::Vec3::new(1.5, 80.0, 0.5),
        }])
        .expect("set targets");

    std::thread::sleep(std::time::Duration::from_millis(10));
    let report = thread.tick_npcs().expect("tick NPCs");
    assert!(report.attacks >= 1, "the adjacent Rat attacks: {report:?}");
    assert_eq!(report.alive, 1);
    let actors = thread.npc_actors().expect("actors");
    assert_eq!(actors.len(), 1);
    let (id, actor) = &actors[0];
    assert_eq!(*id, attacker.id);
    assert_eq!(
        actor.health(),
        10.0,
        "a player target takes no NPC damage yet"
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn spawn_commands_handle_unknown_roles_and_find_positions() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("spawn-commands");
    let thread = open(&save_dir);
    let error = thread
        .spawn_npc("No_Such_Role", transform([0.5, 80.0, 0.5], 0.0))
        .expect_err("unknown role");
    assert!(error.to_string().contains("unknown NPC role"));

    thread.load_chunk(0, 0).expect("load the platform chunk");
    for x in 0..=8 {
        for z in 0..=8 {
            thread.set_block(x, 80, z, 1).expect("floor");
            for y in 81..=83 {
                thread.set_block(x, y, z, 0).expect("clear headroom");
            }
        }
    }
    let center = hytale_world::collision::Vec3::new(4.5, 81.0, 4.5);
    let near = thread
        .spawn_npc_near(
            "Rat",
            center,
            2,
            hytale_npc::spawn_position::SpawnMovementMode::Walk,
        )
        .expect("spawn near")
        .expect("a walkable platform");
    assert_eq!(near.kind, RuntimeEntityKind::Npc);
    assert_eq!(near.transform.position.y, 81.0);

    let selected = thread
        .spawn_npc_in_environment(
            "Env_Portal_Goblin_Surface",
            center,
            2,
            hytale_npc::spawn_position::SpawnMovementMode::Walk,
        )
        .expect("environment spawn")
        .expect("a selected role on the platform");
    assert_eq!(selected.kind, RuntimeEntityKind::Npc);
    assert!(thread.npc_actors().expect("actors").len() >= 2);

    thread
        .set_npc_spawn_targets(vec![hytale_server::NpcSpawnTarget {
            environment: "Env_Portal_Goblin_Surface".to_string(),
            center,
            radius: 2,
            mode: hytale_npc::spawn_position::SpawnMovementMode::Walk,
            density: 1.0,
            segments: 2048,
        }])
        .expect("set population target");
    let first = thread.tick_npc_spawns().expect("population tick");
    assert_eq!(first.spawned, 2, "expected population is two NPCs");
    assert_eq!(first.population, 2);
    let second = thread.tick_npc_spawns().expect("saturated tick");
    assert_eq!(second.spawned, 0);
    assert_eq!(second.population, 2);

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
