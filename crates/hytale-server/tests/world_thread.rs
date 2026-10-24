//! World-thread view changes: demand ticks, unloads and events.

use std::path::PathBuf;

use hytale_protocol::generated::common::ArmorVisibilityOption;
use hytale_protocol::generated::wire::NetworkChannel;
use hytale_server::{WorldEvent, WorldThread, WorldThreadOptions};
use hytale_world::chunk::stream::DEFAULT_COMPRESSION_LEVEL;
use hytale_world::world_clock::parse_iso8601;
use hytale_world::world_config::{WorldConfig, WorldUuid};

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
        "hytale-world-thread-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn config_assets_are_cached_per_world() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("config-cache");
    let options = WorldThreadOptions::with_defaults(
        hytale_server::GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, _start) = WorldThread::spawn(options).expect("spawn the world thread");

    let first = thread.config_asset_packets().expect("first config burst");
    let second = thread.config_asset_packets().expect("second config burst");
    assert!(
        std::sync::Arc::ptr_eq(&first, &second),
        "sessions must share one encoded ConfigAssets"
    );
    assert_eq!(first.packets.len(), 17);
    assert_eq!(
        first
            .packets
            .iter()
            .map(|packet| packet.id)
            .collect::<Vec<_>>(),
        vec![41, 87, 65, 42, 63, 83, 61, 84, 40, 44, 45, 43, 55, 57, 56, 52, 54]
    );
    assert!(!first.tags.is_empty());

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn ticks_the_view_and_unloads_chunks_that_leave_it() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("view");
    let options = WorldThreadOptions::with_defaults(
        hytale_server::GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    // Defaults: radius 0 around the player chunk, one chunk per tick.
    let (thread, start) = WorldThread::spawn(options).expect("spawn the world thread");
    assert_eq!(start.initial_loads, 1);

    let mut generated = 0;
    loop {
        let tick = thread.tick().expect("tick");
        generated += tick.generated;
        if tick.remaining == 0 {
            break;
        }
    }
    assert_eq!(generated, 1);
    assert_eq!(
        thread.try_event(),
        Some(WorldEvent::ChunkLoaded {
            x: start.player_chunk.0,
            z: start.player_chunk.1,
            generated: true,
        })
    );

    // The connection task turns the event into this batch and writes the
    // frames to the chunk stream.
    let batch = thread
        .chunk_packets(start.player_chunk.0, start.player_chunk.1, false)
        .expect("encode the chunk packets");
    assert_eq!((batch.x, batch.z), start.player_chunk);
    assert!(!batch.packets.is_empty());
    assert!(batch
        .packets
        .iter()
        .all(|packet| packet.channel == NetworkChannel::Chunks));
    let frames = batch
        .frames(DEFAULT_COMPRESSION_LEVEL)
        .expect("frame the batch");
    assert_eq!(frames.len(), batch.packets.len());
    assert!(frames.iter().all(|frame| frame.len() > 8));

    // Move one chunk away: the old chunk leaves the view and is saved/unloaded,
    // and the new center is queued instead.
    let view = thread
        .set_view(start.player_chunk.0 + 1, start.player_chunk.1, 0)
        .expect("set view");
    assert_eq!(view.queued, 1);
    assert_eq!(view.unloaded, 1);

    let tick = thread.tick().expect("tick the new view");
    assert_eq!(tick.generated, 1);
    assert_eq!(tick.remaining, 0);

    let mut unloaded = false;
    let mut loaded_new = false;
    while let Some(event) = thread.try_event() {
        match event {
            WorldEvent::ChunkUnloaded { x, z } => {
                unloaded = true;
                assert_eq!((x, z), start.player_chunk);
            }
            WorldEvent::ChunkLoaded { x, z, generated } => {
                assert!(generated);
                if (x, z) == (start.player_chunk.0 + 1, start.player_chunk.1) {
                    loaded_new = true;
                }
            }
        }
    }
    assert!(unloaded);
    assert!(loaded_new);

    assert!(thread.shutdown().expect("shutdown") >= 1);
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn saved_equipment_loads_and_unknown_items_fall_back() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("equipment");
    let players_dir = save_dir.join("players");
    std::fs::create_dir_all(&players_dir).expect("create players directory");
    let source = std::fs::read_to_string(
        repo_root()
            .join("testdata/golden/server/player-data/22345678-1234-5678-9abc-def012345678.json"),
    )
    .expect("read the Java equipment fixture")
    .replace("Weapon_Longsword_Adamantite", "Missing_Hotbar_Item")
    .replace("Armor_Adamantite_Chest", "Missing_Armor_Item");
    std::fs::write(
        players_dir.join("22345678-1234-5678-9abc-def012345678.json"),
        source,
    )
    .expect("write the player save");

    let options = WorldThreadOptions::with_defaults(
        hytale_server::GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, start) = WorldThread::spawn(options).expect("spawn the world thread");
    assert_eq!(start.player_uuid, "22345678-1234-5678-9abc-def012345678");

    let appearance = thread.player_appearance().expect("player appearance");
    let model = appearance.model.expect("saved model");
    assert_eq!(model.id, "Player");
    assert_eq!(model.scale, 1.0);
    assert_eq!(
        appearance.armor[0].as_deref(),
        Some("Armor_Adamantite_Head")
    );
    assert_eq!(appearance.armor[1], None, "unknown armor id must fall back");
    assert_eq!(
        appearance.armor[2].as_deref(),
        Some("Armor_Adamantite_Hands")
    );
    assert_eq!(
        appearance.armor[3].as_deref(),
        Some("Armor_Adamantite_Legs")
    );
    assert_eq!(
        appearance.right_hand, None,
        "unknown hotbar item must fall back"
    );
    assert_eq!(appearance.left_hand.as_deref(), Some("Potion_Health"));

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn block_edits_resolve_items_and_survive_save_reload() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("block-edit");
    let options = || {
        WorldThreadOptions::with_defaults(
            hytale_server::GeneratorOptions {
                assets_pack: repo_root().join("Assets/Assets.zip"),
                assets_root: repo_root().join("Assets/extracted"),
                world_dir: repo_root().join("testdata/Default"),
                workers: 1,
            },
            save_dir.clone(),
        )
    };
    let (thread, start) = WorldThread::spawn(options()).expect("spawn the world thread");
    while thread.tick().expect("tick").remaining > 0 {}

    let rock = thread
        .block_id_for_item("Rock_Stone")
        .expect("resolve Rock_Stone")
        .expect("Rock_Stone is a block item");
    assert!(rock > 0);
    assert_eq!(
        thread
            .block_id_for_item("Weapon_Longsword_Adamantite")
            .expect("resolve sword"),
        None,
        "non-block items must not place a block"
    );
    assert_eq!(
        thread
            .block_id_for_item("Definitely_Not_An_Item")
            .expect("resolve unknown item"),
        None
    );

    let (x, y, z) = (
        start.player_chunk.0 * 32 + 8,
        200,
        start.player_chunk.1 * 32 + 8,
    );
    assert_eq!(thread.get_block(x, y, z).expect("read before"), Some(0));
    assert_eq!(
        thread.place_block(x, y, z, rock).expect("place the block"),
        Some((0, rock))
    );
    assert_eq!(thread.get_block(x, y, z).expect("read after"), Some(rock));
    assert!(thread.save().expect("save") >= 1);
    thread.shutdown().expect("shutdown");

    let (thread, _start) = WorldThread::spawn(options()).expect("respawn the world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    assert_eq!(
        thread.get_block(x, y, z).expect("read reloaded"),
        Some(rock),
        "block edit must survive save/reload"
    );
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn registered_views_share_chunks_and_release_them_on_teardown() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("registered-views");
    let options = WorldThreadOptions::with_defaults(
        hytale_server::GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, start) = WorldThread::spawn(options).expect("spawn the world thread");
    let shared = (start.player_chunk.0 + 5, start.player_chunk.1 + 5);

    // The first session view replaces the startup base view.
    let first = thread
        .add_view(shared.0, shared.1, 0)
        .expect("register first view");
    let second = thread
        .add_view(shared.0, shared.1, 0)
        .expect("register second view");
    while thread.tick().expect("tick").remaining > 0 {}
    assert!(thread
        .loaded_chunks()
        .expect("loaded chunks")
        .contains(&shared));

    // One viewer leaving keeps terrain its peer still needs.
    let report = thread.remove_view(first).expect("remove first view");
    assert_eq!(report.unloaded, 0);
    assert!(thread
        .loaded_chunks()
        .expect("loaded chunks")
        .contains(&shared));

    // The last viewer releases it.
    let report = thread.remove_view(second).expect("remove second view");
    assert_eq!(report.unloaded, 1);
    assert!(!thread
        .loaded_chunks()
        .expect("loaded chunks")
        .contains(&shared));

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn player_snapshot_round_trips_a_session_transform() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("player-snapshot");
    let options = || {
        WorldThreadOptions::with_defaults(
            hytale_server::GeneratorOptions {
                assets_pack: repo_root().join("Assets/Assets.zip"),
                assets_root: repo_root().join("Assets/extracted"),
                world_dir: repo_root().join("testdata/Default"),
                workers: 1,
            },
            save_dir.clone(),
        )
    };
    let uuid: WorldUuid = "12345678-1234-5678-9abc-def012345678"
        .parse()
        .expect("player UUID");
    let transform = hytale_world::player_data::PlayerTransform::new(
        hytale_world::player_data::Position {
            x: 41.5,
            y: 200.0,
            z: -17.25,
        },
        hytale_world::player_data::Rotation {
            pitch: 0.25,
            yaw: 1.5,
            roll: 0.0,
        },
    );

    let (thread, _start) = WorldThread::spawn(options()).expect("spawn the world thread");
    let initial = thread.player_snapshot(uuid).expect("initial snapshot");
    assert_eq!(
        thread.player_snapshot(uuid).expect("second snapshot"),
        initial
    );
    let mut inventory = initial.inventory.clone();
    inventory
        .hotbar
        .container
        .set_slot(
            0,
            Some(
                hytale_inventory::ItemStack::from_fields(
                    "Rock_Stone",
                    5,
                    0.0,
                    0.0,
                    None,
                    None,
                    false,
                )
                .expect("inventory stack"),
            ),
        )
        .expect("set hotbar");
    thread
        .save_player_state(uuid, transform, inventory.clone())
        .expect("save player state");
    let saved = thread.player_snapshot(uuid).expect("saved snapshot");
    assert_eq!(saved.transform, transform);
    assert_eq!(saved.inventory, inventory);
    thread.shutdown().expect("shutdown");

    let (thread, _start) = WorldThread::spawn(options()).expect("respawn the world thread");
    let reloaded = thread.player_snapshot(uuid).expect("reloaded snapshot");
    assert_eq!(
        reloaded.transform, transform,
        "the session transform must survive a world restart"
    );
    assert_eq!(
        reloaded.inventory, inventory,
        "the session inventory must survive a world restart"
    );
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn world_clock_progresses_pauses_and_survives_save_reload() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }

    let save_dir = temp_dir("clock");
    let mut config = WorldConfig::new(WorldUuid::derive(0, "ClockTest"), 0, None);
    config.daytime_duration_seconds = Some(100);
    config.nighttime_duration_seconds = Some(100);
    config
        .save(WorldConfig::path_in(&save_dir))
        .expect("write config overrides");

    let options = || {
        WorldThreadOptions::with_defaults(
            hytale_server::GeneratorOptions {
                assets_pack: repo_root().join("Assets/Assets.zip"),
                assets_root: repo_root().join("Assets/extracted"),
                world_dir: repo_root().join("testdata/Default"),
                workers: 1,
            },
            save_dir.clone(),
        )
    };
    let (thread, _start) = WorldThread::spawn(options()).expect("spawn the world thread");

    // Overrides win over the GameplayConfig durations.
    let initial = thread.world_time().expect("initial time");
    assert_eq!(initial.daytime_duration_seconds, 100);
    assert_eq!(initial.nighttime_duration_seconds, 100);
    assert_eq!(initial.total_moon_phases, 5);
    assert!(!initial.time_paused);
    let expected_initial = parse_iso8601("0001-01-01T05:30:00Z").expect("default instant");
    assert!(
        (initial.game_time_seconds - expected_initial.seconds).abs() <= 1,
        "expected the Java 05:30 default, got {initial:?}"
    );

    // The join packet comes from `Server/GameplayConfigs/Default.json` (the
    // asset itself fails Java validation, so this also covers the internal
    // reader) with Java's default flags.
    let gameplay = thread.gameplay_config().expect("gameplay config");
    assert!(gameplay.world_config.allow_block_breaking);
    assert!(gameplay.world_config.allow_block_gathering);
    assert!(gameplay.world_config.allow_block_placement);
    assert!(gameplay.combat_config.display_health_bars);
    assert!(gameplay.combat_config.display_combat_text);
    assert_eq!(
        gameplay.player_config.armor_visibility_option,
        ArmorVisibilityOption::All
    );

    // 200 real seconds is one full day/night cycle, i.e. 86,400 game seconds.
    let advanced = thread.advance_time(200.0).expect("advance time");
    let elapsed = advanced.game_time_seconds - initial.game_time_seconds;
    assert!(
        (86_398..=86_400).contains(&elapsed),
        "expected one game day, got {elapsed}s"
    );

    // Pausing freezes the clock; saving then reloading restores the exact
    // paused instant and the duration overrides.
    let paused = thread.set_time_paused(true).expect("pause time");
    assert!(paused.time_paused);
    assert!(thread.advance_time(60.0).expect("paused advance") == paused);
    assert!(thread.save().is_ok());
    let saved = thread.world_time().expect("saved state");
    assert_eq!(saved, paused);
    thread.shutdown().expect("shutdown");

    let (thread, _start) = WorldThread::spawn(options()).expect("respawn the world thread");
    let reloaded = thread.world_time().expect("reloaded time");
    assert_eq!(reloaded, saved);
    assert!(reloaded.time_paused);

    // Resuming continues from the same instant.
    let resumed = thread.set_time_paused(false).expect("resume time");
    assert!(!resumed.time_paused);
    assert_eq!(
        (resumed.game_time_seconds, resumed.game_time_nanos),
        (saved.game_time_seconds, saved.game_time_nanos)
    );
    let moved = thread.advance_time(1.0).expect("advance after resume");
    assert!(moved.game_time_seconds > resumed.game_time_seconds);

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
