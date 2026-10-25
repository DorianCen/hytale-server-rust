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
    let target = BlockPosition { x: 0, y: 200, z: 0 };
    let report = dev_join(
        &client,
        &DevJoinOptions {
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
            break_block_after_join: None,
            use_block_after_join: vec![target.clone(), target.clone()],
            send_ready_for_gameplay: true,
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
    assert!(
        updates.len() >= 3,
        "expected place + open + close updates, got {updates:?}"
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
    assert_eq!(server_report.game_loop.block_edits, 1);
    assert_eq!(server_report.game_loop.block_uses, 2);
    assert_eq!(server_report.game_loop.block_state_changes, 2);

    std::fs::remove_dir_all(save_dir).ok();
}
