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
use hytale_net::{connect, ChunkStreamReader, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::common::PlayerSkin;
use hytale_protocol::generated::wire::Wire;
use hytale_protocol::generated::worldmap::UpdateWorldMap;
use hytale_protocol::generated::Packet;
use hytale_protocol::io::{Reader, Uuid};
use hytale_server::{
    serve_session_connection, GeneratorOptions, SessionOptions, WorldThread, WorldThreadOptions,
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
    let mut world_options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    // The prefab is placed relative to the origin; keep the raw spawn so the
    // entity-view assertions stay deterministic.
    world_options.spawn_search = false;
    let (thread, _start) = WorldThread::spawn(world_options).expect("world thread");
    thread
        .place_prefab("Hunt.Generic.Site", 4, 200, 4, 0)
        .expect("place the hunt site prefab");
    while thread.try_event().is_some() {}

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
            commands_after_join: Vec::new(),
            uuid: Uuid::new(1, 2),
            username: "TestBot".to_string(),
            view_radius_chunks: 0,
            skin: Some(PlayerSkin {
                face: Some("FaceA".to_string()),
                haircut: Some("HairA".to_string()),
                ..PlayerSkin::default()
            }),
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(1)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: false,
            place_block_after_join: None,
            break_block_after_join: None,
        },
    )
    .await
    .expect("dev join");

    // `UpdateWorldMap` rides the second server-opened stream. The join report
    // does not consume it, so read the buffered markers directly here.
    let mut world_map = tokio::time::timeout(
        Duration::from_secs(5),
        ChunkStreamReader::accept(session.connection()),
    )
    .await
    .expect("world map stream opened")
    .expect("world map stream");
    let mut marker_ids = Vec::new();
    let read_markers = async {
        loop {
            let frame = world_map
                .next_frame()
                .await
                .expect("world map frame")
                .expect("world map stream open");
            if frame.packet_id != UpdateWorldMap::ID {
                continue;
            }
            let packet = UpdateWorldMap::decode(&mut Reader::new(&frame.payload))
                .expect("decode UpdateWorldMap");
            if let Some(markers) = packet.added_markers {
                marker_ids.extend(
                    markers
                        .into_iter()
                        .map(|marker| (marker.id, marker.marker_image)),
                );
            }
            if marker_ids
                .iter()
                .any(|(id, image)| id == "block:4:201:4" && image == "HuntSite.png")
            {
                break;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(2), read_markers)
        .await
        .expect("world map marker");

    assert!(report.handshake_packets.contains(&364)); // RequestInsecurePlayerOptions
    assert!(report.handshake_packets.contains(&14)); // ConnectAccept
    assert!(report.handshake_packets.contains(&20)); // WorldSettings
    assert!(report.handshake_packets.contains(&22)); // WorldLoadFinished
    assert!(report.game.join_world);
    assert!(report.game.player_list);
    assert!(report.game.self_spawn);
    assert_eq!(report.game.self_spawn_model.as_deref(), Some("Player"));
    let skin = report
        .game
        .self_spawn_skin
        .as_ref()
        .expect("skin from InsecurePlayerOptions");
    assert_eq!(skin.face.as_deref(), Some("FaceA"));
    assert_eq!(skin.haircut.as_deref(), Some("HairA"));
    assert_eq!(
        report.game.self_spawn_armor,
        Some(vec![
            String::new(),
            String::new(),
            String::new(),
            String::new()
        ])
    );
    assert_eq!(report.game.self_spawn_right_hand.as_deref(), Some("Empty"));
    assert_eq!(report.game.self_spawn_left_hand.as_deref(), Some("Empty"));
    assert_eq!(report.game.client_id, Some(1));
    assert!(report.game.sent_ready_for_chunks);
    assert_eq!(report.chunks.len(), 21);
    let settings = report
        .game
        .world_map_settings
        .as_ref()
        .expect("world map settings after the join");
    assert!(settings.enabled);
    assert!(marker_ids
        .iter()
        .any(|(id, image)| id == "block:4:201:4" && image == "HuntSite.png"));
    assert_eq!(
        report
            .observation
            .entities
            .iter()
            .filter(|entity| entity.network_id >= 1_000_000 && !entity.removed)
            .count(),
        3,
        "the prefab's three runtime entities spawn for the joining client"
    );

    session
        .connection()
        .close(0u32.into(), b"dev join test complete");
    let server_report = server_task.await.expect("server task");
    assert_eq!(server_report.username, "TestBot");
    assert_eq!(server_report.view_radius, 0);
    assert_eq!(server_report.stream.chunks, 1);
    assert!(server_report.game.player_list);
    assert!(server_report.game.self_spawn);
    assert!(server_report.game.gameplay_config);
    assert!(server_report.game.time);

    std::fs::remove_dir_all(save_dir).ok();
}
