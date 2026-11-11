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
use std::sync::Arc;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::io::Uuid;
use hytale_server::PermissionRegistry;
use hytale_server::{
    serve_listener, serve_session_connection, GeneratorOptions, RuntimeLimits, ServeOptions,
    SessionOptions, WorldThread, WorldThreadOptions,
};
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
    let runtime_limits = Arc::new(RuntimeLimits::new(20, 16));

    let server_limits = Arc::clone(&runtime_limits);
    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let report = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(30),
                runtime_limits: Some(server_limits),
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
                "/say hello everyone".to_string(),
                "/world".to_string(),
                "/maxplayers".to_string(),
                "/maxviewradius".to_string(),
                "/maxplayers 25".to_string(),
                "/maxviewradius 2".to_string(),
                "/who".to_string(),
                "/whereami".to_string(),
                "/gamemode creative".to_string(),
                "/nope".to_string(),
                "/setblock 0 200 0 Rock_Stone 90".to_string(),
                // Plain chat is relayed to every player (including the sender).
                "hello chat".to_string(),
                r#"/say {"RawText":"styled hello","Bold":true}"#.to_string(),
            ],
            arg_value_requests_after_join: vec![
                ("hytale:player".to_string(), "Command".to_string()),
                ("hytale:item".to_string(), "Rock_St".to_string()),
                ("hytale:game_mode".to_string(), "c".to_string()),
            ],
            place_mode_after_join: None,
            interaction_chains_after_join: Vec::new(),
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
    for expected in [
        "hello everyone",
        "Current world:",
        "Max players:",
        "Max view radius:",
        "Max players set to 25",
        "Max view radius set to 2",
        "player(s): CommandBot",
        "Position:",
        "Game mode set to creative",
        "unknown command `nope`",
        "CommandBot: hello chat",
        "styled hello",
        "Set block (0, 200, 0)",
    ] {
        assert!(
            report
                .observation
                .server_messages
                .iter()
                .any(|message| message.contains(expected)),
            "missing `{expected}` in messages: {:?}",
            report.observation.server_messages
        );
    }
    assert_eq!(runtime_limits.max_players(), 25);
    assert_eq!(runtime_limits.max_view_radius(), 2);

    let messages = &report.observation.formatted_messages;
    assert!(
        messages.iter().any(|message| {
            message.message_id.as_deref() == Some("server.chat.playerMessage")
                && message.raw_text.as_deref() == Some("CommandBot: hello chat")
        }),
        "player chat should keep the Java translation id: {messages:?}"
    );
    assert!(
        messages.iter().any(|message| {
            message
                .message_id
                .as_deref()
                .is_some_and(|id| id == "server.chat.broadcastMessage")
                && message.raw_text.as_deref() == Some("CommandBot: hello everyone")
                && message.color.as_deref() == Some("#00ffff")
        }),
        "say should keep its translation and colour: {messages:?}"
    );
    assert!(
        messages.iter().any(
            |message| message.raw_text.as_deref() == Some("styled hello")
                && message.bold == Some(true)
        ),
        "say JSON should keep styling: {messages:?}"
    );

    let responses = &report.observation.arg_values_responses;
    assert!(
        responses.iter().any(|response| {
            response.arg_type_id.as_deref() == Some("hytale:player")
                && response
                    .values
                    .as_ref()
                    .is_some_and(|values| values.iter().any(|value| value == "CommandBot"))
        }),
        "player suggestions: {responses:?}"
    );
    assert!(
        responses.iter().any(|response| {
            response.arg_type_id.as_deref() == Some("hytale:item")
                && response
                    .values
                    .as_ref()
                    .is_some_and(|values| values.iter().any(|value| value == "Rock_Stone"))
        }),
        "item suggestions: {responses:?}"
    );
    assert!(
        responses.iter().any(|response| {
            response.arg_type_id.as_deref() == Some("hytale:game_mode")
                && response
                    .values
                    .as_ref()
                    .is_some_and(|values| values.iter().any(|value| value == "creative"))
        }),
        "game-mode suggestions: {responses:?}"
    );
    assert!(
        report
            .observation
            .arg_cache_invalidations
            .iter()
            .any(|ids| ids.iter().any(|id| id == "hytale:player")),
        "player-list changes should invalidate the player suggestion cache: {:?}",
        report.observation.arg_cache_invalidations
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

    let setblock = report
        .observation
        .block_updates
        .iter()
        .find(|update| (update.x, update.y, update.z) == (0, 200, 0))
        .expect("the /setblock ServerSetBlock");
    assert!(setblock.block_id > 0);
    assert_eq!(setblock.rotation, 1, "90 degrees is yaw index 1");

    session.connection().close(0u32.into(), b"done");
    let report = tokio::time::timeout(Duration::from_secs(10), server_task)
        .await
        .expect("server deadline")
        .expect("server task");
    assert!(report.game.join_world, "join state: {report:?}");
    assert!(
        report.game_loop.commands >= 13,
        "commands handled: {}",
        report.game_loop.commands
    );
    assert_eq!(report.game_loop.chat_messages, 1);
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn an_operator_can_target_another_player() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let universe_root = temp_dir("targets-universe");
    let (bound_tx, bound_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut permissions = PermissionRegistry::new();
    permissions.grant_operator("AdminBot");
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
            handshake_timeout: Duration::from_secs(180),
            world_join_timeout: Duration::from_secs(180),
            idle_timeout: Duration::from_secs(180),
            permissions: Arc::new(permissions),
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

    let target_identity = TlsIdentity::generate("target-client").expect("target cert");
    let target = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &target_identity,
    )
    .await
    .expect("connect target");
    let target_task = tokio::spawn(async move {
        dev_join(
            &target,
            &DevJoinOptions {
                uuid: Uuid::new(7, 1),
                username: "TargetBot".to_string(),
                view_radius_chunks: 0,
                skin: None,
                movement_after_join: None,
                movement_after_edits: None,
                observe_after_join: Some(Duration::from_secs(3)),
                disconnect_after: None,
                keep_alive: true,
                use_block_after_join: Vec::new(),
                send_ready_for_gameplay: true,
                place_block_after_join: None,
                break_block_after_join: None,
                commands_after_join: Vec::new(),
                arg_value_requests_after_join: Vec::new(),
                place_mode_after_join: None,
                interaction_chains_after_join: Vec::new(),
            },
        )
        .await
        .map_err(|error| error.to_string())
    });
    // Wait until the target's join completed and its keep-alive tasks hold the
    // session open, so the admin's commands always resolve a registered player.
    let target_result = target_task.await.expect("target task");
    let target_report = match target_result {
        Ok(report) => report,
        Err(error) => {
            let _ = shutdown_tx.send(());
            let report = tokio::time::timeout(Duration::from_secs(30), server_task)
                .await
                .expect("server deadline")
                .expect("server task")
                .expect("listener");
            panic!(
                "target join failed: {error}; server errors: {:?}",
                report.errors
            );
        }
    };
    tokio::time::sleep(Duration::from_secs(1)).await;

    let admin_identity = TlsIdentity::generate("admin-client").expect("admin cert");
    let admin = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &admin_identity,
    )
    .await
    .expect("connect admin");
    let admin_report = dev_join(
        &admin,
        &DevJoinOptions {
            uuid: Uuid::new(7, 2),
            username: "AdminBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(6)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            place_block_after_join: None,
            break_block_after_join: None,
            commands_after_join: vec![
                "/give TargetBot Rock_Stone 2".to_string(),
                "/tp TargetBot 12 200 12".to_string(),
                "/gamemode TargetBot creative".to_string(),
                "/say targeted".to_string(),
            ],
            arg_value_requests_after_join: Vec::new(),
            place_mode_after_join: None,
            interaction_chains_after_join: Vec::new(),
        },
    )
    .await
    .expect("admin join");

    for expected in [
        "Queued Rock_Stone x2 for TargetBot",
        "Queued teleport of TargetBot to 12.0, 200.0, 12.0",
        "Queued game mode `creative` for TargetBot",
        "targeted",
    ] {
        assert!(
            admin_report
                .observation
                .server_messages
                .iter()
                .any(|message| message.contains(expected)),
            "missing `{expected}` in admin messages: {:?}",
            admin_report.observation.server_messages
        );
    }
    let admin_tree = admin_report
        .observation
        .command_tree
        .as_ref()
        .expect("admin command tree");
    assert!(admin_tree
        .commands
        .as_ref()
        .expect("commands")
        .iter()
        .any(|command| command.name.as_deref() == Some("stop")));

    let target_tree = target_report
        .observation
        .command_tree
        .as_ref()
        .expect("target command tree");
    assert!(
        !target_tree
            .commands
            .as_ref()
            .expect("commands")
            .iter()
            .any(|command| command.name.as_deref() == Some("stop")),
        "a non-operator must not see `/stop`"
    );

    admin.connection().close(0u32.into(), b"target test done");
    let _ = shutdown_tx.send(());
    let server_report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task")
        .expect("listener result");
    let target_summary = server_report
        .sessions
        .iter()
        .find(|session| session.username == "TargetBot")
        .expect("target session summary");
    assert!(
        target_summary.control_actions >= 3,
        "target control actions: {}",
        target_summary.control_actions
    );
    assert_eq!(target_summary.commands, 0);

    std::fs::remove_dir_all(universe_root).ok();
}
