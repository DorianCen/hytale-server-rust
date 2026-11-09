//! Loopback test for the in-game command framework.
//!
//! Needs the base-game pack (the world thread generates a chunk) and loopback
//! UDP, so it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test commands -- --ignored
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
        "hytale-commands-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn the_bot_runs_commands_over_chat() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("world");
    let mut world_options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    world_options.spawn_search = false;
    let (thread, _start) = WorldThread::spawn(world_options).expect("world thread");

    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server_identity = TlsIdentity::generate("localhost").expect("server certificate");
    let server = ServerEndpointBuilder::new(bind_addr, server_identity)
        .bind()
        .expect("bind server");
    let server_addr = server.local_addr();
    let client_identity = TlsIdentity::generate("command-client").expect("client cert");

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
        .expect("session");
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
            uuid: Uuid::new(9, 1),
            username: "CommandBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(3)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            place_block_after_join: None,
            break_block_after_join: None,
            commands_after_join: vec![
                "/help".to_string(),
                "/tp 4 200 4".to_string(),
                "/give Rock_Stone 2".to_string(),
                "/save".to_string(),
            ],
        },
    )
    .await
    .expect("dev join with commands");

    let tree = report
        .observation
        .command_tree
        .as_ref()
        .expect("command tree");
    let commands = tree.commands.as_ref().expect("commands");
    for name in ["help", "tp", "give", "save", "stop", "version"] {
        assert!(
            commands
                .iter()
                .any(|command| command.name.as_deref() == Some(name)),
            "command tree is missing `{name}`"
        );
    }

    assert!(
        report
            .observation
            .server_messages
            .iter()
            .any(|message| message.contains("Teleported to 4.0, 200.0, 4.0")),
        "messages: {:?}",
        report.observation.server_messages
    );
    assert!(
        report
            .observation
            .server_messages
            .iter()
            .any(|message| message.contains("Gave Rock_Stone x2")),
        "messages: {:?}",
        report.observation.server_messages
    );
    assert!(
        report
            .observation
            .server_messages
            .iter()
            .any(|message| message.contains("Saved world")),
        "messages: {:?}",
        report.observation.server_messages
    );
    assert!(
        report.observation.teleports.iter().any(|position| {
            (position.x - 4.0).abs() < f64::EPSILON
                && (position.y - 200.0).abs() < f64::EPSILON
                && (position.z - 4.0).abs() < f64::EPSILON
        }),
        "teleports: {:?}",
        report.observation.teleports
    );

    session.connection().close(0u32.into(), b"done");
    let report = tokio::time::timeout(Duration::from_secs(10), server_task)
        .await
        .expect("server deadline")
        .expect("server task");
    assert!(report.game.join_world, "join state: {report:?}");
    assert!(
        report.game_loop.commands >= 4,
        "commands handled: {}",
        report.game_loop.commands
    );
}
