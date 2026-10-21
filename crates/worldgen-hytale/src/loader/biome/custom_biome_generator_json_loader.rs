// Ported from: `.hytale-source/.../worldgen/loader/biome/CustomBiomeGeneratorJsonLoader.java`
//! Java `loader.biome.CustomBiomeGeneratorJsonLoader`.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::biome::biome::Biome;
use crate::biome::custom_biome_generator::CustomBiomeGenerator;
use crate::biome::tile_biome::TileBiome;
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::util::condition::HashSetIntCondition;
use procedural::json::double_threshold_json_loader::DoubleThresholdJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::ConstantIntCondition;
use procedural::noise::condition::{IDoubleThreshold, IIntCondition};
use procedural::noise::property::NoiseProperty;

/// Java `CustomBiomeGeneratorJsonLoader.Constants`.
pub mod constants {
    pub const KEY_NOISE_MASK: &str = "NoiseMask";
    pub const KEY_BIOME_MASK: &str = "BiomeMask";
    pub const KEY_PRIORITY: &str = "Priority";

    pub const ERROR_BIOME_ERROR_MASK: &str =
        "Could not find tile biome \"%s\" for biome mask. Typo or disabled tile biome?";
}

/// Java `loader.biome.CustomBiomeGeneratorJsonLoader`.
pub struct CustomBiomeGeneratorJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CustomBiomeGenerator>`.
    pub base: JsonLoaderBase,
    /// Java `protected final BiomeFileContext biomeContext`.
    pub biome_context: Rc<BiomeFileContext>,
    /// Java `protected final Biome[] tileBiomes`.
    pub tile_biomes: Vec<Rc<TileBiome>>,
}

impl CustomBiomeGeneratorJsonLoader {
    /// Java `CustomBiomeGeneratorJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, BiomeFileContext biomeContext, Biome[] tileBiomes)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        tile_biomes: Vec<Rc<TileBiome>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CustomBiomeGenerator"), data_folder, json),
            biome_context,
            tile_biomes,
        }
    }

    /// Java `CustomBiomeGenerator load()`.
    pub fn load(&self) -> Rc<CustomBiomeGenerator> {
        Rc::new(CustomBiomeGenerator::new(
            self.load_noise_property(),
            self.load_noise_threshold(),
            self.load_biome_mask(),
            self.load_priority(),
        ))
    }

    /// Java `@Nullable NoiseProperty loadNoiseProperty()`.
    pub fn load_noise_property(&self) -> Rc<dyn NoiseProperty> {
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE_MASK),
        )
        .load()
    }

    /// Java `IDoubleThreshold loadNoiseThreshold()`.
    pub fn load_noise_threshold(&self) -> Rc<dyn IDoubleThreshold> {
        let noise_mask = self.base.get(constants::KEY_NOISE_MASK).unwrap();
        let threshold = noise_mask
            .as_object()
            .unwrap()
            .get(procedural::json::noise_mask_condition_json_loader::constants::KEY_THRESHOLD)
            .cloned();
        DoubleThresholdJsonLoader::new(self.base.seed.clone(), &self.base.data_folder, threshold)
            .load()
    }

    /// Java `IIntCondition loadBiomeMask()`.
    pub fn load_biome_mask(&self) -> Rc<dyn IIntCondition> {
        let mut biome_mask: Rc<dyn IIntCondition> = Rc::new(ConstantIntCondition::DEFAULT_TRUE);
        if self.base.has(constants::KEY_BIOME_MASK) {
            let name_biome_map = Self::generate_name_biome_mapping(&self.tile_biomes);
            let biome_mask_array = self.base.get(constants::KEY_BIOME_MASK).unwrap();
            let Some(array) = biome_mask_array.as_array() else {
                panic!("Invalid json-type for BiomeMask property. Must be an array!");
            };
            let mut biome_set = HashSet::with_capacity(array.len());
            for element in array {
                let biome_name = JsonLoaderBase::get_as_string(element);
                let Some(biome) = name_biome_map.get(&biome_name) else {
                    // Java `Objects.requireNonNull(biome, biomeName)`.
                    panic!("{biome_name}");
                };
                biome_set.insert(biome.get_id());
            }
            biome_mask = Rc::new(HashSetIntCondition::new(biome_set));
        }
        biome_mask
    }

    /// Java `Map<String, Biome> generateNameBiomeMapping()`.
    fn generate_name_biome_mapping(
        tile_biomes: &[Rc<TileBiome>],
    ) -> HashMap<String, Rc<TileBiome>> {
        let mut map = HashMap::new();
        for biome in tile_biomes {
            map.insert(biome.get_name().to_string(), Rc::clone(biome));
        }
        map
    }

    /// Java `int loadPriority()`.
    pub fn load_priority(&self) -> i32 {
        if self.base.has(constants::KEY_PRIORITY) {
            return JsonLoaderBase::get_as_int(&self.base.get(constants::KEY_PRIORITY).unwrap());
        }
        0
    }
}
