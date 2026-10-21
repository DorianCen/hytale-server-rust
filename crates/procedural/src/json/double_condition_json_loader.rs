// Ported from: `.hytale-source/.../procedurallib/json/DoubleConditionJsonLoader.java`
//! Java `DoubleConditionJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::double_threshold_json_loader::DoubleThresholdJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::condition::IDoubleCondition;
use crate::noise::condition::{
    DefaultDoubleCondition, DoubleThresholdCondition, SingleDoubleCondition,
};

/// Java `DoubleConditionJsonLoader.Constants`.
mod constants {
    pub const ERROR_NO_DEFAULT: &str = "Default value is not set and condition is not defined.";
    pub const ERROR_FAILED: &str = "Failed to load \"%s\" as DoubleCondition";
}

/// Java `DoubleConditionJsonLoader`.
pub struct DoubleConditionJsonLoader {
    /// Java `extends JsonLoader<K, IDoubleCondition>`.
    pub base: JsonLoaderBase,
    /// Java `protected final Boolean defaultValue` — `@Nullable` in Java.
    pub default_value: Option<bool>,
}

impl DoubleConditionJsonLoader {
    /// Java `DoubleConditionJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`
    /// — defaults the value to `true`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self::with_default_value(
            seed.append(".DoubleCondition"),
            data_folder,
            json,
            Some(true),
        )
    }

    /// Java `DoubleConditionJsonLoader(SeedString<K> seed, Path dataFolder,
    /// JsonElement json, Boolean defaultValue)` — no seed append; the default
    /// may be `null` in Java (→ `None`).
    pub fn with_default_value(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default_value: Option<bool>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            default_value,
        }
    }

    /// Java `IDoubleCondition load()`.
    pub fn load(&self) -> Rc<dyn IDoubleCondition> {
        // Java: `json == null || json.isJsonNull()`.
        if matches!(&self.base.json, None | Some(Value::Null)) {
            let default_value = match self.default_value {
                Some(default_value) => default_value,
                None => panic!("{}", constants::ERROR_NO_DEFAULT),
            };
            return Rc::new(if default_value {
                DefaultDoubleCondition::DEFAULT_TRUE
            } else {
                DefaultDoubleCondition::DEFAULT_FALSE
            });
        }

        let json = self.base.json.as_ref().unwrap();

        // Java: `json.isJsonPrimitive() || (json.isJsonArray() &&
        // json.getAsJsonArray().size() == 1 &&
        // json.getAsJsonArray().get(0).isJsonPrimitive())` — gson's
        // `JsonArray.getAsDouble()` unwraps a single element, so the array
        // form resolves to its only entry.
        let is_primitive = json.is_number() || json.is_string() || json.is_boolean();
        let is_single_primitive_array = json.is_array() && {
            let array = json.as_array().unwrap();
            array.len() == 1
                && (array[0].is_number() || array[0].is_string() || array[0].is_boolean())
        };

        if is_primitive || is_single_primitive_array {
            let limit = if is_primitive {
                JsonLoaderBase::get_as_double(json)
            } else {
                JsonLoaderBase::get_as_double(&json.as_array().unwrap()[0])
            };
            return Rc::new(SingleDoubleCondition::new(limit));
        }

        if json.is_array() {
            return Rc::new(DoubleThresholdCondition::new(
                DoubleThresholdJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(json.clone()),
                )
                .load(),
            ));
        }

        // Java: `throw new Error(String.format(ERROR_FAILED, json))`.
        panic!(
            "{}",
            constants::ERROR_FAILED.replace("%s", &json.to_string())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> DoubleConditionJsonLoader {
        DoubleConditionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn primitive_is_a_single_condition() {
        let condition = loader(serde_json::json!(0.5)).load();
        assert!(condition.eval(0.499)); // value < limit
        assert!(!condition.eval(0.5));
        assert!(!condition.eval(0.6));
    }

    #[test]
    fn single_entry_array_unwraps_to_the_entry() {
        // gson's JsonArray.getAsDouble() unwraps the single element.
        let condition = loader(serde_json::json!([0.5])).load();
        assert!(condition.eval(0.499));
        assert!(!condition.eval(0.5));
    }

    #[test]
    fn multi_entry_array_is_a_threshold_condition() {
        let condition = loader(serde_json::json!([[0.0, 0.2], [0.8, 1.0]])).load();
        assert!(condition.eval(0.1));
        assert!(condition.eval(0.9));
        assert!(!condition.eval(0.5));
    }

    #[test]
    fn null_json_falls_back_to_the_default() {
        let condition = DoubleConditionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            None,
        );
        assert!(condition.load().eval(999.0));

        let condition = DoubleConditionJsonLoader::with_default_value(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(Value::Null),
            Some(false),
        );
        assert!(!condition.load().eval(999.0));
    }

    #[test]
    #[should_panic(expected = "Default value is not set and condition is not defined.")]
    fn null_json_without_default_panics_verbatim() {
        let condition = DoubleConditionJsonLoader::with_default_value(
            SeedString::new("test"),
            std::path::Path::new("."),
            None,
            None,
        );
        condition.load();
    }

    #[test]
    #[should_panic(expected = "Failed to load")]
    fn object_json_panics() {
        loader(serde_json::json!({ "Min": 0.0 })).load();
    }
}
