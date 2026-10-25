//! Real-pack block collision shapes.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_world::collision::{Aabb, BlockCollisionTable};

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

fn table_and_registry() -> Option<(BlockCollisionTable, AssetRegistry)> {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    let registry = AssetRegistry::load_p0(&pack).expect("load P0");
    let table = BlockCollisionTable::from_assets(&registry);
    Some((table, registry))
}

#[test]
fn solid_blocks_get_their_hitboxes_and_air_does_not_collide() {
    let Some((table, registry)) = table_and_registry() else {
        return;
    };
    let rock = registry
        .block_types
        .id_of("Rock_Stone")
        .expect("Rock_Stone id");
    let shape = table.shape(rock).expect("Rock_Stone collides");
    assert_eq!(shape.boxes, vec![Aabb::unit()]);

    assert!(table.shape(0).is_none(), "Empty must not collide");
    let empty = registry.block_types.id_of("Empty").expect("Empty id");
    assert_eq!(empty, 0);
    assert!(table.shape(empty).is_none());

    let half = registry
        .block_types
        .id_of("Rock_Stone_Brick_Half")
        .expect("half-slab id");
    let half_shape = table.shape(half).expect("the half slab collides");
    assert!(
        half_shape
            .boxes
            .iter()
            .all(|bx| (bx.max.y - 0.5).abs() < 1e-6),
        "the half slab top must be 0.5: {:?}",
        half_shape.boxes
    );
}

#[test]
fn bench_hitboxes_keep_their_multi_box_shape_and_rotate() {
    let Some((table, registry)) = table_and_registry() else {
        return;
    };
    let bench = registry
        .block_types
        .id_of("Bench_Alchemy")
        .expect("Bench_Alchemy id");
    let shape = table.shape(bench).expect("bench collides");
    assert!(shape.boxes.len() > 1, "the bench uses several boxes");

    let mut unrotated = Vec::new();
    shape.world_boxes(0, (10, 20, 30), &mut unrotated);
    let mut rotated = Vec::new();
    shape.world_boxes(1, (10, 20, 30), &mut rotated);
    assert_eq!(unrotated.len(), rotated.len());
    assert_ne!(unrotated, rotated, "yaw 90 must rotate the boxes");
    assert!(
        unrotated
            .iter()
            .all(|bx| bx.min.x >= 9.0 && bx.max.x <= 11.0),
        "boxes must stay anchored around the block origin: {unrotated:?}"
    );
}
