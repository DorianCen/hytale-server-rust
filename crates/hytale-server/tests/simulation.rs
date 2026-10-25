//! World-thread block simulation: support, falling blocks, fluids and drops.

use std::path::PathBuf;

use hytale_server::{GeneratorOptions, WorldThread, WorldThreadOptions};
use hytale_world::world::WorldSave;
use hytale_world::world_config::WorldUuid;

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
        "hytale-simulation-{}-{nonce}-{name}",
        std::process::id()
    ))
}

fn world(name: &str) -> Option<(WorldThread, PathBuf)> {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let save_dir = temp_dir(name);
    let options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, _start) = WorldThread::spawn(options).expect("spawn the world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    Some((thread, save_dir))
}

fn block_id(thread: &WorldThread, item: &str) -> i32 {
    thread
        .block_id_for_item(item)
        .expect("block lookup")
        .unwrap_or_else(|| panic!("{item} must be a block item"))
}

#[test]
fn support_breaks_a_plant_and_reports_its_drop() {
    let Some((thread, save_dir)) = world("support") else {
        return;
    };
    let dirt = block_id(&thread, "Soil_Dirt");
    let bush = block_id(&thread, "Plant_Bush");
    thread.set_block(4, 200, 4, dirt).expect("dirt");
    thread.set_block(4, 201, 4, bush).expect("bush");
    thread.simulate(0.05).expect("first simulation tick");
    assert_eq!(thread.get_block(4, 201, 4).expect("read bush"), Some(bush));

    thread.set_block(4, 200, 4, 0).expect("remove dirt");
    let report = thread.simulate(0.05).expect("support simulation tick");
    assert_eq!(thread.get_block(4, 201, 4).expect("read bush"), Some(0));
    assert!(report.support_changes >= 1, "{report:?}");
    assert!(
        report
            .drops
            .iter()
            .any(|(x, y, z, drop)| (*x, *y, *z) == (4, 201, 4)
                && (drop.item_id.is_some() || drop.drop_list_id.is_some())),
        "{report:?}"
    );

    let stone = block_id(&thread, "Rock_Stone");
    let drops = thread.block_drops(stone).expect("stone drops");
    assert_eq!(
        drops.first().and_then(|drop| drop.item_id.as_deref()),
        Some("Rock_Stone_Cobble")
    );
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_supported_falling_block_falls_one_cell_per_tick_and_lands() {
    let Some((thread, save_dir)) = world("falling") else {
        return;
    };
    let stone = block_id(&thread, "Rock_Stone");
    let dirt = block_id(&thread, "Soil_Dirt");
    let barrel = block_id(&thread, "Debug_Falling_Barrel");
    thread.set_block(4, 199, 8, stone).expect("floor");
    thread.set_block(4, 200, 8, dirt).expect("support");
    thread.set_block(4, 201, 8, barrel).expect("barrel");
    thread.simulate(0.05).expect("settle");
    assert_eq!(
        thread.get_block(4, 201, 8).expect("read barrel"),
        Some(barrel)
    );

    thread.set_block(4, 200, 8, 0).expect("remove support");
    thread.simulate(0.05).expect("start falling");
    thread.simulate(0.05).expect("land");
    assert_eq!(
        thread.get_block(4, 200, 8).expect("landed barrel"),
        Some(barrel)
    );
    assert_eq!(thread.simulate(0.05).expect("idle").falling_moves, 0);

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn block_health_damages_and_regenerates_through_the_world_thread() {
    let Some((thread, save_dir)) = world("health") else {
        return;
    };
    let stone = block_id(&thread, "Rock_Stone");
    thread.set_block(4, 200, 20, stone).expect("stone");
    assert!((thread.block_health(4, 200, 20).expect("health") - 1.0).abs() < 1e-6);
    let remaining = thread.damage_block(4, 200, 20, 0.25).expect("damage block");
    assert!((remaining - 0.75).abs() < 1e-6);
    assert!((thread.block_health(4, 200, 20).expect("health") - 0.75).abs() < 1e-6);

    for _ in 0..100 {
        thread.simulate(0.05).expect("regeneration tick");
    }
    assert!(
        thread.block_health(4, 200, 20).expect("health") > 0.75,
        "block health regenerates after the Java delay"
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_forced_random_tick_spreads_grass_to_tagged_soil() {
    let Some((thread, save_dir)) = world("random-tick") else {
        return;
    };
    let stone = block_id(&thread, "Rock_Stone");
    let grass = block_id(&thread, "Soil_Grass");
    let dirt = block_id(&thread, "Soil_Dirt");
    thread.set_block(4, 199, 16, stone).expect("floor");
    thread.set_block(4, 200, 16, grass).expect("grass");
    thread.set_block(5, 200, 16, dirt).expect("dirt");
    // Java `SpreadTo` requires the default light level 6; the generator does
    // not compute local light, so give the block above full sky light.
    thread
        .set_local_light(4, 201, 16, 0xF000)
        .expect("sky light");
    let report = thread
        .force_random_tick(4, 200, 16)
        .expect("force random tick");
    assert_eq!(
        thread.get_block(5, 200, 16).expect("spread target"),
        Some(grass),
        "{report:?}"
    );
    assert!(
        report
            .block_changes
            .iter()
            .any(|change| change.block_id == grass),
        "{report:?}"
    );

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn a_water_source_spreads_into_the_adjacent_cell() {
    let Some((thread, save_dir)) = world("fluid") else {
        return;
    };
    let stone = block_id(&thread, "Rock_Stone");
    let source = thread.fluid_id("Water_Source").expect("Water_Source id");
    let flow = thread.fluid_id("Water").expect("Water id");
    thread.set_block(4, 200, 12, stone).expect("floor");
    thread
        .set_fluid(4, 201, 12, source, 1)
        .expect("place water source");
    // `Water_Source` uses the default 0.5 s flow rate: 10 world ticks.
    let mut report = None;
    for _ in 0..10 {
        report = Some(thread.simulate(0.05).expect("fluid tick"));
    }
    let report = report.expect("fluid ticks");
    let (fluid_id, level) = thread
        .get_fluid(5, 201, 12)
        .expect("read spread fluid")
        .expect("loaded chunk");
    assert_eq!(fluid_id, flow, "{report:?}");
    assert_eq!(level, 7, "{report:?}");
    assert!(!report.fluid_changes.is_empty(), "{report:?}");

    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn forced_weather_overrides_the_real_environment_forecast() {
    if !assets_available() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return;
    }
    let save_dir = temp_dir("forced-weather");
    let uuid = WorldUuid::derive(7, "forced-weather");
    let (mut save, _) = WorldSave::open(&save_dir, 7, Some("ForcedWeather".to_string()), uuid)
        .expect("create the save");
    save.config_mut().forced_weather = Some("Zone1_Cloudy_Medium".to_string());
    save.save_config().expect("save config");
    drop(save);

    let options = WorldThreadOptions::with_defaults(
        GeneratorOptions {
            assets_pack: repo_root().join("Assets/Assets.zip"),
            assets_root: repo_root().join("Assets/extracted"),
            world_dir: repo_root().join("testdata/Default"),
            workers: 1,
        },
        save_dir.clone(),
    );
    let (thread, _start) = WorldThread::spawn(options).expect("spawn the world thread");
    while thread.tick().expect("tick").remaining > 0 {}
    let weather = thread
        .weather_at(4, 200, 16)
        .expect("weather lookup")
        .expect("forced weather");
    assert!(weather > 0);
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}

#[test]
fn weather_at_resolves_the_real_environment_forecast() {
    let Some((thread, save_dir)) = world("weather") else {
        return;
    };
    let weather = thread
        .weather_at(4, 200, 16)
        .expect("weather lookup")
        .expect("the default environment has a forecast");
    assert!(weather > 0, "Unknown must not win the forecast: {weather}");
    // Forced/live weather packets are covered by the world-clock/weather state
    // tests; this checks the world-thread asset wiring.
    thread.shutdown().expect("shutdown");
    std::fs::remove_dir_all(save_dir).ok();
}
