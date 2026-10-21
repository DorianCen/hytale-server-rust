// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimateRuleJsonLoader.java`
//! Java `loader.climate.ClimateRuleJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::climate::climate_search::{Range, Rule};
use crate::loader::climate::climate_noise_json_loader::KEY_CONTINENT;
use crate::loader::climate::climate_point_json_loader::{KEY_INTENSITY, KEY_TEMPERATURE};
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ClimateRuleJsonLoader.Constants.KEY_TARGET`.
pub const KEY_TARGET: &str = "Target";
/// Java `ClimateRuleJsonLoader.Constants.KEY_RADIUS`.
pub const KEY_RADIUS: &str = "Radius";
/// Java `ClimateRuleJsonLoader.Constants.KEY_WEIGHT`.
pub const KEY_WEIGHT: &str = "Weight";

/// Java `BiomeJsonLoader.Constants.KEY_FADE` — the fade range key.
pub const KEY_FADE: &str = "Fade";

/// Java `ClimateRuleJsonLoader`.
pub struct ClimateRuleJsonLoader {
    /// Java `extends JsonLoader<K, ClimateSearch.Rule>`.
    pub base: JsonLoaderBase,
}

impl ClimateRuleJsonLoader {
    /// Java `ClimateRuleJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `ClimateSearch.Rule load()`.
    pub fn load(&self) -> Rule {
        Rule::new(
            self.load_range(KEY_CONTINENT),
            self.load_range(KEY_TEMPERATURE),
            self.load_range(KEY_INTENSITY),
            self.load_range(KEY_FADE),
        )
    }

    /// Java `ClimateSearch.Range loadRange(String key)`.
    pub fn load_range(&self, key: &str) -> Range {
        if self.base.has(key) {
            let json = self.base.must_get_object(key, None);
            let target = Self::load_target(&json);
            let radius = Self::load_radius(&json);
            let weight = Self::load_weight(&json);
            return Range::new(target, radius, weight);
        }
        Range::DEFAULT
    }

    /// Java `double loadTarget(JsonObject json)`.
    pub fn load_target(json: &Value) -> f64 {
        Self::must_get_number(json, KEY_TARGET)
    }

    /// Java `double loadRadius(JsonObject json)`.
    pub fn load_radius(json: &Value) -> f64 {
        Self::must_get_number(json, KEY_RADIUS)
    }

    /// Java `double loadWeight(JsonObject json)`.
    pub fn load_weight(json: &Value) -> f64 {
        Self::must_get_number(json, KEY_WEIGHT)
    }

    /// Java `mustGet(key, json.get(key), null, Double.class, JsonLoader::isNumber, JsonElement::getAsDouble)`.
    fn must_get_number(json: &Value, key: &str) -> f64 {
        match json.get(key) {
            None => panic!("Missing property '{key}'"),
            Some(element) => {
                if !element.is_number() {
                    panic!("Property '{key}' must be of type 'Double'");
                }
                JsonLoaderBase::get_as_double(element)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracts the message of a caught panic.
    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            return (*message).to_string();
        }
        "<non-string panic>".to_string()
    }

    fn loader(json: Value) -> ClimateRuleJsonLoader {
        ClimateRuleJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json))
    }

    #[test]
    fn missing_ranges_use_the_default() {
        let rule = loader(serde_json::json!({})).load();
        assert_eq!(rule.continent, Range::DEFAULT);
        assert_eq!(rule.temperature, Range::DEFAULT);
        assert_eq!(rule.intensity, Range::DEFAULT);
        assert_eq!(rule.fade, Range::DEFAULT);
    }

    #[test]
    fn ranges_are_read() {
        let rule = loader(serde_json::json!({
            "Continent": { "Target": 0.0, "Radius": 0.3, "Weight": 1.0 },
            "Fade": { "Target": 1.0, "Radius": 0.5, "Weight": 0.5 }
        }))
        .load();
        assert_eq!(rule.continent.value, 0.0);
        assert_eq!(rule.continent.radius, 0.3);
        assert_eq!(rule.continent.weight, 1.0);
        assert_eq!(rule.fade.value, 1.0);
        assert_eq!(rule.fade.radius, 0.5);
        assert_eq!(rule.fade.weight, 0.5);
        // The untouched ranges stay at the default.
        assert_eq!(rule.temperature, Range::DEFAULT);
    }

    #[test]
    fn missing_target_panics() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(serde_json::json!({
                "Continent": { "Radius": 0.3, "Weight": 1.0 }
            }))
            .load()
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert_eq!(message, "Missing property 'Target'");
    }
}
