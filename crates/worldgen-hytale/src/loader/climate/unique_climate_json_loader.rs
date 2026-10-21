// Ported from: `.hytale-source/.../worldgen/loader/climate/UniqueClimateJsonLoader.java`
//! Java `loader.climate.UniqueClimateJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::climate::climate_search::{self, Rule};
use crate::climate::unique_climate_generator::Entry;
use crate::loader::climate::climate_rule_json_loader::ClimateRuleJsonLoader;
use crate::loader::color_util::ColorUtil;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::support::Vector2i;

/// Java `UniqueClimateJsonLoader.Constants.KEY_ZONE`.
pub const KEY_ZONE: &str = "Name";
/// Java `UniqueClimateJsonLoader.Constants.KEY_PARENT`.
pub const KEY_PARENT: &str = "Parent";
/// Java `UniqueClimateJsonLoader.Constants.KEY_COLOR`.
pub const KEY_COLOR: &str = "Color";
/// Java `UniqueClimateJsonLoader.Constants.KEY_RADIUS`.
pub const KEY_RADIUS: &str = "Radius";
/// Java `UniqueClimateJsonLoader.Constants.KEY_ORIGIN_X`.
pub const KEY_ORIGIN_X: &str = "OriginX";
/// Java `UniqueClimateJsonLoader.Constants.KEY_ORIGIN_Y`.
pub const KEY_ORIGIN_Y: &str = "OriginY";
/// Java `UniqueClimateJsonLoader.Constants.KEY_DISTANCE`.
pub const KEY_DISTANCE: &str = "Distance";
/// Java `UniqueClimateJsonLoader.Constants.KEY_MIN_DISTANCE`.
pub const KEY_MIN_DISTANCE: &str = "MinDistance";
/// Java `UniqueClimateJsonLoader.Constants.KEY_RULE`.
pub const KEY_RULE: &str = "Rule";

/// Java `UniqueClimateJsonLoader.Constants.DEFAULT_RADIUS`.
pub const DEFAULT_RADIUS: i32 = 8;
/// Java `UniqueClimateJsonLoader.Constants.DEFAULT_OFFSET`.
pub const DEFAULT_OFFSET: i32 = 8;
/// Java `UniqueClimateJsonLoader.Constants.DEFAULT_SEARCH_RADIUS`.
pub const DEFAULT_SEARCH_RADIUS: i32 = climate_search::DEFAULT_RADIUS;
/// Java `UniqueClimateJsonLoader.Constants.DEFAULT_SEARCH_MIN_RADIUS`.
pub const DEFAULT_SEARCH_MIN_RADIUS: i32 = climate_search::STEP_SIZE;

/// Java `UniqueClimateJsonLoader`.
pub struct UniqueClimateJsonLoader {
    /// Java `extends JsonLoader<K, UniqueClimateGenerator.Entry>`.
    pub base: JsonLoaderBase,
}

impl UniqueClimateJsonLoader {
    /// Java `UniqueClimateJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `UniqueClimateGenerator.Entry load()`.
    pub fn load(&self) -> Entry {
        Entry {
            zone: self.load_name(),
            parent: self.load_parent(),
            color: self.load_color(),
            radius: self.load_radius(),
            origin: self.load_origin(),
            min_distance: self.load_min_distance(),
            max_distance: self.load_distance(),
            rule: self.load_rule(),
        }
    }

    /// Java `String loadName()`.
    pub fn load_name(&self) -> String {
        self.base.must_get_string(KEY_ZONE, None)
    }

    /// Java `String loadParent()`.
    pub fn load_parent(&self) -> String {
        self.base
            .must_get_string(KEY_PARENT, Some(ENTRY_DEFAULT_PARENT))
    }

    /// Java `int loadColor()`.
    pub fn load_color(&self) -> i32 {
        let color = self.base.must_get_string(KEY_COLOR, None);
        ColorUtil::hex_string(&color)
    }

    /// Java `int loadRadius()`.
    pub fn load_radius(&self) -> i32 {
        self.must_get_int(KEY_RADIUS, DEFAULT_RADIUS)
    }

    /// Java `Vector2i loadOrigin()`.
    pub fn load_origin(&self) -> Vector2i {
        let x = self.must_get_int(KEY_ORIGIN_X, DEFAULT_OFFSET);
        let y = self.must_get_int(KEY_ORIGIN_Y, DEFAULT_OFFSET);
        Vector2i { x, y }
    }

    /// Java `int loadDistance()`.
    pub fn load_distance(&self) -> i32 {
        self.must_get_int(KEY_DISTANCE, DEFAULT_SEARCH_RADIUS)
    }

    /// Java `int loadMinDistance()`.
    pub fn load_min_distance(&self) -> i32 {
        self.must_get_int(KEY_MIN_DISTANCE, DEFAULT_SEARCH_MIN_RADIUS)
    }

    /// Java `ClimateSearch.Rule loadRule()`.
    pub fn load_rule(&self) -> Rule {
        ClimateRuleJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(self.base.must_get_object(KEY_RULE, None)),
        )
        .load()
    }

    /// Java `mustGetNumber(key, defaultValue).intValue()`.
    fn must_get_int(&self, key: &str, default_value: i32) -> i32 {
        match self.base.get(key) {
            None => default_value,
            Some(element) => {
                if !element.is_number() {
                    panic!("Property '{key}' must be of type 'Number'");
                }
                JsonLoaderBase::get_as_int(&element)
            }
        }
    }
}

/// Java `UniqueClimateGenerator.Entry.DEFAULT_PARENT`.
pub const ENTRY_DEFAULT_PARENT: &str = "";

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> UniqueClimateJsonLoader {
        UniqueClimateJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json))
    }

    #[test]
    fn defaults_are_applied() {
        let entry = loader(serde_json::json!({
            "Name": "Zone1_Spawn",
            "Color": "#ff0000",
            "Rule": {}
        }))
        .load();

        assert_eq!(entry.zone, "Zone1_Spawn");
        assert_eq!(entry.parent, "");
        assert_eq!(entry.color, 0xFF0000);
        assert_eq!(entry.radius, 8);
        assert_eq!(entry.origin, Vector2i { x: 8, y: 8 });
        assert_eq!(entry.min_distance, 100);
        assert_eq!(entry.max_distance, 5000);
    }

    #[test]
    fn values_are_read() {
        let entry = loader(serde_json::json!({
            "Name": "Zone1_Temple",
            "Parent": "Zone1_Spawn",
            "Color": "#ffff00",
            "Radius": 20,
            "OriginX": -3,
            "OriginY": 7,
            "Distance": 400,
            "MinDistance": 250,
            "Rule": { "Continent": { "Target": 0.0, "Radius": 0.5, "Weight": 0.5 } }
        }))
        .load();

        assert_eq!(entry.zone, "Zone1_Temple");
        assert_eq!(entry.parent, "Zone1_Spawn");
        assert_eq!(entry.color, 0xFFFF00);
        assert_eq!(entry.radius, 20);
        assert_eq!(entry.origin, Vector2i { x: -3, y: 7 });
        assert_eq!(entry.min_distance, 250);
        assert_eq!(entry.max_distance, 400);
        assert_eq!(entry.rule.continent.weight, 0.5);
    }
}
