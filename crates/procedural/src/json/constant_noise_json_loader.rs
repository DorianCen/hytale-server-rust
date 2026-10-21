// Ported from: `.hytale-source/.../procedurallib/json/ConstantNoiseJsonLoader.java`
//! Java `ConstantNoiseJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::constant_noise::ConstantNoise;

/// Java `ConstantNoiseJsonLoader`.
pub struct ConstantNoiseJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl ConstantNoiseJsonLoader {
    /// Java `ConstantNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".ConstantNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> ConstantNoise {
        ConstantNoise::new(self.load_value())
    }

    /// Java `double loadValue()` — keyword `Value`, default `0.5`.
    pub fn load_value(&self) -> f64 {
        const KEY_VALUE: &str = "Value";
        const DEFAULT_VALUE: f64 = 0.5;
        if self.base.has(KEY_VALUE) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_VALUE).unwrap())
        } else {
            DEFAULT_VALUE
        }
    }
}
