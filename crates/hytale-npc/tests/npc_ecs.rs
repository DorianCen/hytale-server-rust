//! T60b: real-role ECS blackboard/motion schedule.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_npc::combat::{NpcActor, NpcTarget};
use hytale_npc::ecs::NpcEcs;
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

fn flat_world(name: &str) -> World {
    let dir = std::env::temp_dir().join(format!(
        "hytale-npc-ecs-{}-{}-{name}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut world = World::new(WorldStorage::new(dir));
    let mut column = ChunkColumn::new(0, 0);
    for x in 0..=15 {
        for z in 0..=15 {
            column.set_block(x, 0, z, 1, 0, 0).unwrap();
        }
    }
    world.insert_chunk(column);
    world
}

#[test]
fn real_fly_and_dive_roles_move_vertically_through_ecs() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("pack");
    let registry = AssetRegistry::load_p0(&pack).expect("registry");
    let roles = RoleCatalog::load_root(&repo_root().join("Assets/extracted")).expect("roles");
    let world = flat_world("vertical");

    for (role_key, target_y) in [
        ("Void_Anomaly_Crystal_Stutter", 1_000.0),
        ("Template_Swimming_Aggressive", 2.0),
    ] {
        let role = roles
            .resolve(role_key)
            .expect("resolve")
            .unwrap_or_else(|| panic!("{role_key} exists"));
        let mut actor =
            NpcActor::from_assets(&registry, &role, Vec3::new(0.5, 4.0, 0.5), 0.0).expect("actor");
        actor.target = Some(7);
        assert!(actor.motion.moves_vertically(), "{role_key}");
        let mut ecs = NpcEcs::new();
        ecs.insert(1, actor);
        let targets = [NpcTarget {
            id: 7,
            position: Vec3::new(5.5, target_y, 5.5),
        }];
        let mut moved = false;
        for _ in 0..20 {
            let report = ecs
                .tick_actor(1, 0.25, &world, |block| block != 0, &targets, 0.5)
                .expect("actor");
            moved |= report.moved;
        }
        let actor = ecs.get(&1).expect("actor");
        assert!(moved, "{role_key} did not move");
        assert!(
            (actor.position.y - 4.0).abs() > 0.01 || (actor.position.x - 0.5).abs() > 0.01,
            "{role_key} did not use its vertical controller"
        );
        if role_key == "Void_Anomaly_Crystal_Stutter" {
            assert!(
                actor.position.y <= 1.0 + actor.motion.max_height_over_ground + 1e-6,
                "fly altitude clamped to the ground-relative ceiling: {}",
                actor.position.y
            );
        }
    }
}

#[test]
fn real_fly_and_dive_roles_clamp_altitude_and_depth() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("pack");
    let registry = AssetRegistry::load_p0(&pack).expect("registry");
    let roles = RoleCatalog::load_root(&repo_root().join("Assets/extracted")).expect("roles");

    let fly = roles
        .resolve("Void_Anomaly_Crystal_Stutter")
        .expect("resolve")
        .expect("fly role");
    let fly = NpcActor::from_assets(&registry, &fly, Vec3::new(0.5, 4.0, 0.5), 0.0)
        .expect("fly actor")
        .motion;
    let ground = 1.0;
    let high = fly.clamp_altitude(10_000.0, ground);
    assert!(
        high <= ground + fly.max_height_over_ground + 1e-6,
        "MaxHeightOverGround {high}"
    );
    let low = fly.clamp_altitude(-10_000.0, ground);
    assert!(
        low >= ground + fly.min_height_over_ground - 1e-6,
        "MinHeightOverGround {low}"
    );
    let blended = fly.blend_altitude(100.0, 10.0);
    assert!((10.0..=100.0).contains(&blended), "blended {blended}");

    let dive = roles
        .resolve("Template_Swimming_Aggressive")
        .expect("resolve")
        .expect("dive role");
    let dive = NpcActor::from_assets(&registry, &dive, Vec3::new(0.5, 4.0, 0.5), 0.0)
        .expect("dive actor")
        .motion;
    let surface = 10.0;
    let deep = dive.clamp_dive_depth(-10_000.0, surface);
    assert!(
        deep >= surface - dive.max_dive_depth - 1e-6,
        "MaxDiveDepth {deep}"
    );
    let shallow = dive.clamp_dive_depth(10_000.0, surface);
    assert!(
        shallow <= surface - dive.min_dive_depth + 1e-6,
        "MinDiveDepth {shallow}"
    );
}

#[test]
fn real_role_ticks_through_ecs_blackboard_and_motion_components() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("pack");
    let registry = AssetRegistry::load_p0(&pack).expect("registry");
    let roles = RoleCatalog::load_root(&repo_root().join("Assets/extracted")).expect("roles");
    let role = roles.resolve("Rat").expect("resolve").expect("Rat");
    let actor =
        NpcActor::from_assets(&registry, &role, Vec3::new(0.5, 1.0, 0.5), 0.0).expect("actor");
    assert!(!actor.builder.is_empty(), "Rat has instruction nodes");
    assert_eq!(actor.motion.mode, hytale_npc::motion::MotionMode::Walk);

    let mut ecs = NpcEcs::new();
    ecs.insert(1, actor);
    let world = flat_world("schedule");
    let targets = [NpcTarget {
        id: 7,
        position: Vec3::new(8.5, 1.0, 0.5),
    }];
    let mut moved = false;
    let mut report = None;
    for _ in 0..20 {
        let tick = ecs
            .tick_actor(1, 0.5, &world, |block| block != 0, &targets, 0.5)
            .expect("actor exists");
        moved |= tick.moved;
        report = Some(tick);
    }
    let report = report.expect("a tick ran");
    assert_eq!(report.target, Some(7));
    assert!(moved, "the ECS motion schedule moved the actor");
    let actor = ecs.get(&1).expect("actor");
    assert_eq!(actor.blackboard.target, Some(7));
    assert!(!actor.blackboard.state.is_empty());
    assert!(ecs.get(&1).expect("actor").motion.chase_speed(1.0) > 0.0);
}
