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
    build_connect, build_connect_frame, drain_frames, hex, DEFAULT_PROTOCOL_BUILD,
    DEFAULT_PROTOCOL_CRC,
};
use hytale_net::{ClientEndpoint, TlsIdentity};
use hytale_protocol::generated::wire::Packet;
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
    /// Seconds to wait for a reply before exiting successfully.
    #[arg(long, default_value_t = 5)]
    timeout: u64,
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

    let (mut send, mut recv) = session.connection().open_bi().await?;
    send.write_all(&frame).await?;
    send.finish()?;

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
