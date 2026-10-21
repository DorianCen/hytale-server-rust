// Ported from: `.hytale-source/.../worldgen/loader/zone/ZoneBiomesJsonLoader.java`
//! Java `loader.zone.ZoneBiomesJsonLoader` — the weighted tile-biome map of a
//! zone.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::biome::tile_biome::TileBiome;
use crate::loader::biome::tile_biome_json_loader::TileBiomeJsonLoader;
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::load_json_object;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ZoneBiomesJsonLoader.Constants`.
pub mod constants {
    pub const ERROR_BIOME_FILES_NULL: &str = "Biome files error occured.";
    pub const ERROR_BIOME_FAILED: &str = "Error while loading tile biome \"%s\" from \"%s\"";
    pub const ERROR_NO_TILE_BIOMES: &str = "Could not find any tile biomes for this zone!";

    pub const FILE_TILE_PREFIX: &str = "Tile.";
    pub const FILE_TILE_SUFFIX: &str = ".json";
}

/// Java `loader.zone.ZoneBiomesJsonLoader`.
pub struct ZoneBiomesJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, IWeightedMap<TileBiome>>`.
    pub base: JsonLoaderBase,
    /// Java `protected final ZoneFileContext zoneContext`.
    pub zone_context: Rc<ZoneFileContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl ZoneBiomesJsonLoader {
    /// Java `ZoneBiomesJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, ZoneFileContext zone)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            zone_context,
            registries,
        }
    }

    /// Java `IWeightedMap<TileBiome> load()`.
    pub fn load(&self) -> Rc<dyn IWeightedMap<Rc<TileBiome>>> {
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        let tile_biomes = self.zone_context.get_tile_biomes().borrow();
        for (_, biome_context) in tile_biomes.iter() {
            let biome = self.load_biome(Rc::clone(biome_context));
            builder.put(Rc::clone(&biome), biome.get_weight());
        }
        if builder.size() == 0 {
            panic!("{}", constants::ERROR_NO_TILE_BIOMES);
        }
        Rc::new(builder.build())
    }

    /// Java `TileBiome loadBiome(BiomeFileContext biomeContext)`.
    fn load_biome(&self, biome_context: Rc<BiomeFileContext>) -> Rc<TileBiome> {
        let name = biome_context.base.get_name().to_string();
        let path = biome_context.base.get_path().to_path_buf();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let biome_json = load_json_object(&path);
            TileBiomeJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                Some(biome_json),
                Rc::clone(&biome_context),
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
