// Ported from: `.hytale-source/.../procedurallib/json/PerlinNoiseJsonLoader.java`
//! Java `PerlinNoiseJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::general_noise::InterpolationMode;
use crate::noise::perlin_noise::PerlinNoise;

/// Java `PerlinNoiseJsonLoader`.
pub struct PerlinNoiseJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl PerlinNoiseJsonLoader {
    /// Java `PerlinNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".PerlinNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> PerlinNoise {
        PerlinNoise::new(self.load_interpolation_function())
    }

    /// Java `GeneralNoise.InterpolationFunction loadInterpolationFunction()`.
    pub fn load_interpolation_function(&self) -> InterpolationMode {
        const KEY_INTERPOLATION_MODE: &str = "InterpolationMode";
        // Java default: GeneralNoise.InterpolationMode.QUINTIC.
        let mut interpolation_mode = InterpolationMode::Quintic;
        if self.base.has(KEY_INTERPOLATION_MODE) {
            interpolation_mode = Self::interpolation_mode_value_of(&JsonLoaderBase::get_as_string(
                &self.base.get(KEY_INTERPOLATION_MODE).unwrap(),
            ));
        }
        interpolation_mode
    }

    /// Java `GeneralNoise.InterpolationMode.valueOf(String)`.
    pub fn interpolation_mode_value_of(name: &str) -> InterpolationMode {
        match name {
            "LINEAR" => InterpolationMode::Linear,
            "HERMITE" => InterpolationMode::Hermite,
            "QUINTIC" => InterpolationMode::Quintic,
            other => panic!("No enum constant GeneralNoise.InterpolationMode.{other}"),
        }
    }
}
