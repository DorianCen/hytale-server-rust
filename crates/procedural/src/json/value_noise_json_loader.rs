// Ported from: `.hytale-source/.../procedurallib/json/ValueNoiseJsonLoader.java`
//! Java `ValueNoiseJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::json::perlin_noise_json_loader::PerlinNoiseJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::general_noise::InterpolationMode;
use crate::noise::value_noise::ValueNoise;

/// Java `ValueNoiseJsonLoader`.
pub struct ValueNoiseJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl ValueNoiseJsonLoader {
    /// Java `ValueNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".ValueNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> ValueNoise {
        ValueNoise::new(self.load_interpolation_function())
    }

    /// Java `GeneralNoise.InterpolationFunction loadInterpolationFunction()`
    /// (same shape as the Perlin loader).
    pub fn load_interpolation_function(&self) -> InterpolationMode {
        const KEY_INTERPOLATION_MODE: &str = "InterpolationMode";
        let mut interpolation_mode = InterpolationMode::Quintic;
        if self.base.has(KEY_INTERPOLATION_MODE) {
            interpolation_mode = PerlinNoiseJsonLoader::interpolation_mode_value_of(
                &JsonLoaderBase::get_as_string(&self.base.get(KEY_INTERPOLATION_MODE).unwrap()),
            );
        }
        interpolation_mode
    }
}
