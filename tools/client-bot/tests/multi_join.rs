//! Persistent listener loopback: concurrent sessions, disconnect and rejoin.
//!
//! Needs the base-game pack (world generation) and loopback UDP, so it is
//! `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test multi_join -- --ignored
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
        "hytale-multi-join-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn two_clients_join_disconnect_and_rejoin_without_restart() {
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
    options.max_connections = 4;
    options.bound_tx = Some(bound_tx);

    let server_task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let server_addr = tokio::time::timeout(Duration::from_secs(10), bound_rx)
        .await
        .expect("bound address")
        .expect("bound sender");

    let first_identity = TlsIdentity::generate("multi-client-1").expect("client cert");
    let second_identity = TlsIdentity::generate("multi-client-2").expect("client cert");
    let first = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &first_identity,
    )
    .await
    .expect("connect first");
    let second = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &second_identity,
    )
    .await
    .expect("connect second");

    let first_transform = Position {
        x: 4.0,
        y: 200.0,
        z: 4.0,
    };
    let first_block = BlockPosition { x: 4, y: 200, z: 4 };
    let first_options = DevJoinOptions {
        uuid: Uuid::new(1, 1),
        username: "FirstBot".to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: Some(first_transform.clone()),
        movement_after_edits: None,
        observe_after_join: Some(Duration::from_secs(4)),
        disconnect_after: Some(Duration::from_millis(2_500)),
        keep_alive: false,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: false,
        place_block_after_join: Some((first_block.clone(), "Rock_Stone".to_string())),
        break_block_after_join: None,
    };
    let second_options = DevJoinOptions {
        uuid: Uuid::new(1, 2),
        username: "SecondBot".to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: Some(Position {
            x: 8.0,
            y: 200.0,
            z: 8.0,
        }),
        movement_after_edits: None,
        observe_after_join: Some(Duration::from_secs(3)),
        disconnect_after: None,
        keep_alive: true,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: false,
        place_block_after_join: Some((
            BlockPosition { x: 8, y: 200, z: 8 },
            "Rock_Stone".to_string(),
        )),
        break_block_after_join: Some(BlockPosition { x: 8, y: 200, z: 8 }),
    };

    // The second bot joins while the first is still observing, so both see
    // each other's spawn and movement. The first disconnects itself 2.5 s after
    // starting, while the second is still observing.
    let first_future = dev_join(&first, &first_options);
    let second_future = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        dev_join(&second, &second_options).await
    };
    let (first_report, second_report) = tokio::join!(first_future, second_future);
    let first_report = first_report.expect("first join");
    let second_report = second_report.expect("second join");

    for report in [&first_report, &second_report] {
        assert_eq!(report.chunks.len(), 21);
        assert!(report.game.join_world);
        assert!(report.game.player_list);
        assert!(report.game.self_spawn);
    }

    let first_id = first_report.game.client_id.expect("first network id");
    let second_id = second_report.game.client_id.expect("second network id");
    assert_ne!(first_id, second_id, "network ids must be unique");

    // The second client is added to the first client's player list (or was
    // already in its join list, depending on registration order).
    assert!(
        first_report.observation.added.contains(&Uuid::new(1, 2))
            || first_report
                .game
                .player_list_uuids
                .contains(&Uuid::new(1, 2)),
        "first client did not learn about the second: {:?} / {:?}",
        first_report.observation.added,
        first_report.game.player_list_uuids
    );
    assert!(
        second_report
            .game
            .player_list_uuids
            .contains(&Uuid::new(1, 1)),
        "second client did not see the first in its player list: {:?}",
        second_report.game.player_list_uuids
    );

    // Each client sees the other's entity and the other's movement position.
    assert!(
        first_report
            .observation
            .entities
            .iter()
            .any(|entity| entity.network_id == second_id),
        "first client did not see the second entity: {:?}",
        first_report.observation.entities
    );
    assert!(
        second_report
            .observation
            .entities
            .iter()
            .any(|entity| entity.network_id == first_id),
        "second client did not see the first entity: {:?}",
        second_report.observation.entities
    );
    assert!(
        first_report.observation.entities.iter().any(|entity| {
            entity.position.as_ref().is_some_and(|position| {
                (position.x - 8.0).abs() < 0.01 && (position.z - 8.0).abs() < 0.01
            })
        }),
        "first client did not see the second player's movement: {:?}",
        first_report.observation.entities
    );
    assert!(
        second_report.observation.entities.iter().any(|entity| {
            entity.position.as_ref().is_some_and(|position| {
                (position.x - 4.0).abs() < 0.01 && (position.z - 4.0).abs() < 0.01
            })
        }),
        "second client did not see the first player's movement: {:?}",
        second_report.observation.entities
    );

    // The second client places and then breaks a block; the first client is
    // still observing and must receive both authoritative chunk-stream edits.
    let target = BlockPosition { x: 8, y: 200, z: 8 };
    assert!(
        first_report.observation.block_updates.iter().any(|update| {
            (update.x, update.y, update.z) == (target.x, target.y, target.z) && update.block_id != 0
        }),
        "first client did not see the block placement: {:?}",
        first_report.observation.block_updates
    );
    assert!(
        first_report.observation.block_updates.iter().any(|update| {
            (update.x, update.y, update.z) == (target.x, target.y, target.z) && update.block_id == 0
        }),
        "first client did not see the block break: {:?}",
        first_report.observation.block_updates
    );

    // Closing the first session removes its entity and player-list entry from
    // the still-connected second client.
    assert!(
        second_report.observation.removed.contains(&Uuid::new(1, 1)),
        "second client did not get the player-list removal: {:?}",
        second_report.observation.removed
    );
    assert!(
        second_report
            .observation
            .entities
            .iter()
            .any(|entity| entity.removed && entity.network_id == first_id),
        "second client did not get the entity removal: {:?}",
        second_report.observation.entities
    );

    // Reconnect with the first player's identity. The saved transform and the
    // edited block must both still be there: the self spawn restores the moved
    // position, and breaking the block the first session placed produces the
    // authoritative zero update instead of a rejected empty-block interaction.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let reconnect_identity = TlsIdentity::generate("multi-client-1-again").expect("client cert");
    let reconnected = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &reconnect_identity,
    )
    .await
    .expect("reconnect first");
    let reconnect_report = dev_join(
        &reconnected,
        &DevJoinOptions {
            uuid: Uuid::new(1, 1),
            username: "FirstBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(2)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: false,
            place_block_after_join: None,
            break_block_after_join: Some(first_block.clone()),
        },
    )
    .await
    .expect("first reconnect");
    assert_eq!(reconnect_report.chunks.len(), 21);
    let spawn = reconnect_report
        .game
        .self_spawn_position
        .as_ref()
        .expect("self-spawn transform");
    assert!(
        (spawn.x - first_transform.x).abs() < 0.01
            && (spawn.y - first_transform.y).abs() < 0.01
            && (spawn.z - first_transform.z).abs() < 0.01,
        "reconnect did not restore the saved transform: {spawn:?}"
    );
    assert!(
        reconnect_report
            .observation
            .block_updates
            .iter()
            .any(|update| {
                (update.x, update.y, update.z) == (first_block.x, first_block.y, first_block.z)
                    && update.block_id == 0
            }),
        "reconnect did not see the block edit survive teardown: {:?}",
        reconnect_report.observation.block_updates
    );
    assert!(
        reconnect_report
            .game
            .player_list_uuids
            .contains(&Uuid::new(1, 2)),
        "reconnected client did not see the still-connected second player: {:?}",
        reconnect_report.game.player_list_uuids
    );
    reconnected
        .connection()
        .close(0u32.into(), b"reconnect test complete");

    // A new connection joins after the first disconnect, without a restart,
    // and sees the still-connected second player in its player list.
    let third_identity = TlsIdentity::generate("multi-client-3").expect("client cert");
    let third = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &third_identity,
    )
    .await
    .expect("connect third");
    let third_report = dev_join(
        &third,
        &DevJoinOptions {
            uuid: Uuid::new(1, 3),
            username: "ThirdBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: None,
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: false,
            place_block_after_join: None,
            break_block_after_join: None,
        },
    )
    .await
    .expect("third join");
    assert_eq!(third_report.chunks.len(), 21);
    assert!(
        third_report
            .game
            .player_list_uuids
            .contains(&Uuid::new(1, 2)),
        "third client did not see the second player: {:?}",
        third_report.game.player_list_uuids
    );

    second.connection().close(0u32.into(), b"second bot done");
    third.connection().close(0u32.into(), b"third bot done");

    let _ = shutdown_tx.send(());
    let report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task")
        .expect("server report");
    assert!(
        report.errors.is_empty(),
        "session errors: {:?}",
        report.errors
    );
    assert_eq!(report.peak_connections, 2, "two clients were concurrent");
    assert!(report
        .sessions
        .iter()
        .any(|session| session.username == "FirstBot"));
    assert!(report
        .sessions
        .iter()
        .any(|session| session.username == "SecondBot"));
    assert!(report
        .sessions
        .iter()
        .any(|session| session.username == "ThirdBot"));
    assert_eq!(
        report
            .sessions
            .iter()
            .filter(|session| session.username == "FirstBot")
            .count(),
        2,
        "the reconnect must finish as its own session"
    );
    assert_eq!(report.sessions.len(), 4);
    assert!(
        report
            .sessions
            .iter()
            .all(|session| session.movement_rejections == 0 && session.movement_corrections == 0),
        "normal movement must not be rejected or corrected: {:?}",
        report.sessions
    );

    std::fs::remove_dir_all(universe_root).ok();
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn an_idle_session_is_torn_down_and_the_listener_keeps_accepting() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let universe_root = temp_root("idle-universe");
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
            idle_timeout: Duration::from_secs(2),
            ..SessionOptions::default()
        },
    );
    options.view_radius = 0;
    options.max_connections = 4;
    options.bound_tx = Some(bound_tx);

    let server_task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let server_addr = tokio::time::timeout(Duration::from_secs(10), bound_rx)
        .await
        .expect("bound address")
        .expect("bound sender");

    let idle_identity = TlsIdentity::generate("idle-client").expect("client cert");
    let idle = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &idle_identity,
    )
    .await
    .expect("connect idle");
    let idle_report = dev_join(
        &idle,
        &DevJoinOptions {
            uuid: Uuid::new(3, 1),
            username: "IdleBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(8)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: false,
            place_block_after_join: None,
            break_block_after_join: None,
        },
    )
    .await
    .expect("idle join");
    assert_eq!(idle_report.chunks.len(), 21);

    // The server ends the idle session and returns to accepting connections.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let active_identity = TlsIdentity::generate("active-client").expect("client cert");
    let active = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &active_identity,
    )
    .await
    .expect("connect active");
    let active_report = dev_join(
        &active,
        &DevJoinOptions {
            uuid: Uuid::new(3, 2),
            username: "ActiveBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_millis(500)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: false,
            place_block_after_join: None,
            break_block_after_join: None,
        },
    )
    .await
    .expect("active join");
    assert_eq!(active_report.chunks.len(), 21);
    active.connection().close(0u32.into(), b"active bot done");

    let _ = shutdown_tx.send(());
    let report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task")
        .expect("server report");
    assert!(
        report.errors.is_empty(),
        "session errors: {:?}",
        report.errors
    );
    let idle_summary = report
        .sessions
        .iter()
        .find(|session| session.username == "IdleBot")
        .expect("idle session summary");
    assert!(idle_summary.idle_timeout);
    assert!(idle_summary.disconnected);
    assert!(report
        .sessions
        .iter()
        .any(|session| session.username == "ActiveBot"));

    std::fs::remove_dir_all(universe_root).ok();
}
