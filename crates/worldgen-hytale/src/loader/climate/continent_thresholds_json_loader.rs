// Ported from: `.hytale-source/.../worldgen/loader/climate/ContinentThresholdsJsonLoader.java`
//! Java `loader.climate.ContinentThresholdsJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::climate::climate_noise::Thresholds;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ContinentThresholdsJsonLoader.Constants.KEY_LAND`.
pub const KEY_LAND: &str = "Land";
/// Java `ContinentThresholdsJsonLoader.Constants.KEY_ISLAND`.
pub const KEY_ISLAND: &str = "Island";
/// Java `ContinentThresholdsJsonLoader.Constants.KEY_BEACH_SIZE`.
pub const KEY_BEACH_SIZE: &str = "BeachSize";
/// Java `ContinentThresholdsJsonLoader.Constants.KEY_SHALLOW_OCEAN_SIZE`.
pub const KEY_SHALLOW_OCEAN_SIZE: &str = "ShallowOceanSize";

/// Java `ClimateGridJsonLoader` — the thresholds loader.
pub struct ContinentThresholdsJsonLoader {
    /// Java `extends JsonLoader<K, ClimateNoise.Thresholds>`.
    pub base: JsonLoaderBase,
}

impl ContinentThresholdsJsonLoader {
    /// Java `ContinentThresholdsJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `ClimateNoise.Thresholds load()`.
    pub fn load(&self) -> Thresholds {
        Thresholds::new(
            self.load_land_threshold(),
            self.load_island_threshold(),
            self.load_beach_size(),
            self.load_shallow_ocean_size(),
        )
    }

    /// Java `double loadLandThreshold()`.
    pub fn load_land_threshold(&self) -> f64 {
        self.base
            .must_get_number(KEY_LAND, Some(Thresholds::LAND_DEFAULT))
    }

    /// Java `double loadIslandThreshold()`.
    pub fn load_island_threshold(&self) -> f64 {
        self.base
            .must_get_number(KEY_ISLAND, Some(Thresholds::ISLAND_DEFAULT))
    }

    /// Java `double loadBeachSize()`.
    pub fn load_beach_size(&self) -> f64 {
        self.base
            .must_get_number(KEY_BEACH_SIZE, Some(Thresholds::BEACH_SIZE_DEFAULT))
    }

    /// Java `double loadShallowOceanSize()`.
    pub fn load_shallow_ocean_size(&self) -> f64 {
        self.base.must_get_number(
            KEY_SHALLOW_OCEAN_SIZE,
            Some(Thresholds::SHALLOW_OCEAN_SIZE_DEFAULT),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> ContinentThresholdsJsonLoader {
        ContinentThresholdsJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json))
    }

    #[test]
    fn defaults_are_applied() {
        let thresholds = loader(serde_json::json!({})).load();
        assert_eq!(thresholds.land, 0.5);
        assert_eq!(thresholds.island, 0.8);
        assert_eq!(thresholds.beach_size, 0.05);
        assert_eq!(thresholds.shallow_ocean_size, 0.15);
    }

    #[test]
    fn values_are_read() {
        let thresholds = loader(serde_json::json!({
            "Land": 0.5,
            "Island": 0.75,
            "BeachSize": 0.02,
            "ShallowOceanSize": 0.08
        }))
        .load();
        assert_eq!(thresholds.land, 0.5);
        assert_eq!(thresholds.island, 0.75);
        assert_eq!(thresholds.beach_size, 0.02);
        assert_eq!(thresholds.shallow_ocean_size, 0.08);
        // The derived thresholds follow the Java constructor.
        assert_eq!(thresholds.land_shore_inner, 0.48);
        assert_eq!(thresholds.island_shore_inner, 0.77);
        assert_eq!(thresholds.land_shallow_ocean_outer, 0.58);
        assert_eq!(thresholds.island_shallow_ocean_outer, 0.71);
    }
}
