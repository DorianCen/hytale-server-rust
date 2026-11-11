//! T70 multi-player and persistence smoke.
//!
//! Two clients join the same world, see each other's held items and a shared
//! NPC, receive each other's edits/removals, reconnect, and then survive a full
//! world-thread restart with the saved transform, inventory, NPC and block edit.
//!
//! Needs the base-game pack and loopback UDP, so it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test multiplayer_persistence -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::common::{BlockPosition, Position};
use hytale_protocol::io::Uuid;
use hytale_server::{
    serve_session_connection_with_registry, GeneratorOptions, PlayerRegistry, SessionOptions,
    SessionReport, WorldThread, WorldThreadOptions,
};
use hytale_world::player_data::{
    PlayerData, PlayerStorage, PlayerTransform, Position as PlayerPosition, Rotation,
};
use hytale_world::runtime::RuntimeEntity;
use hytale_world::world_config::WorldUuid;
use serde_json::json;

const FIRST_ROLE: &str = "Frog_Green";
const HELD_BLOCK: &str = "Rock_Stone";
const SECOND_HELD_ITEM: &str = "Soil_Grass";

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
        "hytale-multiplayer-persistence-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn generator() -> GeneratorOptions {
    GeneratorOptions {
        assets_pack: repo_root().join("Assets/Assets.zip"),
        assets_root: repo_root().join("Assets/extracted"),
        world_dir: repo_root().join("testdata/Default"),
        workers: 1,
    }
}

fn wire_uuid(uuid: WorldUuid) -> Uuid {
    uuid.to_string().parse().expect("wire UUID")
}

fn transform(x: f64, y: f64, z: f64) -> PlayerTransform {
    PlayerTransform::new(
        PlayerPosition { x, y, z },
        Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    )
}

fn write_player(
    save_dir: &std::path::Path,
    uuid: WorldUuid,
    position: PlayerTransform,
    item_id: &str,
    quantity: i32,
) {
    let mut player = PlayerData::new(uuid, position);
    player.set_world_gen_id(0);
    player.insert_component(
        "HotbarInventory",
        json!({
            "Inventory": {
                "Id": "Simple",
                "Capacity": 9,
                "Items": {
                    "0": {
                        "Id": item_id,
                        "Quantity": quantity,
                        "Durability": 0.0,
                        "MaxDurability": 0.0,
                        "QualityOverride": 0,
                        "OverrideDroppedItemAnimation": false
                    }
                }
            },
            "ActiveSlot": 0
        }),
    );
    player.insert_component(
        "UtilityInventory",
        json!({"Inventory": {"Id": "Simple", "Capacity": 4, "Items": {}}, "ActiveSlot": 0}),
    );
    player.insert_component(
        "ArmorInventory",
        json!({"Inventory": {"Id": "Simple", "Capacity": 4, "Items": {}}}),
    );
    PlayerStorage::for_save_dir(save_dir)
        .save(uuid, &player)
        .expect("write player save");
}

fn world_options(save_dir: PathBuf) -> WorldThreadOptions {
    let mut options = WorldThreadOptions::with_defaults(generator(), save_dir);
    options.view_radius = 0;
    options.chunks_per_tick = 8;
    // Keep the exact saved transforms; the test chooses the flat test area.
    options.spawn_search = false;
    options
}

fn start_world(save_dir: PathBuf) -> WorldThread {
    let (thread, _start) = WorldThread::spawn(world_options(save_dir)).expect("world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    thread
}

/// Accept `count` sessions and run them concurrently through one registry.
fn start_server(
    thread: Arc<WorldThread>,
    count: usize,
) -> (
    SocketAddr,
    tokio::task::JoinHandle<Vec<Result<SessionReport, hytale_server::SessionError>>>,
) {
    let server = ServerEndpointBuilder::new(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        TlsIdentity::generate("localhost").expect("server certificate"),
    )
    .bind()
    .expect("bind server");
    let server_addr = server.local_addr();
    let players = Arc::new(PlayerRegistry::new());
    let options = SessionOptions {
        default_view_radius: 0,
        handshake_timeout: Duration::from_secs(60),
        world_join_timeout: Duration::from_secs(30),
        idle_timeout: Duration::from_secs(60),
        ..SessionOptions::default()
    };
    let task = tokio::spawn(async move {
        let mut sessions = Vec::new();
        for _ in 0..count {
            let accepted = server.accept().await.expect("accept");
            let connection = accepted.connection().clone();
            let thread = Arc::clone(&thread);
            let players = Arc::clone(&players);
            let options = options.clone();
            sessions.push(tokio::spawn(async move {
                let result =
                    serve_session_connection_with_registry(&connection, &thread, &options, players)
                        .await;
                let _ = tokio::time::timeout(Duration::from_secs(10), connection.closed()).await;
                result
            }));
        }
        let mut reports = Vec::new();
        for session in sessions {
            reports.push(session.await.expect("session task"));
        }
        server.close();
        reports
    });
    (server_addr, task)
}

fn npc_role(entity: &RuntimeEntity) -> Option<&str> {
    entity.components.get("Npc")?.get("Role")?.as_str()
}

fn hotbar_quantity(report: &client_bot::DevJoinReport) -> Option<i32> {
    report
        .game
        .inventory
        .as_ref()?
        .hotbar
        .as_ref()?
        .items
        .as_ref()?
        .get(&0)
        .map(|item| item.quantity)
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn two_clients_reconnect_and_survive_a_world_restart() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("save");
    let first_uuid: WorldUuid = "00000000-0000-0000-0000-000000000071"
        .parse()
        .expect("first UUID");
    let second_uuid: WorldUuid = "00000000-0000-0000-0000-000000000072"
        .parse()
        .expect("second UUID");
    let first_transform = transform(40.5, 200.0, 40.5);
    let second_transform = transform(48.5, 200.0, 48.5);
    write_player(&save_dir, first_uuid, first_transform, HELD_BLOCK, 5);
    write_player(
        &save_dir,
        second_uuid,
        second_transform,
        SECOND_HELD_ITEM,
        2,
    );

    let thread = start_world(save_dir.clone());
    let rock = thread
        .block_id_for_item(HELD_BLOCK)
        .expect("resolve the held block")
        .expect("Rock_Stone is a block item");
    // Keep the test actors on a small flat platform above the generated world.
    for x in 40..=48 {
        for z in 40..=48 {
            thread.set_block(x, 199, z, rock).expect("floor block");
        }
    }
    let npc = thread
        .spawn_npc(FIRST_ROLE, transform(44.5, 200.0, 44.5))
        .expect("spawn the shared NPC");
    assert!(npc.id >= 1, "the NPC got a stable runtime id");
    let first_target = BlockPosition {
        x: 41,
        y: 200,
        z: 40,
    };
    let second_target = BlockPosition {
        x: 49,
        y: 200,
        z: 48,
    };
    let _ = thread.runtime_entity_events().expect("drain spawn events");

    let thread = Arc::new(thread);
    let (server_addr, server_task) = start_server(Arc::clone(&thread), 3);

    let first_identity = TlsIdentity::generate("multiplayer-first").expect("client cert");
    let second_identity = TlsIdentity::generate("multiplayer-second").expect("client cert");
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

    let first_options = DevJoinOptions {
        commands_after_join: vec!["/save".to_string()],
        uuid: wire_uuid(first_uuid),
        username: "FirstSmoke".to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: Some(Position {
            x: first_transform.position.x,
            y: first_transform.position.y,
            z: first_transform.position.z,
        }),
        movement_after_edits: None,
        observe_after_join: Some(Duration::from_secs(24)),
        disconnect_after: Some(Duration::from_secs(20)),
        keep_alive: false,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: true,
        place_block_after_join: Some((first_target.clone(), HELD_BLOCK.to_string())),
        break_block_after_join: None,
    };
    let second_options = DevJoinOptions {
        commands_after_join: Vec::new(),
        uuid: wire_uuid(second_uuid),
        username: "SecondSmoke".to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: Some(Position {
            x: second_transform.position.x,
            y: second_transform.position.y,
            z: second_transform.position.z,
        }),
        movement_after_edits: None,
        observe_after_join: Some(Duration::from_secs(24)),
        disconnect_after: None,
        keep_alive: true,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: true,
        place_block_after_join: Some((second_target.clone(), HELD_BLOCK.to_string())),
        break_block_after_join: Some(second_target.clone()),
    };
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
    assert_eq!(
        hotbar_quantity(&first_report),
        Some(4),
        "the first client's placement consumes one held block"
    );
    assert_eq!(hotbar_quantity(&second_report), Some(1));
    let first_id = first_report.game.client_id.expect("first network id");
    let second_id = second_report.game.client_id.expect("second network id");
    assert_ne!(first_id, second_id, "network ids must be unique");

    // Both clients see the shared NPC in their initial runtime-entity snapshot.
    for report in [&first_report, &second_report] {
        assert!(
            report
                .observation
                .entities
                .iter()
                .any(|entity| entity.model_asset.as_deref() == Some(FIRST_ROLE)),
            "client did not receive the shared {} spawn: {:?}",
            FIRST_ROLE,
            report.observation.entities
        );
    }

    // Held items are replicated on remote player spawns/updates.
    assert!(
        first_report.observation.entities.iter().any(|entity| {
            entity.network_id == second_id && entity.right_hand.as_deref() == Some(SECOND_HELD_ITEM)
        }),
        "first client did not see the second player's held item: {:?}",
        first_report.observation.entities
    );
    assert!(
        second_report.observation.entities.iter().any(|entity| {
            entity.network_id == first_id && entity.right_hand.as_deref() == Some(HELD_BLOCK)
        }),
        "second client did not see the first player's held item: {:?}",
        second_report.observation.entities
    );

    // The second client's placement and break reached the first client.
    assert!(
        first_report.observation.block_updates.iter().any(|update| {
            (update.x, update.y, update.z) == (second_target.x, second_target.y, second_target.z)
                && update.block_id != 0
        }),
        "first client did not see the second placement: {:?}",
        first_report.observation.block_updates
    );
    assert!(
        first_report.observation.block_updates.iter().any(|update| {
            (update.x, update.y, update.z) == (second_target.x, second_target.y, second_target.z)
                && update.block_id == 0
        }),
        "first client did not see the second break: {:?}",
        first_report.observation.block_updates
    );
    assert!(
        first_report
            .observation
            .server_messages
            .iter()
            .any(|message| message.contains("Saved world")),
        "the first client's /save did not complete: {:?}",
        first_report.observation.server_messages
    );

    // The first client disconnects while the second is still connected; the
    // second sees both the player-list removal and the entity removal.
    assert!(
        second_report
            .observation
            .removed
            .contains(&wire_uuid(first_uuid)),
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

    // Reconnect with the same identity and restore the saved transform,
    // inventory (one held block consumed) and the still-connected peer.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let reconnect_identity = TlsIdentity::generate("multiplayer-first-again").expect("client cert");
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
            commands_after_join: Vec::new(),
            uuid: wire_uuid(first_uuid),
            username: "FirstSmoke".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(2)),
            disconnect_after: None,
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            place_block_after_join: None,
            break_block_after_join: None,
        },
    )
    .await
    .expect("first reconnect");
    let spawn = reconnect_report
        .game
        .self_spawn_position
        .as_ref()
        .expect("reconnect self-spawn transform");
    assert!(
        (spawn.x - first_transform.position.x).abs() < 0.01
            && (spawn.y - first_transform.position.y).abs() < 0.01
            && (spawn.z - first_transform.position.z).abs() < 0.01,
        "reconnect did not restore the saved transform: {spawn:?}"
    );
    assert_eq!(
        hotbar_quantity(&reconnect_report),
        Some(4),
        "reconnect must restore the held stack after one placement"
    );
    assert!(
        reconnect_report
            .game
            .player_list_uuids
            .contains(&wire_uuid(second_uuid)),
        "reconnect did not see the still-connected peer: {:?}",
        reconnect_report.game.player_list_uuids
    );

    reconnected
        .connection()
        .close(0u32.into(), b"multiplayer smoke complete");
    second
        .connection()
        .close(0u32.into(), b"second smoke complete");
    tokio::time::sleep(Duration::from_millis(300)).await;

    let reports = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert_eq!(
        reports.len(),
        3,
        "first, second and reconnect sessions finish"
    );
    let reports: Vec<SessionReport> = reports
        .into_iter()
        .map(|report| report.expect("session"))
        .collect();
    assert_eq!(
        reports
            .iter()
            .filter(|report| report.username == "FirstSmoke")
            .count(),
        2,
        "the reconnect must be its own session"
    );

    let thread = Arc::into_inner(thread).expect("the server released its world thread");
    thread.save().expect("save before restart");
    thread.shutdown().expect("shutdown before restart");

    // Restart the same save. The world edit, both player documents and the NPC
    // runtime entity must all be loaded again.
    let restarted = start_world(save_dir.clone());
    assert_eq!(
        restarted
            .get_block(first_target.x, first_target.y, first_target.z)
            .expect("read the first persisted block"),
        Some(rock),
        "the first client's placed block must survive the world restart"
    );
    assert_eq!(
        restarted
            .get_block(second_target.x, second_target.y, second_target.z)
            .expect("read the second persisted block"),
        Some(0),
        "the second client's break must survive the world restart"
    );
    let first_snapshot = restarted
        .player_snapshot(first_uuid)
        .expect("first snapshot");
    assert_eq!(first_snapshot.transform, first_transform);
    assert_eq!(
        first_snapshot
            .inventory
            .hotbar
            .container
            .get_slot(0)
            .expect("hotbar slot")
            .expect("held stack")
            .quantity,
        4,
        "the held stack must survive the world restart"
    );
    let second_snapshot = restarted
        .player_snapshot(second_uuid)
        .expect("second snapshot");
    assert_eq!(second_snapshot.transform, second_transform);
    assert_eq!(
        second_snapshot
            .inventory
            .hotbar
            .container
            .get_slot(0)
            .expect("hotbar slot")
            .expect("held stack")
            .quantity,
        1,
        "the second player's held stack must survive the world restart"
    );
    assert!(
        restarted
            .entities()
            .expect("entities")
            .iter()
            .any(|entity| npc_role(entity) == Some(FIRST_ROLE)),
        "the shared NPC must survive the world restart"
    );

    // A fresh client after the restart sees the restored player and block.
    let restarted = Arc::new(restarted);
    let (restart_addr, restart_task) = start_server(Arc::clone(&restarted), 1);
    let restart_identity = TlsIdentity::generate("multiplayer-restart").expect("client cert");
    let restart_client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        restart_addr,
        "localhost",
        &restart_identity,
    )
    .await
    .expect("connect after restart");
    let restart_report = dev_join(
        &restart_client,
        &DevJoinOptions {
            commands_after_join: Vec::new(),
            uuid: wire_uuid(first_uuid),
            username: "FirstSmoke".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(5)),
            disconnect_after: Some(Duration::from_secs(2)),
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            place_block_after_join: None,
            break_block_after_join: None,
        },
    )
    .await
    .expect("join after restart");
    let spawn = restart_report
        .game
        .self_spawn_position
        .as_ref()
        .expect("restart self-spawn transform");
    assert!(
        (spawn.x - first_transform.position.x).abs() < 0.01
            && (spawn.y - first_transform.position.y).abs() < 0.01
            && (spawn.z - first_transform.position.z).abs() < 0.01,
        "restart did not restore the saved transform: {spawn:?}"
    );
    assert_eq!(
        restart_report.chunks.len(),
        21,
        "the client after restart receives the full chunk snapshot"
    );

    let reports = tokio::time::timeout(Duration::from_secs(30), restart_task)
        .await
        .expect("restart server shutdown deadline")
        .expect("restart server task");
    assert_eq!(reports.len(), 1);
    if let Some(Ok(report)) = reports.into_iter().next() {
        assert_eq!(report.username, "FirstSmoke");
    }

    let restarted = Arc::into_inner(restarted).expect("restart server released the world thread");
    restarted.shutdown().expect("shutdown after restart");

    std::fs::remove_dir_all(save_dir).ok();
}
