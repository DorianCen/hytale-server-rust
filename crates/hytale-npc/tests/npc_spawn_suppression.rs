//! Real-pack spawn suppression, NPC groups and live marker/beacon controllers.

use std::path::PathBuf;

use hytale_npc::flock::FlockRng;
use hytale_npc::spawn_special::{
    BeaconCatalog, BeaconController, SpawnMarkerCatalog, SpawnMarkerController,
};
use hytale_npc::spawn_suppression::{NpcGroupCatalog, SpawnSuppressionIndex};
use hytale_world::collision::Vec3;
use hytale_world::world_clock::{GameInstant, WorldClock, WorldClockSettings, ZERO_YEAR_SECONDS};

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

fn clock_at(hour: f64) -> WorldClock {
    WorldClock::from_game_time(
        Some(GameInstant::new(
            ZERO_YEAR_SECONDS + (hour * 3_600.0) as i64,
            0,
        )),
        WorldClockSettings {
            total_moon_phases: 5,
            ..WorldClockSettings::default()
        },
        false,
    )
}

struct FixedRng(f64);

impl FlockRng for FixedRng {
    fn next_f64(&mut self) -> f64 {
        self.0
    }
}

#[test]
fn real_npc_groups_and_suppression_zones_load() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let groups =
        NpcGroupCatalog::load_root(&repo_root().join("Assets/extracted")).expect("load NPC groups");
    assert_eq!(groups.len(), 114);
    assert!(groups.contains_role("Aggressive", "Rat"));
    assert!(groups.contains_role("Aggressive", "Goblin_Scrapper"));
    assert!(!groups.contains_role("Passive", "Rat"));
    assert!(groups.contains_role("Vermin", "Snake_Rattle"));

    let mut index = SpawnSuppressionIndex::load_root(&repo_root().join("Assets/extracted"))
        .expect("load suppression index");
    assert_eq!(index.configs.len(), 5);
    assert_eq!(index.groups.len(), 114);
    assert!(index.configs.get("Suppression_Portal_Goblin_Rat").is_some());
    assert!(index
        .configs
        .get("Test")
        .is_some_and(|config| { config.suppressed_groups == vec!["Trork", "Prey"] }));

    index.insert(
        7,
        "Suppression_Portal_Goblin_Rat",
        Vec3::new(10.0, 64.0, 10.0),
    );
    assert!(index.suppresses_role("Rat", Vec3::new(11.0, 64.0, 11.0)));
    assert!(!index.suppresses_role("Rat", Vec3::new(40.0, 64.0, 10.0)));
    assert!(!index.suppresses_role("Bunny", Vec3::new(11.0, 64.0, 11.0)));
    assert!(index.suppresses_marker(Vec3::new(10.0, 64.0, 10.0)));
    assert_eq!(index.zones_for_role("Rat").len(), 1);

    assert!(!index.remove(7).is_none());
    assert!(!index.suppresses_role("Rat", Vec3::new(11.0, 64.0, 11.0)));
}

#[test]
fn live_marker_and_beacon_controllers_select_roles_and_keep_cooldowns() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let markers = SpawnMarkerCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load spawn markers");
    let marker = markers.get("Bat").expect("Bat marker");
    let mut controller = SpawnMarkerController::new("Bat", Vec3::new(4.0, 64.0, 4.0), 0.0);
    let request = controller
        .trigger(marker, &mut FixedRng(0.0))
        .expect("Bat marker spawn");
    assert_eq!(request.role, "Bat");
    assert_eq!(request.cooldown_seconds, Some(86_400.0));
    assert!(controller.cooldown_remaining > 0.0);
    assert!(controller.trigger(marker, &mut FixedRng(0.0)).is_none());

    let persisted = controller.to_json();
    let mut restored = SpawnMarkerController::new("Bat", Vec3::default(), 0.0);
    restored.apply_json(&persisted);
    assert_eq!(restored.cooldown_remaining, controller.cooldown_remaining);
    assert_eq!(restored.last_role.as_deref(), Some("Bat"));

    // The Bat marker's deactivation distance gates automatic triggering.
    let mut automatic = SpawnMarkerController::new("Bat", Vec3::new(4.0, 64.0, 4.0), 0.0);
    assert!(automatic
        .tick(
            marker,
            &[Vec3::new(4.0, 64.0, 200.0)],
            false,
            1.0,
            1.0,
            &mut FixedRng(0.0)
        )
        .is_none());

    let beacons =
        BeaconCatalog::load_root(&repo_root().join("Assets/extracted")).expect("load beacons");
    let beacon = beacons.get("Edible_Rat").expect("Edible_Rat beacon");
    let mut controller = BeaconController::new(
        "Edible_Rat",
        Vec3::new(2.0, 64.0, 2.0),
        beacon,
        &mut FixedRng(0.0),
    );
    let request = controller
        .tick(beacon, &clock_at(12.0), 1.0, 1.0, &mut FixedRng(0.0))
        .expect("Edible_Rat beacon spawn");
    assert_eq!(request.roles, vec!["Edible_Rat"]);
    assert!(request.cooldown_seconds.is_some());
    assert_eq!(controller.spawned, 1);
    assert!(controller
        .tick(beacon, &clock_at(12.0), 1.0, 1.0, &mut FixedRng(0.0))
        .is_none());

    let state = controller.to_json();
    let restored = BeaconController::from_state("Edible_Rat", Vec3::default(), &state);
    assert_eq!(restored.spawned, 1);
    assert!(restored.cooldown_remaining > 0.0);
}
