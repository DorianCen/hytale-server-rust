// Ported from: `.hytale-source/.../procedurallib/json/PointNoiseJsonLoader.java`
//! Java `PointNoiseJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::point_noise::PointNoise;

/// Java `PointNoiseJsonLoader`.
pub struct PointNoiseJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, PointNoise>`.
    pub base: JsonLoaderBase,
}

impl PointNoiseJsonLoader {
    /// Java `PointNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `PointNoise load()`.
    pub fn load(&self) -> PointNoise {
        const KEY_X: &str = "X";
        const KEY_Y: &str = "Y";
        const KEY_Z: &str = "Z";
        const KEY_INNER_RADIUS: &str = "InnerRadius";
        const KEY_OUTER_RADIUS: &str = "OuterRadius";
        const DEFAULT_COORD: f64 = 0.0;
        const DEFAULT_RADIUS: f64 = 0.0;

        PointNoise::new(
            self.base.must_get_number(KEY_X, Some(DEFAULT_COORD)),
            self.base.must_get_number(KEY_Y, Some(DEFAULT_COORD)),
            self.base.must_get_number(KEY_Z, Some(DEFAULT_COORD)),
            self.base
                .must_get_number(KEY_INNER_RADIUS, Some(DEFAULT_RADIUS)),
            self.base
                .must_get_number(KEY_OUTER_RADIUS, Some(DEFAULT_RADIUS)),
        )
    }
}
