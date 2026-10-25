//! Real-pack weather store and forecast checks.

use std::path::PathBuf;

use hytale_assets::{AssetPack, AssetRegistry};
use hytale_world::weather::{environment_forecasts, WeatherState};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry() -> Option<AssetRegistry> {
    if matches!(
        std::env::var("HYTALE_TEST_ASSETS").as_deref(),
        Ok("0" | "false" | "off")
    ) {
        eprintln!("HYTALE_TEST_ASSETS disables the asset-backed test");
        return None;
    }
    let pack = repo_root().join("Assets/Assets.zip");
    if !pack.is_file() {
        eprintln!("the base-game asset pack is not available — the test is skipped");
        return None;
    }
    let pack = AssetPack::open(pack).expect("open the pack");
    Some(AssetRegistry::load_p0(&pack).expect("load P0"))
}

#[test]
fn weather_ids_and_environment_forecasts_load_from_the_real_pack() {
    let Some(registry) = registry() else {
        return;
    };
    assert_eq!(registry.weather_id("Unknown"), Some(0));
    let cloudy = registry
        .weather_id("Zone1_Cloudy_Medium")
        .expect("Zone1_Cloudy_Medium");
    assert!(cloudy > 0);

    let forecasts = environment_forecasts(&registry);
    let default = registry
        .environments
        .id_of("Default")
        .expect("Default environment");
    let forecast = forecasts.get(&default).expect("Default forecast");
    for hour in 0..24 {
        let selected = forecast
            .select(hour, 0.5)
            .unwrap_or_else(|| panic!("hour {hour} selects a weather"));
        assert!(selected.weather_index > 0);
        assert!(!selected.weather_id.is_empty());
    }

    let mut state = WeatherState::new(None);
    assert!(state.update(0, 0, 1, &forecasts));
    assert_eq!(state.effective_weather(default), Some(cloudy));
    state.set_forced_weather(Some(cloudy));
    assert_eq!(state.effective_weather(default), Some(cloudy));
    state.set_forced_weather(None);
    assert_eq!(state.effective_weather(default), Some(cloudy));
}
