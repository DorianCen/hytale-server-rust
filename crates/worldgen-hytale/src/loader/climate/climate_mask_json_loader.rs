// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimateMaskJsonLoader.java`
//! Java `loader.climate.ClimateMaskJsonLoader` — loads the `Mask.json`
//! configuration of a world into a [`ClimateMaskProvider`].

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::climate::climate_graph::ClimateGraph;
use crate::climate::climate_mask_provider::ClimateMaskProvider;
use crate::climate::climate_noise::ClimateNoise;
use crate::climate::unique_climate_generator::UniqueClimateGenerator;
use crate::loader::climate::climate_graph_json_loader::{ClimateGraphJsonLoader, KEY_CLIMATE};
use crate::loader::climate::climate_noise_json_loader::{ClimateNoiseJsonLoader, KEY_NOISE};
use crate::loader::climate::unique_climate_generator_json_loader::UniqueClimateGeneratorJsonLoader;
use procedural::json::coordinate_randomizer_json_loader::CoordinateRandomizerJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::random::{EmptyCoordinateRandomizer, ICoordinateRandomizer};

/// Java `ClimateMaskJsonLoader.Constants.KEY_RANDOMIZER`.
pub const KEY_RANDOMIZER: &str = "Randomizer";
/// Java `ClimateMaskJsonLoader.Constants.KEY_UNIQUE_ZONES`.
pub const KEY_UNIQUE_ZONES: &str = "UniqueZones";

/// Java `ClimateMaskJsonLoader`.
pub struct ClimateMaskJsonLoader {
    /// Java `extends JsonLoader<K, ClimateMaskProvider>`.
    pub base: JsonLoaderBase,
}

impl ClimateMaskJsonLoader {
    /// Java `ClimateMaskJsonLoader(SeedString<K> seed, Path dataFolder, Path maskFile)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, mask_file: &Path) -> Self {
        let json = Self::load_mask_file_json(mask_file);
        Self {
            base: JsonLoaderBase::new(seed, data_folder, Some(json)),
        }
    }

    /// Java `ClimateMaskProvider load()`.
    pub fn load(&self) -> Rc<ClimateMaskProvider> {
        Rc::new(ClimateMaskProvider::new(
            self.load_randomizer(),
            Rc::new(self.load_climate_noise()),
            Rc::new(self.load_climate_graph()),
            Rc::new(self.load_unique_climate_generator()),
        ))
    }

    /// Java `ICoordinateRandomizer loadRandomizer()`.
    pub fn load_randomizer(&self) -> Rc<dyn ICoordinateRandomizer> {
        if self.base.has(KEY_RANDOMIZER) {
            return CoordinateRandomizerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(KEY_RANDOMIZER),
            )
            .load();
        }
        Rc::new(EmptyCoordinateRandomizer)
    }

    /// Java `ClimateNoise loadClimateNoise()`.
    pub fn load_climate_noise(&self) -> ClimateNoise {
        ClimateNoiseJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(self.base.must_get_object(KEY_NOISE, None)),
        )
        .load()
    }

    /// Java `ClimateGraph loadClimateGraph()`.
    pub fn load_climate_graph(&self) -> ClimateGraph {
        ClimateGraphJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(self.base.must_get_object(KEY_CLIMATE, None)),
        )
        .load()
    }

    /// Java `UniqueClimateGenerator loadUniqueClimateGenerator()`.
    pub fn load_unique_climate_generator(&self) -> UniqueClimateGenerator {
        UniqueClimateGeneratorJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(
                self.base
                    .must_get_array(KEY_UNIQUE_ZONES, Some(serde_json::json!([]))),
            ),
        )
        .load()
    }

    /// Java `static JsonObject loadMaskFileJson(Path file)`.
    pub fn load_mask_file_json(file: &Path) -> Value {
        match std::fs::read_to_string(file) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(value) => {
                    if !value.is_object() {
                        panic!(
                            "Failed to load Mask.json: {} is not a JSON object",
                            file.display()
                        );
                    }
                    value
                }
                Err(e) => panic!("Failed to load Mask.json: {e}"),
            },
            Err(e) => panic!("Failed to load Mask.json: {e}"),
        }
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
    fn missing_mask_file_panics() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ClimateMaskJsonLoader::new(
                SeedString::new("0"),
                Path::new("."),
                Path::new("/definitely/missing/Mask.json"),
            )
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert!(message.starts_with("Failed to load Mask.json"), "{message}");
    }
}
