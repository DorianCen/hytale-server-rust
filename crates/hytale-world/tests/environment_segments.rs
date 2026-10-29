//! Runtime environment segment counting used by NPC spawning.

use hytale_world::chunk::column::ChunkColumn;
use hytale_world::world::{World, WorldStorage};

#[test]
fn environment_segment_count_follows_allocated_sections() {
    let dir = std::env::temp_dir().join(format!(
        "hytale-environment-segments-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut world = World::new(WorldStorage::new(dir.clone()));
    let mut column = ChunkColumn::new(0, 0);
    assert!(column.set_environment(1, 1, 1, 7));
    assert!(column.set_environment(2, 40, 2, 7));
    assert!(column.set_environment(3, 1, 3, 8));
    world.insert_chunk(column);

    assert_eq!(world.environment_segment_count(7), 2);
    assert_eq!(world.environment_segment_count(8), 1);
    assert_eq!(world.environment_segment_count(9), 0);
    let centers = world.environment_section_centers(7);
    assert_eq!(centers.len(), 2);
    assert_eq!(
        centers[0],
        hytale_world::collision::Vec3::new(16.0, 16.0, 16.0)
    );
    assert_eq!(
        centers[1],
        hytale_world::collision::Vec3::new(16.0, 48.0, 16.0)
    );
    assert_eq!(world.get_environment(1, 1, 1), 7);
    std::fs::remove_dir_all(dir).ok();
}
