// Ported from: `.hytale-source/.../worldgen/loader/zone/ZoneCustomBiomesJsonLoader.java`
//! Java `loader.zone.ZoneCustomBiomesJsonLoader` — the custom biomes of a
//! zone, sorted by descending `CustomBiomeGenerator` priority.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::biome::custom_biome::CustomBiome;
use crate::biome::tile_biome::TileBiome;
use crate::loader::biome::custom_biome_json_loader::CustomBiomeJsonLoader;
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::load_json_object;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ZoneCustomBiomesJsonLoader.Constants`.
pub mod constants {
    pub const ERROR_BIOME_FILES_NULL: &str = "Biome files error occured.";
    pub const ERROR_BIOME_FAILED: &str = "Error while loading custom biome \"%s\" from \"%s\"";
    pub const ERROR_NO_CUSTOM_GENERATOR: &str =
        "Could not find custom biome generator for custom biome \"%s\" at \"%s\"";

    pub const FILE_CUSTOM_PREFIX: &str = "Custom.";
    pub const FILE_CUSTOM_SUFFIX: &str = ".json";
}

/// Java `loader.zone.ZoneCustomBiomesJsonLoader`.
pub struct ZoneCustomBiomesJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CustomBiome[]>`.
    pub base: JsonLoaderBase,
    /// Java `protected final ZoneFileContext zoneContext`.
    pub zone_context: Rc<ZoneFileContext>,
    /// Java `protected final Biome[] tileBiomes`.
    pub tile_biomes: Vec<Rc<TileBiome>>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl ZoneCustomBiomesJsonLoader {
    /// Java `ZoneCustomBiomesJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, ZoneFileContext zoneContext, Biome[] tileBiomes)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
        tile_biomes: Vec<Rc<TileBiome>>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            zone_context,
            tile_biomes,
            registries,
        }
    }

    /// Java `CustomBiome[] load()`.
    pub fn load(&self) -> Vec<Rc<CustomBiome>> {
        let mut biomes = Vec::new();
        let custom_biomes = self.zone_context.get_custom_biomes().borrow();
        for (_, biome_context) in custom_biomes.iter() {
            let biome = self.load_biome(Rc::clone(biome_context));
            biomes.push(biome);
        }

        // Java `Arrays.sort(biomes, PRIORITY_SORTER)` — descending priority.
        biomes.sort_by(|a, b| {
            b.get_custom_biome_generator()
                .get_priority()
                .cmp(&a.get_custom_biome_generator().get_priority())
        });
        biomes
    }

    /// Java `CustomBiome loadBiome(BiomeFileContext biomeContext)`.
    fn load_biome(&self, biome_context: Rc<BiomeFileContext>) -> Rc<CustomBiome> {
        let name = biome_context.base.get_name().to_string();
        let path = biome_context.base.get_path().to_path_buf();
        let tile_biomes = self.tile_biomes.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let biome_json = load_json_object(&path);
            CustomBiomeJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                Some(biome_json),
                Rc::clone(&biome_context),
                tile_biomes,
                Rc::clone(&self.registries),
            )
            .load()
        }));
        match result {
            Ok(biome) => biome,
            Err(_) => panic!(
                "{}",
                constants::ERROR_BIOME_FAILED
                    .replacen("%s", &name, 1)
                    .replacen("%s", &path.display().to_string(), 1)
            ),
        }
    }
}
