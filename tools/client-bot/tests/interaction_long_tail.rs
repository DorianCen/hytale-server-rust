//! T71 interaction long-tail loopback: SyncPlayerPreferences place mode,
//! active-slot chain, charged/wielding roots and Block_Secondary placement flags.
//!
//! ```sh
//! cargo test -p client-bot --test interaction_long_tail -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions, InteractionChainRequest};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_protocol::generated::common::{BlockPosition, InteractionType, PlaceMode};
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
        "hytale-interaction-long-tail-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn player_json(uuid: Uuid) -> String {
    let binary = base64_uuid(uuid);
    format!(
        r#"{{
  "Components": {{
    "HotbarInventory": {{
      "Inventory": {{
        "Id": "Simple",
        "Capacity": 9,
        "Items": {{
          "0": {{
            "Id": "Rock_Stone",
            "Quantity": 4,
            "Durability": 0.0,
            "MaxDurability": 0.0,
            "QualityOverride": 0,
            "OverrideDroppedItemAnimation": false
          }},
          "1": {{
            "Id": "Soil_Dirt",
            "Quantity": 4,
            "Durability": 0.0,
            "MaxDurability": 0.0,
            "QualityOverride": 0,
            "OverrideDroppedItemAnimation": false
          }}
        }}
      }},
      "ActiveSlot": 0
    }},
    "Transform": {{
      "Position": {{"X": 0.0, "Y": 200.0, "Z": 0.0}},
      "Rotation": {{"Pitch": 0.0, "Yaw": 0.0, "Roll": 0.0}}
    }},
    "UUID": {{"UUID": {{"$binary": "{binary}", "$type": "04"}}}},
    "WorldGenId": {{"WorldGenId": 0}}
  }}
}}"#
    )
}

fn base64_uuid(uuid: Uuid) -> String {
    let bytes = uuid.to_bytes();
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(b2 & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn preferences_and_interaction_roots_reach_the_server() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("save");
    let uuid = Uuid::new(8, 1);
    let players_dir = save_dir.join("players");
    std::fs::create_dir_all(&players_dir).expect("players directory");
    std::fs::write(players_dir.join(format!("{uuid}.json")), player_json(uuid))
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
    world_options.spawn_search = false;
    let (thread, _start) = WorldThread::spawn(world_options).expect("world thread");

    let assets = thread.asset_registry().expect("assets");
    let root_id = |key: &str| {
        assets
            .root_interactions
            .id_of(key)
            .unwrap_or_else(|| panic!("missing root interaction `{key}`"))
    };
    let active_slot_root = root_id("Root_NPC_Skeleton_Burnt_Praetorian_EquipSecondSword");
    let wielding_root = root_id("Root_Weapon_Shield_Secondary_Guard");
    let charging_root = root_id("Consume_Charge");
    let placement_root = root_id("Block_Secondary");
    let stone_block = thread
        .block_id_for_item("Rock_Stone")
        .expect("rock item lookup")
        .expect("Rock_Stone block id");
    let soil_block = thread
        .block_id_for_item("Soil_Dirt")
        .expect("soil item lookup")
        .expect("Soil_Dirt block id");
    assert_ne!(stone_block, soil_block);

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
        .expect("interaction session");
        let _ = tokio::time::timeout(Duration::from_secs(10), accepted.connection().closed()).await;
        thread.shutdown().expect("world shutdown");
        server.close();
        report
    });

    let identity = TlsIdentity::generate("interaction-bot").expect("client cert");
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
            uuid,
            username: "InteractionBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(4)),
            disconnect_after: None,
            keep_alive: false,
            place_block_after_join: Some((target.clone(), "Rock_Stone".to_string())),
            break_block_after_join: None,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            commands_after_join: vec!["/gamemode creative".to_string()],
            place_mode_after_join: Some(PlaceMode::Replace),
            interaction_chains_after_join: vec![
                InteractionChainRequest {
                    root_interaction: active_slot_root,
                    interaction_type: InteractionType::Use,
                    block_position: None,
                    item_in_hand_id: None,
                    charge_value: 0.0,
                    chaining_index: -1,
                    flag_index: -1,
                },
                InteractionChainRequest {
                    root_interaction: wielding_root,
                    interaction_type: InteractionType::Secondary,
                    block_position: None,
                    item_in_hand_id: None,
                    charge_value: 0.0,
                    chaining_index: -1,
                    flag_index: -1,
                },
                InteractionChainRequest {
                    root_interaction: charging_root,
                    interaction_type: InteractionType::Secondary,
                    block_position: None,
                    item_in_hand_id: None,
                    charge_value: 4.0,
                    chaining_index: -1,
                    flag_index: -1,
                },
                InteractionChainRequest {
                    root_interaction: placement_root,
                    interaction_type: InteractionType::Secondary,
                    block_position: Some(target.clone()),
                    item_in_hand_id: Some("Soil_Dirt".to_string()),
                    charge_value: 0.0,
                    chaining_index: -1,
                    flag_index: -1,
                },
            ],
            arg_value_requests_after_join: Vec::new(),
        },
    )
    .await
    .expect("interaction join");

    // SyncPlayerPreferences -> Replace branch -> active slot 1 (Rock_Stone).
    assert_eq!(
        report.game.active_slots.get(&-1),
        Some(&1),
        "active slot must change to slot 1: {:?}",
        report.game.active_slots
    );

    // The Block_Secondary placement chain runs with the Replace place mode. A
    // stone was placed first; the held soil should quick-replace it.
    let updates: Vec<_> = report
        .observation
        .block_updates
        .iter()
        .filter(|update| (update.x, update.y, update.z) == (target.x, target.y, target.z))
        .collect();
    assert!(
        updates.iter().any(|update| update.block_id == stone_block),
        "initial stone placement missing: {updates:?}"
    );
    assert!(
        updates
            .last()
            .is_some_and(|update| update.block_id == soil_block),
        "quick-replaced soil placement missing: {updates:?} (soil id {soil_block})"
    );

    // The active-slot/wielding/charging roots must all be acknowledged with the
    // root the client sent, rather than cancelled.
    for root in [active_slot_root, wielding_root, charging_root] {
        assert!(
            report
                .observation
                .interaction_chains
                .iter()
                .any(|chain| chain.override_root_interaction == root),
            "root {root} was not acked: {:?}",
            report.observation.interaction_chains
        );
    }

    client
        .connection()
        .close(0u32.into(), b"interaction test done");
    let server_report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert!(
        server_report.game_loop.block_uses >= 3,
        "override interaction chains should count as uses: {server_report:?}"
    );

    std::fs::remove_dir_all(save_dir).ok();
}
