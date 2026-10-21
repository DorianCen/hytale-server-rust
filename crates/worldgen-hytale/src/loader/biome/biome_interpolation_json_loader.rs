// Ported from: `.hytale-source/.../worldgen/loader/biome/BiomeInterpolationJsonLoader.java`
//! Java `loader.biome.BiomeInterpolationJsonLoader`.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::biome::biome_interpolation::BiomeInterpolation;
use crate::chunk::MAX_RADIUS;
use crate::loader::biome::biome_mask_json_loader::{BiomeMaskJsonLoader, LoadedBiomeMask};
use crate::loader::context::zone_file_context::ZoneFileContext;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `BiomeInterpolationJsonLoader.Constants`.
pub mod constants {
    pub const KEY_DEFAULT_RADIUS: &str = "DefaultRadius";
    pub const KEY_RADIUS: &str = "Radius";
    pub const KEY_BIOMES: &str = "Biomes";
    pub const KEY_MASK: &str = "Mask";

    pub const SEED_OFFSET_MASK: &str = "InterpolationMask";

    pub const ERROR_MISSING_PROPERTY: &str = "Missing property %s";
    pub const ERROR_INVALID_BIOME_LIST: &str =
        "Invalid json-type for Biomes property. Must be an array!";
    pub const ERROR_INVALID_BIOME_ENTRY: &str =
        "Invalid json-type for biome entry. Must be an object!";
    pub const ERROR_DUPLICATE_BIOME: &str = "Duplicate biome detected in interpolation rules";
    pub const ERROR_BIOME_RADIUS: &str = "Biome interpolation radius %s is outside the range 0-%s";
    pub const ERROR_DEFAULT_RADIUS: &str =
        "Default biome interpolation radius %s lies outside the range 0-5";
}

/// Java `loader.biome.BiomeInterpolationJsonLoader`.
pub struct BiomeInterpolationJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, BiomeInterpolation>`.
    pub base: JsonLoaderBase,
    /// Java `protected final ZoneFileContext zoneFileContext`.
    pub zone_file_context: Rc<ZoneFileContext>,
}

impl BiomeInterpolationJsonLoader {
    /// Java `BiomeInterpolationJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, ZoneFileContext zoneFileContext)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_file_context: Rc<ZoneFileContext>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            zone_file_context,
        }
    }

    /// Java `BiomeInterpolation load()`.
    pub fn load(&self) -> BiomeInterpolation {
        let default_radius = self.load_default_radius();
        let biome_radii = self.load_biome_radii(default_radius);
        BiomeInterpolation::create(default_radius, biome_radii)
    }

    /// Java `int loadDefaultRadius()`.
    pub fn load_default_radius(&self) -> i32 {
        if !self.base.has(constants::KEY_DEFAULT_RADIUS) {
            return MAX_RADIUS;
        }

        let radius =
            JsonLoaderBase::get_as_int(&self.base.get(constants::KEY_DEFAULT_RADIUS).unwrap());
        if !(0..=MAX_RADIUS).contains(&radius) {
            panic!(
                "Default biome interpolation radius {radius} lies outside the range 0-{MAX_RADIUS}"
            );
        }

        radius
    }

    /// Java `Int2IntMap loadBiomeRadii(int maxRadius)`.
    pub fn load_biome_radii(&self, max_radius: i32) -> HashMap<i32, i32> {
        if !self.base.has(constants::KEY_BIOMES) {
            return HashMap::new();
        }

        let biomes = self.base.get(constants::KEY_BIOMES).unwrap();
        let Some(array) = biomes.as_array() else {
            panic!("{}", constants::ERROR_INVALID_BIOME_LIST);
        };

        let mut biome_radii = HashMap::new();
        for entry in array {
            self.load_biome_entry(entry, max_radius, &mut biome_radii);
        }

        biome_radii
    }

    /// Java `void loadBiomeEntry(JsonElement entry, int defaultRadius, Int2IntMap biomeRadii)`.
    pub fn load_biome_entry(
        &self,
        entry: &Value,
        default_radius: i32,
        biome_radii: &mut HashMap<i32, i32>,
    ) {
        let Some(object) = entry.as_object() else {
            panic!("{}", constants::ERROR_INVALID_BIOME_ENTRY);
        };

        let radius = Self::load_biome_radius(object, default_radius);
        if radius == default_radius {
            return;
        }

        let mask = self.load_biome_mask(object);
        Self::add_biomes(mask, radius, biome_radii);
    }

    /// Java `@Nullable IIntCondition loadBiomeMask(JsonObject entry)`.
    fn load_biome_mask(&self, entry: &serde_json::Map<String, Value>) -> LoadedBiomeMask {
        let Some(mask) = entry.get(constants::KEY_MASK) else {
            panic!("Missing property {}", constants::KEY_MASK);
        };

        BiomeMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(mask.clone()),
            constants::SEED_OFFSET_MASK,
            self.zone_file_context.clone(),
        )
        .load()
    }

    /// Java `static int loadBiomeRadius(JsonObject entry, int maxRadius)`.
    fn load_biome_radius(entry: &serde_json::Map<String, Value>, max_radius: i32) -> i32 {
        let Some(radius) = entry.get(constants::KEY_RADIUS) else {
            panic!("Missing property {}", constants::KEY_RADIUS);
        };

        let radius = JsonLoaderBase::get_as_int(radius);
        if radius < 0 || radius > max_radius {
            panic!("Biome interpolation radius {radius} is outside the range 0-{max_radius}");
        }

        radius
    }

    /// Java `static void addBiomes(IIntCondition mask, int radius, Int2IntMap biomeRadii)`.
    fn add_biomes(mask: LoadedBiomeMask, radius: i32, biome_radii: &mut HashMap<i32, i32>) {
        let LoadedBiomeMask::Set(biomes) = mask else {
            return;
        };

        let radius2 = radius * radius;
        for biome in biomes {
            if biome_radii.contains_key(&biome) {
                panic!("{}", constants::ERROR_DUPLICATE_BIOME);
            }

            biome_radii.insert(biome, radius2);
        }
    }
}
