//! Placement fidelity at the world-storage layer: rotated blocks and
//! neighbour-driven falling blocks.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_world::chunk::column::ChunkColumn;
use hytale_world::chunk::palette::NameLookup;
use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::simulation::{BlockSimulation, SimulationTables};
use hytale_world::world::{World, WorldSave, WorldStorage};
use hytale_world::world_config::WorldUuid;

struct TestNames;

impl NameLookup for TestNames {
    fn name_of(&self, id: i32) -> Option<&str> {
        Some(Box::leak(format!("id_{id}").into_boxed_str()))
    }

    fn id_of(&self, name: &str) -> Option<i32> {
        name.strip_prefix("id_")?.parse().ok()
    }
}

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

fn registry() -> Option<AssetRegistry> {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let pack = AssetPack::open(repo_root().join("Assets/Assets.zip")).expect("open the pack");
    Some(AssetRegistry::load_p0(&pack).expect("load P0"))
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-placement-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn uuid() -> WorldUuid {
    "12345678-1234-5678-9abc-def012345678".parse().unwrap()
}

#[test]
fn block_rotation_survives_save_and_reopen() {
    let dir = temp_dir("rotation");
    let names = TestNames;
    let codec = ChunkCodec::from_lookups(&names, &names, &names);
    let mut column = ChunkColumn::new(4, -5);
    column
        .set_block(1, 64, 1, 5, 41, 0)
        .expect("set a rotated block");
    assert_eq!(column.get_rotation(1, 64, 1), 41);

    {
        let (mut save, _) = WorldSave::open(&dir, 7, None, uuid()).expect("open");
        save.world_mut().insert_chunk(column);
        assert_eq!(save.save_all(&codec).expect("save"), 1);
    }

    {
        let (mut save, _) = WorldSave::open(&dir, 0, None, uuid()).expect("reopen");
        assert!(save.world_mut().load_chunk(&codec, 4, -5).expect("load"));
        assert_eq!(save.world().get_block(4 * 32 + 1, 64, -5 * 32 + 1), 5);
        assert_eq!(
            save.world().get_rotation(4 * 32 + 1, 64, -5 * 32 + 1),
            41,
            "the Java rotation tuple index must survive the chunk codec"
        );
    }

    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn removing_a_supporting_neighbour_drops_a_falling_block() {
    let Some(registry) = registry() else {
        return;
    };
    let tables = SimulationTables::from_assets(&registry);
    let stone = registry.block_types.id_of("Rock_Stone").expect("stone");
    let barrel = registry
        .block_types
        .id_of("Debug_Falling_Barrel")
        .expect("Debug_Falling_Barrel");

    let mut world = World::new(WorldStorage::new("/tmp/hytale-placement-falling"));
    world.insert_chunk(ChunkColumn::new(0, 0));
    world.set_block(16, 199, 16, stone).expect("floor");
    world.set_block(16, 200, 16, barrel).expect("barrel");
    let mut simulation = BlockSimulation::new(tables, 7);

    // The supported barrel stays put.
    for _ in 0..4 {
        simulation.tick(&mut world, 0.05);
    }
    assert_eq!(world.get_block(16, 200, 16), barrel);

    // Removing the floor marks the cell changed; the next tick schedules the
    // 3x3x3 neighbourhood and the barrel loses support.
    world.set_block(16, 199, 16, 0).expect("remove floor");
    let mut moved = false;
    for _ in 0..20 {
        let report = simulation.tick(&mut world, 0.05);
        if report.falling_moves > 0 {
            moved = true;
            break;
        }
    }
    assert!(moved, "the unsupported barrel must enter the falling queue");
    assert_ne!(
        world.get_block(16, 200, 16),
        barrel,
        "the barrel must leave its unsupported cell"
    );
}
