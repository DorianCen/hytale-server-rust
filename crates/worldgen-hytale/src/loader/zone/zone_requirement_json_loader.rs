// Ported from: `.hytale-source/.../worldgen/loader/zone/ZoneRequirementJsonLoader.java`
//! Java `loader.zone.ZoneRequirementJsonLoader`.

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use super::unique_zone_entry_json_loader::UniqueZoneEntryJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ZonePatternProviderJsonLoader.Constants.KEY_MASK_MAPPING`.
const KEY_MASK_MAPPING: &str = "MaskMapping";
/// Java `ZonePatternProviderJsonLoader.Constants.KEY_UNIQUE_ZONES`.
const KEY_UNIQUE_ZONES: &str = "UniqueZones";

/// Java `ZoneRequirementJsonLoader`.
pub struct ZoneRequirementJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, Set<String>>`.
    pub base: JsonLoaderBase,
}

impl ZoneRequirementJsonLoader {
    /// Java `ZoneRequirementJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `Set<String> load()`.
    pub fn load(&self) -> HashSet<String> {
        if !self.base.has(KEY_MASK_MAPPING) {
            panic!("Could not find mappings for colors in mask file. Keyword: {KEY_MASK_MAPPING}");
        }

        let mut zone_set = HashSet::new();
        // Note: Java `collectZones` iterates the mapping values but only the
        // unique-zone entries reference zone names through the `Zone` key;
        // the mask-mapping values are zone names directly.
        if let Some(v) = self.base.get(KEY_MASK_MAPPING) {
            Self::collect_mask_mapping_zones(&mut zone_set, Some(&v));
        }
        if let Some(v) = self.base.get(KEY_UNIQUE_ZONES) {
            UniqueZoneEntryJsonLoader::collect_zones(&mut zone_set, Some(&v));
        }

        zone_set
    }

    /// Java `ZoneColorMappingJsonLoader.collectZones(Set<String>, JsonElement)` —
    /// the mask mapping's values are zone names (or arrays of zone names).
    fn collect_mask_mapping_zones(zone_set: &mut HashSet<String>, json: Option<&Value>) {
        let Some(json) = json else {
            return;
        };
        let Some(obj) = json.as_object() else {
            return;
        };

        for (_, value) in obj {
            if let Some(array) = value.as_array() {
                for zone_name in array {
                    zone_set.insert(JsonLoaderBase::get_as_string(zone_name));
                }
            } else {
                zone_set.insert(JsonLoaderBase::get_as_string(value));
            }
        }
    }
}
