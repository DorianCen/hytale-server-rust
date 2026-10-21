// Ported from: `.hytale-source/.../worldgen/loader/zone/ZoneJsonLoader.java`
//! Java `loader.zone.ZoneJsonLoader` — the JSON loader of a [`Zone`].
//!
//! The Java loader evaluates the record components left-to-right (the
//! discovery config, the cave generator, the biome pattern generator and the
//! unique prefab container) and wraps every failure in an `Error` with the
//! matching message.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::biome::biome_pattern_generator::BiomePatternGenerator;
use crate::biome::custom_biome::CustomBiome;
use crate::biome::tile_biome::TileBiome;
use crate::cave::cave_generator::CaveGenerator;
use crate::container::unique_prefab_container::UniquePrefabContainer;
use crate::loader::biome::biome_pattern_generator_json_loader::BiomePatternGeneratorJsonLoader;
use crate::loader::cave::cave_generator_json_loader::CaveGeneratorJsonLoader;
use crate::loader::container::unique_prefab_container_json_loader::UniquePrefabContainerJsonLoader;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::seed_string_resource::SeedStringResource;
use crate::loader::zone::zone_biomes_json_loader::ZoneBiomesJsonLoader;
use crate::loader::zone::zone_custom_biomes_json_loader::ZoneCustomBiomesJsonLoader;
use crate::weighted_map::IWeightedMap;
use crate::zone::discovery_config::ZoneDiscoveryConfig;
use crate::zone::zone::Zone;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ZoneJsonLoader.Constants`.
pub mod constants {
    pub const KEY_BIOME_GENERATOR: &str = "BiomeGenerator";
    pub const KEY_UNIQUE_PREFABS: &str = "UniquePrefabs";
    pub const KEY_DISCOVERY: &str = "Discovery";
    pub const KEY_DISCOVERY_DISPLAY: &str = "Display";
    pub const KEY_DISCOVERY_ZONE: &str = "ZoneName";
    pub const KEY_DISCOVERY_SOUND_EVENT_ID: &str = "SoundEventId";
    pub const KEY_DISCOVERY_ICON: &str = "Icon";
    pub const KEY_DISCOVERY_MAJOR: &str = "Major";
    pub const KEY_DISCOVERY_DURATION: &str = "Duration";
    pub const KEY_DISCOVERY_FADE_IN_DURATION: &str = "FadeInDuration";
    pub const KEY_DISCOVERY_FADE_OUT_DURATION: &str = "FadeOutDuration";
    pub const PATH_CAVE: &str = "Cave";
    pub const SEED_ZONE_SUFFIX: &str = ".Zone-%s";

    pub const ERROR_BIOME_GENERATOR: &str = "Error while loading biome generator.";
    pub const ERROR_TILE_BIOMES: &str = "Error while loading tile biomes.";
    pub const ERROR_CUSTOM_BIOMES: &str = "Error while loading custom biomes.";
    pub const ERROR_CAVE_GENERATOR: &str = "Error while loading cave generator.";
    pub const ERROR_UNIQUE_PREFABS: &str = "Error while loading unique prefabs.";
}

/// Java `loader.zone.ZoneJsonLoader`.
pub struct ZoneJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, Zone>`.
    pub base: JsonLoaderBase,
    /// Java `protected final ZoneFileContext zoneContext`.
    pub zone_context: Rc<ZoneFileContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    pub resource: Rc<SeedStringResource>,
}

impl ZoneJsonLoader {
    /// Java `ZoneJsonLoader(SeedString<SeedStringResource> seed, Path
    /// dataFolder, JsonElement json, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
        registries: Rc<Registries>,
        resource: Rc<SeedStringResource>,
    ) -> Self {
        let name = zone_context.base.get_name().to_string();
        let seed = seed.append(&format!(".Zone-{name}"));
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            zone_context,
            registries,
            resource,
        }
    }

    /// Java `Zone load()`.
    pub fn load(&self) -> Rc<Zone> {
        // Java evaluates the record components left-to-right: discovery, then
        // the cave generator, then the biome pattern generator, then the
        // unique prefab container. The order matters because the cave/unique
        // seams fire before the biome chain.
        let discovery_config = self.load_discovery_config();
        let cave_generator = self.load_cave_generator();
        let biome_pattern_generator = self.load_biome_pattern_generator();
        let unique_prefab_container = self.load_unique_prefab_container();

        Rc::new(Zone {
            id: self.zone_context.base.get_id(),
            name: self.zone_context.base.get_name().to_string(),
            discovery_config,
            cave_generator,
            biome_pattern_generator,
            unique_prefab_container,
        })
    }

    /// Java `ZoneDiscoveryConfig loadDiscoveryConfig()`.
    pub fn load_discovery_config(&self) -> Rc<ZoneDiscoveryConfig> {
        let Some(discovery_element) = self.base.get(constants::KEY_DISCOVERY) else {
            return Rc::new(ZoneDiscoveryConfig::default());
        };
        if !discovery_element.is_object() {
            return Rc::new(ZoneDiscoveryConfig::default());
        }

        let object = discovery_element.as_object().unwrap();
        let get_optional = |key: &str| object.get(key).filter(|v| !v.is_null()).cloned();

        let display =
            get_optional(constants::KEY_DISCOVERY_DISPLAY).map(|v| JsonLoaderBase::get_as_bool(&v));
        let zone_name =
            get_optional(constants::KEY_DISCOVERY_ZONE).map(|v| JsonLoaderBase::get_as_string(&v));
        let sound_event_id = get_optional(constants::KEY_DISCOVERY_SOUND_EVENT_ID)
            .map(|v| JsonLoaderBase::get_as_string(&v));
        let icon =
            get_optional(constants::KEY_DISCOVERY_ICON).map(|v| JsonLoaderBase::get_as_string(&v));
        let major =
            get_optional(constants::KEY_DISCOVERY_MAJOR).map(|v| JsonLoaderBase::get_as_bool(&v));
        let duration = get_optional(constants::KEY_DISCOVERY_DURATION)
            .map(|v| JsonLoaderBase::get_as_double(&v) as f32);
        let fade_in_duration = get_optional(constants::KEY_DISCOVERY_FADE_IN_DURATION)
            .map(|v| JsonLoaderBase::get_as_double(&v) as f32);
        let fade_out_duration = get_optional(constants::KEY_DISCOVERY_FADE_OUT_DURATION)
            .map(|v| JsonLoaderBase::get_as_double(&v) as f32);

        Rc::new(ZoneDiscoveryConfig::of(
            display,
            zone_name,
            sound_event_id,
            icon,
            major,
            duration,
            fade_in_duration,
            fade_out_duration,
        ))
    }

    /// Java `BiomePatternGenerator loadBiomePatternGenerator()`.
    pub fn load_biome_pattern_generator(&self) -> Rc<BiomePatternGenerator> {
        let tile_biomes = self.load_biomes();
        let custom_biomes = self.load_custom_biomes(&tile_biomes);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            BiomePatternGeneratorJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_BIOME_GENERATOR),
                tile_biomes,
                custom_biomes,
            )
            .load()
        }))
        .unwrap_or_else(|_| panic!("{}", constants::ERROR_BIOME_GENERATOR))
    }

    /// Java `IWeightedMap<TileBiome> loadBiomes()`.
    pub fn load_biomes(&self) -> Rc<dyn IWeightedMap<Rc<TileBiome>>> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ZoneBiomesJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_BIOME_GENERATOR),
                Rc::clone(&self.zone_context),
                Rc::clone(&self.registries),
            )
            .load()
        }))
        .unwrap_or_else(|_| panic!("{}", constants::ERROR_TILE_BIOMES))
    }

    /// Java `CustomBiome[] loadCustomBiomes(Biome[] tileBiomes)`.
    pub fn load_custom_biomes(
        &self,
        tile_biomes: &Rc<dyn IWeightedMap<Rc<TileBiome>>>,
    ) -> Vec<Rc<CustomBiome>> {
        let tile_biomes: Vec<Rc<TileBiome>> = tile_biomes.internal_keys().to_vec();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ZoneCustomBiomesJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_BIOME_GENERATOR),
                Rc::clone(&self.zone_context),
                tile_biomes,
                Rc::clone(&self.registries),
            )
            .load()
        }))
        .unwrap_or_else(|_| panic!("{}", constants::ERROR_CUSTOM_BIOMES))
    }

    /// Java `CaveGenerator loadCaveGenerator()`.
    ///
    /// The Java loader reads the zone's `Cave/Caves.json`; a zone without
    /// that file has **no** cave generator (the Java loader returns `null`).
    pub fn load_cave_generator(&self) -> Option<Rc<dyn CaveGenerator>> {
        let cave_folder = self.zone_context.base.get_path().join(constants::PATH_CAVE);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            CaveGeneratorJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.json.clone(),
                Some(cave_folder),
                Rc::clone(&self.zone_context),
                Rc::clone(&self.resource),
                Rc::clone(&self.registries),
            )
            .load()
        }));
        match result {
            Ok(generator) => generator,
            Err(_) => panic!("{}", constants::ERROR_CAVE_GENERATOR),
        }
    }

    /// Java `UniquePrefabContainer loadUniquePrefabContainer()`.
    ///
    /// The unique prefab container holds the *unique* prefabs of a zone (the
    /// spawn points, the landmarks, the temples, …). The generators are
    /// loaded through the real
    /// [`crate::loader::container::unique_prefab_container_json_loader::UniquePrefabContainerJsonLoader`];
    /// the container keeps its Java seed offset, so the *seed chain* of the
    /// container stays correct.
    pub fn load_unique_prefab_container(&self) -> Rc<UniquePrefabContainer> {
        Rc::new(
            UniquePrefabContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_UNIQUE_PREFABS),
                Rc::clone(&self.zone_context),
                Rc::clone(&self.registries),
            )
            .load(),
        )
    }
}
