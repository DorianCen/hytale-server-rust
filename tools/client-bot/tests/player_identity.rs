//! T75: per-UUID player identity and provider spawns through one listener.
//!
//! ```sh
//! cargo test -p client-bot --test player_identity -- --ignored
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_net::{connect, TlsIdentity};
use hytale_server::{
    serve_listener, GeneratorOptions, ServeOptions, SessionOptions, UniverseRuntime,
};
use hytale_world::universe::WorldProviderKind;
use hytale_world::world_config::{ConfigObject, WorldConfig, WorldUuid};
use serde_json::json;
use tokio::sync::oneshot;

const FIRST_UUID: &str = "40000000-0000-0000-0000-000000000075";
const SECOND_UUID: &str = "50000000-0000-0000-0000-000000000075";

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
        "hytale-player-identity-{}-{nonce}-{name}",
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

fn configure_world(root: &std::path::Path) {
    let mut runtime = UniverseRuntime::open(root, generator()).expect("open universe");
    let descriptor = runtime
        .create_world("Default", 7, WorldProviderKind::Default)
        .expect("create Default");
    let config_path = WorldConfig::path_in(&descriptor.save_dir);
    let mut config = WorldConfig::load(&config_path).expect("load config");
    config.spawn_provider = Some(
        serde_json::from_value::<ConfigObject>(json!({
            "Id": "Individual",
            "SpawnPoints": [
                {"X": 8.5, "Y": 120, "Z": 8.5},
                {"X": 12.5, "Y": 120, "Z": 12.5}
            ]
        }))
        .expect("provider config"),
    );
    config.save(&config_path).expect("save config");
}

async fn start_listener(
    root: &std::path::Path,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<hytale_server::ServeReport, hytale_server::ServeError>>,
) {
    let (bound_tx, bound_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut options = ServeOptions::new(
        root,
        generator(),
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        SessionOptions {
            default_view_radius: 0,
            handshake_timeout: Duration::from_secs(60),
            world_join_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            ..SessionOptions::default()
        },
    );
    options.view_radius = 0;
    options.max_connections = 4;
    options.bound_tx = Some(bound_tx);
    let task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let address = tokio::time::timeout(Duration::from_secs(180), bound_rx)
        .await
        .expect("bound address deadline")
        .expect("bound address");
    (address, shutdown_tx, task)
}

fn join_options(uuid: WorldUuid, username: &str) -> DevJoinOptions {
    DevJoinOptions {
        uuid: uuid.to_string().parse().expect("wire uuid"),
        username: username.to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: None,
        movement_after_edits: None,
        observe_after_join: Some(Duration::from_secs(3)),
        disconnect_after: Some(Duration::from_secs(2)),
        keep_alive: false,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: true,
        place_block_after_join: None,
        break_block_after_join: None,
        commands_after_join: Vec::new(),
        place_mode_after_join: None,
        interaction_chains_after_join: Vec::new(),
        arg_value_requests_after_join: Vec::new(),
    }
}

async fn join_pair(address: SocketAddr) -> (client_bot::DevJoinReport, client_bot::DevJoinReport) {
    let first_client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        address,
        "localhost",
        &TlsIdentity::generate("identity-first").expect("first cert"),
    )
    .await
    .expect("connect first");
    let second_client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        address,
        "localhost",
        &TlsIdentity::generate("identity-second").expect("second cert"),
    )
    .await
    .expect("connect second");
    let first_uuid: WorldUuid = FIRST_UUID.parse().expect("first uuid");
    let second_uuid: WorldUuid = SECOND_UUID.parse().expect("second uuid");
    let first_options = join_options(first_uuid, "IdentityFirst");
    let second_options = join_options(second_uuid, "IdentitySecond");
    let first = dev_join(&first_client, &first_options);
    let second = dev_join(&second_client, &second_options);
    let (first, second) = tokio::join!(first, second);
    let first = first.expect("first join");
    let second = second.expect("second join");
    first_client
        .connection()
        .close(0u32.into(), b"identity done");
    second_client
        .connection()
        .close(0u32.into(), b"identity done");
    (first, second)
}

fn spawn_x(report: &client_bot::DevJoinReport) -> f64 {
    report
        .game
        .self_spawn_position
        .as_ref()
        .expect("self spawn position")
        .x
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn two_uuids_get_distinct_provider_spawns_and_survive_restart() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let root = temp_root("universe");
    configure_world(&root);
    let players = root.join("Default").join("players");
    assert!(
        !players.is_dir()
            || std::fs::read_dir(&players)
                .expect("players")
                .next()
                .is_none(),
        "creating the world catalog must not create a player"
    );

    let (address, shutdown_tx, server_task) = start_listener(&root).await;
    let (first, second) = join_pair(address).await;
    assert_eq!(spawn_x(&first), 8.5);
    assert_eq!(spawn_x(&second), 12.5);
    let first_uuid: WorldUuid = FIRST_UUID.parse().expect("first uuid");
    let second_uuid: WorldUuid = SECOND_UUID.parse().expect("second uuid");
    assert!(players.join(format!("{first_uuid}.json")).is_file());
    assert!(players.join(format!("{second_uuid}.json")).is_file());
    let player_docs = std::fs::read_dir(&players)
        .expect("players")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .count();
    assert_eq!(player_docs, 2, "the listener creates only joining UUIDs");
    let _ = shutdown_tx.send(());
    let served = tokio::time::timeout(Duration::from_secs(120), server_task)
        .await
        .expect("listener shutdown deadline")
        .expect("listener task")
        .expect("listener report");
    assert_eq!(served.rejected, 0);

    // Restart the listener from the same root. The saved transforms must win
    // over the provider for existing documents.
    let (address, shutdown_tx, server_task) = start_listener(&root).await;
    let (first, second) = join_pair(address).await;
    assert_eq!(spawn_x(&first), 8.5);
    assert_eq!(spawn_x(&second), 12.5);
    let _ = shutdown_tx.send(());
    let served = tokio::time::timeout(Duration::from_secs(120), server_task)
        .await
        .expect("restart shutdown deadline")
        .expect("restart listener task")
        .expect("restart listener report");
    assert_eq!(served.rejected, 0);

    std::fs::remove_dir_all(root).ok();
}
