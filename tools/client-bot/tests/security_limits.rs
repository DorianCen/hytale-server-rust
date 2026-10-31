//! Abuse-path loopback tests for T62.
//!
//! The server must fail closed when a client floods frames or declares a
//! payload above the session ceiling.
//!
//! Needs the base-game pack and loopback UDP, so both tests are `#[ignore]`d.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{
    build_connect, build_connect_frame, build_frame, decode_connection_packet, drain_frames,
    DEFAULT_PROTOCOL_BUILD, DEFAULT_PROTOCOL_CRC,
};
use hytale_net::{connect, FrameStreamError, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::connection::{InsecurePlayerOptions, Ping, ServerDisconnect};
use hytale_protocol::generated::setup::RequestAssets;
use hytale_protocol::generated::Packet;
use hytale_protocol::io::{ProtocolError, Uuid};
use hytale_server::{
    serve_session_connection, GeneratorOptions, SessionError, SessionLimits, SessionOptions,
    SessionReport, WorldThread, WorldThreadOptions,
};
use tokio::task::JoinHandle;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        return false;
    }
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-security-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn spawn_server(
    limits: SessionLimits,
) -> (SocketAddr, JoinHandle<Result<SessionReport, SessionError>>) {
    let save_dir = temp_dir("save");
    let (thread, _start) = WorldThread::spawn(WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    ))
    .expect("world thread");

    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server = ServerEndpointBuilder::new(
        bind_addr,
        TlsIdentity::generate("localhost").expect("server certificate"),
    )
    .bind()
    .expect("bind server");
    let server_addr = server.local_addr();
    let task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let result = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(30),
                limits,
                ..SessionOptions::default()
            },
        )
        .await;
        let _ = tokio::time::timeout(Duration::from_secs(5), accepted.connection().closed()).await;
        thread.shutdown().expect("world shutdown");
        server.close();
        std::fs::remove_dir_all(save_dir).ok();
        result
    });
    (server_addr, task)
}

async fn connect_client(server_addr: SocketAddr) -> hytale_net::ClientSession {
    let identity = TlsIdentity::generate("security-bot").expect("client cert");
    connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &identity,
    )
    .await
    .expect("connect")
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn an_inbound_frame_flood_is_disconnected() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let (server_addr, server_task) = spawn_server(SessionLimits {
        inbound_packet_burst: 2,
        inbound_packets_per_second: 1,
        ..SessionLimits::default()
    });
    let client = connect_client(server_addr).await;
    let (mut send, mut recv) = client.connection().open_bi().await.expect("stream");

    let connect = build_connect(
        DEFAULT_PROTOCOL_CRC,
        DEFAULT_PROTOCOL_BUILD,
        "2026.01.01",
        "en-US",
        None,
    );
    send.write_all(&build_connect_frame(&connect).expect("connect frame"))
        .await
        .expect("send connect");
    let options = InsecurePlayerOptions {
        uuid: Uuid::new(7, 7),
        username: "FloodBot".to_string(),
        skin: None,
    };
    send.write_all(&build_frame(&options).expect("options frame"))
        .await
        .expect("send options");
    for _ in 0..10 {
        send.write_all(&build_frame(&Ping::default()).expect("ping frame"))
            .await
            .expect("send ping");
    }
    send.finish().expect("finish");

    let reply = recv.read_to_end(8 * 1024 * 1024).await.expect("read reply");
    let mut buffer = reply;
    let records = drain_frames(&mut buffer).expect("frames");
    let disconnect = records
        .iter()
        .find(|record| record.packet_id == ServerDisconnect::ID)
        .unwrap_or_else(|| panic!("a server disconnect: {records:?}"));
    let description = decode_connection_packet(disconnect.packet_id, &disconnect.payload)
        .expect("known connection packet")
        .expect("decodable disconnect");
    assert!(
        description.contains("packet rate limit exceeded"),
        "unexpected disconnect: {description}"
    );

    client.connection().close(0u32.into(), b"rate test done");
    let error = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert!(matches!(
        error,
        Err(SessionError::RateLimited { phase: "setup" })
    ));
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn an_oversized_inbound_frame_is_rejected_before_allocation() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let limits = SessionLimits {
        max_inbound_packet_bytes: 1024,
        ..SessionLimits::default()
    };
    let (server_addr, server_task) = spawn_server(limits);
    let client = connect_client(server_addr).await;
    let (mut send, mut recv) = client.connection().open_bi().await.expect("stream");

    let connect = build_connect(
        DEFAULT_PROTOCOL_CRC,
        DEFAULT_PROTOCOL_BUILD,
        "2026.01.01",
        "en-US",
        None,
    );
    send.write_all(&build_connect_frame(&connect).expect("connect frame"))
        .await
        .expect("send connect");
    let mut header = [0u8; 8];
    header[..4].copy_from_slice(&2048u32.to_le_bytes());
    header[4..].copy_from_slice(&RequestAssets::ID.to_le_bytes());
    send.write_all(&header).await.expect("send header");
    send.finish().expect("finish");

    let _ = recv.read_to_end(1024).await;
    client.connection().close(0u32.into(), b"size test done");
    let error = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    match error {
        Err(SessionError::Frame(FrameStreamError::Protocol(ProtocolError::PayloadTooLarge {
            packet_id,
            max,
            ..
        }))) => {
            assert_eq!(packet_id, RequestAssets::ID);
            assert_eq!(max, 1024);
        }
        other => panic!("expected an oversized-frame rejection, got {other:?}"),
    }
}
