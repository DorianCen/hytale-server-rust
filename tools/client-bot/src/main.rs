//! Minimal scripted Hytale client.
//!
//! ```sh
//! cargo run -p client-bot -- --host 127.0.0.1:5520
//! cargo run -p client-bot -- --dry-run
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use client_bot::{
    auth_join, build_connect, build_connect_frame, dev_join, drain_frames, hex,
    receive_chunk_stream, AuthJoinOptions, DevJoinOptions, DEFAULT_PROTOCOL_BUILD,
    DEFAULT_PROTOCOL_CRC,
};
use hytale_auth::{AuthKey, AuthService};
use hytale_net::{ClientEndpoint, TlsIdentity};
use hytale_protocol::generated::wire::Packet;
use hytale_protocol::io::Uuid;
use tokio::time::timeout;

#[derive(Debug, Parser)]
#[command(name = "client-bot", version, about)]
struct Cli {
    /// Server address.
    #[arg(long, default_value = "127.0.0.1:5520")]
    host: SocketAddr,
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

    let identity = TlsIdentity::generate("hytale-client-bot")?;
    let endpoint = ClientEndpoint::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)), &identity)?;
    let session = endpoint.connect(cli.host, &cli.server_name).await?;

    if let Some(peer) = session.peer_identity() {
        println!("server certificate sha256: {}", peer.fingerprint_hex());
    }

    if cli.dev_join {
        let report = dev_join(
            &session,
            &DevJoinOptions {
                uuid: Uuid::new(
                    0x1234_5678_1234_5678u64 as i64,
                    0x9abc_def0_1234_5678u64 as i64,
                ),
                username: cli.username.clone(),
                view_radius_chunks: cli.view_radius,
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
