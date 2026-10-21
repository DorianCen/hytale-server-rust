//! End-to-end test of the **unique prefabs** of the Default world.
//!
//! The Default world has two zones with unique prefabs: `Zone1_Spawn` (the
//! spawn room — the spawn point of the world) and `Zone1_Temple` (the portal
//! monument). Both zones are *unique zones* (they are painted into the mask
//! per seed), so their prefabs are placed at the center of the zone through
//! the inverse domain warp.
//!
//! The test loads the real Default world through the whole loader chain and
//! checks that
//!
//! * the unique prefab containers of the Spawn / Temple zones are loaded
//!   (the *generators* are there — the 5c seam is gone),
//! * the per-seed generation produces the entries (and is deterministic),
//! * the spawn points are computed from the entries,
//! * the unique prefab is really **pasted** into the chunk that contains it
//!   (the block ids of the spawn room are in the chunk with the `PREFAB`
//!   priority), and
//! * the world passes the [`worldgen_hytale::chunk::validation_util::ValidationUtil`].

use std::path::PathBuf;
use std::rc::Rc;

use procedural::json::SeedString;
use worldgen_hytale::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_hytale::chunk::validation_util::ValidationUtil;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::prefab::buffer::prefab_buffer_call::PrefabBufferCallState;
use worldgen_hytale::prefab::prefab_rotation::PrefabRotation;
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

/// The seeds the golden dumps use.
const SEEDS: [i32; 4] = [0, 1, 42, 1234567];

fn manifest_root() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
}

/// The loaded Default world.
fn default_world() -> Rc<worldgen_hytale::chunk::chunk_generator::ChunkGenerator> {
    let root = manifest_root();
    let config = WorldGenConfig::new(root, "Default".to_string(), "0.0.0".to_string());
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("0"), config);
    loader.load()
}

/// The zones of the Default world that carry unique prefabs.
fn zones_with_unique_prefabs(
    generator: &worldgen_hytale::chunk::chunk_generator::ChunkGenerator,
) -> Vec<(String, usize)> {
    generator
        .get_zone_pattern_provider()
        .get_zones()
        .iter()
        .filter(|zone| !zone.unique_prefab_container.get_generators().is_empty())
        .map(|zone| {
            (
                zone.name.clone(),
                zone.unique_prefab_container.get_generators().len(),
            )
        })
        .collect()
}

#[test]
fn the_default_world_loads_the_unique_prefab_containers() {
    let generator = default_world();
    let zones = zones_with_unique_prefabs(&generator);
    assert!(
        zones.len() >= 2,
        "the Spawn and the Temple zone carry unique prefabs, got {zones:?}"
    );
    let names: Vec<&str> = zones.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        names.contains(&"Zone1_Spawn"),
        "the spawn zone has unique prefabs, got {names:?}"
    );
    assert!(
        names.contains(&"Zone1_Temple"),
        "the temple zone has unique prefabs, got {names:?}"
    );
    for (_, count) in &zones {
        assert_eq!(*count, 1, "every unique zone has one generator");
    }
}

#[test]
fn the_unique_prefabs_are_generated_for_every_seed() {
    let generator = default_world();
    for seed in SEEDS {
        let entries = generator.generate_unique_prefabs(seed);
        assert_eq!(
            entries.len(),
            2,
            "the spawn and the temple prefab are placed (seed {seed})"
        );
        let names: Vec<&str> = entries.iter().map(|entry| entry.get_name()).collect();
        assert_eq!(names, vec!["Spawn", "Temple"], "the Java entry order");
        assert!(
            entries[0].is_spawn_location(),
            "the spawn prefab is the spawn location"
        );
        assert!(
            !entries[1].is_spawn_location(),
            "the temple prefab is not a spawn location"
        );
        // The rotation of the spawn is ROTATION_270, the temple ROTATION_90.
        assert_eq!(entries[0].get_rotation(), PrefabRotation::Rotation270);
        assert_eq!(entries[1].get_rotation(), PrefabRotation::Rotation90);
    }
}

#[test]
fn the_unique_prefab_generation_is_deterministic() {
    let generator = default_world();
    for seed in SEEDS {
        let first = generator.generate_unique_prefabs(seed);
        let second = generator.generate_unique_prefabs(seed);
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.get_position(), b.get_position(), "the same position");
            assert_eq!(a.get_rotation(), b.get_rotation(), "the same rotation");
        }
    }
}

#[test]
fn the_unique_prefabs_differ_between_the_seeds() {
    let generator = default_world();
    let first = generator.generate_unique_prefabs(0);
    let second = generator.generate_unique_prefabs(1);
    assert_ne!(
        first[0].get_position(),
        second[0].get_position(),
        "the spawn room moves with the seed"
    );
}

#[test]
fn the_spawn_points_follow_the_unique_prefabs() {
    let generator = default_world();
    for seed in SEEDS {
        let spawn_points = generator.get_spawn_points(seed);
        assert_eq!(
            spawn_points.len(),
            1,
            "only the spawn room is a spawn point"
        );

        let entry = &generator.generate_unique_prefabs(seed)[0];
        let position = entry.get_position();
        let mut offset = entry.get_spawn_offset();
        entry.get_rotation().rotate(&mut offset);

        let spawn = spawn_points[0].get_position();
        assert_eq!(spawn.x, offset.x + f64::from(position.0) + 0.5);
        assert_eq!(spawn.y, offset.y + f64::from(position.1));
        assert_eq!(spawn.z, offset.z + f64::from(position.2) + 0.5);
        // The yaw of the spawn is the negated yaw of the prefab rotation.
        assert_eq!(
            spawn_points[0].get_rotation().yaw(),
            -entry.get_rotation().get_yaw()
        );
    }
}

#[test]
fn the_spawn_points_are_deterministic() {
    let generator = default_world();
    let first = generator.get_spawn_points(0);
    let second = generator.get_spawn_points(0);
    assert_eq!(first.len(), second.len());
    assert_eq!(*first[0].get_position(), *second[0].get_position());
}

/// The blocks of the prefab of an entry (the rotated, world-relative
/// coordinates of the first `limit` blocks).
fn prefab_blocks(
    entry: &worldgen_hytale::container::unique_prefab_container::UniquePrefabEntry,
    limit: usize,
) -> Vec<(i32, i32, i32, i32)> {
    let buffer = entry.get_prefab_supplier().get();
    let access = buffer.new_access();
    let rotation = entry.get_rotation();
    let position = entry.get_position();

    let mut blocks = Vec::new();
    let call = PrefabBufferCallState::new(hytale_math::fast_random::FastRandom::new(0), rotation);
    // The traversal yields the *rotated* coordinates of the columns; the
    // world position is the entry position plus the rotated column.
    access.for_each(
        &mut |_cx: i32, _cz: i32, _count: usize, _t: &PrefabBufferCallState| true,
        &mut |visit, _t: &PrefabBufferCallState| {
            if blocks.len() < limit {
                blocks.push((
                    position.0 + visit.x,
                    position.1 + visit.y,
                    position.2 + visit.z,
                    visit.block_id,
                ));
            }
        },
        None,
        &call,
    );
    blocks
}

#[test]
fn the_unique_prefab_is_pasted_into_its_chunk() {
    let generator = default_world();
    let entry = generator.generate_unique_prefabs(0)[0].clone();
    let position = entry.get_position();

    // The chunk that contains the spawn room.
    let chunk_x = position.0.div_euclid(32);
    let chunk_z = position.2.div_euclid(32);

    let chunk = GeneratedChunk::with_coordinates(
        hytale_math::chunk_util::index_chunk(chunk_x, chunk_z),
        chunk_x,
        chunk_z,
    );
    let mut execution = ChunkGeneratorExecution::with_registry(
        0,
        Rc::clone(&generator),
        chunk,
        Rc::new(Registries::load()),
    );
    let mut resource = ChunkGeneratorResource::new();
    execution.execute(0, &mut resource);

    let blocks = prefab_blocks(&entry, 64);
    assert!(!blocks.is_empty(), "the spawn room has blocks");

    let mut checked = 0;
    for (world_x, world_y, world_z, block_id) in blocks {
        let local_x = world_x - chunk_x * 32;
        let local_z = world_z - chunk_z * 32;
        if !(0..32).contains(&local_x) || !(0..32).contains(&local_z) {
            continue;
        }
        if !(0..worldgen_model::block_priority::HEIGHT).contains(&world_y) {
            continue;
        }
        assert_eq!(
            execution.get_block(local_x, world_y, local_z),
            block_id,
            "the spawn room block at ({world_x}, {world_y}, {world_z})"
        );
        assert_eq!(
            execution
                .get_priority_chunk()
                .get(local_x, world_y, local_z),
            worldgen_model::block_priority::PREFAB,
            "the spawn room block at ({world_x}, {world_y}, {world_z}) has the \
             PREFAB priority"
        );
        checked += 1;
    }
    assert!(checked > 0, "the spawn room overlaps the chunk");
}

#[test]
fn the_default_world_passes_the_validation() {
    let generator = default_world();
    assert!(
        !ValidationUtil::is_invalid(generator.get_zone_pattern_provider()),
        "every prefab of the Default world loads"
    );
}
