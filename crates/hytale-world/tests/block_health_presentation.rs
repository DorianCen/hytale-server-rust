//! T57 block-health presentation seam: absolute entries and replace cleanup.

use std::path::PathBuf;

use hytale_world::chunk::column::ChunkColumn;
use hytale_world::world::{World, WorldStorage};

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-block-health-presentation-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn damaged_blocks_are_listed_and_cleared_on_replacement() {
    let dir = temp_dir("entries");
    let mut world = World::new(WorldStorage::new(dir.clone()));
    let mut column = ChunkColumn::new(3, 4);
    column.set_block(1, 65, 2, 7, 0, 0).expect("set block");
    world.insert_chunk(column);

    assert!(world.block_health_entries().is_empty());
    let remaining = world.damage_block(3 * 32 + 1, 65, 4 * 32 + 2, 0.25);
    assert!((remaining - 0.75).abs() < f32::EPSILON);

    let entries = world.block_health_entries_in_chunk(3, 4);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, 3 * 32 + 1);
    assert_eq!(entries[0].1, 65);
    assert_eq!(entries[0].2, 4 * 32 + 2);
    assert!((entries[0].3 - 0.75).abs() < f32::EPSILON);
    assert_eq!(world.block_health_entries(), entries);

    // Replacing the block must not leave the old block's damage behind.
    assert!(world
        .set_block(3 * 32 + 1, 65, 4 * 32 + 2, 8)
        .expect("replace block"));
    assert!(world.block_health_entries_in_chunk(3, 4).is_empty());
    assert!((world.block_health(3 * 32 + 1, 65, 4 * 32 + 2) - 1.0).abs() < f32::EPSILON);

    std::fs::remove_dir_all(dir).ok();
}
