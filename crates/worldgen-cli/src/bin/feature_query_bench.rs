//! `feature_query_bench` — the performance probe for the headless feature
//! placement query (`ChunkGenerator::features_in`).
//!
//! Not part of `cargo test` and not a dependency of the seed explorer: this is
//! the manual harness behind the query's cost table in [`QUERY.md`]. Numbers are
//! printed as `key value` lines so a table can be built mechanically.
//!
//! Usage (from the workspace root, release only — debug is roughly an order of
//! magnitude slower):
//!
//! ```sh
//! cargo run --release -p worldgen-cli --bin feature_query_bench -- \
//!   [world-dir] [seed]
//! ```
//!
//! Defaults: `testdata/Default`, seed 0.
//!
//! The probe runs, one run each, with an all-kinds no-filter
//! [`PlacementFilter`]: square rects centred on the origin of 256, 512, 1024,
//! 2048, 5000 and 10,000 blocks per side. The rects are nested, so a later run
//! re-covers ground the earlier runs touched and starts with a warmer
//! generator cache; each run is still one fresh `features_in` call on a fresh
//! `BlockRect`, and the interesting growth (the padded scan plus the
//! per-lattice-point predicates) dominates the cache effect at these sizes. The
//! 64×64 warm-up rect near (4096, 4096) is disjoint from every measured rect,
//! so the first measured run starts as cold as the generator allows (the same
//! rule as `query_bench`).
//!
//! The 10,000 leg costs hours, so the published table measures through 5000 and
//! extrapolates 10,000 from the (area-linear) growth; the cost table, the
//! numbers and the growth analysis live in [`QUERY.md`]. The ladder keeps
//! 10,000 so the probe stays the one place the eventual full run happens.
//!
//! [`QUERY.md`]: https://github.com/DorianCen/hytale-server-rust/blob/main/QUERY.md

use std::path::PathBuf;
use std::time::Instant;

use worldgen_hytale::chunk::chunk_generator::ChunkGenerator;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::query::{
    BlockRect, FeaturePlacement, PlacementFilter, PlacementKind, PlacementKindMask,
};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_hytale::world_gen_provider::{HytaleWorldGenProvider, Semver};

/// The probed square sizes in blocks per side, smallest first.
const RECT_SIZES: [i32; 6] = [256, 512, 1024, 2048, 5000, 10000];

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

/// A square rect of `size` blocks per side centred on the origin.
///
/// Even sizes cannot be symmetric around a block, so the rect starts at
/// `-(size / 2)` and covers exactly `size` blocks on each axis.
fn centred_rect(size: i32) -> BlockRect {
    let min = -(size / 2);
    BlockRect {
        min_x: min,
        min_z: min,
        max_x: min + size - 1,
        max_z: min + size - 1,
    }
}

/// The `VmHWM` line of `/proc/self/status` in kB, when the platform has one.
fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

/// Per-kind counts plus a checksum that keeps the placements alive.
fn count_kinds(placements: &[FeaturePlacement]) -> (usize, usize, usize, i64) {
    let mut unique = 0usize;
    let mut biome = 0usize;
    let mut spawn = 0usize;
    let mut checksum = 0i64;
    for placement in placements {
        match placement.kind {
            PlacementKind::UniquePrefab => unique += 1,
            PlacementKind::BiomePrefab => biome += 1,
            PlacementKind::Spawn => spawn += 1,
            // `PlacementKind` is `#[non_exhaustive]`; a future kind must be
            // counted explicitly when it lands.
            _ => {}
        }
        checksum = checksum
            .wrapping_add(i64::from(placement.position.0))
            .wrapping_add(i64::from(placement.position.2))
            .wrapping_add(placement.name.len() as i64);
    }
    (unique, biome, spawn, checksum)
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

    if cfg!(debug_assertions) {
        println!(
            "warning: this probe is built in debug mode; run it with \
             `cargo run --release ...` — debug timings are not comparable"
        );
    }

    println!("world {}", world.display());
    println!("seed {seed}");

    let loader = ChunkGeneratorJsonLoader::new(
        procedural::json::SeedString::new("ChunkGenerator"),
        make_config(&world),
    );
    let started = Instant::now();
    let generator: std::rc::Rc<ChunkGenerator> = loader.load();
    println!(
        "generator_load_seconds {:.6}",
        started.elapsed().as_secs_f64()
    );

    let filter = PlacementFilter {
        kinds: PlacementKindMask::all(),
        ..PlacementFilter::default()
    };

    // Warm-up on a disjoint 64×64 rect so the measured rects start cold.
    let warmup = generator.features_in(
        seed,
        BlockRect {
            min_x: 4096,
            min_z: 4096,
            max_x: 4096 + 63,
            max_z: 4096 + 63,
        },
        &filter,
    );
    let mut checksum = warmup.len() as i64;
    drop(warmup);

    for size in RECT_SIZES {
        let rect = centred_rect(size);
        let started = Instant::now();
        let placements = generator.features_in(seed, rect, &filter);
        let seconds = started.elapsed().as_secs_f64();

        let (unique, biome, spawn, placement_checksum) = count_kinds(&placements);
        checksum = checksum
            .wrapping_add(placement_checksum)
            .wrapping_add(placements.len() as i64);

        println!("rect_{size}_seconds {seconds:.6}");
        println!("rect_{size}_placements {}", placements.len());
        println!("rect_{size}_unique {unique}");
        println!("rect_{size}_biome {biome}");
        println!("rect_{size}_spawn {spawn}");
    }

    match peak_rss_kb() {
        Some(kb) => println!("peak_rss_kb {kb}"),
        None => println!("peak_rss_kb unavailable"),
    }

    // Keep the checksum alive so the runs are not optimized away.
    println!("probe_checksum {checksum}");
}
