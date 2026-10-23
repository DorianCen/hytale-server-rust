//! End-to-end loopback: world thread → packet batch → QUIC chunk stream.
//!
//! This is the closest thing to the join chunk stream until the auth/session
//! state machine lands. It needs the base-game pack and binds loopback UDP, so
//! it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p hytale-server --test chunk_serve -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use hytale_net::{connect, ChunkStreamReader, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::wire::Packet;
use hytale_protocol::generated::world::{SetChunk, SetChunkEnvironments, SetColumn};
use hytale_server::{serve_chunk_connection, GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::chunk::stream::DEFAULT_COMPRESSION_LEVEL;

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
        "hytale-chunk-serve-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn the_server_streams_a_generated_chunk_to_a_client() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("serve");
    let options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, start) = WorldThread::spawn(options).expect("world thread");

    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
    let server = ServerEndpointBuilder::new(bind_addr, server_identity)
        .bind()
        .expect("bind server");
    let server_addr = server.local_addr();
    let client_identity = TlsIdentity::generate("chunk-serve-client").expect("client cert");

    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let report = serve_chunk_connection(
            &thread,
            accepted.connection(),
            start.player_chunk,
            0,
            DEFAULT_COMPRESSION_LEVEL,
        )
        .await
        .expect("serve the chunk");
        let _ = tokio::time::timeout(Duration::from_secs(5), accepted.connection().closed()).await;
        thread.shutdown().expect("shutdown");
        server.close();
        report
    });

    let session = connect(bind_addr, server_addr, "localhost", &client_identity)
        .await
        .expect("connect");
    let mut reader = ChunkStreamReader::accept(session.connection())
        .await
        .expect("accept the chunk stream");

    let mut ids = Vec::new();
    while let Some(frame) = reader.next_frame().await.expect("read the stream") {
        ids.push(frame.packet_id);
    }
    assert_eq!(ids.iter().filter(|id| **id == SetColumn::ID).count(), 1);
    assert_eq!(ids.iter().filter(|id| **id == SetChunk::ID).count(), 10);
    assert_eq!(
        ids.iter()
            .filter(|id| **id == SetChunkEnvironments::ID)
            .count(),
        10
    );
    assert_eq!(ids.len(), 21);

    session
        .connection()
        .close(0u32.into(), b"chunk-serve test complete");
    let report = server_task.await.expect("server task");
    assert_eq!(report.chunks, 1);
    assert_eq!(report.packets, 21);
    assert_eq!(report.frames, 21);

    std::fs::remove_dir_all(save_dir).ok();
}
