//! T59b bot scenario: observe NPC spawn, chase and despawn, and verify the
//! server recorded an attack from the same session.
//!
//! Needs the base-game pack and loopback UDP, so it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test npc_scenario -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
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
        "hytale-npc-scenario-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn player_json() -> String {
    r#"{
  "Components": {
    "Transform": {
      "Position": {"X": 0.5, "Y": 81.0, "Z": 0.5},
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
async fn bot_observes_npc_spawn_chase_and_despawn() {
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
    while thread.tick().expect("tick").remaining > 0 {}

    thread.load_chunk(0, 0).expect("load platform");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass block");
    for x in 0..=12 {
        for z in 0..=12 {
            thread.set_block(x, 80, z, soil).expect("floor");
            for y in 81..=84 {
                thread.set_block(x, y, z, 0).expect("clear headroom");
            }
        }
    }
    let npc = thread
        .spawn_npc(
            "Rat",
            hytale_world::player_data::PlayerTransform::new(
                hytale_world::player_data::Position {
                    x: 3.5,
                    y: 81.0,
                    z: 3.5,
                },
                hytale_world::player_data::Rotation {
                    pitch: 0.0,
                    yaw: 0.0,
                    roll: 0.0,
                },
            ),
        )
        .expect("spawn Rat");
    let _ = thread.runtime_entity_events().expect("drain spawn events");

    let thread = Arc::new(thread);
    let bind_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    let server = ServerEndpointBuilder::new(
        bind_addr,
        TlsIdentity::generate("localhost").expect("server certificate"),
    )
    .bind()
    .expect("bind server");
    let server_addr = server.local_addr();
    let server_world = Arc::clone(&thread);
    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let report = serve_session_connection(
            accepted.connection(),
            &server_world,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(60),
                world_join_timeout: Duration::from_secs(30),
                idle_timeout: Duration::from_secs(240),
                ..SessionOptions::default()
            },
        )
        .await
        .expect("NPC scenario session");
        let _ = tokio::time::timeout(Duration::from_secs(10), accepted.connection().closed()).await;
        server.close();
        report
    });

    let kill_world = Arc::clone(&thread);
    let killer = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(55)).await;
        kill_world
            .damage_npc(npc.id, 999.0, "Physical")
            .expect("kill Rat")
            .expect("Rat actor");
    });

    let identity = TlsIdentity::generate("npc-bot").expect("client cert");
    let client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &identity,
    )
    .await
    .expect("connect");
    let report = dev_join(
        &client,
        &DevJoinOptions {
            commands_after_join: Vec::new(),
            uuid,
            username: "NpcBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(65)),
            disconnect_after: None,
            keep_alive: false,
            place_block_after_join: None,
            break_block_after_join: None,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            arg_value_requests_after_join: Vec::new(),
        },
    )
    .await
    .expect("NPC bot join");

    killer.await.expect("killer task");
    let server_report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    let entity_id = report
        .observation
        .entities
        .iter()
        .find(|entity| entity.model_asset.as_deref() == Some("Rat"))
        .map(|entity| entity.network_id)
        .expect("the bot received the Rat spawn");
    let rat_updates: Vec<_> = report
        .observation
        .entities
        .iter()
        .filter(|entity| entity.network_id == entity_id)
        .collect();
    let spawn = rat_updates
        .iter()
        .find(|entity| entity.model_asset.as_deref() == Some("Rat"))
        .expect("Rat spawn update");
    let moved = rat_updates.iter().any(|update| {
        update.position.as_ref().is_some_and(|position| {
            spawn.position.as_ref().is_none_or(|start| {
                (position.x - start.x).abs() > 0.5
                    || (position.y - start.y).abs() > 0.5
                    || (position.z - start.z).abs() > 0.5
            })
        })
    });
    assert!(moved, "the bot observed the Rat chase the player");
    assert!(
        rat_updates.iter().any(|update| update.removed),
        "the bot observed the Rat despawn"
    );

    assert!(
        server_report.game_loop.npc_attacks >= 1,
        "the server recorded the NPC attack: {:?}",
        server_report.game_loop
    );

    client.connection().close(0u32.into(), b"npc scenario done");
    let thread = Arc::into_inner(thread).expect("world thread is uniquely owned");
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
