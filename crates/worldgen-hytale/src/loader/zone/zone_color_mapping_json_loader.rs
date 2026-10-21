// Ported from: `.hytale-source/.../worldgen/loader/zone/ZoneColorMappingJsonLoader.java`
//! Java `loader.zone.ZoneColorMappingJsonLoader`.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::loader::color_util::ColorUtil;
use crate::zone::zone::Zone;
use crate::zone::zone_color_mapping::ZoneColorMapping;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ZoneColorMappingJsonLoader`.
pub struct ZoneColorMappingJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, ZoneColorMapping>`.
    pub base: JsonLoaderBase,
    /// Java `protected final Map<String, Zone> zoneLookup`.
    zone_lookup: Rc<HashMap<String, Rc<Zone>>>,
}

impl ZoneColorMappingJsonLoader {
    /// Java `ZoneColorMappingJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, Map<String, Zone> zoneLookup)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_lookup: Rc<HashMap<String, Rc<Zone>>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            zone_lookup,
        }
    }

    /// Java `ZoneColorMapping load()`.
    pub fn load(&self) -> ZoneColorMapping {
        let mut color_mapping = ZoneColorMapping::new();
        let Some(obj) = self.base.json.as_ref().and_then(|j| j.as_object()) else {
            // Java `json.getAsJsonObject()` — ClassCastException when absent.
            panic!("MaskMapping must be a JSON object");
        };
        for (key, value) in obj {
            let rgb = ColorUtil::hex_string(key);
            if let Some(arr) = value.as_array() {
                let mut zones = Vec::with_capacity(arr.len());
                for zone_name in arr {
                    let zone_name = JsonLoaderBase::get_as_string(zone_name);
                    zones.push(self.lookup(&zone_name, key));
                }
                color_mapping.add_zones(rgb, zones);
            } else {
                let zone_name = JsonLoaderBase::get_as_string(value);
                let zone = self.lookup(&zone_name, key);
                color_mapping.add(rgb, zone);
            }
        }
        color_mapping
    }

    fn lookup(&self, zone_name: &str, color_key: &str) -> Rc<Zone> {
        match self.zone_lookup.get(zone_name) {
            Some(zone) => zone.clone(),
            None => panic!("Zone with name {zone_name} was not found for color {color_key}!"),
        }
    }
}
