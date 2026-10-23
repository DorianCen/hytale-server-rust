//! `hytale-server` — CoreServer rewrite binary.
//!
//! Startup wiring for the asset registry, universe, network listener, auth
//! providers, command framework, and world threads. The binary owns no format
//! or gameplay logic of its own; it composes the library crates.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use hytale_server::GeneratorOptions;
use hytale_server::{
    run_chunk_serve, run_chunk_tool, run_headless, run_player_data_tool, run_world_boot,
    run_world_config_tool, ChunkServeOptions, ChunkToolOptions, HeadlessOptions,
    PlayerDataToolOptions, WorldBootOptions, WorldConfigToolOptions, WorldThreadOptions,
};
use hytale_world::player_data::{Position, Rotation};

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
    /// Load or create a world `config.json` with the Java-compatible codec.
    WorldConfig(WorldConfigArgs),
    /// Load or create a player save (`players/<uuid>.json`).
    PlayerData(PlayerDataArgs),
    /// Boot a complete save: config, player and stored chunks.
    WorldBoot(WorldBootArgs),
    /// Serve one client's chunk stream over QUIC (transport demo, no auth).
    ChunkServe(ChunkServeArgs),
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

#[derive(Debug, Args)]
struct WorldConfigArgs {
    /// Optional source config to load before writing the save directory copy.
    #[arg(long)]
    input: Option<PathBuf>,
    /// World save root; the config is written to `<save>/config.json`.
    #[arg(long, default_value = "target/headless-world")]
    save: PathBuf,
    /// Seed used only when a new config is created.
    #[arg(long, default_value_t = 0)]
    seed: i64,
    /// Display name used only when a new config is created.
    #[arg(long)]
    name: Option<String>,
    /// UUID used when a new config is created (or to replace a loaded one).
    #[arg(long)]
    uuid: Option<String>,
}

#[derive(Debug, Args)]
struct PlayerDataArgs {
    /// Optional source player document to load before writing the save copy.
    #[arg(long)]
    input: Option<PathBuf>,
    /// World save root; the file is written to `<save>/players/<uuid>.json`.
    #[arg(long, default_value = "target/headless-world")]
    save: PathBuf,
    /// Player UUID (required when creating; overrides a loaded document).
    #[arg(long)]
    uuid: Option<String>,
    /// Spawn X for a new player.
    #[arg(long, default_value_t = 0.0)]
    x: f64,
    /// Spawn Y for a new player.
    #[arg(long, default_value_t = 0.0)]
    y: f64,
    /// Spawn Z for a new player.
    #[arg(long, default_value_t = 0.0)]
    z: f64,
    /// Pitch (radians) for a new player.
    #[arg(long, default_value_t = 0.0)]
    pitch: f32,
    /// Yaw (radians) for a new player.
    #[arg(long, default_value_t = 0.0)]
    yaw: f32,
    /// Roll (radians) for a new player.
    #[arg(long, default_value_t = 0.0)]
    roll: f32,
    /// `WorldGenId` component for a new player.
    #[arg(long, default_value_t = 0)]
    world_gen_id: i32,
}

#[derive(Debug, Args)]
struct WorldBootArgs {
    /// Asset pack (zip or unpacked directory) for the chunk codec.
    #[arg(long, default_value = "Assets/Assets.zip")]
    assets_pack: PathBuf,
    /// Unpacked asset tree used for prefabs, environments and tint gradients.
    #[arg(long, default_value = "Assets/extracted")]
    assets_root: PathBuf,
    /// World-gen data folder (`World.json`).
    #[arg(long, default_value = "testdata/Default")]
    world: PathBuf,
    /// World save root (`config.json`, `chunks/`, `players/`).
    #[arg(long, default_value = "target/headless-world")]
    save: PathBuf,
    /// Worldgen worker count.
    #[arg(long, default_value_t = 1)]
    workers: usize,
    /// Chunk radius loaded/generated around the player (0 = the player chunk).
    #[arg(long, default_value_t = 0)]
    view_radius: u32,
    /// Maximum chunks loaded/generated per world-thread tick.
    #[arg(long, default_value_t = 8)]
    chunks_per_tick: usize,
    /// Seed used only when a new config is created.
    #[arg(long, default_value_t = 0)]
    seed: i64,
    /// Display name used only when a new config is created.
    #[arg(long)]
    name: Option<String>,
    /// World UUID used only when a new config is created.
    #[arg(long)]
    uuid: Option<String>,
    /// Player to load or create.
    #[arg(long)]
    player_uuid: Option<String>,
    /// Optional block edit as `x,y,z,block_id` in world coordinates.
    #[arg(long, value_parser = parse_block)]
    set_block: Option<(i32, i32, i32, i32)>,
}

#[derive(Debug, Args)]
struct ChunkServeArgs {
    /// Asset pack (zip or unpacked directory).
    #[arg(long, default_value = "Assets/Assets.zip")]
    assets_pack: PathBuf,
    /// Unpacked asset tree for prefabs, environments and tint gradients.
    #[arg(long, default_value = "Assets/extracted")]
    assets_root: PathBuf,
    /// World-gen data folder (`World.json`).
    #[arg(long, default_value = "testdata/Default")]
    world: PathBuf,
    /// World save root.
    #[arg(long, default_value = "target/headless-world")]
    save: PathBuf,
    /// QUIC bind address.
    #[arg(long, default_value = "127.0.0.1:5520")]
    bind: std::net::SocketAddr,
    /// Seed used only when a new config is created.
    #[arg(long, default_value_t = 0)]
    seed: i64,
    /// Display name used only when a new config is created.
    #[arg(long)]
    name: Option<String>,
    /// World UUID used only when a new config is created.
    #[arg(long)]
    uuid: Option<String>,
    /// Player to load or create.
    #[arg(long)]
    player_uuid: Option<String>,
    /// Worldgen worker count.
    #[arg(long, default_value_t = 1)]
    workers: usize,
    /// Chunk radius sent to the client.
    #[arg(long, default_value_t = 0)]
    view_radius: u32,
    /// Maximum chunks loaded/generated per world-thread tick.
    #[arg(long, default_value_t = 8)]
    chunks_per_tick: usize,
    /// zstd compression level for chunk frames.
    #[arg(long, default_value_t = 3)]
    compression_level: i32,
    /// After the snapshot, move the view one chunk and stream the update.
    #[arg(long)]
    move_demo: bool,
    /// Run the dev/insecure Connect → Setup handshake before streaming.
    #[arg(long)]
    handshake: bool,
    /// `ServerInfo.serverName`.
    #[arg(long, default_value = "Rust CoreServer")]
    server_name: String,
    /// `ServerInfo.motd`.
    #[arg(long, default_value = "dev")]
    motd: String,
    /// `ServerInfo.maxPlayers`.
    #[arg(long, default_value_t = 20)]
    max_players: i32,
    /// Seconds allowed for the whole handshake.
    #[arg(long, default_value_t = 30)]
    handshake_timeout: u64,
    /// Seconds to wait for the client to close after the stream ends.
    #[arg(long, default_value_t = 5)]
    close_timeout: u64,
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
        Some(Command::WorldConfig(args)) => run_world_config_command(args),
        Some(Command::PlayerData(args)) => run_player_data_command(args),
        Some(Command::WorldBoot(args)) => run_world_boot_command(args),
        Some(Command::ChunkServe(args)) => run_chunk_serve_command(args),
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

fn run_chunk_serve_command(args: ChunkServeArgs) -> ExitCode {
    let options = ChunkServeOptions {
        world: WorldThreadOptions {
            generator: GeneratorOptions {
                assets_pack: args.assets_pack,
                assets_root: args.assets_root,
                world_dir: args.world,
                workers: args.workers,
            },
            save_dir: args.save,
            seed: args.seed,
            display_name: args.name,
            world_uuid: args.uuid,
            player_uuid: args.player_uuid,
            view_radius: args.view_radius,
            chunks_per_tick: args.chunks_per_tick,
        },
        bind_addr: args.bind,
        center: None,
        compression_level: args.compression_level,
        move_demo: args.move_demo,
        handshake: args.handshake,
        session: hytale_server::SessionOptions {
            server_name: args.server_name,
            motd: args.motd,
            max_players: args.max_players,
            default_view_radius: args.view_radius,
            chunk_compression_level: args.compression_level,
            handshake_timeout: std::time::Duration::from_secs(args.handshake_timeout),
        },
        close_timeout: std::time::Duration::from_secs(args.close_timeout),
    };
    match run_chunk_serve(&options) {
        Ok(report) => {
            println!(
                "streamed {} chunks ({} packets, {} frames, {} bytes) to {}",
                report.stream.chunks,
                report.stream.packets,
                report.stream.frames,
                report.stream.payload_bytes,
                report.bound_addr
            );
            println!(
                "world {} player {} at chunk ({}, {})",
                report.world_uuid, report.player_uuid, report.player_chunk.0, report.player_chunk.1
            );
            if let Some(session) = report.session {
                println!(
                    "handshake ok: {} view_radius={} chunks={}",
                    session.username, session.view_radius, session.stream.chunks
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("chunk-serve failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_world_boot_command(args: WorldBootArgs) -> ExitCode {
    let options = WorldBootOptions {
        assets_pack: args.assets_pack,
        assets_root: args.assets_root,
        world_dir: args.world,
        save_dir: args.save,
        workers: args.workers,
        view_radius: args.view_radius,
        chunks_per_tick: args.chunks_per_tick,
        seed: args.seed,
        display_name: args.name,
        world_uuid: args.uuid,
        player_uuid: args.player_uuid,
        set_block: args.set_block,
    };
    match run_world_boot(&options) {
        Ok(report) => {
            println!(
                "world boot {}: uuid={} seed={} config_created={}",
                report.save_dir.display(),
                report.world_uuid,
                report.seed,
                report.config_created
            );
            println!(
                "player {} (created: {})",
                report.player_uuid, report.player_created
            );
            println!(
                "chunks: stored={} requested={} loaded={} generated={} saved={} radius={}",
                report.stored_chunks,
                report.requested_chunks,
                report.loaded_chunks,
                report.generated_chunks,
                report.saved_chunks,
                report.view_radius
            );
            if let Some((x, y, z, before, after)) = report.block_edit {
                println!("block edit ({x}, {y}, {z}): {before} -> {after}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("world-boot failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_player_data_command(args: PlayerDataArgs) -> ExitCode {
    let options = PlayerDataToolOptions {
        input: args.input,
        save_dir: args.save,
        uuid: args.uuid,
        position: Position {
            x: args.x,
            y: args.y,
            z: args.z,
        },
        rotation: Rotation {
            pitch: args.pitch,
            yaw: args.yaw,
            roll: args.roll,
        },
        world_gen_id: args.world_gen_id,
    };
    match run_player_data_tool(&options) {
        Ok(report) => {
            println!(
                "player {} position=({}, {}, {}) components={} (loaded: {})",
                report.uuid,
                report.position.x,
                report.position.y,
                report.position.z,
                report.component_count,
                report.loaded_existing
            );
            println!("saved {}", report.path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("player-data failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_world_config_command(args: WorldConfigArgs) -> ExitCode {
    let options = WorldConfigToolOptions {
        input: args.input,
        save_dir: args.save,
        seed: args.seed,
        display_name: args.name,
        uuid: args.uuid,
    };
    match run_world_config_tool(&options) {
        Ok(report) => {
            println!(
                "world config v{} uuid={} seed={} name={} (loaded: {})",
                report.version,
                report.uuid,
                report.seed,
                report.display_name.as_deref().unwrap_or("<none>"),
                report.loaded_existing
            );
            println!("saved {}", report.path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("world-config failed: {error}");
            ExitCode::FAILURE
        }
    }
}
