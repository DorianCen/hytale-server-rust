//! `query_bench` — the T04 performance-baseline probe for the cheap column
//! queries, the per-seed feature lists and the chunk-generation path.
//!
//! Not part of `cargo test` and not a dependency of the seed explorer: this is
//! the manual harness behind `docs/benchmarks.md` in the explorer repository
//! (T04). Numbers are printed as `key value` lines so a table can be built
//! mechanically.
//!
//! Usage (from the workspace root):
//!
//! ```sh
//! cargo run --release -p worldgen-cli --bin query_bench -- \
//!   [world-dir] [seed] [runs] [chunk-workers]
//! ```
//!
//! Defaults: `testdata/Default`, seed 0, 5 runs, 1 chunk worker (parallel
//! scaling is measured by `worldgen-cli bench`; this probe keeps the chunk
//! path deterministic and serial).
//!
//! Measured, per run (median + p95 over `runs` runs):
//!
//! - 65,536 biome samples: a 256×256 grid at step 2 (a 512×512-block tile
//!   region at the origin);
//! - 65,536 height samples on the same region (core-data warm from the biome
//!   loop, so this isolates the height-generation cost);
//! - 65,536 combined biome+height samples at step 1 (a 256×256-block region);
//! - the chunk path for the same 256×256-block region: 64 chunks through the
//!   worker pool plus a compact surface extraction per chunk (top-block scan,
//!   top-fluid scan, tint lookup — the cost T09's summary extraction will
//!   pay), reported separately from generation;
//! - `get_unique_prefabs` and `get_spawn_points`.
//!
//! Warm-up rule (from the archived BENCHMARKS.md): column queries are warmed
//! on a 64×64 grid at step 8 around (4096, 4096) so the measured region
//! starts cold; the chunk path creates a fresh `WorldGenService` per run so
//! every run pays the same cold-cache cost (≥1 run apart, caches also expire
//! via the 20 s idle TTL).

use std::path::PathBuf;
use std::time::Instant;

use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen::{WorldGen, WorldGenOptions, WorldGenRequest, WorldGenService};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_hytale::world_gen_provider::{HytaleWorldGenProvider, Semver};
use worldgen_model::chunk::GeneratedChunk;

/// Median and p95 of a non-empty sample set.
fn median_p95(values: &[f64]) -> (f64, f64) {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("times are finite"));
    let count = sorted.len();
    let median = if count % 2 == 1 {
        sorted[count / 2]
    } else {
        (sorted[count / 2 - 1] + sorted[count / 2]) / 2.0
    };
    let p95_index = ((count as f64) * 0.95).ceil() as usize - 1;
    (median, sorted[p95_index.min(count - 1)])
}

fn print_stats(key: &str, values: &[f64], scale: f64) {
    let (median, p95) = median_p95(values);
    println!(
        "{key}_median {:.6} {key}_p95 {:.6}",
        median * scale,
        p95 * scale
    );
}

/// The compact surface extraction T09 will perform per chunk: top-block scan,
/// top-fluid scan and the per-column tint lookup. Returns the count of
/// non-empty columns so the loops cannot be optimized away.
fn extract_summary(chunk: &GeneratedChunk) -> u64 {
    let mut columns = 0u64;
    for z in 0..32i32 {
        for x in 0..32i32 {
            let mut top_block = false;
            for y in (0..320i32).rev() {
                if chunk.get_block(x, y, z) != 0 {
                    top_block = true;
                    break;
                }
            }
            let mut top_fluid = false;
            for y in (0..320i32).rev() {
                if chunk.get_fluid_id(x, y, z) != 0 {
                    top_fluid = true;
                    break;
                }
            }
            let _tint = chunk.get_tint(x, z);
            columns += u64::from(top_block) + u64::from(top_fluid);
        }
    }
    columns
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
    let runs: usize = args
        .get(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(5);
    let runs = runs.max(1);
    let chunk_workers: usize = args
        .get(4)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);

    println!("world {}", world.display());
    println!("seed {seed} runs {runs} chunk_workers {chunk_workers}");

    // --- column path: one generator, one load -----------------------------
    let loader = ChunkGeneratorJsonLoader::new(
        procedural::json::SeedString::new("ChunkGenerator"),
        make_config(&world),
    );
    let started = Instant::now();
    let generator = loader.load();
    println!(
        "generator_load_seconds {:.6}",
        started.elapsed().as_secs_f64()
    );

    // Warm-up on a disjoint 64×64 grid so the measured region starts cold.
    for z in 0..64i32 {
        for x in 0..64i32 {
            generator.get_zone_biome_result_at(seed, 4096 + x * 8, 4096 + z * 8);
            generator.get_height(seed, 4096 + x * 8, 4096 + z * 8);
        }
    }

    let mut checksum = 0i64;
    let mut biome_times = Vec::with_capacity(runs);
    let mut height_times = Vec::with_capacity(runs);
    let mut step1_times = Vec::with_capacity(runs);
    for _ in 0..runs {
        // 65,536 biome samples: 256×256 grid at step 2 over (0,0)..(512,512).
        let started = Instant::now();
        for z in 0..256i32 {
            for x in 0..256i32 {
                let result = generator.get_zone_biome_result_at(seed, x * 2, z * 2);
                if let Some(biome) = result.get_biome() {
                    checksum += i64::from(biome.get_id());
                }
            }
        }
        biome_times.push(started.elapsed().as_secs_f64());

        // 65,536 height samples on the same (now biome-warm) region.
        let started = Instant::now();
        for z in 0..256i32 {
            for x in 0..256i32 {
                checksum += i64::from(generator.get_height(seed, x * 2, z * 2));
            }
        }
        height_times.push(started.elapsed().as_secs_f64());

        // 65,536 combined samples at step 1 over (0,0)..(256,256).
        let started = Instant::now();
        for z in 0..256i32 {
            for x in 0..256i32 {
                generator.get_zone_biome_result_at(seed, x, z);
                checksum += i64::from(generator.get_height(seed, x, z));
            }
        }
        step1_times.push(started.elapsed().as_secs_f64());
    }
    print_stats("biome_65536_seconds", &biome_times, 1.0);
    print_stats("height_65536_seconds", &height_times, 1.0);
    print_stats("biome_height_step1_65536_seconds", &step1_times, 1.0);

    // --- per-seed lists -----------------------------------------------------
    let mut unique_times = Vec::with_capacity(runs);
    let mut spawn_times = Vec::with_capacity(runs);
    let mut unique_count = 0usize;
    let mut spawn_count = 0usize;
    for _ in 0..runs {
        let started = Instant::now();
        let entries = generator.get_unique_prefabs(seed);
        unique_times.push(started.elapsed().as_secs_f64());
        unique_count = entries.len();
        let started = Instant::now();
        let spawn = generator.get_spawn_points(seed);
        spawn_times.push(started.elapsed().as_secs_f64());
        spawn_count = spawn.len();
    }
    print_stats("unique_prefabs_seconds", &unique_times, 1.0);
    print_stats("spawn_points_seconds", &spawn_times, 1.0);
    println!("unique_prefabs_count {unique_count} spawn_points_count {spawn_count}");

    // --- chunk path: fresh service per run, 64 chunks + extraction ---------
    let mut chunk_gen_times = Vec::with_capacity(runs);
    let mut chunk_extract_times = Vec::with_capacity(runs);
    let mut chunk_pipeline_nanos = Vec::with_capacity(runs);
    for _ in 0..runs {
        let options = WorldGenOptions {
            workers: chunk_workers,
            seed_label: "ChunkGenerator".to_string(),
            assets_root: None,
            registries_path: None,
            registries: None,
        };
        let service = WorldGenService::new(make_config(&world), options)
            .map_err(|error| error.to_string())
            .expect("the worker pool starts");
        let mut generation_secs = 0.0f64;
        let mut extraction_secs = 0.0f64;
        let mut pipeline_nanos = 0u64;
        for chunk_z in 0..8i32 {
            for chunk_x in 0..8i32 {
                let started = Instant::now();
                let result = service
                    .generate(WorldGenRequest::new(seed, chunk_x, chunk_z))
                    .map_err(|error| error.to_string())
                    .expect("chunk generation succeeds");
                generation_secs += started.elapsed().as_secs_f64();
                pipeline_nanos += result.timings.total_nanos;
                let started = Instant::now();
                checksum += extract_summary(&result.chunk) as i64;
                extraction_secs += started.elapsed().as_secs_f64();
            }
        }
        service.shutdown();
        chunk_gen_times.push(generation_secs);
        chunk_extract_times.push(extraction_secs);
        chunk_pipeline_nanos.push(pipeline_nanos as f64 * 1.0e-9);
    }
    print_stats("chunk_region64_generation_seconds", &chunk_gen_times, 1.0);
    print_stats(
        "chunk_region64_extraction_seconds",
        &chunk_extract_times,
        1.0,
    );
    print_stats(
        "chunk_region64_pipeline_avg_per_chunk_seconds",
        &chunk_pipeline_nanos,
        1.0 / 64.0,
    );

    // Keep the sums alive so the loops are not optimized away.
    println!("probe_checksum {checksum}");
}
