//! Live portal session handoff over the loopback listener.
//!
//! ```sh
//! cargo test -p client-bot --test portal_session -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, TlsIdentity};
use hytale_protocol::io::Uuid;
use hytale_server::{serve_listener, GeneratorOptions, ServeOptions, SessionOptions};
use tokio::sync::oneshot;

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

fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-portal-session-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn a_connected_player_arrives_in_goblin_breach_after_the_handoff() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let universe_root = temp_root("universe");
    let (bound_tx, bound_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut options = ServeOptions::new(
        &universe_root,
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        SessionOptions {
            default_view_radius: 0,
            handshake_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(30),
            portal_on_join: Some("Goblin_Breach".to_string()),
            ..SessionOptions::default()
        },
    );
    options.view_radius = 0;
    options.max_connections = 2;
    options.bound_tx = Some(bound_tx);

    let server_task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let server_addr = tokio::time::timeout(Duration::from_secs(10), bound_rx)
        .await
        .expect("bound address")
        .expect("bound sender");

    let identity = TlsIdentity::generate("portal-client").expect("client cert");
    let client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &identity,
    )
    .await
    .expect("connect");
    let player_uuid = Uuid::new(9, 1);
    let report = tokio::time::timeout(
        Duration::from_secs(180),
        dev_join(
            &client,
            &DevJoinOptions {
                uuid: player_uuid,
                username: "PortalBot".to_string(),
                view_radius_chunks: 0,
                skin: None,
                movement_after_join: None,
                movement_after_edits: None,
                observe_after_join: Some(Duration::from_secs(20)),
                disconnect_after: Some(Duration::from_millis(1_500)),
                keep_alive: false,
                use_block_after_join: Vec::new(),
                send_ready_for_gameplay: true,
                place_block_after_join: None,
                break_block_after_join: None,
            },
        ),
    )
    .await
    .expect("client join timeout")
    .expect("client join");
    assert!(report.game.join_world);
    assert!(report.game.clear_chunks);

    let _ = shutdown_tx.send(());
    let served = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown timeout")
        .expect("server task")
        .expect("server result");
    assert_eq!(served.sessions.len(), 1);
    assert_eq!(
        served.sessions[0].portal_world.as_deref(),
        Some("Portals_Portals_Goblins")
    );

    let target_player = universe_root
        .join("Portals_Portals_Goblins")
        .join("players")
        .join(format!("{player_uuid}.json"));
    assert!(
        target_player.is_file(),
        "the live session's player document arrives in the instance world"
    );
    let source_player = universe_root
        .join("default")
        .join("players")
        .join(format!("{player_uuid}.json"));
    assert!(!source_player.is_file(), "the source document is removed");

    std::fs::remove_dir_all(universe_root).ok();
}
