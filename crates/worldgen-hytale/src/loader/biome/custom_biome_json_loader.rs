// Ported from: `.hytale-source/.../worldgen/loader/biome/CustomBiomeJsonLoader.java`
//! Java `loader.biome.CustomBiomeJsonLoader`.
//!
//! ⚠ The Java `SEED_PREFIX` is `"TileBiome-%s"` (a copy-paste bug — no leading
//! dot, and the tile-biome prefix). It is transcribed verbatim because the
//! seed suffixes feed the noise derivation.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::biome::custom_biome::CustomBiome;
use crate::biome::custom_biome_generator::CustomBiomeGenerator;
use crate::biome::tile_biome::TileBiome;
use crate::loader::biome::biome_json_loader::BiomeJsonLoader;
use crate::loader::biome::custom_biome_generator_json_loader::CustomBiomeGeneratorJsonLoader;
use crate::loader::context::biome_file_context::BiomeFileContext;
use procedural::json::SeedString;

/// Java `CustomBiomeJsonLoader.Constants`.
pub mod constants {
    pub const KEY_CUSTOM_BIOME_GENERATOR: &str = "CustomBiomeGenerator";
    /// Java `SEED_PREFIX` — verbatim (see the module note).
    pub const SEED_PREFIX: &str = "TileBiome-%s";
}

/// Java `loader.biome.CustomBiomeJsonLoader`.
pub struct CustomBiomeJsonLoader {
    /// Java `extends BiomeJsonLoader`.
    pub base: BiomeJsonLoader,
    /// Java `protected final Biome[] tileBiomes`.
    pub tile_biomes: Vec<Rc<TileBiome>>,
}

impl CustomBiomeJsonLoader {
    /// Java `CustomBiomeJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, BiomeFileContext biomeContext, Biome[] tileBiomes)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        tile_biomes: Vec<Rc<TileBiome>>,
        registries: Rc<Registries>,
    ) -> Self {
        let name = biome_context.base.get_name().to_string();
        let seed = seed.append(&format!("TileBiome-{name}"));
        Self {
            base: BiomeJsonLoader::new(seed, data_folder, json, biome_context, registries),
            tile_biomes,
        }
    }

    /// Java `CustomBiome load()`.
    pub fn load(&self) -> Rc<CustomBiome> {
        // Java `ZoneCustomBiomesJsonLoader` throws an NPE (message = the
        // biome path) when the custom biome has no generator. The Rust
        // `CustomBiome` cannot hold a null generator, so the same panic is
        // raised here.
        let generator = self
            .load_custom_biome_generator()
            .unwrap_or_else(|| panic!("{}", self.base.biome_context.base.get_path().display()));
        Rc::new(CustomBiome::new(
            self.base.biome_context.base.get_id(),
            self.base.biome_context.base.get_name().to_string(),
            self.base.load_interpolation(),
            generator,
            self.base.load_terrain_height_threshold(),
            self.base.load_cover_container(),
            self.base.load_layer_containers(),
            self.base.load_prefab_container(),
            self.base.load_tint_container(),
            self.base.load_environment_container(),
            self.base.load_water_container(),
            self.base.load_fade_container(),
            self.base.load_heightmap_noise(),
            self.base.load_color(),
        ))
    }

    /// Java `@Nullable CustomBiomeGenerator loadCustomBiomeGenerator()`.
    pub fn load_custom_biome_generator(&self) -> Option<Rc<CustomBiomeGenerator>> {
        if self.base.base.has(constants::KEY_CUSTOM_BIOME_GENERATOR) {
            return Some(
                CustomBiomeGeneratorJsonLoader::new(
                    self.base.base.seed.clone(),
                    &self.base.base.data_folder,
                    self.base.base.get(constants::KEY_CUSTOM_BIOME_GENERATOR),
                    self.base.biome_context.clone(),
                    self.tile_biomes.clone(),
                )
                .load(),
            );
        }
        None
    }
}
