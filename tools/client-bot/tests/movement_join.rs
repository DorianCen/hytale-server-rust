//! Movement authority loopback: a normal walk is not corrected, a wall is.
//!
//! Needs the base-game pack (world generation) and loopback UDP, so it is
//! `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test movement_join -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, TlsIdentity};
use hytale_protocol::generated::common::{BlockPosition, Position};
use hytale_protocol::io::Uuid;
use hytale_server::{serve_listener, GeneratorOptions, ServeOptions, SessionOptions};
use tokio::sync::oneshot;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn assets_available() -> bool {
    repo_root().join("Assets/Assets.zip").is_file()
}

fn temp_root(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "hytale-movement-join-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn a_wall_correction_is_sent_but_a_free_walk_is_not() {
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

    let identity = TlsIdentity::generate("movement-bot").expect("client cert");
    let session = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &identity,
    )
    .await
    .expect("connect");

    let wall = BlockPosition { x: 5, y: 200, z: 4 };
    let report = dev_join(
        &session,
        &DevJoinOptions {
            uuid: Uuid::new(7, 1),
            username: "MovementBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            // First move into open air, place a wall next to the player, then
            // walk into it.
            movement_after_join: Some(Position {
                x: 4.0,
                y: 200.0,
                z: 4.0,
            }),
            movement_after_edits: Some(Position {
                x: 8.0,
                y: 200.0,
                z: 4.0,
            }),
            observe_after_join: Some(Duration::from_secs(2)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            place_block_after_join: Some((wall.clone(), "Rock_Stone".to_string())),
            break_block_after_join: None,
        },
    )
    .await
    .expect("movement join");

    let settings = report
        .game
        .movement_settings
        .as_ref()
        .expect("UpdateMovementSettings");
    assert_eq!(settings.base_speed, 5.5);
    assert_eq!(settings.jump_force, 11.8);

    assert_eq!(
        report.observation.block_updates,
        vec![client_bot::ObservedBlockUpdate {
            x: wall.x,
            y: wall.y,
            z: wall.z,
            block_id: 347,
        }],
        "the wall must be placed before the collision walk"
    );

    session
        .connection()
        .close(0u32.into(), b"movement test complete");
    let _ = shutdown_tx.send(());
    let server_report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task")
        .expect("server report");
    assert!(
        server_report.errors.is_empty(),
        "{:?}",
        server_report.errors
    );
    let summary = server_report.sessions.first().expect("one session");
    assert_eq!(summary.movement_packets, 2);
    assert_eq!(summary.movement_rejections, 0);
    assert_eq!(summary.movement_corrections, 1);
    assert_eq!(summary.movement_steps, 0);
    std::fs::remove_dir_all(universe_root).ok();

    // The server clamped the walk at the block face and sent the authoritative
    // correction on the setup stream.
    let correction = report
        .observation
        .teleports
        .last()
        .expect("collision correction teleport");
    assert!(
        (correction.x - (5.0 - 0.325)).abs() < 1e-3,
        "unexpected correction: {correction:?} (all: {:?})",
        report.observation.teleports
    );
    assert!((correction.z - 4.0).abs() < 1e-6, "{correction:?}");
}
