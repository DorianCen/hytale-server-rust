//! Real-pack encounter and attack-graph checks.
//!
//! These load `Assets/extracted` and `Assets/Assets.zip`; `./tools/test.sh
//! assets` runs them and the fast tier skips them through `HYTALE_TEST_ASSETS=0`.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_interactions::catalog::InteractionCatalog;
use hytale_npc::combat::{AttackAction, CombatProfile, NpcActor, NpcTarget};
use hytale_npc::encounter::{
    BeaconMessage, EncounterAction, EncounterCatalog, EncounterContext, EncounterTarget,
};
use hytale_npc::role::RoleCatalog;
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::collision::Vec3;
use hytale_world::world::{World, WorldStorage};

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

fn target() -> EncounterTarget {
    EncounterTarget {
        id: 7,
        position: Vec3::new(0.5, 1.0, 0.5),
        health: 124.0,
        max_health: 124.0,
        is_player: false,
        attitude: vec!["Hostile".to_string()],
        groups: vec!["Test_Boss".to_string()],
        role: Some("Test_Boss_Basic_Intro".to_string()),
        invulnerable: true,
    }
}

#[test]
fn the_real_encounter_store_loads_and_variants_resolve() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = EncounterCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the encounter catalog");
    assert_eq!(catalog.len(), 67);
    assert_eq!(catalog.spawnable().count(), 41);

    let encounter = catalog
        .resolve("Example_Encounter")
        .expect("Example_Encounter");
    assert_eq!(encounter.start_state, "start");
    assert_eq!(encounter.instructions.len(), 1);

    let variant = catalog.resolve("Example_Variant").expect("Example_Variant");
    assert_eq!(variant.parameters["TriggerRange"], 60.0);
    assert_eq!(
        variant.parameters["LogMessage"],
        "Encounter variant: this message and the trigger range were overridden."
    );

    let host = catalog
        .resolve("Example_Macro_Slot_Boss_Variant")
        .expect("macro-slot variant");
    let instructions = serde_json::to_string(&host.instructions).expect("instructions");
    assert!(instructions.contains("Test_Manual_Marker"));
    assert!(!instructions.contains("\"$Null\""));

    let portal = catalog
        .resolve("Example_Portal_Drain")
        .expect("portal drain");
    assert_eq!(portal.instructions.len(), 1);
}

#[test]
fn real_state_transitions_run_in_order() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = EncounterCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the encounter catalog");
    let document = catalog
        .resolve("Example_State_Transitions")
        .expect("Example_State_Transitions");
    let mut runtime = document.runtime();
    assert_eq!(runtime.state(), "Idle");

    let player = EncounterContext::default().with_player(Vec3::new(0.5, 1.0, 0.5));
    let first = runtime.tick(0.1, &player);
    assert_eq!(runtime.state(), "Active");
    assert_eq!(
        first.actions,
        vec![EncounterAction::State {
            state: "Active".to_string()
        }]
    );

    let transition = runtime.tick(0.1, &player);
    assert_eq!(
        transition.actions[0],
        EncounterAction::Log {
            message: "[Encounter] --> ACTIVE: round starting".to_string()
        }
    );
    assert!(matches!(
        transition.actions[1],
        EncounterAction::TimerStart { ref name, seconds, .. }
            if name == "RoundTimer" && (seconds - 5.0).abs() < 1e-9
    ));
    assert_eq!(
        runtime.state(),
        "Active",
        "transition actions run before the instruction tree"
    );

    let cooldown = runtime.tick(5.0, &player);
    assert_eq!(runtime.state(), "Cooldown");
    assert_eq!(
        cooldown.actions.last(),
        Some(&EncounterAction::State {
            state: "Cooldown".to_string()
        })
    );

    let cooldown_actions = runtime.tick(0.1, &player);
    assert_eq!(
        cooldown_actions.actions[0],
        EncounterAction::Log {
            message: "[Encounter] --> COOLDOWN".to_string()
        }
    );
    assert!(matches!(
        cooldown_actions.actions[1],
        EncounterAction::TimerStart { ref name, seconds, .. }
            if name == "CooldownTimer" && (seconds - 3.0).abs() < 1e-9
    ));

    // The player leaves before the cooldown expires; the encounter returns to
    // Idle through the second nested instruction (no player).
    let idle = runtime.tick(3.0, &EncounterContext::default());
    assert_eq!(runtime.state(), "Idle");
    assert_eq!(
        idle.actions.last(),
        Some(&EncounterAction::State {
            state: "Idle".to_string()
        })
    );
}

#[test]
fn real_example_encounter_runs_spawner_and_timer_actions() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = EncounterCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the encounter catalog");
    let document = catalog
        .resolve("Example_Encounter")
        .expect("Example_Encounter");
    let mut runtime = document.runtime();
    let player = EncounterContext::default().with_player(Vec3::new(0.0, 0.0, 0.0));

    let first = runtime.tick(0.1, &player);
    assert_eq!(first.actions.len(), 3);
    assert!(matches!(
        first.actions[0],
        EncounterAction::TriggerSpawners {
            range,
            count: 2,
            ..
        } if (range - 40.0).abs() < 1e-9
    ));
    assert!(matches!(
        first.actions[1],
        EncounterAction::TimerStart { ref name, seconds, .. }
            if name == "SecondRound" && (seconds - 5.0).abs() < 1e-9
    ));
    assert_eq!(
        first.actions[2],
        EncounterAction::Log {
            message: "Encounter Manager Example: Spawned first round!".to_string()
        }
    );

    let waiting = runtime.tick(4.0, &player);
    assert_eq!(waiting.actions, Vec::new());
    let second = runtime.tick(1.0, &player);
    assert_eq!(second.actions.len(), 2);
    assert!(matches!(
        second.actions[0],
        EncounterAction::TriggerSpawners { count: 2, .. }
    ));
    assert_eq!(
        second.actions[1],
        EncounterAction::Log {
            message: "Encounter Manager Example: Spawned second round!".to_string()
        }
    );
}

#[test]
fn real_boss_encounter_runs_boss_bar_music_weather_and_phases() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = EncounterCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the encounter catalog");
    let document = catalog.resolve("Example_Boss").expect("Example_Boss");
    let mut runtime = document.runtime();
    assert_eq!(runtime.state(), "Intro");

    let mut context = EncounterContext::default()
        .with_player(Vec3::new(0.0, 1.0, 0.0))
        .with_target("Boss", target());
    let intro = runtime.tick(0.1, &context);
    assert!(intro.actions.iter().any(|action| matches!(
        action,
        EncounterAction::BossBar { name, .. }
            if name == "server_no_translation.npcRoles.Test_Boss_Basic_Intro.name"
    )));

    context.beacons.push(BeaconMessage::new("Boss_In_Position"));
    let phase = runtime.tick(0.1, &context);
    assert_eq!(runtime.state(), "Phase_1");
    assert!(phase.actions.iter().any(|action| matches!(
        action,
        EncounterAction::Weather { weather, .. } if weather == "Blood_Moon"
    )));
    assert!(phase.actions.iter().any(|action| matches!(
        action,
        EncounterAction::StartMusic { music_container }
            if music_container == "Track_Encounter_Cave_Test"
    )));
    assert!(phase.actions.iter().any(|action| matches!(
        action,
        EncounterAction::TargetRole { role, .. } if role == "Test_Boss_Basic"
    )));

    // Drop the boss below half health. The second phase transition replaces
    // the role, plays the encounter intensity state and enters Phase_2.
    let mut context = context.clone();
    context.targets.get_mut("Boss").unwrap().health = 20.0;
    let first = runtime.tick(0.1, &context);
    let second = runtime.tick(0.1, &context);
    assert_eq!(runtime.state(), "Phase_2");
    assert!(first
        .actions
        .iter()
        .chain(second.actions.iter())
        .any(|action| matches!(
            action,
            EncounterAction::AudioState { value, .. } if value == "Climax"
        )));
}

#[test]
fn real_rat_attack_graph_uses_the_interaction_damage_node() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    let interactions = InteractionCatalog::load(&pack).expect("load the interaction catalog");
    let roles = RoleCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the role catalog");
    let rat = roles
        .resolve("Rat")
        .expect("resolve Rat")
        .expect("Rat exists");
    let profile = CombatProfile::from_role(&rat).expect("Rat profile");
    assert_eq!(profile.attack_root.as_deref(), Some("Root_NPC_Rat_Attack"));
    let action: AttackAction = profile.attack_action().expect("attack action");
    assert_eq!(action.attack.as_deref(), Some("Root_NPC_Rat_Attack"));

    let hits = profile.attack_hits(&interactions, &action);
    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.source.as_deref(), Some("Bite_Damage"));
    assert!((hit.runtime - 0.25).abs() < 1e-9);
    let (calculator, effects) = hit.select(0.0, None);
    let calculator = calculator.expect("role damage override");
    let range = calculator.compute_range(1.0);
    assert!((range[0] - 8.1).abs() < 1e-4);
    assert!((range[1] - 9.9).abs() < 1e-4);
    assert!(effects.is_some_and(|effects| effects.knockback.is_some()));

    let dir = std::env::temp_dir().join(format!(
        "hytale-npc-encounter-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut world = World::new(WorldStorage::new(dir.clone()));
    let mut column = ChunkColumn::new(0, 0);
    for x in 0..=15 {
        for z in 0..=15 {
            column.set_block(x, 0, z, 1, 0, 0).unwrap();
        }
    }
    world.insert_chunk(column);

    let mut actor = NpcActor::from_assets_with_interactions(
        &registry,
        &rat,
        Vec3::new(0.5, 1.0, 0.5),
        0.0,
        Some(&interactions),
    )
    .expect("actor");
    let targets = [NpcTarget {
        id: 7,
        position: Vec3::new(1.5, 1.0, 0.5),
    }];
    let attack = actor
        .tick(0.5, &world, |block| block != 0, &targets, 0.0)
        .attack
        .expect("Rat bites the target");
    assert_eq!(attack.target, 7);
    assert_eq!(attack.source.as_deref(), Some("Bite_Damage"));
    assert_eq!(attack.attack_root.as_deref(), Some("Root_NPC_Rat_Attack"));
    assert_eq!(attack.damages, vec![("Physical".to_string(), 8.1)]);
    assert!(attack
        .effects
        .is_some_and(|effects| effects.knockback.is_some()));
    std::fs::remove_dir_all(dir).ok();
}
