//! End-to-end **generation** smoke test of the Default world.
//!
//! The test loads the real Default world through the whole loader chain
//! (world JSON → mask → zones → caves → biome prefab containers → **unique
//! prefab containers**) and then generates **one chunk** through the real
//! [`ChunkGeneratorExecution::execute`] pipeline (tint → environment →
//! terrain → covers → **caves** → **prefabs** → water).
//!
//! The caves are the stage that exercises the Phase-4 and Phase-5a chain:
//! the cave graph grows, the prefab cave nodes resolve their prefab through
//! the prefab store and paste it into the chunk. The **prefab** stage
//! exercises Phase 5b: the biome prefab containers place their prefabs with
//! the `PREFAB` priority. The **unique prefabs** (Phase 5c) are placed in
//! the chunk that contains them — the second test generates *that* chunk and
//! checks that the spawn room is really in it.
//!
//! The test fails if any `SEAM(phase-…)` marker is hit.

use std::path::PathBuf;
use std::rc::Rc;

use procedural::json::SeedString;
use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_hytale::container::unique_prefab_container::UniquePrefabEntry;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::prefab::buffer::prefab_buffer_call::PrefabBufferCallState;
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

/// Whether the base-game asset pack is available (it is not part of the
/// source tree — see `.gitignore`). The cave prefabs of the Default world
/// live in it.
fn assets_available() -> bool {
    PathBuf::from(format!(
        "{}/../../Assets/extracted/Server/Prefabs",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
    .is_dir()
}

fn manifest_root() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").unwrap()
    ))
}

/// The loaded Default world.
fn default_world() -> Rc<ChunkGenerator> {
    let root = manifest_root();
    let config = WorldGenConfig::new(root, "Default".to_string(), "0.0.0".to_string());
    let loader = ChunkGeneratorJsonLoader::new(SeedString::new("0"), config);
    loader.load()
}

/// The priority histogram of the chunk.
fn priority_histogram(execution: &ChunkGeneratorExecution) -> Vec<(i8, usize)> {
    let mut counts = vec![0usize; 32];
    for y in 0..worldgen_model::block_priority::HEIGHT {
        for x in 0..32 {
            for z in 0..32 {
                let priority = execution.get_priority_chunk().get(x, y, z);
                let index = (priority.max(0) as usize).min(31);
                counts[index] += 1;
            }
        }
    }
    counts
        .into_iter()
        .enumerate()
        .filter(|(_, count)| *count > 0)
        .map(|(priority, count)| (priority as i8, count))
        .collect()
}

/// The block grid of the chunk (the parity fingerprint).
fn block_grid(execution: &ChunkGeneratorExecution) -> Vec<i32> {
    let mut blocks = Vec::with_capacity(32 * 32 * worldgen_model::block_priority::HEIGHT as usize);
    for y in 0..worldgen_model::block_priority::HEIGHT {
        for x in 0..32 {
            for z in 0..32 {
                blocks.push(execution.get_block(x, y, z));
            }
        }
    }
    blocks
}

/// Generates the chunk `(chunk_x, chunk_z)` with the seed 0.
fn generate_chunk(
    generator: &Rc<ChunkGenerator>,
    chunk_x: i32,
    chunk_z: i32,
) -> ChunkGeneratorExecution {
    let chunk = GeneratedChunk::with_coordinates(
        hytale_math::chunk_util::index_chunk(chunk_x, chunk_z),
        chunk_x,
        chunk_z,
    );
    let mut execution = ChunkGeneratorExecution::with_registry(
        0,
        Rc::clone(generator),
        chunk,
        Rc::new(Registries::load()),
    );
    let mut resource = ChunkGeneratorResource::new();
    execution.execute(0, &mut resource);
    execution
}

#[test]
fn the_default_world_generates_a_chunk_with_the_cave_and_prefab_stages() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    // The whole chain runs: the world JSON, the climate mask, the 28 zones,
    // the cave configuration of every zone, the biome prefab containers and
    // the unique prefab containers (the prefab names resolve through the
    // prefab store).
    let generator = default_world();

    // Generate the chunk (0, 0) with the seed 0.
    let execution = generate_chunk(&generator, 0, 0);

    // The terrain was written (the filling block of the biome).
    let mut solid = 0;
    for y in 0..worldgen_model::block_priority::HEIGHT {
        if execution.get_block(4, y, 4) != 0 {
            solid += 1;
        }
    }
    assert!(
        solid > 0,
        "the generated chunk must contain the terrain blocks"
    );

    // The caves reached the chunk: the cave nodes carved into it and the
    // prefabs of the caves were pasted (the `CAVE_PREFAB` priority) — the
    // proof that the Phase-4 (caves) and the Phase-5a (prefab store, buffer
    // and paste) chain is real.
    let mut cave = 0;
    let mut cave_prefab = 0;
    let mut prefab_cave = 0;
    for y in 0..worldgen_model::block_priority::HEIGHT {
        for x in 0..32 {
            for z in 0..32 {
                match execution.get_priority_chunk().get(x, y, z) {
                    p if p == worldgen_model::block_priority::CAVE => cave += 1,
                    p if p == worldgen_model::block_priority::CAVE_PREFAB => cave_prefab += 1,
                    p if p == worldgen_model::block_priority::PREFAB_CAVE => prefab_cave += 1,
                    _ => {}
                }
            }
        }
    }
    assert!(cave > 0, "the cave stage must carve the chunk");
    assert!(
        cave_prefab > 0,
        "the cave prefabs must be pasted into the chunk"
    );
    assert!(
        prefab_cave > 0,
        "the prefab cave nodes must be pasted into the chunk"
    );

    // The **prefab stage** (Phase 5b) placed the biome prefabs: the
    // `PREFAB` priority (the regular prefab categories) must be present in
    // the chunk.
    let prefab = priority_histogram(&execution)
        .into_iter()
        .find(|(priority, _)| *priority == worldgen_model::block_priority::PREFAB)
        .map(|(_, count)| count)
        .unwrap_or(0);
    assert!(
        prefab > 0,
        "the prefab stage must place the biome prefabs (PREFAB priority); \
         the priority histogram is {:?}",
        priority_histogram(&execution)
    );
}

#[test]
fn the_default_world_chunk_priority_histogram_is_pinned() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();
    let execution = generate_chunk(&generator, 0, 0);

    // The priority histogram of the chunk (0, 0) of the seed 0 — the
    // fingerprint of the Phase-3/4/5 chain. The *unique* prefabs (Phase 5c)
    // do not change it: the spawn room and the temple portal land in other
    // chunks (the spawn zone is painted far away from the origin).
    //
    // These are the exact Java `chunks.json` counts for this chunk (the
    // chunk-level parity test `tests/chunks_golden.rs` compares every voxel).
    let histogram = priority_histogram(&execution);
    assert_eq!(
        histogram,
        vec![
            (0, 204871), // NONE
            (1, 2493),   // FILLING
            (2, 93930),  // LAYER
            (3, 511),    // COVER
            (5, 187),    // CAVE_COVER
            (6, 15892),  // CAVE
            (7, 1312),   // CAVE_PREFAB
            (8, 8462),   // PREFAB_CAVE
            (9, 22),     // PREFAB
        ],
        "the priority histogram of the Default world chunk (0, 0) changed"
    );
}

#[test]
fn the_default_world_chunk_generation_is_deterministic() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();

    let generate = || {
        let execution = generate_chunk(&generator, 0, 0);
        (block_grid(&execution), priority_histogram(&execution))
    };

    let (first_blocks, first_priorities) = generate();
    let (second_blocks, second_priorities) = generate();

    assert_eq!(
        first_blocks, second_blocks,
        "the generated chunk must be deterministic"
    );
    assert_eq!(
        first_priorities, second_priorities,
        "the priority histogram must be deterministic"
    );
}

/// The blocks of the prefab of a unique prefab entry (the world coordinates
/// of the first `limit` blocks of the prefab).
fn prefab_blocks(entry: &UniquePrefabEntry, limit: usize) -> Vec<(i32, i32, i32, i32)> {
    let buffer = entry.get_prefab_supplier().get();
    let access = buffer.new_access();
    let rotation = entry.get_rotation();
    let position = entry.get_position();

    let mut blocks = Vec::new();
    let call = PrefabBufferCallState::new(hytale_math::fast_random::FastRandom::new(0), rotation);
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
fn the_unique_prefab_is_part_of_the_chunk_output() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();

    // The unique prefabs of the seed 0 (the spawn room and the temple
    // portal) — the Phase-5c chain.
    let entries = generator.generate_unique_prefabs(0);
    assert_eq!(entries.len(), 2, "the spawn and the temple prefab");

    // The spawn room lands in a chunk far away from the origin — the chunk
    // (0, 0) of the smoke test is *not* the chunk of the unique prefab.
    let spawn = &entries[0];
    let position = spawn.get_position();
    let chunk_x = position.0.div_euclid(32);
    let chunk_z = position.2.div_euclid(32);
    assert_ne!(
        (chunk_x, chunk_z),
        (0, 0),
        "the unique prefab of the seed 0 does not land in the chunk (0, 0)"
    );

    // The chunk that contains the spawn room carries the room's blocks with
    // the `PREFAB` priority.
    let execution = generate_chunk(&generator, chunk_x, chunk_z);
    let mut checked = 0;
    for (world_x, world_y, world_z, block_id) in prefab_blocks(spawn, 128) {
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
    assert!(checked > 0, "the spawn room overlaps its chunk");
}
