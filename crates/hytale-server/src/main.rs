//! `hytale-server` — CoreServer rewrite binary.
//!
//! Startup wiring for the asset registry, universe, network listener, auth
//! providers, command framework, and world threads. The binary owns no format
//! or gameplay logic of its own; it composes the library crates.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use hytale_auth::live::{LiveAuth, LiveAuthConfig};
use hytale_auth::{AuthKey, AuthService};
use hytale_commands::PermissionRegistry;
use hytale_server::GeneratorOptions;
use hytale_server::{
    run_chunk_serve, run_chunk_tool, run_headless, run_player_data_tool, run_serve, run_world_boot,
    run_world_config_tool, AuthMode, BuiltinHost, BuiltinRuntime, ChunkServeOptions,
    ChunkToolOptions, HeadlessOptions, PlayerDataToolOptions, ServeOptions, SessionLimits,
    SessionOptions, StartOptions, WorldBootOptions, WorldConfigToolOptions, WorldThreadOptions,
};
use hytale_world::player_data::{Position, Rotation};

#[derive(Debug, Parser)]
#[command(name = "hytale-server", version, about = "Hytale CoreServer rewrite")]
struct Cli {
    /// Validate every enabled asset store and the encoded config burst.
    #[arg(long)]
    validate_assets: bool,
    /// Asset pack (zip or unpacked directory) used by `--validate-assets`.
    #[arg(long, default_value = "Assets/Assets.zip")]
    pack: PathBuf,
    /// Print the compiled builtin modules and exit.
    #[arg(long, global = true)]
    list_modules: bool,
    /// Extra builtin module (`Group:Name`) to enable in addition to defaults.
    #[arg(long = "module", global = true, value_delimiter = ',')]
    modules: Vec<String>,
    /// Builtin module (`Group:Name`) to disable. Dependents fail closed.
    #[arg(long = "disable-module", global = true, value_delimiter = ',')]
    disable_modules: Vec<String>,
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
    /// Create/list/show/save/delete worlds in a persistent universe.
    Universe(UniverseArgs),
    /// Serve one client's chunk stream over QUIC (transport demo, no auth).
    ChunkServe(ChunkServeArgs),
    /// Persistent multi-session listener over a universe.
    Serve(ServeArgs),
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
    /// Optional NPC role to spawn at the player/world spawn.
    #[arg(long)]
    spawn_npc: Option<String>,
    /// Discover loaded environments and populate their configured NPC density.
    #[arg(long)]
    auto_population: bool,
}

#[derive(Debug, Args)]
struct UniverseArgs {
    /// Universe root (`universe.json` plus one directory per world).
    #[arg(long, default_value = "target/universe")]
    root: PathBuf,
    #[command(subcommand)]
    command: UniverseCommand,
}

#[derive(Debug, Subcommand)]
enum UniverseCommand {
    /// Create a world.
    Create {
        /// World name (directory-safe, unique case-insensitively).
        name: String,
        /// Generation seed.
        #[arg(long, default_value_t = 0)]
        seed: i64,
        /// Chunk provider: default, flat, void or dummy.
        #[arg(long, default_value = "default", value_parser = parse_world_provider)]
        provider: hytale_world::universe::WorldProviderKind,
    },
    /// List catalogued worlds.
    List,
    /// Show one world's descriptor.
    Show {
        /// World name or UUID.
        name: String,
    },
    /// Save one world's config (live threads save chunks themselves).
    Save {
        /// World name or UUID.
        name: String,
    },
    /// Delete a world.
    Delete {
        /// World name or UUID.
        name: String,
        /// Keep the save directory and only remove the catalog entry.
        #[arg(long)]
        keep_files: bool,
    },
    /// Make a world the universe default.
    Default {
        /// World name or UUID.
        name: String,
    },
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
    /// Seconds to wait for the client's `ClientReady` after `JoinWorld`.
    #[arg(long, default_value_t = 30)]
    world_join_timeout: u64,
    /// Authentication mode: `insecure`, `local` or `authenticated`.
    ///
    /// `authenticated` performs the live Hytale OAuth device flow (or uses
    /// `HYTALE_SERVER_SESSION_TOKEN` + `HYTALE_SERVER_IDENTITY_TOKEN`) and is
    /// required by release game clients.
    #[arg(long, default_value = "insecure")]
    auth_mode: String,
    /// Base64 PKCS#8 Ed25519 key for `--auth-mode local` (generated when absent).
    #[arg(long)]
    auth_key: Option<String>,
    /// Expected/granted JWT issuer for `--auth-mode local`.
    #[arg(long, default_value = "rust-local")]
    auth_issuer: String,
    /// Expected/granted JWT audience for `--auth-mode local`.
    #[arg(long, default_value = "rust-server")]
    auth_audience: String,
    /// Seconds a game session may be idle before the server disconnects it.
    #[arg(long, default_value_t = 300)]
    idle_timeout: u64,
    /// Seconds to wait for the client to close after the stream ends.
    #[arg(long, default_value_t = 5)]
    close_timeout: u64,
}

#[derive(Debug, Args)]
struct ServeArgs {
    /// Universe root (`universe.json` plus one directory per world).
    #[arg(long, default_value = "target/universe")]
    universe: PathBuf,
    /// World name or UUID to serve; defaults to the universe default.
    #[arg(long)]
    world: Option<String>,
    /// Asset pack (zip or unpacked directory).
    #[arg(long, default_value = "Assets/Assets.zip")]
    assets_pack: PathBuf,
    /// Unpacked asset tree for prefabs, environments and tint gradients.
    #[arg(long, default_value = "Assets/extracted")]
    assets_root: PathBuf,
    /// World-gen data folder (`World.json`).
    #[arg(long, default_value = "testdata/Default")]
    world_data: PathBuf,
    /// QUIC bind address. A loopback address also binds the other loopback
    /// family on the same port, so `localhost` works whether it resolves to
    /// `127.0.0.1` or `::1`. Use `[::]:PORT` for dual-stack on all interfaces.
    #[arg(long, default_value = "127.0.0.1:5520")]
    bind: std::net::SocketAddr,
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
    /// Optional NPC role to spawn at the world spawn on startup.
    #[arg(long)]
    spawn_npc: Option<String>,
    /// Discover loaded environments and populate their configured NPC density.
    #[arg(long)]
    auto_population: bool,
    /// Concurrent session limit.
    #[arg(long, default_value_t = 20)]
    max_connections: usize,
    /// Seconds allowed for the whole handshake.
    #[arg(long, default_value_t = 30)]
    handshake_timeout: u64,
    /// Seconds to wait for the client's `ClientReady` after `JoinWorld`.
    #[arg(long, default_value_t = 30)]
    world_join_timeout: u64,
    /// Seconds a game session may be idle before the server disconnects it.
    #[arg(long, default_value_t = 300)]
    idle_timeout: u64,
    /// Maximum view radius a client may request, in chunks.
    #[arg(long, default_value_t = 16)]
    max_view_radius: u32,
    /// Maximum inbound client packets per second.
    #[arg(long, default_value_t = 2048)]
    max_packets_per_second: u32,
    /// Inbound packet burst before the rate limiter disconnects.
    #[arg(long, default_value_t = 4096)]
    packet_burst: u32,
    /// Maximum inbound packet payload bytes.
    #[arg(long, default_value_t = 4194304)]
    max_packet_bytes: u32,
    /// Maximum queued broadcast messages per session.
    #[arg(long, default_value_t = 4096)]
    max_queued_messages: usize,
    /// Seconds to wait for sessions during shutdown before aborting them.
    #[arg(long, default_value_t = 10)]
    shutdown_timeout: u64,
    /// Seconds to wait for the client to close after its session ends.
    #[arg(long, default_value_t = 5)]
    close_timeout: u64,
    /// Write a JSONL packet trace (ids, channels, order, payload hashes).
    #[arg(long)]
    trace: Option<PathBuf>,
    /// `ServerInfo.serverName`.
    #[arg(long, default_value = "Rust CoreServer")]
    server_name: String,
    /// `ServerInfo.motd`.
    #[arg(long, default_value = "dev")]
    motd: String,
    /// `ServerInfo.maxPlayers`.
    #[arg(long, default_value_t = 20)]
    max_players: i32,
    /// Authentication mode: `insecure`, `local` or `authenticated`.
    ///
    /// `authenticated` performs the live Hytale OAuth device flow (or uses
    /// `HYTALE_SERVER_SESSION_TOKEN` + `HYTALE_SERVER_IDENTITY_TOKEN`) and is
    /// required by release game clients.
    #[arg(long, default_value = "insecure")]
    auth_mode: String,
    /// Base64 PKCS#8 Ed25519 key for `--auth-mode local` (generated when absent).
    #[arg(long)]
    auth_key: Option<String>,
    /// Expected/granted JWT issuer for `--auth-mode local`.
    #[arg(long, default_value = "rust-local")]
    auth_issuer: String,
    /// Expected/granted JWT audience for `--auth-mode local`.
    #[arg(long, default_value = "rust-server")]
    auth_audience: String,
    /// Grant operator permissions to a player name (repeatable).
    #[arg(long = "op")]
    op: Vec<String>,
    /// Enable the stdin command console (default: only when stdin is a terminal).
    #[arg(long, conflicts_with = "no_console")]
    console: bool,
    /// Disable the stdin command console.
    #[arg(long)]
    no_console: bool,
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

fn parse_world_provider(value: &str) -> Result<hytale_world::universe::WorldProviderKind, String> {
    value.parse()
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.list_modules {
        return run_list_modules_command();
    }
    if cli.validate_assets {
        return run_validate_assets_command(cli.pack);
    }
    let mut builtin_host = match start_builtin_host(&cli) {
        Ok(host) => host,
        Err(error) => {
            eprintln!("builtin module startup failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let builtin = builtin_host.runtime();
    let code = match cli.command {
        Some(Command::Headless(args)) => run_headless_command(args),
        Some(Command::LoadChunk(args)) => run_load_chunk_command(args),
        Some(Command::WorldConfig(args)) => run_world_config_command(args),
        Some(Command::PlayerData(args)) => run_player_data_command(args),
        Some(Command::WorldBoot(args)) => run_world_boot_command(args, builtin),
        Some(Command::Universe(args)) => run_universe_command(args),
        Some(Command::ChunkServe(args)) => run_chunk_serve_command(args, builtin),
        Some(Command::Serve(args)) => run_serve_command(args, builtin),
        None => {
            println!("hytale-server {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
    };
    let stopped = builtin_host.shutdown();
    for (module, message) in &stopped.failed {
        eprintln!("builtin module `{module}` failed to shut down: {message}");
    }
    code
}

fn run_list_modules_command() -> ExitCode {
    let host = match BuiltinHost::compiled() {
        Ok(host) => host,
        Err(error) => {
            eprintln!("could not load builtin manifests: {error}");
            return ExitCode::FAILURE;
        }
    };
    let manifests = host.manifests();
    println!("builtin modules: {} compiled", manifests.len());
    for manifest in manifests {
        let id = manifest.identifier();
        let mode = if manifest.disabled_by_default {
            "opt-in"
        } else {
            "default"
        };
        println!(
            "  {id} v{} ({mode}, main {})",
            manifest.version,
            manifest.main.as_deref().unwrap_or("<none>")
        );
    }
    ExitCode::SUCCESS
}

fn start_builtin_host(cli: &Cli) -> Result<BuiltinHost, hytale_server::HostError> {
    let mut host = BuiltinHost::compiled()?;
    let report = host.start(&StartOptions {
        requested: cli.modules.clone(),
        disabled: cli.disable_modules.clone(),
    })?;
    println!(
        "builtin modules: {} enabled, {} disabled",
        report.enabled.len(),
        report.disabled.len()
    );
    Ok(host)
}

fn run_validate_assets_command(pack: PathBuf) -> ExitCode {
    match hytale_server::asset_validation::validate_assets(&pack) {
        Ok(report) => {
            let valid = hytale_server::asset_validation::print_report(&pack, &report);
            if valid {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("asset validation failed: {error}");
            ExitCode::FAILURE
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

fn run_serve_command(args: ServeArgs, builtin: BuiltinRuntime) -> ExitCode {
    let auth = match args.auth_mode.as_str() {
        "insecure" => AuthMode::Insecure,
        "local" => {
            let key = match args.auth_key.as_deref() {
                Some(encoded) => match AuthKey::from_pkcs8_base64(encoded) {
                    Ok(key) => key,
                    Err(error) => {
                        eprintln!("invalid --auth-key: {error}");
                        return ExitCode::FAILURE;
                    }
                },
                None => match AuthKey::generate() {
                    Ok(key) => {
                        println!(
                            "generated local auth key (pkcs8 base64): {}",
                            key.pkcs8_base64()
                        );
                        println!("local auth key id: {}", key.key_id());
                        key
                    }
                    Err(error) => {
                        eprintln!("could not generate an auth key: {error}");
                        return ExitCode::FAILURE;
                    }
                },
            };
            AuthMode::local(AuthService::new(
                key,
                args.auth_issuer.clone(),
                args.auth_audience.clone(),
            ))
        }
        "authenticated" => {
            let config = LiveAuthConfig::default();
            let session_token = std::env::var("HYTALE_SERVER_SESSION_TOKEN").ok();
            let identity_token = std::env::var("HYTALE_SERVER_IDENTITY_TOKEN").ok();
            let live = match (session_token, identity_token) {
                (Some(session_token), Some(identity_token)) => {
                    println!("using Hytale session tokens from the environment");
                    LiveAuth::from_tokens(config, session_token, identity_token)
                }
                _ => LiveAuth::load_or_connect(config),
            };
            match live {
                Ok(live) => AuthMode::authenticated(live),
                Err(error) => {
                    eprintln!("live authentication failed: {error}");
                    return ExitCode::FAILURE;
                }
            }
        }
        other => {
            eprintln!(
                "unknown --auth-mode `{other}` (expected `insecure`, `local` or `authenticated`)"
            );
            return ExitCode::FAILURE;
        }
    };

    let mut permissions = PermissionRegistry::new();
    for operator in &args.op {
        permissions.grant_operator(operator);
    }
    let session = SessionOptions {
        server_name: args.server_name,
        motd: args.motd,
        max_players: args.max_players,
        default_view_radius: args.view_radius,
        chunk_compression_level: args.compression_level,
        handshake_timeout: std::time::Duration::from_secs(args.handshake_timeout),
        world_join_timeout: std::time::Duration::from_secs(args.world_join_timeout),
        idle_timeout: std::time::Duration::from_secs(args.idle_timeout),
        auth,
        limits: SessionLimits {
            max_inbound_packet_bytes: args.max_packet_bytes,
            inbound_packet_burst: args.packet_burst,
            inbound_packets_per_second: args.max_packets_per_second,
            max_view_radius: args.max_view_radius,
            max_queued_messages: args.max_queued_messages,
        },
        block_break_damage: 1.0,
        trace: None,
        portal_router: None,
        portal_on_join: None,
        stop_signal: None,
        permissions: Arc::new(permissions),
    };
    let console = args.console || (!args.no_console && std::io::stdin().is_terminal());
    let options = ServeOptions {
        universe_root: args.universe,
        world: args.world,
        generator: GeneratorOptions {
            assets_pack: args.assets_pack,
            assets_root: args.assets_root,
            world_dir: args.world_data,
            workers: args.workers,
        },
        bind_addr: args.bind,
        max_connections: args.max_connections,
        view_radius: args.view_radius,
        chunks_per_tick: args.chunks_per_tick,
        session,
        spawn_npc: args.spawn_npc,
        auto_population: args.auto_population,
        builtin,
        shutdown_timeout: std::time::Duration::from_secs(args.shutdown_timeout),
        close_timeout: std::time::Duration::from_secs(args.close_timeout),
        trace_path: args.trace,
        console,
        bound_tx: None,
    };
    match run_serve(options) {
        Ok(report) => {
            println!(
                "serve stopped: world {} ({}) on {}",
                report.world_name, report.world_uuid, report.bound_addr
            );
            println!(
                "sessions={} rejected={} peak={} saved_chunks={}",
                report.sessions.len(),
                report.rejected,
                report.peak_connections,
                report.saved_chunks
            );
            for session in &report.sessions {
                println!(
                    "  {} auth={} chunks={} unloaded={} disconnected={} idle_timeout={} rate_limited={} queue_overflow={}",
                    session.username,
                    session.auth_mode,
                    session.chunks,
                    session.unloaded,
                    session.disconnected,
                    session.idle_timeout,
                    session.rate_limited,
                    session.queue_overflow
                );
            }
            for error in &report.errors {
                eprintln!("session error: {error}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("serve failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_chunk_serve_command(args: ChunkServeArgs, builtin: BuiltinRuntime) -> ExitCode {
    let auth = match args.auth_mode.as_str() {
        "insecure" => AuthMode::Insecure,
        "local" => {
            let key = match args.auth_key.as_deref() {
                Some(encoded) => match AuthKey::from_pkcs8_base64(encoded) {
                    Ok(key) => key,
                    Err(error) => {
                        eprintln!("invalid --auth-key: {error}");
                        return ExitCode::FAILURE;
                    }
                },
                None => match AuthKey::generate() {
                    Ok(key) => {
                        println!(
                            "generated local auth key (pkcs8 base64): {}",
                            key.pkcs8_base64()
                        );
                        println!("local auth key id: {}", key.key_id());
                        key
                    }
                    Err(error) => {
                        eprintln!("could not generate an auth key: {error}");
                        return ExitCode::FAILURE;
                    }
                },
            };
            AuthMode::local(AuthService::new(
                key,
                args.auth_issuer.clone(),
                args.auth_audience.clone(),
            ))
        }
        other => {
            eprintln!("unknown --auth-mode `{other}` (expected `insecure` or `local`)");
            return ExitCode::FAILURE;
        }
    };
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
            spawn_search: true,
            chunks_per_tick: args.chunks_per_tick,
            builtin,
        },
        bind_addr: args.bind,
        center: None,
        compression_level: args.compression_level,
        move_demo: args.move_demo,
        handshake: args.handshake,
        session: SessionOptions {
            server_name: args.server_name,
            motd: args.motd,
            max_players: args.max_players,
            default_view_radius: args.view_radius,
            chunk_compression_level: args.compression_level,
            handshake_timeout: std::time::Duration::from_secs(args.handshake_timeout),
            world_join_timeout: std::time::Duration::from_secs(args.world_join_timeout),
            idle_timeout: std::time::Duration::from_secs(args.idle_timeout),
            auth,
            limits: SessionLimits::default(),
            block_break_damage: 1.0,
            trace: None,
            portal_router: None,
            portal_on_join: None,
            stop_signal: None,
            permissions: Arc::new(PermissionRegistry::permissive()),
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
                    "handshake ok: {} view_radius={} auth={} chunks={}",
                    session.username, session.view_radius, session.auth_mode, session.stream.chunks
                );
                println!(
                    "assets: required={} requested={} sent={} bytes={} parts={} config={}",
                    session.assets.required,
                    session.assets.requested,
                    session.assets.sent,
                    session.assets.bytes,
                    session.assets.parts,
                    session.assets.config_packets
                );
                println!(
                    "game: clear_chunks={} join_world={} player_list={} self_spawn={} gameplay={} time={} ready_chunks={} ready_gameplay={}",
                    session.game.clear_chunks,
                    session.game.join_world,
                    session.game.player_list,
                    session.game.self_spawn,
                    session.game.gameplay_config,
                    session.game.time,
                    session.game.client_ready_for_chunks,
                    session.game.client_ready_for_gameplay
                );
                println!(
                    "game loop: teleport={} acked={} movement={} view_changes={} loop_chunks={}",
                    session.game_loop.teleport_sent,
                    session.game_loop.teleport_acked,
                    session.game_loop.movement_packets,
                    session.game_loop.view_changes,
                    session.game_loop.chunks
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

fn run_universe_command(args: UniverseArgs) -> ExitCode {
    use hytale_world::universe::Universe;

    let mut universe = match Universe::open(&args.root) {
        Ok(universe) => universe,
        Err(error) => {
            eprintln!("universe open failed: {error}");
            return ExitCode::FAILURE;
        }
    };
    let result = match args.command {
        UniverseCommand::Create {
            name,
            seed,
            provider,
        } => universe
            .create_world(&name, seed, provider)
            .map(|descriptor| {
                println!(
                    "created {} (uuid={} seed={} provider={} path={})",
                    descriptor.name,
                    descriptor.uuid,
                    descriptor.seed,
                    descriptor.provider,
                    descriptor.save_dir.display()
                );
            }),
        UniverseCommand::List => {
            let default = universe.default_world().map(|world| world.uuid);
            for descriptor in universe.list_worlds() {
                let marker = if Some(descriptor.uuid) == default {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{marker} {} uuid={} seed={} provider={} path={}",
                    descriptor.name,
                    descriptor.uuid,
                    descriptor.seed,
                    descriptor.provider,
                    descriptor.save_dir.display()
                );
            }
            println!(
                "{} world(s) in {}",
                universe.list_worlds().len(),
                args.root.display()
            );
            Ok(())
        }
        UniverseCommand::Show { name } => universe.resolve(&name).map(|descriptor| {
            println!("name: {}", descriptor.name);
            println!("uuid: {}", descriptor.uuid);
            println!("seed: {}", descriptor.seed);
            println!("provider: {}", descriptor.provider);
            println!("save_dir: {}", descriptor.save_dir.display());
            println!("config: {}", descriptor.config_path().display());
        }),
        UniverseCommand::Save { name } => universe.save_world(&name).map(|()| {
            println!("saved config for `{name}`");
        }),
        UniverseCommand::Delete { name, keep_files } => {
            let operation = if keep_files {
                universe.unload_world(&name)
            } else {
                universe.delete_world(&name)
            };
            operation.map(|()| {
                println!(
                    "{} `{name}`",
                    if keep_files {
                        "removed from catalog"
                    } else {
                        "deleted"
                    }
                );
            })
        }
        UniverseCommand::Default { name } => universe.set_default_world(&name).map(|()| {
            println!("default world is `{name}`");
        }),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("universe command failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_world_boot_command(args: WorldBootArgs, builtin: BuiltinRuntime) -> ExitCode {
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
        spawn_npc: args.spawn_npc,
        auto_population: args.auto_population,
        builtin,
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
            if let Some((role, id)) = report.spawned_npc {
                println!("spawned NPC `{role}` as runtime entity {id}");
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
