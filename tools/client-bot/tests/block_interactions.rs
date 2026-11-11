//! Door block-use interaction loopback: place a door, right-click it twice and
//! observe the server's authoritative open/close state changes.
//!
//! ```sh
//! cargo test -p client-bot --test block_interactions -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::common::BlockPosition;
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
        "hytale-block-interactions-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn player_json() -> String {
    r#"{
  "Components": {
    "HotbarInventory": {
      "Inventory": {
        "Id": "Simple",
        "Capacity": 9,
        "Items": {
          "0": {
            "Id": "Furniture_Ancient_Door",
            "Quantity": 2,
            "Durability": 0.0,
            "MaxDurability": 0.0,
            "QualityOverride": 0,
            "OverrideDroppedItemAnimation": false
          }
        }
      },
      "ActiveSlot": 0
    },
    "Transform": {
      "Position": {"X": 0.0, "Y": 200.0, "Z": 0.0},
      "Rotation": {"Pitch": 0.0, "Yaw": 0.0, "Roll": 0.0}
    },
    "UUID": {"UUID": {"$binary": "AAAAAAAAAAYAAAAAAAAAAQ==", "$type": "04"}},
    "WorldGenId": {"WorldGenId": 0}
  }
}"#
    .to_string()
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn door_use_toggles_the_block_state() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("save");
    let uuid = Uuid::new(6, 1);
    let players_dir = save_dir.join("players");
    std::fs::create_dir_all(&players_dir).expect("players directory");
    std::fs::write(players_dir.join(format!("{uuid}.json")), player_json())
        .expect("write player save");

    let mut world_options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    // Keep the saved y=200 transform so the test target stays in reach; the
    // default spawn search would move the player to the surface.
    world_options.spawn_search = false;
    let (thread, _start) = WorldThread::spawn(world_options).expect("world thread");

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
        let report = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(60),
                world_join_timeout: Duration::from_secs(30),
                idle_timeout: Duration::from_secs(30),
                block_break_damage: 0.5,
                ..SessionOptions::default()
            },
        )
        .await
        .expect("block interaction session");
        let _ = tokio::time::timeout(Duration::from_secs(10), accepted.connection().closed()).await;
        thread.shutdown().expect("world shutdown");
        server.close();
        report
    });

    let identity = TlsIdentity::generate("door-bot").expect("client cert");
    let client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &identity,
    )
    .await
    .expect("connect");
    let target = BlockPosition {
        x: -1,
        y: 112,
        z: -1,
    };
    let report = dev_join(
        &client,
        &DevJoinOptions {
            commands_after_join: Vec::new(),
            uuid,
            username: "DoorBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(3)),
            disconnect_after: None,
            keep_alive: false,
            place_block_after_join: Some((target.clone(), "Furniture_Ancient_Door".to_string())),
            break_block_after_join: Some(target.clone()),
            use_block_after_join: vec![target.clone(), target.clone()],
            send_ready_for_gameplay: true,
            arg_value_requests_after_join: Vec::new(),
        },
    )
    .await
    .expect("door join");

    let updates: Vec<_> = report
        .observation
        .block_updates
        .iter()
        .filter(|update| (update.x, update.y, update.z) == (target.x, target.y, target.z))
        .collect();
    assert_eq!(
        updates.len(),
        3,
        "a partial hit must not remove the block: {updates:?}"
    );
    let damage = report
        .observation
        .block_damages
        .iter()
        .find(|damage| (damage.x, damage.y, damage.z) == (target.x, target.y, target.z))
        .expect("the partial block-health packet");
    assert!((damage.damage - 0.5).abs() < 1e-6);
    assert!((damage.delta + 0.5).abs() < 1e-6);
    assert_eq!(
        report
            .observation
            .block_damages
            .iter()
            .filter(|damage| (damage.x, damage.y, damage.z) == (target.x, target.y, target.z))
            .count(),
        1,
        "the immediate hit must not be replayed by the simulation tick"
    );
    let placed = updates[0].block_id;
    assert_ne!(placed, 0);
    assert_ne!(updates[1].block_id, placed, "first use must open the door");
    assert_ne!(
        updates[2].block_id, updates[1].block_id,
        "second use must close/change the door state"
    );

    client.connection().close(0u32.into(), b"door test done");
    let server_report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert_eq!(server_report.game_loop.block_edits, 2);
    assert_eq!(server_report.game_loop.block_uses, 2);
    assert_eq!(server_report.game_loop.block_state_changes, 2);

    std::fs::remove_dir_all(save_dir).ok();
}

fn container_player_json() -> String {
    r#"{
  "Components": {
    "HotbarInventory": {
      "Inventory": {
        "Id": "Simple",
        "Capacity": 9,
        "Items": {
          "0": {
            "Id": "Furniture_Crude_Chest_Large",
            "Quantity": 2,
            "Durability": 0.0,
            "MaxDurability": 0.0,
            "QualityOverride": 0,
            "OverrideDroppedItemAnimation": false
          }
        }
      },
      "ActiveSlot": 0
    },
    "Transform": {
      "Position": {"X": 0.0, "Y": 200.0, "Z": 0.0},
      "Rotation": {"Pitch": 0.0, "Yaw": 0.0, "Roll": 0.0}
    },
    "UUID": {"UUID": {"$binary": "AAAAAAAAAAYAAAAAAAAAAg==", "$type": "04"}},
    "WorldGenId": {"WorldGenId": 0}
  }
}"#
    .to_string()
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn container_use_opens_a_window_and_map_image() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("container-save");
    let uuid = Uuid::new(6, 2);
    let players_dir = save_dir.join("players");
    std::fs::create_dir_all(&players_dir).expect("players directory");
    std::fs::write(
        players_dir.join(format!("{uuid}.json")),
        container_player_json(),
    )
    .expect("write player save");

    let mut world_options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    // Keep the saved y=200 transform so the test target stays in reach; the
    // default spawn search would move the player to the surface.
    world_options.spawn_search = false;
    let (thread, _start) = WorldThread::spawn(world_options).expect("world thread");

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
        let report = serve_session_connection(
            accepted.connection(),
            &thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(60),
                world_join_timeout: Duration::from_secs(30),
                idle_timeout: Duration::from_secs(30),
                ..SessionOptions::default()
            },
        )
        .await
        .expect("container session");
        let _ = tokio::time::timeout(Duration::from_secs(10), accepted.connection().closed()).await;
        thread.shutdown().expect("world shutdown");
        server.close();
        report
    });

    let identity = TlsIdentity::generate("container-bot").expect("client cert");
    let client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &identity,
    )
    .await
    .expect("connect");
    let target = BlockPosition {
        x: -1,
        y: 112,
        z: -1,
    };
    let report = dev_join(
        &client,
        &DevJoinOptions {
            commands_after_join: Vec::new(),
            uuid,
            username: "ContainerBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(3)),
            disconnect_after: None,
            keep_alive: false,
            place_block_after_join: Some((
                target.clone(),
                "Furniture_Crude_Chest_Large".to_string(),
            )),
            break_block_after_join: None,
            use_block_after_join: vec![target.clone()],
            send_ready_for_gameplay: true,
            arg_value_requests_after_join: Vec::new(),
        },
    )
    .await
    .expect("container join");

    let open = report
        .observation
        .windows
        .iter()
        .find(|window| window.event == client_bot::ObservedWindowEvent::Open)
        .expect("the container OpenWindow");
    assert_eq!(
        open.window_type,
        Some(hytale_protocol::generated::common::WindowType::Container)
    );
    assert_eq!(open.capacity, Some(36));
    assert_eq!(open.occupied_slots, Some(0));
    assert!(
        open.window_data
            .as_deref()
            .is_some_and(|data| data.contains("Furniture_Crude_Chest_Large")),
        "window_data must carry the block item id: {:?}",
        open.window_data
    );
    assert!(
        report
            .observation
            .pages
            .contains(&hytale_protocol::generated::common::Page::Bench),
        "the container opens on the bench page: {:?}",
        report.observation.pages
    );
    assert!(
        report.game.world_map_settings.is_some(),
        "the join sends world-map settings"
    );

    client
        .connection()
        .close(0u32.into(), b"container test done");
    let server_report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert_eq!(server_report.game.map_chunks, 1);
    assert_eq!(server_report.game_loop.windows_opened, 1);
    assert_eq!(server_report.game_loop.windows_closed, 1);

    std::fs::remove_dir_all(save_dir).ok();
}
