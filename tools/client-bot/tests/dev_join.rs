//! Loopback test for the dev/insecure join handshake and chunk stream.
//!
//! Needs the base-game pack (the world thread generates a chunk) and loopback
//! UDP, so it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test dev_join -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::io::Uuid;
use hytale_server::{
    serve_session_connection, GeneratorOptions, SessionOptions, WorldThread, WorldThreadOptions,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_dir(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-dev-join-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn the_bot_completes_the_dev_handshake_and_receives_chunks() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("join");
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
    let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
    let server = ServerEndpointBuilder::new(bind_addr, server_identity)
        .bind()
        .expect("bind server");
    let server_addr = server.local_addr();
    let client_identity = TlsIdentity::generate("dev-join-client").expect("client cert");

    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let report = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(30),
                ..SessionOptions::default()
            },
        )
        .await
        .expect("handshake");
        let _ = tokio::time::timeout(Duration::from_secs(5), accepted.connection().closed()).await;
        thread.shutdown().expect("shutdown");
        server.close();
        report
    });

    let session = connect(bind_addr, server_addr, "localhost", &client_identity)
        .await
        .expect("connect");
    let report = dev_join(
        &session,
        &DevJoinOptions {
            uuid: Uuid::new(1, 2),
            username: "TestBot".to_string(),
            view_radius_chunks: 0,
        },
    )
    .await
    .expect("dev join");

    assert!(report.handshake_packets.contains(&364)); // RequestInsecurePlayerOptions
    assert!(report.handshake_packets.contains(&14)); // ConnectAccept
    assert!(report.handshake_packets.contains(&20)); // WorldSettings
    assert!(report.handshake_packets.contains(&22)); // WorldLoadFinished
    assert_eq!(report.chunks.len(), 21);

    session
        .connection()
        .close(0u32.into(), b"dev join test complete");
    let server_report = server_task.await.expect("server task");
    assert_eq!(server_report.username, "TestBot");
    assert_eq!(server_report.view_radius, 0);
    assert_eq!(server_report.stream.chunks, 1);

    std::fs::remove_dir_all(save_dir).ok();
}
