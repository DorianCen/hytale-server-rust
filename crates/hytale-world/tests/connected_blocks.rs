//! Patterned connected-block evaluation against the real pipe assets.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::connected::ConnectedBlockCatalog;
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

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-connected-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn world(name: &str) -> (World, PathBuf) {
    let dir = temp_dir(name);
    let mut world = World::new(WorldStorage::new(&dir));
    world.insert_chunk(ChunkColumn::new(0, 0));
    (world, dir)
}

#[test]
fn a_pipe_resolves_its_corner_variant_and_updates_neighbours() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    assert_eq!(catalog.len(), 3, "three patterned pipe rule sets load");

    let straight = assets
        .block_types
        .id_of("Metal_Goblin_Iron_Pipe_Large")
        .expect("straight pipe");
    let corner = assets
        .block_types
        .id_of("Metal_Goblin_Iron_Pipe_Large_Corner")
        .expect("corner pipe");
    let (mut world, dir) = world("pipe");

    // A pipe with one neighbour above and a rotated corner pipe to the north
    // resolves to the corner shape (Up + North connections, no straight run).
    world
        .set_block_full(10, 64, 10, straight, 0, 0)
        .expect("origin pipe");
    world
        .set_block_full(10, 64, 9, corner, 0, 0)
        .expect("north corner pipe");

    let changes = catalog.apply(&mut world, &assets, 10, 64, 10, 0, [0, 1, 0], true);
    assert!(
        changes.is_empty(),
        "one north connection is not a corner yet: {changes:?}"
    );

    // Adding the upper pipe and refreshing its neighbourhood must update the
    // already-placed origin to the corner shape.
    world
        .set_block_full(10, 65, 10, straight, 0, 0)
        .expect("upper pipe");
    let changes = catalog.update_around(&mut world, &assets, 10, 65, 10, [0, 0, 0]);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (10, 64, 10) && change.after == corner
        }),
        "the neighbour update should resolve the origin corner: {changes:?}"
    );
    assert_eq!(world.get_block(10, 64, 10), corner);

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_pipe_without_connections_keeps_its_base_shape() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    let straight = assets
        .block_types
        .id_of("Metal_Goblin_Iron_Pipe_Large")
        .expect("straight pipe");
    let (mut world, dir) = world("pipe-negative");
    world
        .set_block_full(4, 64, 4, straight, 0, 0)
        .expect("isolated pipe");

    let changes = catalog.apply(&mut world, &assets, 4, 64, 4, 0, [0, 0, 0], true);
    assert!(changes.is_empty(), "no rule matches an isolated pipe");
    assert_eq!(world.get_block(4, 64, 4), straight);

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_fence_resolves_its_custom_template_corner() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    let base = assets
        .block_types
        .id_of("Deco_Iron_Bars")
        .expect("iron bars base");
    let corner = assets
        .block_types
        .id_of("Deco_Iron_Bars_Corner")
        .expect("iron bars corner");
    let (mut world, dir) = world("fence");

    world
        .set_block_full(10, 64, 10, base, 0, 0)
        .expect("origin fence");
    world
        .set_block_full(9, 64, 10, base, 0, 0)
        .expect("west fence connection");
    world
        .set_block_full(10, 64, 11, corner, 2, 0)
        .expect("south rotated corner connection");

    let changes = catalog.apply(&mut world, &assets, 10, 64, 10, 0, [0, 1, 0], true);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (10, 64, 10) && change.after == corner
        }),
        "the fence should resolve to its Corner template shape: {changes:?}"
    );
    assert_eq!(world.get_block(10, 64, 10), corner);

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_stair_resolves_straight_and_corner_variants() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    let straight = assets
        .block_types
        .id_of("Wood_Amber_Trunk_Stairs")
        .expect("stair base");
    let corner = assets
        .block_types
        .id_of("*Wood_Amber_Trunk_Stairs_State_Definitions_Corner_Left")
        .expect("stair corner state");
    let (mut world, dir) = world("stair");

    world
        .set_block_full(6, 64, 6, corner, 0, 0)
        .expect("isolated corner stair");
    let changes = catalog.apply(&mut world, &assets, 6, 64, 6, 0, [0, 1, 0], true);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (6, 64, 6) && change.after == straight
        }),
        "the isolated stair should resolve back to Straight: {changes:?}"
    );
    assert_eq!(world.get_block(6, 64, 6), straight);

    // A perpendicular stair behind the origin turns it into a left corner.
    world
        .set_block_full(6, 64, 5, straight, 1, 0)
        .expect("north stair");
    let changes = catalog.apply(&mut world, &assets, 6, 64, 6, 0, [0, 1, 0], true);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (6, 64, 6) && change.after == corner
        }),
        "the perpendicular stair should resolve a left corner: {changes:?}"
    );

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_roof_resolves_its_hollow_variant_without_support() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    let roof = assets
        .block_types
        .id_of("Wood_Blackwood_Roof")
        .expect("roof base");
    let hollow = assets
        .block_types
        .id_of("Wood_Blackwood_Roof_Hollow")
        .expect("hollow roof");
    let (mut world, dir) = world("roof");

    world
        .set_block_full(8, 64, 8, roof, 0, 0)
        .expect("unsupported roof");
    let changes = catalog.apply(&mut world, &assets, 8, 64, 8, 0, [0, 1, 0], true);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (8, 64, 8) && change.after == hollow
        }),
        "the unsupported roof should resolve to its Hollow variant: {changes:?}"
    );
    assert_eq!(world.get_block(8, 64, 8), hollow);

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_roof_resolves_a_valley_corner() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    let roof = assets
        .block_types
        .id_of("Wood_Blackwood_Roof")
        .expect("roof base");
    let corner = assets
        .block_types
        .id_of("*Wood_Blackwood_Roof_Hollow_State_Definitions_Inverted_Corner_Left")
        .expect("hollow inverted corner roof");
    let (mut world, dir) = world("roof-valley");

    world
        .set_block_full(10, 64, 10, roof, 0, 0)
        .expect("valley roof");
    // The valley neighbours sit one block above: one behind (north) and one to
    // the left (west), with the right (east) side open.
    world
        .set_block_full(10, 65, 9, roof, 0, 0)
        .expect("valley back roof");
    world
        .set_block_full(9, 65, 10, roof, 1, 0)
        .expect("valley left roof");

    let changes = catalog.apply(&mut world, &assets, 10, 64, 10, 0, [0, 1, 0], true);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (10, 64, 10) && change.after == corner
        }),
        "the roof should resolve a valley corner: {changes:?}"
    );

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_roof_resolves_a_topper_when_four_connections_meet() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    let roof = assets
        .block_types
        .id_of("Wood_Blackwood_Roof")
        .expect("roof base");
    let topper = assets
        .block_types
        .id_of("*Wood_Blackwood_Roof_State_Definitions_Topper")
        .expect("roof topper");
    let (mut world, dir) = world("roof-topper");

    world
        .set_block_full(10, 64, 10, roof, 0, 0)
        .expect("topper roof");
    for (position, rotation) in [
        ([10, 63, 9], 2),
        ([10, 63, 11], 0),
        ([11, 63, 10], 1),
        ([9, 63, 10], 3),
    ] {
        world
            .set_block_full(position[0], position[1], position[2], roof, rotation, 0)
            .expect("topper neighbour");
    }

    let changes = catalog.apply(&mut world, &assets, 10, 64, 10, 0, [0, 1, 0], true);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (10, 64, 10) && change.after == topper
        }),
        "the roof should resolve the four-way topper: {changes:?}"
    );

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_wide_roof_splits_the_old_corner_arm() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open pack");
    let assets = AssetRegistry::load_phase2(&pack).expect("load phase-2 assets");
    let catalog = ConnectedBlockCatalog::from_assets(&assets);
    let straight = assets
        .block_types
        .id_of("Wood_Blackwood_Roof_Shallow")
        .expect("wide roof base");
    let corner = assets
        .block_types
        .id_of("*Wood_Blackwood_Roof_Shallow_State_Definitions_Corner_Left")
        .expect("wide roof corner");
    let (mut world, dir) = world("roof-wide");

    world
        .set_block_full(10, 64, 10, corner, 0, 0)
        .expect("wide roof corner");
    let changes = catalog.apply(&mut world, &assets, 10, 64, 10, 0, [0, 1, 0], true);
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (10, 64, 10) && change.after == straight
        }),
        "the wide roof should return to its straight shape: {changes:?}"
    );
    assert!(
        changes.iter().any(|change| {
            (change.x, change.y, change.z) == (9, 64, 10) && change.after == straight
        }),
        "the wide roof should split the old left arm into a second block: {changes:?}"
    );
    assert_eq!(world.get_block(9, 64, 10), straight);

    std::fs::remove_dir_all(dir).ok();
}
