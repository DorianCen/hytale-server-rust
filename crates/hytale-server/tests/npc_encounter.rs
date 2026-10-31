//! T59c: a real encounter running through the world thread from trigger to
//! completion, driven by the same player targets a client session publishes.

use std::path::PathBuf;

use hytale_npc::combat::NpcTarget;
use hytale_npc::encounter::{BeaconMessage, EncounterAction, EncounterTarget};
use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::collision::Vec3;
use hytale_world::player_data::{PlayerTransform, Position, Rotation};

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
        "hytale-npc-encounter-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn a_real_encounter_spawn_action_creates_a_runtime_npc() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("spawn-action");
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
    thread.load_chunk(0, 0).expect("load chunk");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass is a block");
    for x in 0..=8 {
        for z in 0..=8 {
            thread.set_block(x, 80, z, soil).expect("floor");
            for y in 81..=84 {
                thread.set_block(x, y, z, 0).expect("headroom");
            }
        }
    }
    let state = thread
        .spawn_encounter("Coffer_Goblin_Wave_Timed")
        .expect("spawn encounter");
    assert_eq!(state, "Start");
    thread
        .set_npc_targets(vec![NpcTarget {
            id: hytale_server::players::PLAYER_NPC_TARGET_OFFSET | 1,
            position: Vec3::new(4.5, 81.0, 4.5),
        }])
        .expect("publish the player position");
    let reports = thread.tick_encounters_for(0.2).expect("encounter tick");
    assert!(
        reports.iter().any(|(_, report)| report
            .actions
            .iter()
            .any(|action| matches!(action, EncounterAction::Spawn { .. }))),
        "{reports:?}"
    );
    assert!(
        !thread.npc_actors().expect("actors").is_empty(),
        "the Spawn action creates a live runtime NPC"
    );
    assert!(
        thread
            .take_npc_presentation_events()
            .expect("presentation events")
            .is_empty(),
        "no NPC builder presentation actions fired in this scenario"
    );

    let fallback = hytale_world::player_data::PlayerTransform::new(
        hytale_world::player_data::Position {
            x: 4.0,
            y: 81.0,
            z: 4.0,
        },
        hytale_world::player_data::Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    );
    let plan = thread
        .portal_plan("Goblin_Breach", fallback)
        .expect("portal plan");
    assert_eq!(plan.instance_id, "Portals/Portals_Goblins");
    assert_eq!(plan.spawn.position.y, 149.0);
    assert_eq!(plan.time_limit_seconds, Some(600.0));

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn encounter_target_actions_update_a_live_npc() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("target-actions");
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
    thread.load_chunk(0, 0).expect("load chunk");

    let transform = hytale_world::player_data::PlayerTransform::new(
        hytale_world::player_data::Position {
            x: 4.5,
            y: 81.0,
            z: 4.5,
        },
        hytale_world::player_data::Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    );
    let npc = thread.spawn_npc("Rat", transform).expect("spawn npc");
    let state = thread
        .spawn_encounter("Example_Boss")
        .expect("spawn encounter");
    assert_eq!(state, "Intro");
    thread
        .set_npc_targets(vec![NpcTarget {
            id: hytale_server::players::PLAYER_NPC_TARGET_OFFSET | 1,
            position: Vec3::new(4.5, 81.0, 4.5),
        }])
        .expect("player position");
    thread
        .set_encounter_target(
            "Example_Boss",
            "Boss",
            EncounterTarget {
                id: npc.id,
                position: Vec3::new(4.5, 81.0, 4.5),
                health: 124.0,
                max_health: 124.0,
                is_player: false,
                attitude: vec!["Hostile".to_string()],
                groups: vec!["Test_Boss".to_string()],
                role: Some("Rat".to_string()),
                invulnerable: true,
            },
        )
        .expect("bind the live npc to the Boss slot");
    assert!(thread
        .send_encounter_beacon("Example_Boss", BeaconMessage::new("Boss_In_Position"))
        .expect("send beacon"));
    let reports = thread.tick_encounters_for(0.1).expect("encounter tick");
    assert!(reports.iter().any(|(_, report)| report.actions.iter().any(
        |action| matches!(action, EncounterAction::TargetRole { role, .. } if role == "Test_Boss_Basic")
    )));
    let actors = thread.npc_actors().expect("actors");
    let actor = actors
        .iter()
        .find(|(id, _)| *id == npc.id)
        .map(|(_, actor)| actor)
        .expect("the npc is still live");
    assert_eq!(actor.role, "Test_Boss_Basic");
    assert!(!actor.invulnerable);

    thread
        .force_weather(Some("Blood_Moon"))
        .expect("force encounter weather");
    let assets = thread.asset_registry().expect("assets");
    let blood_moon = assets.weather_id("Blood_Moon").expect("Blood_Moon asset");
    assert_eq!(
        thread.weather_at(4, 81, 4).expect("weather"),
        Some(blood_moon)
    );
    thread.force_weather(None).expect("clear encounter weather");

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_real_npc_builder_place_block_action_changes_the_world() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("place-block");
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
    thread.load_chunk(0, 0).expect("load chunk");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass is a block");
    for x in 0..=8 {
        for z in 0..=8 {
            thread.set_block(x, 80, z, soil).expect("floor");
            for y in 81..=84 {
                thread.set_block(x, y, z, 0).expect("headroom");
            }
        }
    }
    let transform = PlayerTransform::new(
        Position {
            x: 4.5,
            y: 81.0,
            z: 4.5,
        },
        Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    );
    thread
        .spawn_npc("Test_Place_Block", transform)
        .expect("spawn place-block npc");
    for _ in 0..8 {
        std::thread::sleep(std::time::Duration::from_millis(120));
        thread.tick_npcs().expect("npc tick");
    }
    let events = thread.take_block_change_events().expect("block events");
    let change = events
        .iter()
        .find(|event| event.before != event.after)
        .unwrap_or_else(|| {
            panic!("the builder PlaceBlock action produced a block change: {events:?}")
        });
    assert!(change.after > 0);
    assert_eq!(
        thread
            .get_block(change.x, change.y, change.z)
            .expect("read placed block"),
        Some(change.after)
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_real_npc_builder_spawn_action_creates_npcs() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("npc-spawn-action");
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
    thread.load_chunk(0, 0).expect("load chunk");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass is a block");
    for x in 0..=8 {
        for z in 0..=8 {
            thread.set_block(x, 80, z, soil).expect("floor");
            for y in 81..=84 {
                thread.set_block(x, y, z, 0).expect("headroom");
            }
        }
    }
    let transform = PlayerTransform::new(
        Position {
            x: 4.5,
            y: 81.0,
            z: 4.5,
        },
        Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    );
    thread
        .spawn_npc("Test_Spawn_Action", transform)
        .expect("spawn spawn-action npc");
    thread
        .set_npc_targets(vec![NpcTarget {
            id: hytale_server::players::PLAYER_NPC_TARGET_OFFSET | 1,
            position: Vec3::new(4.5, 81.0, 4.5),
        }])
        .expect("player target");
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(120));
        thread.tick_npcs().expect("npc tick");
    }
    let actors = thread.npc_actors().expect("actors");
    assert!(
        actors.len() > 1,
        "the builder Spawn action creates NPCs: {}",
        actors.len()
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn damaging_an_npc_updates_its_entity_stat_snapshot() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("npc-stat-damage");
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
    thread.load_chunk(0, 0).expect("load chunk");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass is a block");
    thread.set_block(4, 80, 4, soil).expect("floor");
    thread.set_block(4, 81, 4, 0).expect("headroom");
    let transform = PlayerTransform::new(
        Position {
            x: 4.5,
            y: 81.0,
            z: 4.5,
        },
        Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    );
    let npc = thread.spawn_npc("Rat", transform).expect("spawn npc");
    let before = thread
        .entity(npc.id)
        .expect("entity")
        .expect("live npc")
        .components["Npc"]["Health"]
        .as_f64()
        .expect("health");
    thread
        .damage_npc(npc.id, 3.0, "Physical")
        .expect("damage npc")
        .expect("live target");
    let entity = thread.entity(npc.id).expect("entity").expect("live npc");
    let after = entity.components["Npc"]["Health"].as_f64().expect("health");
    assert!(after < before, "damaged health {after} < {before}");
    assert!(
        entity.components["EntityStats"].is_object(),
        "the runtime entity carries its EntityStats snapshot"
    );
    assert!(
        thread
            .runtime_entity_events()
            .expect("events")
            .iter()
            .any(|event| format!("{event:?}").contains(&npc.id.to_string())),
        "the damage queues a runtime entity update"
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_real_interaction_root_applies_block_actions() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("interaction-root");
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
    thread.load_chunk(0, 0).expect("load chunk");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass is a block");
    thread.set_block(4, 80, 4, soil).expect("set block");
    assert_eq!(thread.get_block(4, 80, 4).expect("block"), Some(soil));
    assert!(
        thread
            .apply_interaction_root("Block_Break_Adventure", 4, 80, 4, None)
            .expect("apply interaction"),
        "the real Block_Break_Adventure root breaks the block"
    );
    assert_ne!(
        thread.get_block(4, 80, 4).expect("block"),
        Some(soil),
        "the interaction's BreakBlock action changed the block"
    );
    assert!(
        thread
            .take_block_change_events()
            .expect("events")
            .iter()
            .any(|event| event.x == 4 && event.y == 80 && event.z == 4),
        "the block change is queued for broadcast"
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_nearby_item_entity_can_be_picked_up() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("pickup-item");
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
    thread.load_chunk(0, 0).expect("load chunk");
    let item = thread
        .spawn_item_entity("Rock_Stone", 3, 4.5, 81.0, 4.5)
        .expect("spawn item");
    let picked = thread
        .pickup_nearest_item(4.5, 81.0, 4.5, 2.0)
        .expect("pickup")
        .expect("an item is nearby");
    assert_eq!(picked.id, item.id);
    assert_eq!(picked.item_id(), Some("Rock_Stone"));
    assert_eq!(picked.item_quantity(), 3);
    assert!(thread
        .pickup_nearest_item(4.5, 81.0, 4.5, 2.0)
        .expect("second pickup")
        .is_none());

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_real_portal_drain_encounter_updates_the_portal_timer() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("portal-timer");
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
    thread
        .spawn_encounter("Example_Portal_Drain")
        .expect("spawn encounter");
    thread
        .set_npc_targets(vec![NpcTarget {
            id: hytale_server::players::PLAYER_NPC_TARGET_OFFSET | 1,
            position: Vec3::new(0.5, 80.0, 0.5),
        }])
        .expect("player position");
    thread
        .set_encounter_portal_timer("Example_Portal_Drain", Some(60.0))
        .expect("seed portal timer");
    for _ in 0..8 {
        thread.tick_encounters_for(1.0).expect("encounter tick");
    }
    assert_eq!(
        thread
            .encounter_portal_remaining("Example_Portal_Drain")
            .expect("portal timer"),
        Some(30.0)
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_real_boss_encounter_runs_from_trigger_to_completion() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("boss");
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

    let state = thread
        .spawn_encounter("Example_Boss")
        .expect("spawn encounter");
    assert_eq!(state, "Intro");
    assert_eq!(
        thread.encounter_states().expect("states"),
        vec![("Example_Boss".to_string(), "Intro".to_string())]
    );

    thread
        .set_npc_targets(vec![NpcTarget {
            id: hytale_server::players::PLAYER_NPC_TARGET_OFFSET | 1,
            position: Vec3::new(0.5, 80.0, 0.5),
        }])
        .expect("publish the bot position");
    assert!(thread
        .set_encounter_target(
            "Example_Boss",
            "Boss",
            EncounterTarget {
                id: 42,
                position: Vec3::new(2.0, 80.0, 0.5),
                health: 124.0,
                max_health: 124.0,
                is_player: false,
                attitude: vec!["Hostile".to_string()],
                groups: vec!["Test_Boss".to_string()],
                role: Some("Test_Boss_Basic_Intro".to_string()),
                invulnerable: true,
            },
        )
        .expect("set target"));

    let intro = thread.tick_encounters_for(0.1).expect("intro tick");
    assert_eq!(intro[0].1.state, "Intro");
    assert!(intro[0]
        .1
        .actions
        .iter()
        .any(|action| matches!(action, EncounterAction::BossBar { .. })));

    assert!(thread
        .send_encounter_beacon("Example_Boss", BeaconMessage::new("Boss_In_Position"))
        .expect("send beacon"));
    let phase_1 = thread.tick_encounters_for(0.1).expect("phase tick");
    assert_eq!(phase_1[0].1.state, "Phase_1");
    assert!(phase_1[0].1.actions.iter().any(|action| matches!(
        action,
        EncounterAction::Weather { weather, .. } if weather == "Blood_Moon"
    )));
    assert!(phase_1[0]
        .1
        .actions
        .iter()
        .any(|action| matches!(action, EncounterAction::StartMusic { .. })));

    // The bot keeps attacking; drop the boss below half health to trigger the
    // phase transition, then let the remaining player-present branches run.
    thread
        .set_encounter_target(
            "Example_Boss",
            "Boss",
            EncounterTarget {
                id: 42,
                position: Vec3::new(2.0, 80.0, 0.5),
                health: 20.0,
                max_health: 124.0,
                is_player: false,
                attitude: vec!["Hostile".to_string()],
                groups: vec!["Test_Boss".to_string()],
                role: Some("Test_Boss_Basic_Intro".to_string()),
                invulnerable: true,
            },
        )
        .expect("drop boss health");

    // The boss dies: its target slot is released and the fallback branch runs.
    thread
        .clear_encounter_target("Example_Boss", "Boss")
        .expect("release the boss target");
    let mut seen = Vec::new();
    let mut completed = false;
    for _ in 0..8 {
        for (_, report) in thread.tick_encounters_for(0.1).expect("encounter tick") {
            seen.extend(report.actions.iter().map(|action| match action {
                EncounterAction::State { state } => format!("state:{state}"),
                EncounterAction::Log { message } => format!("log:{message}"),
                _ => String::new(),
            }));
            if report.state == "Complete" {
                completed = true;
            }
        }
        if completed {
            break;
        }
    }
    assert!(completed, "encounter reached Complete: {seen:?}");
    assert!(seen.iter().any(|action| action == "state:Phase_2"));
    assert!(seen.iter().any(|action| action == "state:BossDead"));
    assert!(seen.iter().any(|action| action == "state:Complete"));

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
