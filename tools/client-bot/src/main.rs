//! Minimal scripted Hytale client.
//!
//! ```sh
//! cargo run -p client-bot -- --host 127.0.0.1:5520
//! cargo run -p client-bot -- --dry-run
//! ```

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Parser;
use client_bot::load::{run_load, LoadMemoryReport, LoadOptions, ServerLoadReport};
use client_bot::{
    auth_join, build_connect, build_connect_frame, dev_join, drain_frames, hex,
    receive_chunk_stream, AuthJoinOptions, DevJoinOptions, DEFAULT_PROTOCOL_BUILD,
    DEFAULT_PROTOCOL_CRC,
};
use hytale_auth::{AuthKey, AuthService};
use hytale_net::{ClientEndpoint, TlsIdentity};
use hytale_protocol::generated::wire::Packet;
use hytale_protocol::io::Uuid;
use hytale_server::{serve_listener, GeneratorOptions, ServeOptions, SessionOptions};
use tokio::sync::oneshot;
use tokio::time::timeout;

#[derive(Debug, Parser)]
#[command(name = "client-bot", version, about)]
struct Cli {
    /// Server address (hostname or IP; resolved like the game client does).
    #[arg(long, default_value = "127.0.0.1:5520")]
    host: String,
    /// SNI name used for the server certificate check.
    #[arg(long, default_value = "localhost")]
    server_name: String,
    /// `ProtocolSettings.PROTOCOL_CRC`.
    #[arg(long, default_value_t = DEFAULT_PROTOCOL_CRC)]
    protocol_crc: i32,
    /// `ProtocolSettings.PROTOCOL_BUILD_NUMBER`.
    #[arg(long, default_value_t = DEFAULT_PROTOCOL_BUILD)]
    build: i32,
    /// Client version string written into the fixed 20-byte slot.
    #[arg(long, default_value = "2026.01.01")]
    client_version: String,
    /// Client language.
    #[arg(long, default_value = "en-US")]
    language: String,
    /// Optional identity token.
    #[arg(long)]
    token: Option<String>,
    /// Print the Connect payload/frame without connecting.
    #[arg(long)]
    dry_run: bool,
    /// After Connect, read the server's `Chunks` stream and summarize it.
    #[arg(long)]
    chunks: bool,
    /// Run the dev/insecure Connect → Setup handshake, then receive chunks.
    #[arg(long)]
    dev_join: bool,
    /// Send `ClientReady(readyForGameplay)` after the dev join, starting the
    /// teleport/time flow (the game client always sends this).
    #[arg(long)]
    ready_for_gameplay: bool,
    /// Run the authenticated Connect → AuthGrant → setup flow, then chunks.
    #[arg(long)]
    auth_join: bool,
    /// Base64 PKCS#8 Ed25519 key shared with the server (required for `--auth-join`).
    #[arg(long)]
    auth_key: Option<String>,
    /// JWT issuer used by `--auth-join`.
    #[arg(long, default_value = "rust-local")]
    auth_issuer: String,
    /// JWT audience used by `--auth-join`.
    #[arg(long, default_value = "rust-server")]
    auth_audience: String,
    /// Number of advertised common assets to request (default: all).
    #[arg(long)]
    asset_limit: Option<usize>,
    /// Move to this chunk after the teleport ack to exercise a view update.
    #[arg(long, value_parser = parse_chunk)]
    move_chunk: Option<(i32, i32)>,
    /// Username for the dev/insecure `InsecurePlayerOptions`.
    #[arg(long, default_value = "RustBot")]
    username: String,
    /// Requested view radius, in chunks, for the dev join.
    #[arg(long, default_value_t = 1)]
    view_radius: u32,
    /// Seconds to wait for a reply before exiting successfully.
    #[arg(long, default_value_t = 5)]
    timeout: u64,
    /// Seconds to keep observing the game stream after the dev join.
    #[arg(long, default_value_t = 0)]
    observe: u64,
    /// Run the concurrent load harness against an embedded listener.
    #[arg(long)]
    load: bool,
    /// Number of load bots (10/50/100 presets, or any value).
    #[arg(long, default_value_t = 10)]
    players: usize,
    /// Delay between consecutive load-bot joins, in milliseconds.
    #[arg(long, default_value_t = 25)]
    stagger_ms: u64,
    /// Post-join move/chat/edit/save window for each load bot, in seconds.
    #[arg(long, default_value_t = 2)]
    load_window: u64,
    /// JSON report path (default: `target/perf/load-<players>.json`).
    #[arg(long)]
    report: Option<PathBuf>,
    /// Run the load bots against `--host` instead of an embedded listener.
    #[arg(long)]
    external: bool,
    /// Subscribe the load run to a fresh universe root at this path.
    #[arg(long)]
    universe: Option<PathBuf>,
    /// Asset pack used by the embedded listener.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Assets/Assets.zip"))]
    assets_pack: PathBuf,
    /// Extracted asset root used by the embedded listener.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Assets/extracted"))]
    assets_root: PathBuf,
    /// World template used by the embedded listener.
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/Default"))]
    world_dir: PathBuf,
}

fn parse_chunk(value: &str) -> Result<(i32, i32), String> {
    let (x, z) = value
        .split_once(',')
        .ok_or_else(|| "expected `x,z`".to_string())?;
    Ok((
        x.trim()
            .parse()
            .map_err(|_| format!("invalid chunk x: {x}"))?,
        z.trim()
            .parse()
            .map_err(|_| format!("invalid chunk z: {z}"))?,
    ))
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("client-bot: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let connect = build_connect(
        cli.protocol_crc,
        cli.build,
        &cli.client_version,
        &cli.language,
        cli.token.clone(),
    );
    let frame = build_connect_frame(&connect)?;

    if cli.dry_run {
        println!(
            "packet id: {}",
            hytale_protocol::generated::connection::Connect::ID
        );
        println!("payload ({} bytes): {}", frame.len() - 8, hex(&frame[8..]));
        println!("frame ({} bytes): {}", frame.len(), hex(&frame));
        return Ok(());
    }

    if cli.load {
        run_load_mode(&cli).await?;
        return Ok(());
    }

    // Resolve like the game client: the first address wins, which is `::1`
    // before `127.0.0.1` for `localhost` on many systems.
    let host = cli
        .host
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| format!("{} did not resolve to an address", cli.host))?;
    let identity = TlsIdentity::generate("hytale-client-bot")?;
    // Bind the client socket to the same family as the server address; an
    // IPv4-bound client endpoint cannot dial an IPv6 server.
    let bind_addr = match host {
        SocketAddr::V4(_) => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        SocketAddr::V6(_) => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
    };
    let endpoint = ClientEndpoint::bind(bind_addr, &identity)?;
    let session = endpoint.connect(host, &cli.server_name).await?;

    if let Some(peer) = session.peer_identity() {
        println!("server certificate sha256: {}", peer.fingerprint_hex());
    }

    if cli.dev_join {
        let report = dev_join(
            &session,
            &DevJoinOptions {
                commands_after_join: Vec::new(),
                uuid: Uuid::new(
                    0x1234_5678_1234_5678u64 as i64,
                    0x9abc_def0_1234_5678u64 as i64,
                ),
                username: cli.username.clone(),
                view_radius_chunks: cli.view_radius,
                skin: None,
                movement_after_join: None,
                movement_after_edits: None,
                observe_after_join: (cli.observe > 0).then(|| Duration::from_secs(cli.observe)),
                disconnect_after: None,
                keep_alive: false,
                use_block_after_join: Vec::new(),
                send_ready_for_gameplay: cli.ready_for_gameplay,
                place_block_after_join: None,
                break_block_after_join: None,
                arg_value_requests_after_join: Vec::new(),
                place_mode_after_join: None,
                interaction_chains_after_join: Vec::new(),
            },
        )
        .await?;
        println!("handshake packets: {:?}", report.handshake_packets);
        println!("game: {:?}", report.game);
        println!("chunk packets: {}", report.chunks.len());
        for packet in &report.chunks {
            println!("{packet:?}");
        }
        session
            .connection()
            .close(0u32.into(), b"client-bot dev join complete");
        return Ok(());
    }

    if cli.auth_join {
        let encoded = cli
            .auth_key
            .as_deref()
            .ok_or("--auth-join needs --auth-key with the server's PKCS#8 Ed25519 key")?;
        let key = AuthKey::from_pkcs8_base64(encoded)?;
        let auth = AuthService::new(key, cli.auth_issuer.clone(), cli.auth_audience.clone());
        let report = auth_join(
            &session,
            &AuthJoinOptions {
                uuid: Uuid::new(
                    0x1234_5678_1234_5678u64 as i64,
                    0x9abc_def0_1234_5678u64 as i64,
                ),
                username: cli.username.clone(),
                view_radius_chunks: cli.view_radius,
                auth,
                certificate_fingerprint: identity.fingerprint_base64url(),
                asset_limit: cli.asset_limit,
                movement_chunk: cli.move_chunk,
                observe_time: None,
            },
        )
        .await?;
        println!("handshake packets: {:?}", report.handshake_packets);
        println!(
            "auth: grant={} server_token={}",
            report.auth_grant, report.server_auth_token
        );
        println!(
            "assets: advertised={} requested={} received={} parts={} bytes={}",
            report.advertised_assets,
            report.requested_assets,
            report.assets.len(),
            report.asset_parts,
            report.asset_bytes
        );
        for asset in &report.assets {
            println!("  {} {} {} bytes", asset.hash, asset.name, asset.bytes);
        }
        println!(
            "config packets: {:?} translations={} rebuild={}",
            report.config_packets, report.translations, report.rebuild_requested
        );
        println!("game: {:?}", report.game);
        println!("teleport: {}", report.teleport);
        println!("chunk packets: {}", report.chunks.len());
        for packet in &report.chunks {
            println!("{packet:?}");
        }
        if !report.view_update_chunks.is_empty() {
            println!("view update packets: {}", report.view_update_chunks.len());
            for packet in &report.view_update_chunks {
                println!("{packet:?}");
            }
        }
        session
            .connection()
            .close(0u32.into(), b"client-bot auth join complete");
        return Ok(());
    }

    let (mut send, mut recv) = session.connection().open_bi().await?;
    send.write_all(&frame).await?;
    send.finish()?;

    if cli.chunks {
        let received = timeout(
            Duration::from_secs(cli.timeout),
            receive_chunk_stream(&session),
        )
        .await;
        match received {
            Ok(Ok(packets)) => {
                println!("chunk packets: {}", packets.len());
                for packet in &packets {
                    println!("{packet:?}");
                }
            }
            Ok(Err(error)) => {
                session
                    .connection()
                    .close(0u32.into(), b"chunk stream error");
                return Err(error.into());
            }
            Err(_) => println!("chunk stream timeout after {}s", cli.timeout),
        }
        session
            .connection()
            .close(0u32.into(), b"client-bot chunks complete");
        return Ok(());
    }

    let mut buffer = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    let mut disconnected = false;
    loop {
        let read = timeout(Duration::from_secs(cli.timeout), recv.read(&mut chunk)).await;
        let count = match read {
            Ok(Ok(Some(count))) => count,
            Ok(Ok(None)) => break,
            Ok(Err(error)) => {
                // A server-side close after the disconnect packet is clean.
                if disconnected {
                    break;
                }
                return Err(error.into());
            }
            Err(_) => {
                println!("timeout after {}s", cli.timeout);
                break;
            }
        };
        if count == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..count]);
        for record in drain_frames(&mut buffer)? {
            let name = record.name().unwrap_or("unknown");
            println!("{} {} {}", record.packet_id, record.payload.len(), name);
            if let Some(description) = record.describe() {
                println!("  {description}");
            }
            if record.packet_id == 2 {
                disconnected = true;
            }
        }
        if disconnected {
            break;
        }
    }

    session
        .connection()
        .close(0u32.into(), b"client-bot complete");
    Ok(())
}

async fn run_load_mode(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    if cli.players == 0 {
        return Err("--players must be greater than zero".into());
    }
    let report_path = cli
        .report
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("target/perf/load-{}.json", cli.players)));
    let options = LoadOptions {
        players: cli.players,
        stagger: Duration::from_millis(cli.stagger_ms),
        workload_window: Duration::from_secs(cli.load_window),
        ..LoadOptions::default()
    };

    if cli.external {
        let host = cli
            .host
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| format!("{} did not resolve to an address", cli.host))?;
        let mut report = run_load(host, &cli.server_name, &options).await;
        finish_load_report(&mut report, &report_path)?;
        return Ok(());
    }

    let universe_root = cli.universe.clone().unwrap_or_else(|| {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "hytale-load-universe-{}-{nonce}",
            std::process::id()
        ))
    });
    let (bound_tx, bound_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut serve_options = ServeOptions::new(
        &universe_root,
        GeneratorOptions {
            assets_pack: cli.assets_pack.clone(),
            assets_root: cli.assets_root.clone(),
            world_dir: cli.world_dir.clone(),
            workers: 1,
        },
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        SessionOptions {
            // The session registry enforces `max_players`, not only the
            // listener semaphore, so size it for the run.
            max_players: i32::try_from(cli.players + 4).unwrap_or(i32::MAX),
            default_view_radius: cli.view_radius,
            handshake_timeout: Duration::from_secs(60),
            world_join_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            ..SessionOptions::default()
        },
    );
    serve_options.view_radius = cli.view_radius;
    serve_options.max_connections = cli.players + 4;
    serve_options.bound_tx = Some(bound_tx);
    let server_task = tokio::spawn(serve_listener(serve_options, async move {
        let _ = shutdown_rx.await;
    }));
    let server_addr = timeout(Duration::from_secs(180), bound_rx)
        .await
        .map_err(|_| "embedded listener did not bind within 180s")?
        .map_err(|_| "embedded listener dropped its bound-address sender")?;
    println!("load listener bound on {server_addr}");

    let started = Instant::now();
    let mut report = run_load(server_addr, "localhost", &options).await;
    let elapsed = started.elapsed();
    let _ = shutdown_tx.send(());
    let serve_report = timeout(Duration::from_secs(120), server_task)
        .await
        .map_err(|_| "embedded listener did not shut down within 120s")???;
    report.server = Some(ServerLoadReport::from_serve(&serve_report, elapsed));
    report.memory = LoadMemoryReport {
        resident_bytes: hytale_server::resident_bytes(),
        peak_resident_bytes: hytale_server::peak_resident_bytes(),
    };
    if cli.universe.is_none() {
        std::fs::remove_dir_all(&universe_root).ok();
    }
    finish_load_report(&mut report, &report_path)?;
    Ok(())
}

fn finish_load_report(
    report: &mut client_bot::load::LoadReport,
    report_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", report.summary());
    if let Some(server) = &report.server {
        println!(
            "server accepted={} rejected={} teardown={} unexpected_errors={} peak={} \
             chunk_packets={} chunk_frames={} chunk_bytes={} edits={} rejects={} \
             queue_overflow={} rate_limited={} idle_timeout={} ticks={} tick_rate={:.1}/s \
             saved_chunks={}",
            server.accepted,
            server.rejected,
            server.client_teardown_errors,
            server.unexpected_errors.len(),
            server.peak_connections,
            server.chunk_packets,
            server.chunk_frames,
            server.chunk_payload_bytes,
            server.block_edits,
            server.block_edit_rejections,
            server.queue_overflows,
            server.rate_limited,
            server.idle_timeouts,
            server.simulation_ticks,
            server.effective_tick_rate,
            server.saved_chunks,
        );
        if !server.errors.is_empty() {
            println!("server errors: {:?}", server.errors);
        }
    }
    if let Some(resident) = report.memory.resident_bytes {
        println!(
            "rss resident={:.1} MiB peak={:.1} MiB",
            resident as f64 / (1024.0 * 1024.0),
            report.memory.peak_resident_bytes.unwrap_or(resident) as f64 / (1024.0 * 1024.0),
        );
    }
    report.write_json(report_path)?;
    println!("report: {}", report_path.display());
    Ok(())
}
