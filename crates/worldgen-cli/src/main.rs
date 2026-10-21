//! `worldgen-cli` — the headless harness of the Rust Hytale world generator.
//!
//! Commands:
//!
//! * `generate` — generate a set of chunks (worker pool) and dump them as
//!   compact per-column run-length JSON.
//! * `dump-chunk` — the same for exactly one chunk (stdout by default).
//! * `render-map` — render a top-down PNG of the height / zones / biomes.
//! * `validate` — load the world and run `ValidationUtil` over every prefab /
//!   cave node (exit code 1 when invalid).
//! * `bench` — generate a grid of chunks through the worker pool and print the
//!   per-stage timing averages.
//!
//! The world-gen folder is resolved with the Java `HytaleWorldGenProvider`
//! semantics (`--world`, `--name`, `--version`).

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};

use worldgen_hytale::chunk::validation_util::ValidationUtil;
use worldgen_hytale::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use worldgen_hytale::world_gen::{
    WorldGen, WorldGenOptions, WorldGenRequest, WorldGenService, WorldGenStats,
};
use worldgen_hytale::world_gen_config::WorldGenConfig;
use worldgen_hytale::world_gen_provider::HytaleWorldGenProvider;
use worldgen_model::block_priority::HEIGHT;

#[derive(Parser)]
#[command(
    name = "worldgen-cli",
    version,
    about = "Headless Hytale world generator (bit-exact Rust port)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate chunks and dump them as JSON.
    Generate(GenerateArgs),
    /// Generate one chunk and dump it (stdout by default).
    DumpChunk(DumpChunkArgs),
    /// Render a top-down PNG of the world.
    RenderMap(RenderMapArgs),
    /// Validate every prefab / cave node of the world.
    Validate(ValidateArgs),
    /// Benchmark the worker pool.
    Bench(BenchArgs),
}

/// The shared world-resolution arguments (Java `HytaleWorldGenProvider`).
#[derive(Args, Clone)]
struct WorldArgs {
    /// The world-gen folder (`Path` in the Java provider).
    #[arg(long)]
    world: Option<PathBuf>,
    /// The world-gen name (the `<Path>/<Name>` legacy rule).
    #[arg(long, default_value = "Default")]
    name: String,
    /// The world-gen version (only carried/logged).
    #[arg(long, default_value = "0.0.0")]
    version: String,
    /// The extracted base-game asset root (prefabs / environments / tints).
    #[arg(long)]
    assets_root: Option<PathBuf>,
    /// The golden registry dump (`registries.json`).
    #[arg(long)]
    registries: Option<PathBuf>,
}

impl WorldArgs {
    fn config(&self) -> Result<WorldGenConfig, String> {
        let default_path = self.world.clone().unwrap_or_else(default_world_path);
        let provider = HytaleWorldGenProvider {
            name: self.name.clone(),
            version: worldgen_hytale::world_gen_provider::Semver::parse(&self.version)
                .map_err(|error| error.message)?,
            path: None,
        };
        let path = provider
            .resolve_path(&default_path)
            .map_err(|error| error.message)?;
        Ok(WorldGenConfig::new(
            path,
            self.name.clone(),
            self.version.clone(),
        ))
    }

    fn loader(&self) -> Result<ChunkGeneratorJsonLoader, String> {
        let config = self.config()?;
        let mut loader = ChunkGeneratorJsonLoader::new(procedural_seed_label(), config);
        if let Some(assets_root) = &self.assets_root {
            loader = loader.with_assets_root(assets_root);
        }
        if let Some(registries) = &self.registries {
            loader = loader.with_registries_path(registries);
        }
        Ok(loader)
    }

    fn service_options(&self, workers: usize) -> WorldGenOptions {
        WorldGenOptions {
            workers,
            seed_label: "ChunkGenerator".to_string(),
            assets_root: self.assets_root.clone(),
            registries_path: self.registries.clone(),
        }
    }
}

fn procedural_seed_label() -> procedural::json::SeedString {
    procedural::json::SeedString::new("ChunkGenerator")
}

/// The default world-gen folder: the extracted pack when present, then the
/// repository golden copy.
fn default_world_path() -> PathBuf {
    let extracted = PathBuf::from("Assets/extracted/Server/World/Default");
    if extracted.is_dir() {
        return extracted;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/Default")
}

// ---------------------------------------------------------------------------
// generate / dump-chunk
// ---------------------------------------------------------------------------

#[derive(Args)]
struct GenerateArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// The world seed.
    #[arg(long, default_value_t = 0)]
    seed: i32,
    /// A chunk coordinate `x,z` (repeatable).
    #[arg(long = "chunk", value_parser = parse_chunk, required = true)]
    chunks: Vec<(i32, i32)>,
    /// The worker-pool size.
    #[arg(long, default_value_t = 1)]
    workers: usize,
    /// Output file (stdout when omitted).
    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Args)]
struct DumpChunkArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// The world seed.
    #[arg(long, default_value_t = 0)]
    seed: i32,
    /// The chunk coordinate `x,z`.
    #[arg(long, value_parser = parse_chunk, required = true)]
    chunk: (i32, i32),
    /// Output file (stdout when omitted).
    #[arg(long)]
    out: Option<PathBuf>,
}

fn parse_chunk(value: &str) -> Result<(i32, i32), String> {
    let (x, z) = value
        .split_once(',')
        .ok_or_else(|| format!("expected `x,z`, got `{value}`"))?;
    Ok((
        x.trim()
            .parse()
            .map_err(|_| format!("invalid chunk x: {x}"))?,
        z.trim()
            .parse()
            .map_err(|_| format!("invalid chunk z: {z}"))?,
    ))
}

fn run_generate(args: &GenerateArgs) -> Result<(), String> {
    let config = args.world.config()?;
    let service = WorldGenService::new(config, args.world.service_options(args.workers.max(1)))
        .map_err(|error| error.to_string())?;

    let mut chunks = Vec::with_capacity(args.chunks.len());
    for &(chunk_x, chunk_z) in &args.chunks {
        let result = service
            .generate(WorldGenRequest::new(args.seed, chunk_x, chunk_z))
            .map_err(|error| error.to_string())?;
        chunks.push(chunk_json(&result.chunk));
    }
    service.shutdown();

    let document = json!({
        "seed": args.seed,
        "height": HEIGHT,
        "chunks": chunks,
    });
    write_json(document, args.out.as_deref())
}

fn run_dump_chunk(args: &DumpChunkArgs) -> Result<(), String> {
    let generate = GenerateArgs {
        world: args.world.clone(),
        seed: args.seed,
        chunks: vec![args.chunk],
        workers: 1,
        out: args.out.clone(),
    };
    run_generate(&generate)
}

/// The compact per-column run-length encoding of a generated chunk (the same
/// tuple as the golden `chunks.json`).
fn chunk_json(chunk: &worldgen_model::chunk::GeneratedChunk) -> Value {
    let mut columns: Vec<Value> = vec![Value::Null; 1024];
    for x in 0..32 {
        for z in 0..32 {
            let mut runs: Vec<Vec<i64>> = Vec::new();
            for y in 0..HEIGHT {
                let state = [
                    chunk.get_block(x, y, z) as i64,
                    chunk.get_rotation_index(x, y, z) as i64,
                    chunk.get_filler(x, y, z) as i64,
                    chunk.get_fluid_id(x, y, z) as i64,
                    chunk.get_fluid_level(x, y, z) as i64,
                ];
                match runs.last_mut() {
                    Some(last) if last[2..] == state[..] && last[1] + 1 == y as i64 => {}
                    _ => {
                        let mut run = vec![y as i64, y as i64];
                        run.extend_from_slice(&state);
                        runs.push(run);
                    }
                }
                if let Some(last) = runs.last_mut() {
                    last[1] = y as i64;
                }
            }
            // `ChunkUtil.indexColumn(x, z) = (z << 5) | x` — the golden dump
            // order (the x/z fields make the column self-describing anyway).
            columns[((z << 5) | x) as usize] =
                json!({ "x": x, "z": z, "tint": chunk.get_tint(x, z), "runs": runs });
        }
    }
    json!({
        "x": chunk.get_x(),
        "z": chunk.get_z(),
        "index": chunk.get_index(),
        "columns": columns,
        "columnOrder": "ChunkUtil.indexColumn(x, z) = (z << 5) | x",
        "runFormat": "[yStart, yEnd, blockId, rotation, filler, fluidId, fluidLevel]",
    })
}

fn write_json(document: Value, out: Option<&Path>) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&document).map_err(|error| error.to_string())?;
    match out {
        Some(path) => std::fs::write(path, text)
            .map_err(|error| format!("could not write {}: {error}", path.display())),
        None => {
            println!("{text}");
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// render-map
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, clap::ValueEnum)]
enum MapMode {
    Height,
    Zone,
    Biome,
}

#[derive(Args)]
struct RenderMapArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// The world seed.
    #[arg(long, default_value_t = 0)]
    seed: i32,
    /// The centre block coordinate `x,z`.
    #[arg(long, value_parser = parse_chunk, default_value = "0,0")]
    center: (i32, i32),
    /// The image size in pixels.
    #[arg(long, default_value_t = 256)]
    size: u32,
    /// The blocks per pixel.
    #[arg(long, default_value_t = 1)]
    scale: i32,
    /// What to render.
    #[arg(long, value_enum, default_value_t = MapMode::Height)]
    mode: MapMode,
    /// The output PNG.
    #[arg(long, default_value = "map.png")]
    out: PathBuf,
}

fn run_render_map(args: &RenderMapArgs) -> Result<(), String> {
    let generator = args.world.loader()?.load();
    let size = args.size.max(1);
    let scale = args.scale.max(1);
    let half = (size as i32 * scale) / 2;
    let origin_x = args.center.0 - half;
    let origin_z = args.center.1 - half;

    let mut pixels = Vec::with_capacity((size * size * 3) as usize);
    for py in 0..size {
        for px in 0..size {
            let x = origin_x + px as i32 * scale;
            let z = origin_z + py as i32 * scale;
            let rgb = match args.mode {
                MapMode::Height => height_color(generator.get_height(args.seed, x, z)),
                MapMode::Zone => {
                    let result = generator.get_zone_biome_result_at(args.seed, x, z);
                    let name = result
                        .get_zone_result()
                        .and_then(|zone| zone.get_zone().map(|zone| zone.name.clone()))
                        .unwrap_or_default();
                    name_color(&name, 0)
                }
                MapMode::Biome => {
                    let result = generator.get_zone_biome_result_at(args.seed, x, z);
                    let name = result
                        .get_biome()
                        .map(|biome| biome.get_name().to_string())
                        .unwrap_or_default();
                    name_color(&name, 0)
                }
            };
            pixels.extend_from_slice(&rgb);
        }
    }

    let file = File::create(&args.out)
        .map_err(|error| format!("could not create {}: {error}", args.out.display()))?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, size, size);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(&pixels)
        .map_err(|error| error.to_string())?;
    eprintln!(
        "wrote {} ({}×{} px, {} blocks/px, center {},{})",
        args.out.display(),
        size,
        size,
        scale,
        args.center.0,
        args.center.1
    );
    Ok(())
}

fn height_color(height: i32) -> [u8; 3] {
    // Java `ChunkUtil.HEIGHT` is 320: a simple elevation ramp (sea level 64).
    let value = (height.clamp(0, HEIGHT) as f32 / HEIGHT as f32 * 255.0) as u8;
    [value, value, value]
}

/// A stable pseudo-random colour per zone/biome name.
fn name_color(name: &str, salt: u8) -> [u8; 3] {
    let hash = procedural::json::seed_string::java_string_hash_code(name) as u32;
    [
        ((hash & 0xFF) as u8).wrapping_add(salt),
        (((hash >> 8) & 0xFF) as u8).wrapping_add(salt),
        (((hash >> 16) & 0xFF) as u8).wrapping_add(salt),
    ]
}

// ---------------------------------------------------------------------------
// validate
// ---------------------------------------------------------------------------

#[derive(Args)]
struct ValidateArgs {
    #[command(flatten)]
    world: WorldArgs,
}

fn run_validate(args: &ValidateArgs) -> Result<(), String> {
    let generator = args.world.loader()?.load();
    let invalid = ValidationUtil::is_invalid(generator.get_zone_pattern_provider());
    if invalid {
        eprintln!("INVALID: the world contains a broken prefab / cave node");
        return Err(String::new());
    }
    println!("valid: every prefab and cave node resolved");
    Ok(())
}

// ---------------------------------------------------------------------------
// bench
// ---------------------------------------------------------------------------

#[derive(Args)]
struct BenchArgs {
    #[command(flatten)]
    world: WorldArgs,
    /// The world seed.
    #[arg(long, default_value_t = 0)]
    seed: i32,
    /// The first chunk coordinate `x,z`.
    #[arg(long, value_parser = parse_chunk, default_value = "0,0")]
    start: (i32, i32),
    /// The number of chunks (a square grid around `--start`).
    #[arg(long, default_value_t = 16)]
    chunks: usize,
    /// The worker-pool size (defaults to the Java `POOL_SIZE`).
    #[arg(long)]
    workers: Option<usize>,
}

fn run_bench(args: &BenchArgs) -> Result<(), String> {
    let workers = args
        .workers
        .unwrap_or_else(worldgen_hytale::world_gen::default_worker_count);
    let config = args.world.config()?;
    let service = WorldGenService::new(config, args.world.service_options(workers))
        .map_err(|error| error.to_string())?;

    let side = (args.chunks as f64).sqrt().ceil() as i32;
    let started = Instant::now();
    let mut totals = worldgen_hytale::chunk::chunk_generator_execution::StageTimings::default();
    let mut done = 0usize;
    for index in 0..args.chunks {
        let chunk_x = args.start.0 + index as i32 % side;
        let chunk_z = args.start.1 + index as i32 / side;
        let result = service
            .generate(WorldGenRequest::new(args.seed, chunk_x, chunk_z))
            .map_err(|error| error.to_string())?;
        let timings = result.timings;
        totals.tint_nanos += timings.tint_nanos;
        totals.environment_nanos += timings.environment_nanos;
        totals.blocks_nanos += timings.blocks_nanos;
        totals.caves_nanos += timings.caves_nanos;
        totals.prefabs_nanos += timings.prefabs_nanos;
        totals.water_nanos += timings.water_nanos;
        totals.total_nanos += timings.total_nanos;
        done += 1;
    }
    let elapsed = started.elapsed();
    service.shutdown();

    let divisor = done.max(1) as f64;
    let average = |nanos: u64| nanos as f64 * 1.0e-9 / divisor;
    let stats = WorldGenStats {
        chunks: done as u64,
        chunk_seconds: average(totals.total_nanos),
        tint_seconds: average(totals.tint_nanos),
        environment_seconds: average(totals.environment_nanos),
        blocks_seconds: average(totals.blocks_nanos),
        caves_seconds: average(totals.caves_nanos),
        prefabs_seconds: average(totals.prefabs_nanos),
        water_seconds: average(totals.water_nanos),
        ..Default::default()
    };
    let report = json!({
        "workers": workers,
        "chunks": done,
        "wall_seconds": elapsed.as_secs_f64(),
        "chunks_per_second": done as f64 / elapsed.as_secs_f64().max(f64::MIN_POSITIVE),
        "avg_seconds": {
            "total": stats.chunk_seconds,
            "tint": stats.tint_seconds,
            "environment": stats.environment_seconds,
            "blocks": stats.blocks_seconds,
            "caves": stats.caves_seconds,
            "prefabs": stats.prefabs_seconds,
            "water": stats.water_seconds,
        }
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match &cli.command {
        Command::Generate(args) => run_generate(args),
        Command::DumpChunk(args) => run_dump_chunk(args),
        Command::RenderMap(args) => run_render_map(args),
        Command::Validate(args) => run_validate(args),
        Command::Bench(args) => run_bench(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("error: {message}");
            }
            ExitCode::FAILURE
        }
    }
}
