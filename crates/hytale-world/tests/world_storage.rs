//! Runtime world storage: region load/save, dirty tracking and mutation.

use std::path::PathBuf;

use hytale_world::chunk::column::{ChunkColumn, Heightmap};
use hytale_world::chunk::palette::NameLookup;
use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::chunk::NO_HEIGHT;
use hytale_world::world::{World, WorldStorage};

struct TestNames;

impl NameLookup for TestNames {
    fn name_of(&self, id: i32) -> Option<&str> {
        Some(Box::leak(format!("id_{id}").into_boxed_str()))
    }

    fn id_of(&self, name: &str) -> Option<i32> {
        name.strip_prefix("id_")?.parse().ok()
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-world-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn column(x: i32, z: i32) -> ChunkColumn {
    let mut column = ChunkColumn::new(x, z);
    column
        .set_block(0, 32, 0, 7, 3, 5)
        .expect("set the first block");
    column
        .set_block(31, 33, 31, 9, 1, 2)
        .expect("set the second block");
    column.set_tint(4, 5, 0x1234_5678);
    let mut heights = vec![NO_HEIGHT; 32 * 32];
    heights[0] = 33;
    heights[1023] = 12;
    column.set_heightmap(Heightmap::Legacy(heights));
    column.set_needs_physics(false);
    column
}

#[test]
fn storage_round_trips_and_lists_chunks() {
    let dir = temp_dir("storage");
    let names = TestNames;
    let codec = ChunkCodec::from_lookups(&names, &names, &names);
    let source = column(4, -5);

    {
        let mut storage = WorldStorage::new(&dir);
        storage.save_chunk(&codec, &source).expect("save");
        storage.flush().expect("flush");
        assert_eq!(storage.stored_chunk_coordinates().unwrap(), vec![(4, -5)]);
    }
    {
        let mut storage = WorldStorage::new(&dir);
        let loaded = storage
            .load_chunk(&codec, 4, -5)
            .expect("load")
            .expect("present");
        assert_eq!(loaded.x(), 4);
        assert_eq!(loaded.z(), -5);
        assert_eq!(loaded.get_block(0, 32, 0), 7);
        assert_eq!(loaded.get_block(31, 33, 31), 9);
        assert_eq!(loaded.tint(4, 5), 0x1234_5678);
        assert_eq!(loaded.heightmap(), source.heightmap());
        assert!(!loaded.needs_physics());
        storage.remove_chunk(4, -5).expect("remove");
        assert!(storage.stored_chunk_coordinates().unwrap().is_empty());
    }
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn world_loads_mutates_and_saves_back() {
    let dir = temp_dir("world");
    let names = TestNames;
    let codec = ChunkCodec::from_lookups(&names, &names, &names);

    {
        let mut world = World::new(WorldStorage::new(&dir));
        world.insert_chunk(column(4, -5));
        assert_eq!(world.dirty_chunks().len(), 1);
        assert_eq!(world.save_all(&codec).expect("save all"), 1);
        assert!(world.dirty_chunks().is_empty());
    }
    {
        let mut world = World::new(WorldStorage::new(&dir));
        assert!(world.load_chunk(&codec, 4, -5).expect("load"));
        assert_eq!(world.loaded_chunk_count(), 1);
        assert_eq!(world.get_block(4 * 32, 32, -5 * 32), 7);

        assert!(world.set_block(4 * 32, 32, -5 * 32, 12).expect("mutate"));
        assert_eq!(world.get_block(4 * 32, 32, -5 * 32), 12);
        assert_eq!(world.dirty_chunks().len(), 1);
        assert!(world.save_chunk(&codec, 4, -5).expect("save"));
        assert!(world.dirty_chunks().is_empty());
    }
    {
        let mut world = World::new(WorldStorage::new(&dir));
        assert!(world.load_chunk(&codec, 4, -5).expect("reload"));
        assert_eq!(world.get_block(4 * 32, 32, -5 * 32), 12);
        assert_eq!(world.get_block(4 * 32 + 31, 33, -5 * 32 + 31), 9);
    }
    std::fs::remove_dir_all(dir).ok();
}
