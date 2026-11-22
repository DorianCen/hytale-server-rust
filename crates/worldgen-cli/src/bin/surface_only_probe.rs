//! `surface_only_probe` — measures the surface-only query mode against the
//! full chunk pipeline over the same 8×8 chunk region at the origin.
//!
//! Not part of `cargo test`; it backs the explorer's coarse-tile decision. The
//! Default world is loaded **once** and the same generator (and registry
//! snapshot) generates the same 64 chunks twice — full pipeline first, then
//! surface-only — so the second pass is the one with the warmer caches. That
//! order is conservative: it does not inflate the surface-only win.
//!
//! Numbers are printed as `key value` lines, matching `query_bench`:
//!
//! ```sh
//! cargo run --release -p worldgen-cli --bin surface_only_probe -- \
//!   [world-dir] [seed]
//! ```
//!
//! Defaults: `testdata/Default`, seed 0.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::chunk::chunk_generator_resource::ChunkGeneratorResource;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen::{generate_chunk, WorldGenRequest};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_model::chunk::GeneratedChunk;
use worldgen_model::registry::Registries;

use worldgen_hytale::world_gen_provider::{HytaleWorldGenProvider, Semver};

/// The compact surface summary of a chunk: top block, top fluid and tint per
/// column. Returns a count so the loops cannot be optimized away.
fn summary_checksum(chunk: &GeneratedChunk) -> i64 {
    let mut checksum = 0i64;
    for z in 0..32i32 {
        for x in 0..32i32 {
            let mut top_block = 0i32;
            for y in (0..worldgen_model::block_priority::HEIGHT).rev() {
                let block = chunk.get_block(x, y, z);
                if block != 0 {
                    top_block = block;
                    break;
                }
            }
            let mut top_fluid = 0i32;
            for y in (0..worldgen_model::block_priority::HEIGHT).rev() {
                let fluid = chunk.get_fluid_id(x, y, z);
                if fluid != 0 {
                    top_fluid = fluid;
                    break;
                }
            }
            checksum += i64::from(top_block) * 3
                + i64::from(top_fluid) * 5
                + i64::from(chunk.get_tint(x, z) & 0x00FF_FFFF);
        }
    }
    checksum
}

fn make_config(world: &std::path::Path) -> WorldGenConfig {
    let provider = HytaleWorldGenProvider {
        name: "Default".to_string(),
        version: Semver::parse("0.0.0").expect("0.0.0 is valid semver"),
        path: None,
    };
    let path = provider
        .resolve_path(world)
        .expect("the world path resolves like the CLI default");
    WorldGenConfig::new(path, "Default".to_string(), "0.0.0".to_string())
}

/// Generates the 8×8 chunk region at the origin and returns
/// `(seconds, pipeline nanos, summary checksum)`.
fn generate_region(
    generator: &Rc<ChunkGenerator>,
    registry: &Rc<Registries>,
    seed: i32,
    surface_only: bool,
) -> (f64, u64, i64) {
    let mut resource = ChunkGeneratorResource::new();
    let started = Instant::now();
    let mut pipeline_nanos = 0u64;
    let mut checksum = 0i64;
    for chunk_z in 0..8i32 {
        for chunk_x in 0..8i32 {
            let request =
                WorldGenRequest::new(seed, chunk_x, chunk_z).with_surface_only(surface_only);
            let result = generate_chunk(generator, Rc::clone(registry), &mut resource, &request)
                .map_err(|error| error.to_string())
                .expect("chunk generation succeeds");
            pipeline_nanos += result.timings.total_nanos;
            checksum += summary_checksum(&result.chunk);
        }
    }
    (started.elapsed().as_secs_f64(), pipeline_nanos, checksum)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let world = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/Default"));
    let seed: i32 = args
        .get(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    println!("world {}", world.display());
    println!("seed {seed} chunks 64 order full_then_surface_only");

    // The fixture is loaded once; both modes share the generator and the
    // registry snapshot.
    let loader = ChunkGeneratorJsonLoader::new(
        procedural::json::SeedString::new("ChunkGenerator"),
        make_config(&world),
    );
    let started = Instant::now();
    let generator = loader.load();
    let registry = generator
        .get_registries()
        .map(Rc::clone)
        .unwrap_or_else(|| Rc::new(Registries::load()));
    println!(
        "generator_load_seconds {:.6}",
        started.elapsed().as_secs_f64()
    );

    let (full_seconds, full_pipeline_nanos, full_checksum) =
        generate_region(&generator, &registry, seed, false);
    let (surface_only_seconds, surface_only_pipeline_nanos, surface_only_checksum) =
        generate_region(&generator, &registry, seed, true);

    println!("full_64_seconds {full_seconds:.6}");
    println!("surface_only_64_seconds {surface_only_seconds:.6}");
    println!("full_avg_per_chunk_seconds {:.6}", full_seconds / 64.0);
    println!(
        "surface_only_avg_per_chunk_seconds {:.6}",
        surface_only_seconds / 64.0
    );
    println!(
        "full_pipeline_avg_per_chunk_seconds {:.6}",
        full_pipeline_nanos as f64 * 1.0e-9 / 64.0
    );
    println!(
        "surface_only_pipeline_avg_per_chunk_seconds {:.6}",
        surface_only_pipeline_nanos as f64 * 1.0e-9 / 64.0
    );
    println!(
        "speedup {:.3}",
        full_seconds / surface_only_seconds.max(f64::MIN_POSITIVE)
    );
    println!("full_checksum {full_checksum}");
    println!("surface_only_checksum {surface_only_checksum}");
}
