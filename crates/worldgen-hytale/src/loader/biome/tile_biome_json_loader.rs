// Ported from: `.hytale-source/.../worldgen/loader/biome/TileBiomeJsonLoader.java`
//! Java `loader.biome.TileBiomeJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::biome::tile_biome::TileBiome;
use crate::loader::biome::biome_json_loader::BiomeJsonLoader;
use crate::loader::context::biome_file_context::BiomeFileContext;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `TileBiomeJsonLoader.Constants`.
pub mod constants {
    pub const KEY_WEIGHT: &str = "Weight";
    pub const KEY_SIZE_MODIFIER: &str = "SizeModifier";
    pub const SEED_PREFIX: &str = ".TileBiome-%s";
}

/// Java `loader.biome.TileBiomeJsonLoader`.
pub struct TileBiomeJsonLoader {
    /// Java `extends BiomeJsonLoader`.
    pub base: BiomeJsonLoader,
}

impl TileBiomeJsonLoader {
    /// Java `TileBiomeJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, BiomeFileContext biomeContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        let name = biome_context.base.get_name().to_string();
        let seed = seed.append(&format!(".TileBiome-{name}"));
        Self {
            base: BiomeJsonLoader::new(seed, data_folder, json, biome_context, registries),
        }
    }

    /// Java `TileBiome load()`.
    pub fn load(&self) -> Rc<TileBiome> {
        Rc::new(TileBiome::new(
            self.base.biome_context.base.get_id(),
            self.base.biome_context.base.get_name().to_string(),
            self.base.load_interpolation(),
            self.base.load_terrain_height_threshold(),
            self.base.load_cover_container(),
            self.base.load_layer_containers(),
            self.base.load_prefab_container(),
            self.base.load_tint_container(),
            self.base.load_environment_container(),
            self.base.load_water_container(),
            self.base.load_fade_container(),
            self.base.load_heightmap_noise(),
            self.load_weight(),
            self.load_size_modifier(),
            self.base.load_color(),
        ))
    }

    /// Java `double loadWeight()`.
    pub fn load_weight(&self) -> f64 {
        let mut weight = 1.0;
        if self.base.base.has(constants::KEY_WEIGHT) {
            weight =
                JsonLoaderBase::get_as_double(&self.base.base.get(constants::KEY_WEIGHT).unwrap());
        }
        weight
    }

    /// Java `double loadSizeModifier()` — squared because the Euclidean
    /// distance is squared.
    pub fn load_size_modifier(&self) -> f64 {
        let mut size_modifier = 1.0;
        if self.base.base.has(constants::KEY_SIZE_MODIFIER) {
            size_modifier = JsonLoaderBase::get_as_double(
                &self.base.base.get(constants::KEY_SIZE_MODIFIER).unwrap(),
            );
        }
        size_modifier * size_modifier
    }
}
