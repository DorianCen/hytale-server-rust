// Ported from: `.hytale-source/.../procedurallib/json/SimplexNoiseJsonLoader.java`
//! Java `SimplexNoiseJsonLoader` — `SimplexNoise` is a stateless singleton;
//! no JSON keys are read.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::simplex_noise::SimplexNoise;

/// Java `SimplexNoiseJsonLoader`.
pub struct SimplexNoiseJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl SimplexNoiseJsonLoader {
    /// Java `SimplexNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".SimplexNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()` — `SimplexNoise.INSTANCE`.
    pub fn load(&self) -> SimplexNoise {
        SimplexNoise::new()
    }
}
