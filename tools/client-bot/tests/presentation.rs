//! T74 presentation loopback: scripted weather packet observation.
//!
//! ```sh
//! cargo test -p client-bot --test presentation -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, ServerEndpointBuilder, TlsIdentity};
use hytale_npc::encounter::BeaconMessage;
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
        "hytale-presentation-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn player_json() -> String {
    r#"{
  "Components": {
    "Transform": {
      "Position": {"X": 4.5, "Y": 81.0, "Z": 4.5},
      "Rotation": {"Pitch": 0.0, "Yaw": 0.0, "Roll": 0.0}
    },
    "UUID": {"UUID": {"$binary": "AAAAAAAAAAYAAAAAAAAAAw==", "$type": "04"}},
    "WorldGenId": {"WorldGenId": 0}
  }
}"#
    .to_string()
}

#[tokio::test]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn forced_weather_sends_the_presentation_transition() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("save");
    let uuid = Uuid::new(6, 3);
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
    world_options.spawn_search = false;
    let (thread, _start) = WorldThread::spawn(world_options).expect("world thread");
    thread.load_chunk(0, 0).expect("load presentation platform");
    let soil = thread
        .block_id_for_item("Soil_Grass")
        .expect("block lookup")
        .expect("Soil_Grass block");
    for x in 0..=8 {
        for z in 0..=8 {
            thread.set_block(x, 80, z, soil).expect("floor");
            for y in 81..=84 {
                thread.set_block(x, y, z, 0).expect("headroom");
            }
        }
    }
    let weather_index = thread
        .asset_registry()
        .expect("assets")
        .weather_id("Blood_Moon")
        .expect("Blood_Moon weather index");
    let thread = Arc::new(thread);

    let server = ServerEndpointBuilder::new(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        TlsIdentity::generate("localhost").expect("server certificate"),
    )
    .bind()
    .expect("bind server");
    let server_addr = server.local_addr();
    let server_thread = Arc::clone(&thread);
    let server_task = tokio::spawn(async move {
        let accepted = server.accept().await.expect("accept");
        let report = serve_session_connection(
            accepted.connection(),
            &server_thread,
            &SessionOptions {
                default_view_radius: 0,
                handshake_timeout: Duration::from_secs(60),
                world_join_timeout: Duration::from_secs(30),
                idle_timeout: Duration::from_secs(30),
                ..SessionOptions::default()
            },
        )
        .await
        .expect("presentation session");
        let _ = tokio::time::timeout(Duration::from_secs(10), accepted.connection().closed()).await;
        server.close();
        report
    });
    let weather_thread = Arc::clone(&thread);
    let weather_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        weather_thread
            .force_weather(Some("Blood_Moon"))
            .expect("force Blood_Moon");
        weather_thread
            .spawn_encounter("Example_Boss")
            .expect("spawn the boss encounter");
        weather_thread
            .spawn_encounter("Combat_Music_Encounter_Deepworks")
            .expect("spawn the combat-music encounter");
        tokio::time::sleep(Duration::from_millis(300)).await;
        weather_thread
            .send_encounter_beacon(
                "Combat_Music_Encounter_Deepworks",
                BeaconMessage::new("Combat_Ping"),
            )
            .expect("send the combat ping");
        weather_thread
            .spawn_npc(
                "Bear_Voidtaken_Tentacle_Marker",
                hytale_world::player_data::PlayerTransform::new(
                    hytale_world::player_data::Position {
                        x: 6.5,
                        y: 81.0,
                        z: 6.5,
                    },
                    hytale_world::player_data::Rotation {
                        pitch: 0.0,
                        yaw: 0.0,
                        roll: 0.0,
                    },
                ),
            )
            .expect("spawn the particle-marking NPC");
    });

    let client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &TlsIdentity::generate("presentation-bot").expect("client cert"),
    )
    .await
    .expect("connect");
    let report = dev_join(
        &client,
        &DevJoinOptions {
            commands_after_join: Vec::new(),
            uuid,
            username: "PresentationBot".to_string(),
            view_radius_chunks: 0,
            skin: None,
            movement_after_join: None,
            movement_after_edits: None,
            observe_after_join: Some(Duration::from_secs(12)),
            disconnect_after: Some(Duration::from_secs(11)),
            keep_alive: false,
            use_block_after_join: Vec::new(),
            send_ready_for_gameplay: true,
            place_block_after_join: None,
            break_block_after_join: None,
            arg_value_requests_after_join: Vec::new(),
            place_mode_after_join: None,
            interaction_chains_after_join: Vec::new(),
        },
    )
    .await
    .expect("presentation join");
    assert!(
        report
            .observation
            .weather_updates
            .iter()
            .any(|(index, transition)| *index == weather_index && (*transition - 10.0).abs() < 1e-6),
        "the forced weather transition must reach the client: {:?}",
        report.observation.weather_updates
    );
    assert!(
        report
            .observation
            .particle_systems
            .iter()
            .any(
                |(id, _)| id.as_deref() == Some("Bear_Voidtaken_Tentacle_Indicator")
                    || id.as_deref() == Some("Alerted")
            ),
        "the NPC presentation particle must reach the client: {:?}",
        report.observation.particle_systems
    );
    assert!(
        !report.observation.audio_states.is_empty(),
        "the encounter audio state must reach the client"
    );
    assert!(
        !report.observation.sound_events.is_empty(),
        "the encounter PlaySound must reach the client"
    );

    client
        .connection()
        .close(0u32.into(), b"presentation test done");
    let server_report = tokio::time::timeout(Duration::from_secs(30), server_task)
        .await
        .expect("server shutdown deadline")
        .expect("server task");
    assert!(server_report.game_loop.weather_updates >= 1);
    weather_task.await.expect("weather task");
    Arc::into_inner(thread)
        .expect("weather thread released")
        .shutdown()
        .expect("world shutdown");

    std::fs::remove_dir_all(save_dir).ok();
}
