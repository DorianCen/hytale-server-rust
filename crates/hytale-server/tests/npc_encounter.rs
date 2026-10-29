//! T59c: a real encounter running through the world thread from trigger to
//! completion, driven by the same player targets a client session publishes.

use std::path::PathBuf;

use hytale_npc::combat::NpcTarget;
use hytale_npc::encounter::{BeaconMessage, EncounterAction, EncounterTarget};
use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::collision::Vec3;

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
