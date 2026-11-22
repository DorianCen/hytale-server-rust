//! The surface-only query mode: the default stays off, a real fixture chunk
//! generates in both modes, and the untouched surface columns stay identical.
//!
//! Surface-only skips the cave and prefab stages (see
//! [`WorldGenRequest::surface_only`]); the remaining stages own their random
//! streams, so every column that no cave breach / prefab touches must keep the
//! same top block and top fluid as the full pipeline. The fidelity test counts
//! all 1024 columns instead of hand-picking a handful, so the number in its
//! assertion is the actual fixture measure.

use std::path::PathBuf;
use std::rc::Rc;

use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen::WorldGenRequest;
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::block_priority::HEIGHT;
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

/// The Base-game prefabs are required by the full prefab stage and are not
/// part of the source tree (see `.gitignore`); follow the other fixture tests'
/// skip pattern when they are absent.
fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    PathBuf::from(format!(
        "{}/../../Assets/extracted/Server/Prefabs",
        std::env::var("CARGO_MANIFEST_DIR").expect("the manifest dir is set")
    ))
    .is_dir()
}

fn world_root() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../testdata/Default",
        std::env::var("CARGO_MANIFEST_DIR").expect("the manifest dir is set")
    ))
}

fn default_world() -> Rc<ChunkGenerator> {
    let config = WorldGenConfig::new(world_root(), "Default".to_string(), "0.0.0".to_string());
    let loader =
        ChunkGeneratorJsonLoader::new(procedural::json::SeedString::new("ChunkGenerator"), config);
    loader.load()
}

/// Generates one chunk in the requested mode.
fn generate(
    generator: &Rc<ChunkGenerator>,
    seed: i32,
    chunk_x: i32,
    chunk_z: i32,
    surface_only: bool,
) -> GeneratedChunk {
    let chunk = GeneratedChunk::with_coordinates(
        hytale_math::chunk_util::index_chunk(chunk_x, chunk_z),
        chunk_x,
        chunk_z,
    );
    let mut execution = ChunkGeneratorExecution::with_registry(
        seed,
        Rc::clone(generator),
        chunk,
        Rc::new(Registries::load()),
    );
    let mut resource = ChunkGeneratorResource::new();
    let completed = execution.execute_with_mode(seed, &mut resource, None, surface_only);
    assert!(completed, "the chunk generation must not be cancelled");
    execution.into_chunk()
}

/// The top non-air block id of a column (`0` when the column is all air).
fn top_block(chunk: &GeneratedChunk, x: i32, z: i32) -> i32 {
    for y in (0..HEIGHT).rev() {
        let block = chunk.get_block(x, y, z);
        if block != 0 {
            return block;
        }
    }
    0
}

/// The top fluid id of a column (`0` when the column has no fluid).
fn top_fluid(chunk: &GeneratedChunk, x: i32, z: i32) -> i32 {
    for y in (0..HEIGHT).rev() {
        let fluid = chunk.get_fluid_id(x, y, z);
        if fluid != 0 {
            return fluid;
        }
    }
    0
}

#[test]
fn surface_only_defaults_off() {
    let request = WorldGenRequest::new(0, 0, 0);
    assert!(
        !request.surface_only,
        "a plain request must run the full pipeline"
    );
    assert!(
        !request.clone().with_surface_only(false).surface_only,
        "the builder must carry the explicit false through"
    );
    assert!(
        request.with_surface_only(true).surface_only,
        "the builder must turn the query mode on"
    );
}

#[test]
fn surface_only_generates_a_chunk_on_the_fixture() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();

    let full = generate(&generator, 0, 0, 0, false);
    let surface_only = generate(&generator, 0, 0, 0, true);

    // Both modes produced terrain: the fixture chunk is not an all-air chunk.
    let mut full_solid = 0usize;
    let mut surface_only_solid = 0usize;
    for y in 0..HEIGHT {
        if full.get_block(4, y, 4) != 0 {
            full_solid += 1;
        }
        if surface_only.get_block(4, y, 4) != 0 {
            surface_only_solid += 1;
        }
    }
    assert!(
        full_solid > 0 && surface_only_solid > 0,
        "both modes must fill the terrain column: full={full_solid}, \
         surface_only={surface_only_solid}"
    );
    // Both modes filled the tint and the environment map.
    assert_eq!(
        full.get_tint(0, 0),
        surface_only.get_tint(0, 0),
        "the tint map is not a cave/prefab artifact"
    );
    assert_eq!(
        full.get_environment(0, 64, 0),
        surface_only.get_environment(0, 64, 0),
        "the column environment is not a cave/prefab artifact"
    );
}

#[test]
fn surface_only_keeps_the_untouched_surface_columns() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let generator = default_world();

    // The same (seed, chunk) both ways. The chunk (0, 0) of the seed 0 is the
    // fixture chunk the smoke/golden tests use: it carries cave carves and
    // prefabs (see `tests/generation_smoke.rs`), so the comparison is not
    // trivially all-equal.
    let full = generate(&generator, 0, 0, 0, false);
    let surface_only = generate(&generator, 0, 0, 0, true);

    let mut block_matches = 0usize;
    let mut fluid_matches = 0usize;
    let mut both_matches = 0usize;
    for z in 0..32 {
        for x in 0..32 {
            let block_same = top_block(&full, x, z) == top_block(&surface_only, x, z);
            let fluid_same = top_fluid(&full, x, z) == top_fluid(&surface_only, x, z);
            block_matches += usize::from(block_same);
            fluid_matches += usize::from(fluid_same);
            both_matches += usize::from(block_same && fluid_same);
        }
    }

    // Measured on `testdata/Default`, seed 0, chunk (0, 0):
    //   top block: 847/1024 identical
    //   top fluid: 1013/1024 identical
    //   both:      847/1024 identical
    //
    // The 177 differing columns are all covered by the biome prefab this chunk
    // carries: 160 columns show the prefab's top layer one block above the
    // terrain (block 1584 vs the surface-only 2353) and 17 replace the surface
    // block at the same height. No column is lower in the full pipeline, i.e.
    // no cave breached this chunk's surface. The floors leave room for small
    // prefab-pack drift while still failing hard if the skipped stages start
    // perturbing the surface (a coupling bug would break most columns).
    const BLOCK_MATCH_FLOOR: usize = 800;
    const FLUID_MATCH_FLOOR: usize = 1000;
    const BOTH_MATCH_FLOOR: usize = 800;
    eprintln!(
        "surface_only fidelity: block {block_matches}/1024, fluid \
         {fluid_matches}/1024, both {both_matches}/1024"
    );
    assert!(
        block_matches >= BLOCK_MATCH_FLOOR,
        "only prefab/cave columns may change the top block: \
         {block_matches}/1024 (floor {BLOCK_MATCH_FLOOR})"
    );
    assert!(
        fluid_matches >= FLUID_MATCH_FLOOR,
        "only prefab/cave columns may change the top fluid: \
         {fluid_matches}/1024 (floor {FLUID_MATCH_FLOOR})"
    );
    assert!(
        both_matches >= BOTH_MATCH_FLOOR,
        "only prefab/cave columns may change in both: \
         {both_matches}/1024 (floor {BOTH_MATCH_FLOOR})"
    );
}
