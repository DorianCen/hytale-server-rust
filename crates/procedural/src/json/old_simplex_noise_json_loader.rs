// Ported from: `.hytale-source/.../procedurallib/json/OldSimplexNoiseJsonLoader.java`
//! Java `OldSimplexNoiseJsonLoader` — `OldSimplexNoise` is a stateless
//! singleton; no JSON keys are read.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::old_simplex_noise::OldSimplexNoise;

/// Java `OldSimplexNoiseJsonLoader`.
pub struct OldSimplexNoiseJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl OldSimplexNoiseJsonLoader {
    /// Java `OldSimplexNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".OldSimplexNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()` — `OldSimplexNoise.INSTANCE`.
    pub fn load(&self) -> OldSimplexNoise {
        OldSimplexNoise::new()
    }
}
