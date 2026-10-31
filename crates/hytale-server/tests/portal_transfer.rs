//! T60b: cross-world portal handoff.

use std::path::PathBuf;

use hytale_inventory::{ItemStack, PlayerInventory};
use hytale_server::{GeneratorOptions, UniverseRuntime};
use hytale_world::player_data::{PlayerTransform, Position, Rotation};
use hytale_world::universe::WorldProviderKind;
use hytale_world::world_config::WorldUuid;

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
        "hytale-portal-{}-{nonce}-{name}",
        std::process::id()
    ))
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

#[test]
fn returning_from_an_instance_restores_the_source_and_strips_ephemeral_items() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let universe_root = temp_dir("return");
    let mut runtime = UniverseRuntime::open(
        &universe_root,
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
    )
    .expect("open universe");
    runtime
        .create_and_start("Default", 7, WorldProviderKind::Default)
        .expect("start source world");
    let source = runtime.world_thread("Default").expect("source thread");
    let player_uuid: WorldUuid = "12345678-1234-5678-9abc-def012345678"
        .parse()
        .expect("player uuid");
    let source_transform = transform(4.0, 81.0, 4.0);
    let mut inventory = PlayerInventory::new();
    inventory
        .hotbar
        .container
        .set_slot(
            0,
            Some(
                ItemStack::from_fields("Rock_Stone", 5, 0.0, 0.0, None, None, false)
                    .expect("stack"),
            ),
        )
        .expect("hotbar");
    inventory
        .hotbar
        .container
        .set_slot(
            1,
            Some(
                ItemStack::from_fields("Coffer_Key", 1, 0.0, 0.0, None, None, false).expect("key"),
            ),
        )
        .expect("hotbar");

    let arrival = runtime
        .open_portal_instance("Goblin_Breach", transform(0.0, 0.0, 0.0))
        .map(|(arrival, _)| arrival)
        .expect("open portal instance");
    let target = runtime
        .world_thread(&arrival.world_uuid.to_string())
        .expect("target thread");
    runtime
        .transfer_live_player(
            &source,
            &target,
            player_uuid,
            source_transform,
            inventory,
            Some(50.0),
            &arrival,
        )
        .expect("transfer player");
    assert!(runtime.portal_return_point(player_uuid).is_some());
    assert_eq!(runtime.portal_entry_count(arrival.world_uuid), 1);

    let report = runtime
        .return_from_instance(&target, player_uuid)
        .expect("return player");
    assert_eq!(report.from_world, arrival.world_uuid);
    assert_eq!(
        report.to_world,
        source.start_report().world_uuid.parse().unwrap()
    );
    assert_eq!(report.document.transform(), Some(source_transform));

    let returned = source
        .player_snapshot(player_uuid)
        .expect("source snapshot");
    assert_eq!(returned.transform, source_transform);
    assert_eq!(returned.health, Some(50.0));
    assert_eq!(returned.inventory.count_item("Rock_Stone"), 5);
    assert_eq!(
        returned.inventory.count_item("Coffer_Key"),
        0,
        "the portal ephemeral item is removed on return"
    );
    assert!(runtime.portal_return_point(player_uuid).is_none());
    assert_eq!(runtime.portal_entry_count(arrival.world_uuid), 0);
    assert!(
        !target
            .start_report()
            .save_dir
            .join("players")
            .join(format!("{player_uuid}.json"))
            .exists(),
        "the instance copy is removed after returning"
    );

    drop(source);
    drop(target);
    assert!(
        !arrival.plan.close_when_empty,
        "Goblin_Breach keeps its instance open"
    );
    assert!(
        !runtime
            .close_instance_if_empty(arrival.world_uuid, arrival.plan.close_when_empty)
            .expect("close policy check"),
        "CloseWhenEmpty=false keeps the empty instance"
    );
    assert!(
        runtime
            .close_instance_if_empty(arrival.world_uuid, true)
            .expect("close empty instance"),
        "an explicit close policy tears down the empty instance"
    );
    runtime.shutdown().expect("shutdown universe");
    std::fs::remove_dir_all(universe_root).ok();
}

#[test]
fn an_instance_entry_limit_is_enforced() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let universe_root = temp_dir("entry-limit");
    let mut runtime = UniverseRuntime::open(
        &universe_root,
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
    )
    .expect("open universe");
    runtime
        .create_and_start("Default", 7, WorldProviderKind::Default)
        .expect("start source world");
    let source = runtime.world_thread("Default").expect("source thread");
    let (arrival, target) = runtime
        .open_portal_instance("Hederas_Lair", transform(0.0, 0.0, 0.0))
        .expect("open portal instance");
    assert_eq!(arrival.plan.allowed_entry_count, 1);

    let first: WorldUuid = "12345678-1234-5678-9abc-def012345678"
        .parse()
        .expect("first uuid");
    runtime
        .transfer_live_player(
            &source,
            &target,
            first,
            transform(4.0, 81.0, 4.0),
            PlayerInventory::new(),
            None,
            &arrival,
        )
        .expect("first entry");
    assert_eq!(runtime.portal_entry_count(arrival.world_uuid), 1);

    let second: WorldUuid = "12345678-1234-5678-9abc-def012345679"
        .parse()
        .expect("second uuid");
    let error = runtime
        .transfer_live_player(
            &source,
            &target,
            second,
            transform(5.0, 81.0, 5.0),
            PlayerInventory::new(),
            None,
            &arrival,
        )
        .expect_err("the entry limit rejects the second player");
    assert!(error.to_string().contains("entry limit"));

    drop(source);
    drop(target);
    runtime.shutdown().expect("shutdown universe");
    std::fs::remove_dir_all(universe_root).ok();
}

#[test]
fn goblin_breach_creates_the_instance_world_and_moves_the_player() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let universe_root = temp_dir("universe");
    let mut runtime = UniverseRuntime::open(
        &universe_root,
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
    )
    .expect("open universe")
    .with_view(0, 4);

    runtime
        .create_and_start("Default", 7, WorldProviderKind::Default)
        .expect("start source world");
    let source = runtime.world_thread("Default").expect("source thread");
    let player_uuid: WorldUuid = "12345678-1234-5678-9abc-def012345678"
        .parse()
        .expect("player uuid");
    let source_transform = transform(4.0, 81.0, 4.0);
    let mut inventory = PlayerInventory::new();
    inventory
        .hotbar
        .container
        .set_slot(
            0,
            Some(
                ItemStack::from_fields("Rock_Stone", 5, 0.0, 0.0, None, None, false)
                    .expect("stack"),
            ),
        )
        .expect("hotbar");

    let fallback = transform(0.0, 0.0, 0.0);
    let (arrival, target) = runtime
        .open_portal_instance("Goblin_Breach", fallback)
        .expect("open portal instance");
    assert_eq!(arrival.plan.instance_id, "Portals/Portals_Goblins");
    assert_eq!(arrival.plan.spawn.position.y, 149.0);
    assert_eq!(arrival.world_name, "Portals_Portals_Goblins");
    assert!(arrival.started);

    // Opening the same portal again selects the running instance.
    let (second, second_thread) = runtime
        .open_portal_instance("Goblin_Breach", fallback)
        .expect("reopen portal instance");
    assert!(!second.started);
    assert_eq!(second.world_uuid, arrival.world_uuid);
    assert!(std::sync::Arc::ptr_eq(&target, &second_thread));

    let report = runtime
        .transfer_live_player(
            &source,
            &target,
            player_uuid,
            source_transform,
            inventory.clone(),
            Some(42.5),
            &arrival,
        )
        .expect("transfer player");
    assert_eq!(report.player_uuid, player_uuid);
    assert_eq!(report.arrival.world_uuid, arrival.world_uuid);
    assert_eq!(report.document.transform(), Some(arrival.plan.spawn));

    let arrived = target
        .player_snapshot(player_uuid)
        .expect("target snapshot");
    assert_eq!(arrived.transform, arrival.plan.spawn);
    assert_eq!(arrived.inventory, inventory);
    assert_eq!(arrived.health, Some(42.5));
    assert!(
        !source
            .start_report()
            .save_dir
            .join("players")
            .join(format!("{player_uuid}.json"))
            .exists(),
        "the source player document is removed after the handoff"
    );

    drop(source);
    drop(target);
    drop(second_thread);
    runtime.shutdown().expect("shutdown universe");
    std::fs::remove_dir_all(universe_root).ok();
}
