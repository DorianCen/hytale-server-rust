// Ported from: `.hytale-source/.../worldgen/loader/container/FadeContainerJsonLoader.java`
//! Java `loader.container.FadeContainerJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::container::fade_container::FadeContainer;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `FadeContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_FADE_START: &str = "FadeStart";
    pub const KEY_FADE_LENGTH: &str = "FadeLength";
    pub const KEY_TERRAIN_START: &str = "TerrainStart";
    pub const KEY_TERRAIN_LENGTH: &str = "TerrainLength";
    pub const KEY_FADE_HEIGHTMAP: &str = "FadeHeightmap";
}

/// Java `FadeContainerJsonLoader`.
pub struct FadeContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, FadeContainer>`.
    pub base: JsonLoaderBase,
}

impl FadeContainerJsonLoader {
    /// Java `FadeContainerJsonLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".FadeContainer"), data_folder, json),
        }
    }

    /// Java `FadeContainer load()`.
    pub fn load(&self) -> FadeContainer {
        FadeContainer::new(
            self.load_fade_start(),
            self.load_fade_length(),
            self.load_terrain_start(),
            self.load_terrain_length(),
            self.load_fade_heightmap(),
        )
    }

    fn load_fade_start(&self) -> f64 {
        if self.base.has(constants::KEY_FADE_START) {
            JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_FADE_START).unwrap())
        } else {
            0.0
        }
    }

    fn load_fade_length(&self) -> f64 {
        if self.base.has(constants::KEY_FADE_LENGTH) {
            JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_FADE_LENGTH).unwrap())
        } else {
            0.0
        }
    }

    fn load_terrain_start(&self) -> f64 {
        if self.base.has(constants::KEY_TERRAIN_START) {
            JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_TERRAIN_START).unwrap())
        } else {
            0.0
        }
    }

    fn load_terrain_length(&self) -> f64 {
        if self.base.has(constants::KEY_TERRAIN_LENGTH) {
            JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_TERRAIN_LENGTH).unwrap())
        } else {
            0.0
        }
    }

    /// Java `double loadFadeHeightmap()` — defaults to
    /// `Double.NEGATIVE_INFINITY`.
    fn load_fade_heightmap(&self) -> f64 {
        let mut modifier = FadeContainer::NO_FADE_HEIGHTMAP;
        if self.base.has(constants::KEY_FADE_HEIGHTMAP) {
            modifier = JsonLoaderBase::get_as_double(
                &self.base.get(constants::KEY_FADE_HEIGHTMAP).unwrap(),
            );
        }
        modifier
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seed_appends_the_fade_container_suffix() {
        let loader = FadeContainerJsonLoader::new(SeedString::new("0"), Path::new("."), None);
        assert_eq!(loader.base.seed.seed(), "0.FadeContainer");
    }

    #[test]
    fn missing_keys_use_the_java_defaults() {
        let loader = FadeContainerJsonLoader::new(SeedString::new("0"), Path::new("."), None);
        let fade = loader.load();
        assert_eq!(fade.get_mask_fade_start(), 0.0);
        assert_eq!(fade.get_mask_fade_length(), 0.0);
        assert_eq!(fade.get_height_fade_start(), 0.0);
        assert_eq!(fade.get_height_fade_length(), 0.0);
        assert_eq!(fade.get_fade_heightmap(), FadeContainer::NO_FADE_HEIGHTMAP);
    }
}
