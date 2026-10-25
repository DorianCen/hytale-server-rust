//! Inventory loopback: initial inventory payload, held-item block placement
//! consumption, save on disconnect and restore on reconnect.
//!
//! Needs the base-game pack and loopback UDP:
//!
//! ```sh
//! cargo test -p client-bot --test inventory_join -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::common::{BlockPosition, Position};
use hytale_protocol::io::Uuid;
use hytale_server::{
    serve_session_connection, GeneratorOptions, SessionOptions, WorldThread, WorldThreadOptions,
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
        "hytale-inventory-join-{}-{nonce}-{name}",
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
            "Id": "Rock_Stone",
            "Quantity": 5,
            "Durability": 0.0,
            "MaxDurability": 0.0,
            "QualityOverride": 0,
            "OverrideDroppedItemAnimation": false
          }
        }
      },
      "ActiveSlot": 0
    },
    "UtilityInventory": {"Inventory": {"Id": "Simple", "Capacity": 4, "Items": {}}, "ActiveSlot": 0},
    "ArmorInventory": {"Inventory": {"Id": "Simple", "Capacity": 4, "Items": {}}},
    "Transform": {
      "Position": {"X": 0.0, "Y": 200.0, "Z": 0.0},
      "Rotation": {"Pitch": 0.0, "Yaw": 0.0, "Roll": 0.0}
    },
    "UUID": {"UUID": {"$binary": "EjRWeBI0VniavN7wEjRWeA==", "$type": "04"}},
    "WorldGenId": {"WorldGenId": 0}
  }
}"#
    .to_string()
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn held_block_is_consumed_and_restored_on_reconnect() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("save");
    let uuid: Uuid = "12345678-1234-5678-9abc-def012345678"
        .parse()
        .expect("player UUID");
    let players_dir = save_dir.join("players");
    std::fs::create_dir_all(&players_dir).expect("players directory");
    std::fs::write(players_dir.join(format!("{uuid}.json")), player_json())
        .expect("write player save");

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

    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server = ServerEndpointBuilder::new(
        bind_addr,
        TlsIdentity::generate("localhost").expect("server certificate"),
    )
    .bind()
    .expect("bind server");
    let server_addr = server.local_addr();
    let server_task = tokio::spawn(async move {
        let mut reports = Vec::new();
        for _ in 0..2 {
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
            .expect("inventory session");
            let _ =
                tokio::time::timeout(Duration::from_secs(10), accepted.connection().closed()).await;
            reports.push(report);
        }
        thread.shutdown().expect("world shutdown");
        server.close();
        reports
    });

    let first_identity = TlsIdentity::generate("inventory-1").expect("client cert");
    let first = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &first_identity,
    )
    .await
    .expect("connect first");
    let first_report = dev_join(
        &first,
        &DevJoinOptions {
            uuid,
            username: "InventoryBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: Some(Position {
                x: 0.0,
                y: 200.0,
                z: 0.0,
            }),
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(2)),
            disconnect_after: None,
            keep_alive: false,
            place_block_after_join: Some((
                BlockPosition { x: 0, y: 200, z: 0 },
                "Rock_Stone".to_string(),
            )),
            break_block_after_join: None,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
        },
    )
    .await
    .expect("first inventory join");
    let first_inventory = first_report
        .game
        .inventory
        .as_ref()
        .expect("initial inventory packet");
    let hotbar = first_inventory.hotbar.as_ref().expect("hotbar section");
    assert_eq!(
        hotbar
            .items
            .as_ref()
            .and_then(|items| items.get(&0))
            .map(|item| item.quantity),
        Some(4),
        "placing the held block must consume one Rock_Stone"
    );
    assert_eq!(first_report.game.active_slots.get(&-1), Some(&0));
    first
        .connection()
        .close(0u32.into(), b"inventory first done");

    // The save written during teardown must restore the remaining quantity.
    let second_identity = TlsIdentity::generate("inventory-2").expect("client cert");
    let second = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &second_identity,
    )
    .await
    .expect("connect second");
    let second_report = dev_join(
        &second,
        &DevJoinOptions {
            uuid,
            username: "InventoryBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(2)),
            disconnect_after: None,
            keep_alive: false,
            place_block_after_join: None,
            break_block_after_join: None,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
        },
    )
    .await
    .expect("second inventory join");
    let second_inventory = second_report
        .game
        .inventory
        .as_ref()
        .expect("restored inventory packet");
    let hotbar = second_inventory.hotbar.as_ref().expect("hotbar section");
    assert_eq!(
        hotbar
            .items
            .as_ref()
            .and_then(|items| items.get(&0))
            .map(|item| item.quantity),
        Some(4),
        "reconnect must restore the saved inventory"
    );
    second
        .connection()
        .close(0u32.into(), b"inventory second done");

    let reports = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|report| report.game.inventory));
    assert!(reports.iter().all(|report| report.game.active_slots));

    let saved: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(players_dir.join(format!("{uuid}.json")))
            .expect("read saved player"),
    )
    .expect("parse saved player");
    assert_eq!(
        saved["Components"]["HotbarInventory"]["Inventory"]["Items"]["0"]["Quantity"].as_i64(),
        Some(4)
    );

    if std::env::var_os("HYTALE_KEEP_INVENTORY_SAVE").is_some() {
        eprintln!("inventory save kept at {}", save_dir.display());
    } else {
        std::fs::remove_dir_all(save_dir).ok();
    }
}
