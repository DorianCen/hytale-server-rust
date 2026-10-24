//! Loopback test for the authenticated (local EdDSA) join.
//!
//! Needs the base-game pack (the world thread generates a chunk) and loopback
//! UDP, so it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test auth_join -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{auth_join, AuthJoinOptions, ChunkPacketSummary};
use hytale_auth::{AuthKey, AuthService};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::io::Uuid;
use hytale_server::{
    serve_session_connection, AuthMode, GeneratorOptions, SessionOptions, WorldThread,
    WorldThreadOptions,
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
        "hytale-auth-join-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn the_bot_authenticates_streams_assets_and_reaches_game() {
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

    let key = AuthKey::generate().expect("auth key");
    let server_auth = AuthService::new(key.clone(), "rust-local", "rust-server");
    let client_auth = AuthService::new(key.clone(), "rust-local", "rust-server");

    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
    let server = ServerEndpointBuilder::new(bind_addr, server_identity)
        .bind()
        .expect("bind server");
    let server_addr = server.local_addr();
    let client_identity = TlsIdentity::generate("auth-join-client").expect("client cert");
    let client_fingerprint = client_identity.fingerprint_base64url();

    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let report = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(60),
                world_join_timeout: Duration::from_secs(30),
                auth: AuthMode::local(server_auth),
                ..SessionOptions::default()
            },
        )
        .await
        .expect("authenticated session");
        let _ = tokio::time::timeout(Duration::from_secs(10), accepted.connection().closed()).await;
        thread.shutdown().expect("shutdown");
        server.close();
        report
    });

    let session = connect(bind_addr, server_addr, "localhost", &client_identity)
        .await
        .expect("connect");
    let report = auth_join(
        &session,
        &AuthJoinOptions {
            uuid: Uuid::new(1, 2),
            username: "AuthBot".to_string(),
            view_radius_chunks: 0,
            auth: client_auth,
            certificate_fingerprint: client_fingerprint,
            asset_limit: Some(1),
            movement_chunk: Some((1, 0)),
        },
    )
    .await
    .expect("auth join");

    assert!(report.auth_grant);
    assert!(report.server_auth_token);
    assert!(report.advertised_assets > 0);
    assert_eq!(report.requested_assets, 1);
    assert_eq!(report.assets.len(), 1);
    assert!(report.assets[0].bytes > 0);
    assert!(report.asset_parts >= 1);
    assert_eq!(report.asset_bytes, report.assets[0].bytes);
    assert_eq!(
        report.config_packets,
        vec![
            41, // UpdateBlockHitboxes
            87, // UpdatePhysicalMaterials
            65, // UpdateSoundEvents
            42, // UpdateBlockSoundSets
            63, // UpdateFluidFX
            83, // UpdateFluids
            61, // UpdateEnvironments
            84, // UpdateTagPatterns
            40, // UpdateBlockTypes
            44, // UpdateBlockParticleSets
            45, // UpdateBlockBreakingDecals
            43, // UpdateItemSoundSets
            55, // UpdateItemQualities
            57, // UpdateItemReticles
            56, // UpdateItemCategories
            52, // UpdateItemPlayerAnimations
            54, // UpdateItems
        ],
        "config packets must arrive in dependency order"
    );
    assert!(report.translations);
    assert!(report.rebuild_requested);
    assert!(report.game.clear_chunks);
    assert!(report.game.join_world);
    assert!(report.game.player_list);
    assert!(report.game.self_spawn);
    assert_eq!(report.game.client_id, Some(1));
    assert!(report.game.sent_ready_for_chunks);
    assert_eq!(report.chunks.len(), 21);
    assert!(report.teleport);
    assert!(
        report
            .view_update_chunks
            .iter()
            .any(|packet| matches!(packet, ChunkPacketSummary::Unload { x: 0, z: 0 })),
        "expected the old chunk to unload: {:?}",
        report.view_update_chunks
    );
    assert!(
        report
            .view_update_chunks
            .iter()
            .any(|packet| matches!(packet, ChunkPacketSummary::Column { x: 1, z: 0, .. })),
        "expected the new chunk column: {:?}",
        report.view_update_chunks
    );

    session
        .connection()
        .close(0u32.into(), b"auth join test complete");
    let server_report = server_task.await.expect("server task");
    assert_eq!(server_report.username, "AuthBot");
    assert_eq!(server_report.auth_mode, "local");
    assert_eq!(server_report.assets.requested, 1);
    assert_eq!(server_report.assets.sent, 1);
    assert!(server_report.stream.chunks >= 2); // snapshot + movement target
    assert!(server_report.game.client_ready_for_chunks);
    assert!(server_report.game.player_list);
    assert!(server_report.game.self_spawn);
    assert!(server_report.game.gameplay_config);
    assert!(server_report.game.time);
    assert!(server_report.game.client_ready_for_gameplay);
    assert!(server_report.game_loop.ready_for_gameplay);
    assert!(server_report.game_loop.teleport_sent);
    assert!(server_report.game_loop.teleport_acked);
    assert!(server_report.game_loop.movement_packets >= 1);
    assert!(server_report.game_loop.view_changes >= 1);

    std::fs::remove_dir_all(save_dir).ok();
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn the_server_rejects_a_token_bound_to_another_certificate() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("mismatch");
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

    let key = AuthKey::generate().expect("auth key");
    let server_auth = AuthService::new(key.clone(), "rust-local", "rust-server");
    let client_auth = AuthService::new(key, "rust-local", "rust-server");

    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
    let server = ServerEndpointBuilder::new(bind_addr, server_identity)
        .bind()
        .expect("bind server");
    let server_addr = server.local_addr();
    let client_identity = TlsIdentity::generate("mismatch-client").expect("client cert");

    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let result = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(30),
                world_join_timeout: Duration::from_secs(10),
                auth: AuthMode::local(server_auth),
                ..SessionOptions::default()
            },
        )
        .await;
        let _ = tokio::time::timeout(Duration::from_secs(5), accepted.connection().closed()).await;
        thread.shutdown().expect("shutdown");
        server.close();
        result
    });

    let session = connect(bind_addr, server_addr, "localhost", &client_identity)
        .await
        .expect("connect");
    let result = auth_join(
        &session,
        &AuthJoinOptions {
            uuid: Uuid::new(1, 2),
            username: "WrongCertBot".to_string(),
            view_radius_chunks: 0,
            auth: client_auth,
            certificate_fingerprint: "not-the-client-certificate".to_string(),
            asset_limit: Some(0),
            movement_chunk: None,
        },
    )
    .await;

    assert!(result.is_err());
    let server_result = server_task.await.expect("server task");
    assert!(
        matches!(
            server_result,
            Err(hytale_server::SessionError::Auth(
                hytale_auth::AuthError::CertificateBinding
            ))
        ),
        "unexpected server result: {server_result:?}"
    );

    std::fs::remove_dir_all(save_dir).ok();
}
