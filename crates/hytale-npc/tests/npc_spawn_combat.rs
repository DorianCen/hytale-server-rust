//! Real-pack spawn and combat checks.
//!
//! These load `Assets/extracted` and `Assets/Assets.zip`; `./tools/test.sh
//! assets` runs them and the fast tier skips them through `HYTALE_TEST_ASSETS=0`.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_entities::damage::{Damage, DamageCause, DamageCauses, DamageSource};
use hytale_npc::block_set::BlockSetCatalog;
use hytale_npc::combat::{CombatProfile, NpcActor};
use hytale_npc::flock::FlockRng;
use hytale_npc::fluid_tags::FluidTagIndex;
use hytale_npc::role::RoleCatalog;
use hytale_npc::spawn::{WorldSpawn, WorldSpawnCatalog};
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
fn the_real_world_spawn_store_loads() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = WorldSpawnCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the world spawn catalog");
    assert_eq!(catalog.len(), 98);
    let entries: usize = catalog.iter().map(|spawn| spawn.npcs.len()).sum();
    assert_eq!(entries, 271);

    let surface = catalog
        .get("Spawns_Portals_Goblin_Surface")
        .expect("Spawns_Portals_Goblin_Surface");
    assert_eq!(surface.environments, vec!["Env_Portal_Goblin_Surface"]);
    assert_eq!(surface.npcs.len(), 8);
    assert_eq!(surface.day_time_range, [6.0, 18.0]);
    assert!(surface.scale_day_time_range);
    let bear = surface
        .npcs
        .iter()
        .find(|role| role.id == "Bear_Grizzly")
        .expect("Bear_Grizzly entry");
    assert_eq!(bear.weight, 5.0);
    assert_eq!(bear.spawn_block_set.as_deref(), Some("Soil"));

    let inline = surface
        .npcs
        .iter()
        .find(|role| role.id == "Bunny")
        .expect("Bunny entry");
    assert!(
        inline.flock.is_some(),
        "Bunny has an inline Group_Tiny flock"
    );
}

#[test]
fn real_spawn_time_and_moon_gates_match_java_semantics() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let catalog = WorldSpawnCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the world spawn catalog");
    let surface = catalog
        .get("Spawns_Portals_Goblin_Surface")
        .expect("surface spawn");
    assert!(surface.is_eligible(&clock_at(12.0)));
    assert!(!surface.is_eligible(&clock_at(23.0)));

    let night = catalog.get("Tier1_Night_NPC1").expect("Tier1_Night_NPC1");
    assert!(night.is_eligible(&clock_at(23.0)));
    assert!(!night.is_eligible(&clock_at(12.0)));
    assert_eq!(night.moon_phase_weight_modifier(0), 0.0);
    assert_eq!(night.moon_phase_weight_modifier(4), 0.0);
    assert_eq!(night.effective_weight(&night.npcs[0], 0), 0.0);

    let flock = catalog
        .get("Spawns_Portals_Goblin_Surface")
        .expect("surface spawn");
    assert_eq!(
        flock
            .pick_role(&mut FixedRng(0.0), 0)
            .map(|role| role.id.as_str()),
        Some("Boar")
    );
    assert_eq!(
        flock
            .pick_role(&mut FixedRng(0.45), 0)
            .map(|role| role.id.as_str()),
        Some("Bunny")
    );
}

#[test]
fn real_role_combat_profiles_use_role_documents_and_damage_ranges() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let roles = RoleCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the role catalog");

    let rat = roles
        .resolve("Rat")
        .expect("resolve Rat")
        .expect("Rat exists");
    let rat_profile = CombatProfile::from_role(&rat).expect("Rat profile");
    assert_eq!(rat_profile.max_health, 10.0);
    assert_eq!(rat_profile.view_range, 8.0);
    assert_eq!(rat_profile.attack_distance, 2.0);
    assert!((rat_profile.chase_speed() - 4.8).abs() < 1e-9);
    let range = rat_profile.primary_attack().unwrap().compute_range(1.0);
    assert!((range[0] - 8.1).abs() < 1e-5);
    assert!((range[1] - 9.9).abs() < 1e-5);

    let bear = roles
        .resolve("Bear_Grizzly")
        .expect("resolve Bear_Grizzly")
        .expect("Bear_Grizzly exists");
    let bear_profile = CombatProfile::from_role(&bear).expect("Bear profile");
    assert_eq!(bear_profile.max_health, 124.0);
    // Three role-specific attacks plus the inherited generic `Melee_Damage`.
    assert_eq!(bear_profile.attacks.len(), 4);
    assert!((bear_profile.primary_attack().unwrap().compute_range(1.0)[1] - 41.8).abs() < 1e-4);

    let goblin = roles
        .resolve("Goblin_Scrapper")
        .expect("resolve Goblin_Scrapper")
        .expect("Goblin_Scrapper exists");
    let goblin_profile = CombatProfile::from_role(&goblin).expect("Goblin profile");
    assert_eq!(goblin_profile.max_health, 36.0);
    assert!((goblin_profile.primary_attack().unwrap().compute_range(1.0)[1] - 17.6).abs() < 1e-4);
}

#[test]
fn a_real_asset_actor_takes_damage_and_dies() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    let roles = RoleCatalog::load_root(&repo_root().join("Assets/extracted"))
        .expect("load the role catalog");
    let rat = roles
        .resolve("Rat")
        .expect("resolve Rat")
        .expect("Rat exists");

    let mut actor = NpcActor::from_assets(&registry, &rat, Vec3::new(0.5, 1.0, 0.5), 0.0)
        .expect("build the actor");
    assert_eq!(actor.health(), 10.0);
    let causes = DamageCauses::new(registry.damage_causes.iter().filter_map(|record| {
        record
            .id
            .map(|_| DamageCause::from_value(&record.key, &record.value))
    }));
    let first = actor.take_damage(
        &Damage::new(9.0, "Physical", DamageSource::Entity(42)),
        &causes,
    );
    assert_eq!(first.remaining_health, 1.0);
    assert!(!first.died);
    let second = actor.take_damage(
        &Damage::new(9.0, "Physical", DamageSource::Entity(42)),
        &causes,
    );
    assert!(second.died);
    assert!(actor.is_dead());
    let components = actor.runtime_components();
    assert_eq!(components["Npc"]["Role"], "Rat");
    assert_eq!(components["Npc"]["Health"], 0.0);
    assert_eq!(components["Model"]["Model"]["Id"], "Rat");

    let tags = FluidTagIndex::from_assets(&registry);
    let water = registry
        .fluids
        .get("Water")
        .and_then(|record| record.id)
        .expect("Water fluid id");
    let lava = registry
        .fluids
        .get("Lava")
        .and_then(|record| record.id)
        .expect("Lava fluid id");
    assert!(tags.contains("Water", water));
    assert!(!tags.contains("Water", lava));
    assert!(tags.contains("Lava", lava));
    assert_eq!(WorldSpawn::spawn_weight(2.0, 1), 1.0);
}

#[test]
fn real_block_sets_resolve_spawn_floors() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    let catalog = BlockSetCatalog::load(&pack).expect("load block sets");
    assert_eq!(catalog.len(), 80);

    let stone = catalog.resolve(&registry, "Stone").expect("Stone set");
    let rock = registry
        .block_types
        .get("Rock_Stone")
        .and_then(|record| record.id)
        .expect("Rock_Stone block id");
    assert!(stone.contains(&rock));

    // `.append` codecs do not inherit parent fields; StoneAndSoil only lists
    // `Soil*`, matching the Java BlockSet document as written.
    let stone_and_soil = catalog
        .resolve(&registry, "StoneAndSoil")
        .expect("StoneAndSoil set");
    let soil = registry
        .block_types
        .get("Soil_Grass")
        .and_then(|record| record.id)
        .expect("Soil_Grass block id");
    assert!(stone_and_soil.contains(&soil));
    assert!(!stone_and_soil.contains(&rock));

    let soil_set = catalog.resolve(&registry, "Soil").expect("Soil set");
    let leaves = registry
        .block_types
        .get("Plant_Leaves_Oak")
        .and_then(|record| record.id)
        .expect("Plant_Leaves_Oak block id");
    assert!(!soil_set.contains(&leaves), "ExcludeBlockTypes applies");
}
