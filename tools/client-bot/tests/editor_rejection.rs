//! Editor-client rejection loopback test.
//!
//! T61 keeps editor/AssetEditor support out of scope, so an editor `Connect`
//! must be refused before any auth mode (including insecure) can accept it.
//!
//! Needs the base-game pack (the world thread builds its config packets) and
//! loopback UDP, so it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test editor_rejection -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{
    build_connect, build_connect_frame, decode_connection_packet, drain_frames,
    DEFAULT_PROTOCOL_BUILD, DEFAULT_PROTOCOL_CRC,
};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::common::ClientType;
use hytale_server::{
    serve_session_connection, GeneratorOptions, SessionError, SessionOptions, WorldThread,
    WorldThreadOptions,
};

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
        "hytale-editor-rejection-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn editor_clients_are_refused_before_asset_setup() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

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

    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let error = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(30),
                ..SessionOptions::default()
            },
        )
        .await
        .expect_err("an editor connection must be refused");
        let _ = tokio::time::timeout(Duration::from_secs(5), accepted.connection().closed()).await;
        thread.shutdown().expect("world shutdown");
        server.close();
        error
    });

    let identity = TlsIdentity::generate("editor-bot").expect("client cert");
    let client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &identity,
    )
    .await
    .expect("connect");
    let (mut send, mut recv) = client.connection().open_bi().await.expect("stream");

    let mut connect = build_connect(
        DEFAULT_PROTOCOL_CRC,
        DEFAULT_PROTOCOL_BUILD,
        "2026.01.01",
        "en-US",
        None,
    );
    connect.client_type = ClientType::Editor;
    send.write_all(&build_connect_frame(&connect).expect("connect frame"))
        .await
        .expect("send connect");
    send.finish().expect("finish");

    let reply = recv.read_to_end(4096).await.expect("read disconnect");
    let mut buffer = reply;
    let records = drain_frames(&mut buffer).expect("frames");
    assert_eq!(records.len(), 1, "only the disconnect is sent: {records:?}");
    assert_eq!(records[0].packet_id, 2, "ServerDisconnect");
    let description = decode_connection_packet(records[0].packet_id, &records[0].payload)
        .expect("known connection packet")
        .expect("decodable disconnect");
    assert!(
        description.contains("editor connections are not supported"),
        "unexpected disconnect: {description}"
    );

    client.connection().close(0u32.into(), b"editor test done");
    let error = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert!(matches!(
        error,
        SessionError::UnexpectedPacket {
            packet_id: 0,
            state: "auth:editor"
        }
    ));

    std::fs::remove_dir_all(save_dir).ok();
}
