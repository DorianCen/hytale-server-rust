//! `hytale-server` — CoreServer rewrite binary.
//!
//! Startup wiring for the asset registry, universe, network listener, auth
//! providers, command framework, and world threads. The binary owns no format
//! or gameplay logic of its own; it composes the library crates.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use hytale_server::{run_chunk_tool, run_headless, ChunkToolOptions, HeadlessOptions};

#[derive(Debug, Parser)]
#[command(name = "hytale-server", version, about = "Hytale CoreServer rewrite")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Load assets, generate one chunk, convert it to a runtime column and
    /// save a Java-compatible region blob.
    Headless(HeadlessArgs),
    /// Load a saved chunk, optionally edit one block, save it and verify the
    /// reload.
    LoadChunk(LoadChunkArgs),
}

#[derive(Debug, Args)]
struct HeadlessArgs {
    /// Asset pack (zip or unpacked directory) for the P0 registry.
    #[arg(long, default_value = "Assets/Assets.zip")]
    assets_pack: PathBuf,
    /// Unpacked asset tree used for prefabs, environments and tint gradients.
    #[arg(long, default_value = "Assets/extracted")]
    assets_root: PathBuf,
    /// World-gen data folder (`World.json`).
    #[arg(long, default_value = "testdata/Default")]
    world: PathBuf,
    /// World save root; the region file is written below `chunks/`.
    #[arg(long, default_value = "target/headless-world")]
    save: PathBuf,
    /// World seed.
    #[arg(long, default_value_t = 0)]
    seed: i32,
    /// Chunk coordinates as `x,z`.
    #[arg(long, default_value = "0,0", value_parser = parse_chunk)]
    chunk: (i32, i32),
    /// Worldgen worker count.
    #[arg(long, default_value_t = 1)]
    workers: usize,
}

#[derive(Debug, Args)]
struct LoadChunkArgs {
    /// Asset pack (zip or unpacked directory) for the P0 registry.
    #[arg(long, default_value = "Assets/Assets.zip")]
    assets_pack: PathBuf,
    /// World save root containing `chunks/`.
    #[arg(long, default_value = "target/headless-world")]
    save: PathBuf,
    /// Chunk coordinates as `x,z`.
    #[arg(long, default_value = "0,0", value_parser = parse_chunk)]
    chunk: (i32, i32),
    /// Optional block edit as `x,y,z,block_id` in world coordinates.
    #[arg(long, value_parser = parse_block)]
    set_block: Option<(i32, i32, i32, i32)>,
}

fn parse_chunk(value: &str) -> Result<(i32, i32), String> {
    let (x, z) = value
        .split_once(',')
        .ok_or_else(|| "expected `x,z`".to_string())?;
    let x = x
        .trim()
        .parse()
        .map_err(|_| format!("invalid chunk x: {x}"))?;
    let z = z
        .trim()
        .parse()
        .map_err(|_| format!("invalid chunk z: {z}"))?;
    Ok((x, z))
}

fn parse_block(value: &str) -> Result<(i32, i32, i32, i32), String> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err("expected `x,y,z,block_id`".to_string());
    }
    let parse = |name: &str, value: &str| {
        value
            .parse::<i32>()
            .map_err(|_| format!("invalid {name}: {value}"))
    };
    Ok((
        parse("block x", parts[0])?,
        parse("block y", parts[1])?,
        parse("block z", parts[2])?,
        parse("block id", parts[3])?,
    ))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Headless(args)) => run_headless_command(args),
        Some(Command::LoadChunk(args)) => run_load_chunk_command(args),
        None => {
            println!("hytale-server {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
    }
}

fn run_headless_command(args: HeadlessArgs) -> ExitCode {
    let options = HeadlessOptions {
        assets_pack: args.assets_pack,
        assets_root: args.assets_root,
        world_dir: args.world,
        save_dir: args.save,
        seed: args.seed,
        chunk_x: args.chunk.0,
        chunk_z: args.chunk.1,
        workers: args.workers,
    };
    match run_headless(&options) {
        Ok(report) => {
            println!(
                "generated chunk ({}, {}) seed {}: {} blocks, {} non-air, {} bytes",
                report.chunk_x,
                report.chunk_z,
                report.seed,
                report.asset_blocks,
                report.non_air_blocks,
                report.document_bytes
            );
            println!(
                "saved {} blob {} (verified: {})",
                report.region_file.display(),
                report.blob_index,
                report.verified
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("headless boot failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_load_chunk_command(args: LoadChunkArgs) -> ExitCode {
    let options = ChunkToolOptions {
        assets_pack: args.assets_pack,
        save_dir: args.save,
        chunk_x: args.chunk.0,
        chunk_z: args.chunk.1,
        set_block: args.set_block,
    };
    match run_chunk_tool(&options) {
        Ok(report) => {
            println!(
                "loaded chunk ({}, {}): {} non-air blocks",
                report.chunk_x, report.chunk_z, report.non_air_blocks
            );
            if let (Some(before), Some(after)) = (report.block_before, report.block_after) {
                println!("block edit: {before} -> {after}");
            }
            println!(
                "saved {} (verified: {})",
                report.region_file.display(),
                report.verified
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("load-chunk failed: {error}");
            ExitCode::FAILURE
        }
    }
}
