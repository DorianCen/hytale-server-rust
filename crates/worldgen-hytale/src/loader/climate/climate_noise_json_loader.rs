// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimateNoiseJsonLoader.java`
//! Java `loader.climate.ClimateNoiseJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::climate::climate_noise::{ClimateNoise, Grid, Thresholds};
use crate::loader::climate::climate_grid_json_loader::ClimateGridJsonLoader;
use crate::loader::climate::climate_point_json_loader::{KEY_INTENSITY, KEY_TEMPERATURE};
use crate::loader::climate::continent_thresholds_json_loader::ContinentThresholdsJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::property::NoiseProperty;

/// Java `ClimateNoiseJsonLoader.Constants.KEY_NOISE`.
pub const KEY_NOISE: &str = "Noise";
/// Java `ClimateNoiseJsonLoader.Constants.KEY_GRID`.
pub const KEY_GRID: &str = "Grid";
/// Java `ClimateNoiseJsonLoader.Constants.KEY_THRESHOLDS`.
pub const KEY_THRESHOLDS: &str = "Thresholds";
/// Java `ClimateNoiseJsonLoader.Constants.KEY_CONTINENT`.
pub const KEY_CONTINENT: &str = "Continent";

/// Java `ClimateNoiseJsonLoader`.
pub struct ClimateNoiseJsonLoader {
    /// Java `extends JsonLoader<K, ClimateNoise>`.
    pub base: JsonLoaderBase,
}

impl ClimateNoiseJsonLoader {
    /// Java `ClimateNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix (the
    /// nested `NoisePropertyJsonLoader` appends `".NoiseProperty"`).
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `ClimateNoise load()`.
    pub fn load(&self) -> ClimateNoise {
        ClimateNoise::new(
            self.load_grid(),
            self.load_continent_noise(),
            self.load_temperature_noise(),
            self.load_intensity_noise(),
            self.load_thresholds(),
        )
    }

    /// Java `ClimateNoise.Grid loadGrid()`.
    pub fn load_grid(&self) -> Grid {
        ClimateGridJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(KEY_GRID),
        )
        .load()
    }

    /// Java `NoiseProperty loadContinentNoise()`.
    pub fn load_continent_noise(&self) -> Rc<dyn NoiseProperty> {
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(self.base.must_get_object(KEY_CONTINENT, None)),
        )
        .load()
    }

    /// Java `NoiseProperty loadTemperatureNoise()`.
    pub fn load_temperature_noise(&self) -> Rc<dyn NoiseProperty> {
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(self.base.must_get_object(KEY_TEMPERATURE, None)),
        )
        .load()
    }

    /// Java `NoiseProperty loadIntensityNoise()`.
    pub fn load_intensity_noise(&self) -> Rc<dyn NoiseProperty> {
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(self.base.must_get_object(KEY_INTENSITY, None)),
        )
        .load()
    }

    /// Java `ClimateNoise.Thresholds loadThresholds()`.
    pub fn load_thresholds(&self) -> Thresholds {
        ContinentThresholdsJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(self.base.must_get_object(KEY_THRESHOLDS, None)),
        )
        .load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracts the message of a caught panic.
    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            return (*message).to_string();
        }
        "<non-string panic>".to_string()
    }

    #[test]
    fn missing_thresholds_panic() {
        let loader = ClimateNoiseJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({
                "Continent": { "NoiseType": "CONSTANT", "Value": 0.5 },
                "Temperature": { "NoiseType": "CONSTANT", "Value": 0.5 },
                "Intensity": { "NoiseType": "CONSTANT", "Value": 0.5 }
            })),
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader.load()));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert_eq!(message, "Missing property 'Thresholds'");
    }

    #[test]
    fn grid_defaults_to_the_direct_grid() {
        let loader = ClimateNoiseJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({
                "Continent": { "NoiseType": "CONSTANT", "Value": 0.5 },
                "Temperature": { "NoiseType": "CONSTANT", "Value": 0.5 },
                "Intensity": { "NoiseType": "CONSTANT", "Value": 0.5 },
                "Thresholds": {}
            })),
        );
        let noise = loader.load();
        assert_eq!(noise.grid.seed_offset, 0);
        assert_eq!(noise.grid.scale, 1.0);
        assert_eq!(noise.thresholds.land, 0.5);
        assert_eq!(noise.thresholds.island, 0.8);
    }
}
