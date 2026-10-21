// Ported from: `.hytale-source/.../procedurallib/json/DoubleThresholdJsonLoader.java`
//! Java `DoubleThresholdJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::condition::IDoubleThreshold;
use crate::noise::condition::{
    DefaultDoubleThresholdCondition, DoubleThresholdMultiple, DoubleThresholdSingle,
};

/// Java `DoubleThresholdJsonLoader.Constants`.
mod constants {
    pub const ERROR_NO_ENTRY: &str = "Threshold array must contain at least one entry!";
    pub const ERROR_THRESHOLD_SIZE: &str =
        "Threshold array entries must have 2 numbers for lower/upper limit!";
}

/// Java `DoubleThresholdJsonLoader`.
pub struct DoubleThresholdJsonLoader {
    /// Java `extends JsonLoader<K, IDoubleThreshold>`.
    pub base: JsonLoaderBase,
    /// Java `protected final boolean defaultValue`.
    pub default_value: bool,
}

impl DoubleThresholdJsonLoader {
    /// Java `DoubleThresholdJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self::with_default_value(seed.append(".DoubleThreshold"), data_folder, json, true)
    }

    /// Java `DoubleThresholdJsonLoader(SeedString<K> seed, Path dataFolder,
    /// JsonElement json, boolean defaultValue)` — no seed append.
    pub fn with_default_value(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default_value: bool,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            default_value,
        }
    }

    /// Java `IDoubleThreshold load()`.
    pub fn load(&self) -> Rc<dyn IDoubleThreshold> {
        // Java: `json == null || json.isJsonNull()`.
        if matches!(&self.base.json, None | Some(Value::Null)) {
            return Rc::new(if self.default_value {
                DefaultDoubleThresholdCondition::DEFAULT_TRUE
            } else {
                DefaultDoubleThresholdCondition::DEFAULT_FALSE
            });
        }

        let json = self.base.json.as_ref().unwrap();

        if json.is_number() || json.is_string() || json.is_boolean() {
            let value = JsonLoaderBase::get_as_double(json);
            return Rc::new(DoubleThresholdSingle::new(0.0, value));
        }

        // Java: `json.getAsJsonArray()` — a non-array is a fatal cast error.
        let array = match json.as_array() {
            Some(array) => array,
            None => panic!("Threshold must be a JSON primitive or array, got: {json:?}"),
        };
        if array.is_empty() {
            panic!("{}", constants::ERROR_NO_ENTRY);
        }

        if array[0].is_array() {
            let mut entries = Vec::with_capacity(array.len());
            for entry in array {
                let entry = match entry.as_array() {
                    Some(entry) => entry,
                    None => {
                        panic!("Threshold array entries must have 2 numbers for lower/upper limit!")
                    }
                };
                if entry.len() != 2 {
                    panic!("{}", constants::ERROR_THRESHOLD_SIZE);
                }
                entries.push(DoubleThresholdSingle::new(
                    JsonLoaderBase::get_as_double(&entry[0]),
                    JsonLoaderBase::get_as_double(&entry[1]),
                ));
            }
            Rc::new(DoubleThresholdMultiple::new(entries))
        } else {
            if array.len() != 2 {
                panic!("{}", constants::ERROR_THRESHOLD_SIZE);
            }
            Rc::new(DoubleThresholdSingle::new(
                JsonLoaderBase::get_as_double(&array[0]),
                JsonLoaderBase::get_as_double(&array[1]),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> DoubleThresholdJsonLoader {
        DoubleThresholdJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn primitive_is_single_with_min_zero() {
        let threshold = loader(serde_json::json!(0.8));
        let resolved = threshold.load();
        assert!(resolved.eval(0.4));
        assert!(resolved.eval(0.0)); // min == 0, inclusive
        assert!(!resolved.eval(0.9));
    }

    #[test]
    fn pair_is_single_range() {
        let threshold = loader(serde_json::json!([0.25, 0.75]));
        let resolved = threshold.load();
        assert!(resolved.eval(0.25));
        assert!(resolved.eval(0.75));
        assert!(!resolved.eval(0.2));
    }

    #[test]
    fn nested_arrays_are_multiple() {
        let threshold = loader(serde_json::json!([[0.0, 0.2], [0.8, 1.0]]));
        let resolved = threshold.load();
        assert!(resolved.eval(0.1));
        assert!(resolved.eval(0.9));
        assert!(!resolved.eval(0.5));
    }

    #[test]
    fn null_json_uses_the_default() {
        let true_loader = DoubleThresholdJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            None,
        );
        assert!(true_loader.load().eval(12345.0));

        let false_loader = DoubleThresholdJsonLoader::with_default_value(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(Value::Null),
            false,
        );
        assert!(!false_loader.load().eval(12345.0));
    }

    #[test]
    #[should_panic(expected = "Threshold array must contain at least one entry!")]
    fn empty_array_panics_verbatim() {
        loader(serde_json::json!([])).load();
    }

    #[test]
    #[should_panic(expected = "Threshold array entries must have 2 numbers for lower/upper limit!")]
    fn wrong_entry_size_panics_verbatim() {
        loader(serde_json::json!([[0.0, 0.2, 0.4]])).load();
    }
}
