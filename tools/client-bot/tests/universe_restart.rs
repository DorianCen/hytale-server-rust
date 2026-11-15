//! T73 universe restart and multi-world handoff smoke.
//!
//! Proves that a listener-owned universe survives a real process-level restart:
//! two worlds are catalogued and persisted, a player moves through a portal and
//! back, blocks/items/NPCs survive, clients join both worlds through one
//! listener, the listener shuts down with clients connected, and a second
//! listener reconnects them without stale registry/world state.
//!
//! Needs the base-game pack and loopback UDP, so it is `#[ignore]`d.
//!
//! ```sh
//! cargo test -p client-bot --test universe_restart -- --ignored --nocapture
//! ```

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use client_bot::{dev_join, DevJoinOptions};
use hytale_inventory::{ItemStack, PlayerInventory};
use hytale_net::{connect, TlsIdentity};
use hytale_protocol::io::Uuid as WireUuid;
use hytale_server::{
    serve_listener, GeneratorOptions, ServeOptions, SessionOptions, UniverseRuntime,
};
use hytale_world::player_data::{PlayerTransform, Position, Rotation};
use hytale_world::runtime::RuntimeEntity;
use hytale_world::universe::WorldProviderKind;
use hytale_world::world_config::WorldUuid;
use tokio::sync::oneshot;

const SOURCE_WORLD: &str = "Default";
const PORTAL: &str = "Goblin_Breach";
const NPC_ROLE: &str = "Frog_Green";

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
        "hytale-universe-restart-{}-{nonce}-{name}",
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

fn transform(x: f64, y: f64, z: f64) -> PlayerTransform {
    PlayerTransform::new(
        Position { x, y, z },
        Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    )
}

fn inventory(item: &str, quantity: i32) -> PlayerInventory {
    let mut inventory = PlayerInventory::new();
    inventory
        .hotbar
        .container
        .set_slot(
            0,
            Some(
                ItemStack::from_fields(item, quantity, 0.0, 0.0, None, None, false)
                    .expect("item stack"),
            ),
        )
        .expect("hotbar slot");
    inventory
}

fn wire_uuid(uuid: WorldUuid) -> WireUuid {
    uuid.to_string().parse().expect("wire UUID")
}

fn npc_role(entity: &RuntimeEntity) -> Option<&str> {
    entity.components.get("Npc")?.get("Role")?.as_str()
}

fn player_path(save_dir: &std::path::Path, uuid: WorldUuid) -> PathBuf {
    save_dir.join("players").join(format!("{uuid}.json"))
}

fn join_options(
    uuid: WorldUuid,
    username: &str,
    commands: Vec<String>,
    observe: Duration,
    disconnect_after: Option<Duration>,
) -> DevJoinOptions {
    DevJoinOptions {
        uuid: wire_uuid(uuid),
        username: username.to_string(),
        view_radius_chunks: 0,
        skin: None,
        movement_after_join: None,
        movement_after_edits: None,
        observe_after_join: Some(observe),
        disconnect_after,
        keep_alive: false,
        use_block_after_join: Vec::new(),
        send_ready_for_gameplay: true,
        place_block_after_join: None,
        break_block_after_join: None,
        commands_after_join: commands,
        place_mode_after_join: None,
        interaction_chains_after_join: Vec::new(),
        arg_value_requests_after_join: Vec::new(),
    }
}

async fn start_listener(
    universe_root: &std::path::Path,
    prestart_worlds: Vec<String>,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<hytale_server::ServeReport, hytale_server::ServeError>>,
) {
    let (bound_tx, bound_rx) = oneshot::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut options = ServeOptions::new(
        universe_root,
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
    options.prestart_worlds = prestart_worlds;
    options.bound_tx = Some(bound_tx);
    let task = tokio::spawn(serve_listener(options, async move {
        let _ = shutdown_rx.await;
    }));
    let address = tokio::time::timeout(Duration::from_secs(180), bound_rx)
        .await
        .expect("listener bound address deadline")
        .expect("listener bound address");
    (address, shutdown_tx, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the base-game pack and loopback UDP; run explicitly with --ignored"]
async fn the_universe_survives_a_listener_restart_and_multi_world_handoff() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let universe_root = temp_root("universe");
    let first_uuid: WorldUuid = "00000000-0000-0000-0000-000000000073"
        .parse()
        .expect("first player");
    let second_uuid: WorldUuid = "00000000-0000-0000-0000-000000000074"
        .parse()
        .expect("second player");
    let first_transform = transform(20.5, 120.0, 20.5);
    let second_transform = transform(24.5, 120.0, 24.5);
    let first_block = (-3, 121, -3);
    let second_block = (3, 151, 3);
    let first_inventory = inventory("Rock_Stone", 4);
    let second_inventory = inventory("Soil_Grass", 2);

    // --- First universe lifetime: catalogue two worlds, portal, edit, NPC.
    let mut runtime = UniverseRuntime::open(&universe_root, generator()).expect("open universe");
    runtime
        .create_and_start(SOURCE_WORLD, 7, WorldProviderKind::Default)
        .expect("start source world");
    let source = runtime.world_thread(SOURCE_WORLD).expect("source thread");
    let (arrival, instance) = runtime
        .open_portal_instance(PORTAL, transform(0.0, 0.0, 0.0))
        .expect("open portal instance");
    assert_eq!(arrival.world_name, "Portals_Portals_Goblins");

    // The first player enters and returns; the return restores the source
    // transform and strips the portal's ephemeral key.
    source
        .save_player_state(first_uuid, first_transform, inventory("Rock_Stone", 4))
        .expect("seed first player");
    let mut first_arrival_inventory = first_inventory.clone();
    first_arrival_inventory
        .hotbar
        .container
        .set_slot(
            1,
            Some(
                ItemStack::from_fields("Coffer_Key", 1, 0.0, 0.0, None, None, false)
                    .expect("key stack"),
            ),
        )
        .expect("hotbar slot");
    runtime
        .transfer_live_player(
            &source,
            &instance,
            first_uuid,
            first_transform,
            first_arrival_inventory,
            None,
            &arrival,
        )
        .expect("transfer first player");
    let returned = runtime
        .return_from_instance(&instance, first_uuid)
        .expect("return first player");
    assert_eq!(returned.document.transform(), Some(first_transform));
    let first_snapshot = source.player_snapshot(first_uuid).expect("first snapshot");
    assert_eq!(first_snapshot.transform, first_transform);
    assert_eq!(first_snapshot.inventory.count_item("Rock_Stone"), 4);
    assert_eq!(first_snapshot.inventory.count_item("Coffer_Key"), 0);
    assert!(runtime.portal_return_point(first_uuid).is_none());

    // The second player stays in the instance, so both worlds keep a distinct
    // player document across the restart.
    source
        .save_player_state(second_uuid, second_transform, second_inventory.clone())
        .expect("seed second player");
    runtime
        .transfer_live_player(
            &source,
            &instance,
            second_uuid,
            second_transform,
            second_inventory.clone(),
            None,
            &arrival,
        )
        .expect("transfer second player");
    assert_eq!(runtime.portal_entry_count(arrival.world_uuid), 1);
    assert!(!player_path(&source.start_report().save_dir, second_uuid).exists());
    // Pre-generate the instance chunk around the second player's arrival spawn
    // so the post-restart `/world` switch streams a stored chunk instead of
    // generating portal terrain while the client waits.
    let second_spawn = instance
        .player_snapshot(second_uuid)
        .expect("second instance snapshot")
        .transform;
    instance
        .load_chunk(
            second_spawn.position.x.floor() as i32 >> 5,
            second_spawn.position.z.floor() as i32 >> 5,
        )
        .expect("preload instance spawn chunk");

    // Edits and runtime entities in both worlds.
    let rock = source
        .block_type_id("Rock_Stone")
        .expect("rock block")
        .expect("rock id");
    source
        .load_chunk(first_block.0 >> 5, first_block.2 >> 5)
        .expect("load source chunk");
    source
        .set_block(first_block.0, first_block.1, first_block.2, rock)
        .expect("source edit");
    let soil = instance
        .block_type_id("Soil_Dirt")
        .expect("soil block")
        .expect("soil id");
    instance
        .load_chunk(second_block.0 >> 5, second_block.2 >> 5)
        .expect("load instance chunk");
    instance
        .set_block(second_block.0, second_block.1, second_block.2, soil)
        .expect("instance edit");
    source
        .spawn_npc(NPC_ROLE, transform(0.5, 121.0, 0.5))
        .expect("source NPC");
    instance
        .spawn_npc(NPC_ROLE, transform(0.5, 151.0, 0.5))
        .expect("instance NPC");

    let source_dir = source.start_report().save_dir.clone();
    let instance_dir = instance.start_report().save_dir.clone();
    drop(source);
    drop(instance);
    runtime.shutdown().expect("shutdown first universe");
    assert!(source_dir.join("config.json").is_file());
    assert!(instance_dir.join("config.json").is_file());
    assert!(universe_root.join("universe.json").is_file());

    // --- Restart the universe from the same directory and assert persistence.
    let mut restarted =
        UniverseRuntime::open(&universe_root, generator()).expect("reopen universe");
    let worlds = restarted
        .universe()
        .list_worlds()
        .into_iter()
        .map(|world| world.name.clone())
        .collect::<Vec<_>>();
    assert!(worlds.contains(&SOURCE_WORLD.to_string()), "{worlds:?}");
    assert!(worlds.contains(&arrival.world_name), "{worlds:?}");
    assert_eq!(
        restarted
            .universe()
            .default_world()
            .map(|world| world.name.as_str()),
        Some(SOURCE_WORLD)
    );
    restarted
        .start_world(SOURCE_WORLD)
        .expect("restart source world");
    restarted
        .start_world(&arrival.world_name)
        .expect("restart instance world");
    let source = restarted.world_thread(SOURCE_WORLD).expect("source thread");
    let instance = restarted
        .world_thread(&arrival.world_name)
        .expect("instance thread");
    source
        .load_chunk(first_block.0 >> 5, first_block.2 >> 5)
        .expect("load persisted source chunk");
    instance
        .load_chunk(second_block.0 >> 5, second_block.2 >> 5)
        .expect("load persisted instance chunk");
    assert_eq!(
        source
            .get_block(first_block.0, first_block.1, first_block.2)
            .expect("read source edit"),
        Some(rock)
    );
    assert_eq!(
        instance
            .get_block(second_block.0, second_block.1, second_block.2)
            .expect("read instance edit"),
        Some(soil)
    );
    let first_snapshot = source.player_snapshot(first_uuid).expect("first snapshot");
    assert_eq!(first_snapshot.transform, first_transform);
    assert_eq!(first_snapshot.inventory.count_item("Rock_Stone"), 4);
    let second_snapshot = instance
        .player_snapshot(second_uuid)
        .expect("second snapshot");
    assert_eq!(second_snapshot.transform, arrival.plan.spawn);
    assert_eq!(second_snapshot.inventory.count_item("Soil_Grass"), 2);
    assert!(
        source
            .entities()
            .expect("source entities")
            .iter()
            .any(|entity| npc_role(entity) == Some(NPC_ROLE)),
        "the source NPC must survive the restart"
    );
    assert!(
        instance
            .entities()
            .expect("instance entities")
            .iter()
            .any(|entity| npc_role(entity) == Some(NPC_ROLE)),
        "the instance NPC must survive the restart"
    );
    drop(source);
    drop(instance);
    restarted.shutdown().expect("shutdown restarted universe");

    // --- Listener with two live worlds; shut down while clients are connected.
    let (server_addr, shutdown_tx, server_task) =
        start_listener(&universe_root, vec![arrival.world_name.clone()]).await;
    let first_client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &TlsIdentity::generate("restart-first").expect("first cert"),
    )
    .await
    .expect("connect first");
    let second_client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &TlsIdentity::generate("restart-second").expect("second cert"),
    )
    .await
    .expect("connect second");

    let first_options = join_options(
        first_uuid,
        "RestartFirst",
        Vec::new(),
        Duration::from_secs(30),
        None,
    );
    let second_options = join_options(
        second_uuid,
        "RestartSecond",
        vec![format!("/world {}", arrival.world_name)],
        Duration::from_secs(30),
        None,
    );
    let first_join = dev_join(&first_client, &first_options);
    let second_join = dev_join(&second_client, &second_options);
    let shutdown = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let _ = shutdown_tx.send(());
    });
    let (first_report, second_report) = tokio::join!(first_join, second_join);
    let first_report = first_report.expect("first join");
    let second_report = second_report.expect("second join");
    shutdown.await.expect("shutdown timer");
    assert!(first_report.game.join_world);
    assert!(second_report.game.join_world);
    let served = tokio::time::timeout(Duration::from_secs(120), server_task)
        .await
        .expect("listener shutdown deadline")
        .expect("listener task")
        .expect("listener report");
    assert!(
        second_report
            .observation
            .server_messages
            .iter()
            .any(|message| message.contains("Switched to")),
        "second client did not switch worlds: {:?}",
        second_report.observation.server_messages
    );
    assert_eq!(served.rejected, 0);
    assert!(
        served.peak_connections >= 1,
        "the listener must have accepted the clients: {served:?}"
    );

    // --- A second listener reconnects after the restart without stale state.
    let (server_addr, shutdown_tx, server_task) =
        start_listener(&universe_root, vec![arrival.world_name.clone()]).await;
    let first_client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &TlsIdentity::generate("restart-first-again").expect("first cert"),
    )
    .await
    .expect("reconnect first");
    let second_client = connect(
        SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
        server_addr,
        "localhost",
        &TlsIdentity::generate("restart-second-again").expect("second cert"),
    )
    .await
    .expect("reconnect second");
    let first_options = join_options(
        first_uuid,
        "RestartFirst",
        Vec::new(),
        Duration::from_secs(3),
        Some(Duration::from_secs(1)),
    );
    let second_options = join_options(
        second_uuid,
        "RestartSecond",
        vec![format!("/world {}", arrival.world_name)],
        Duration::from_secs(3),
        Some(Duration::from_secs(1)),
    );
    let first_join = dev_join(&first_client, &first_options);
    let second_join = dev_join(&second_client, &second_options);
    let (first_report, second_report) = tokio::join!(first_join, second_join);
    let first_report = first_report.expect("first reconnect");
    let second_report = second_report.expect("second reconnect");
    assert!(first_report.game.join_world);
    assert!(second_report.game.join_world);
    assert!(
        second_report
            .observation
            .server_messages
            .iter()
            .any(|message| message.contains("Switched to")),
        "second client did not switch after restart: {:?}",
        second_report.observation.server_messages
    );
    let _ = shutdown_tx.send(());
    let served = tokio::time::timeout(Duration::from_secs(120), server_task)
        .await
        .expect("second listener shutdown deadline")
        .expect("second listener task")
        .expect("second listener report");
    assert_eq!(served.rejected, 0);
    assert!(
        !served.sessions.is_empty(),
        "the second listener must finish its sessions: {served:?}"
    );
    first_client
        .connection()
        .close(0u32.into(), b"restart smoke done");
    second_client
        .connection()
        .close(0u32.into(), b"restart smoke done");

    std::fs::remove_dir_all(universe_root).ok();
}
