// Ported from: `.hytale-source/.../worldgen/loader/zone/UniqueZoneEntryJsonLoader.java`
//! Java `loader.zone.UniqueZoneEntryJsonLoader`.

use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::loader::color_util::ColorUtil;
use crate::zone::unique::UniqueEntry;
use crate::zone::zone::Zone;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `UniqueZoneEntryJsonLoader`.
pub struct UniqueZoneEntryJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, Zone.UniqueEntry[]>`.
    pub base: JsonLoaderBase,
    /// Java `protected final Map<String, Zone> zoneLookup`.
    zone_lookup: Rc<std::collections::HashMap<String, Rc<Zone>>>,
}

impl UniqueZoneEntryJsonLoader {
    /// Java `UniqueZoneEntryJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, Map<String, Zone> zoneLookup)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_lookup: Rc<std::collections::HashMap<String, Rc<Zone>>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            zone_lookup,
        }
    }

    /// Java `Zone.UniqueEntry[] load()`.
    pub fn load(&self) -> Vec<UniqueEntry> {
        const KEY_ZONE: &str = "Zone";
        const KEY_COLOR: &str = "Color";
        const KEY_PARENT: &str = "Parent";
        const KEY_RADIUS: &str = "Radius";
        const KEY_PADDING: &str = "Padding";
        const DEFAULT_RADIUS: i32 = 1;
        const DEFAULT_PADDING: i32 = 0;

        let Some(json) = &self.base.json else {
            return Vec::new();
        };

        let Some(array_json) = json.as_array() else {
            panic!("Unexpected type for 'UniqueZones' field, expected array");
        };

        let mut entries = Vec::with_capacity(array_json.len());
        for (i, entry) in array_json.iter().enumerate() {
            let Some(entry_obj) = entry.as_object() else {
                panic!("Unexpected type for unique zone entry: #{i}");
            };

            let Some(zone_json) = entry_obj.get(KEY_ZONE) else {
                panic!("Missing 'Zone' field in unique zone entry: #{i}");
            };
            let Some(color_json) = entry_obj.get(KEY_COLOR) else {
                panic!("Missing 'Color' field in unique zone entry: #{i}");
            };
            let Some(parent_json) = entry_obj.get(KEY_PARENT) else {
                panic!("Missing 'Parent' field in unique zone entry: #{i}");
            };

            let zone_name = JsonLoaderBase::get_as_string(zone_json);
            let zone = match self.zone_lookup.get(&zone_name) {
                Some(zone) => zone.clone(),
                None => panic!("Unknown zone '{zone_name}' in unique zone entry: #{i}"),
            };

            let color = ColorUtil::hex_string(&JsonLoaderBase::get_as_string(color_json));
            let parent = Self::load_parent_colors(i, parent_json);
            let radius = entry_obj
                .get(KEY_RADIUS)
                .map(JsonLoaderBase::get_as_int)
                .unwrap_or(DEFAULT_RADIUS);
            let padding = entry_obj
                .get(KEY_PADDING)
                .map(JsonLoaderBase::get_as_int)
                .unwrap_or(DEFAULT_PADDING);

            entries.push(UniqueEntry::new(zone, color, parent, radius, padding));
        }

        entries
    }

    /// Java `static int[] loadParentColors(int index, JsonElement json)`.
    fn load_parent_colors(index: usize, json: &Value) -> Vec<i32> {
        if let Some(arr) = json.as_array() {
            return arr
                .iter()
                .map(|v| ColorUtil::hex_string(&JsonLoaderBase::get_as_string(v)))
                .collect();
        }

        if json.is_string() {
            return vec![ColorUtil::hex_string(&JsonLoaderBase::get_as_string(json))];
        }

        panic!("Unexpected type for 'Parent' field in unique zone entry: #{index}");
    }

    /// Java `static void collectZones(Set<String> zoneSet, JsonElement json)`.
    pub fn collect_zones(zone_set: &mut HashSet<String>, json: Option<&Value>) {
        const KEY_ZONE: &str = "Zone";
        let Some(json) = json else {
            return;
        };

        let Some(array_json) = json.as_array() else {
            panic!("Unexpected type for 'UniqueZones' field, expected array");
        };

        for (i, entry) in array_json.iter().enumerate() {
            let Some(entry_obj) = entry.as_object() else {
                panic!("Unexpected type for unique zone entry: #{i}");
            };

            let Some(zone) = entry_obj.get(KEY_ZONE) else {
                panic!("Missing 'Zone' field in unique zone entry: #{i}");
            };

            zone_set.insert(JsonLoaderBase::get_as_string(zone));
        }
    }
}
