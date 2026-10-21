// Ported from: `.hytale-source/.../procedurallib/json/NoiseHeightThresholdInterpreterJsonLoader.java`
//! Java `NoiseHeightThresholdInterpreterJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use crate::json::noise_property_json_loader::NoisePropertyJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::condition::{IHeightThresholdInterpreter, NoiseHeightThresholdInterpreter};
use crate::noise::support::NoiseProperty;

/// Java `NoiseHeightThresholdInterpreterJsonLoader.Constants`.
mod constants {
    pub const KEY_NOISE: &str = "Noise";
    pub const KEY_THRESHOLDS: &str = "Thresholds";
    pub const KEY_KEYS: &str = "Keys";

    pub const ERROR_NO_NOISE: &str =
        "Could not find noise map data in NoiseHeightThresholdInterpreter. Keyword: Noise";
    pub const ERROR_NO_THRESHOLDS: &str =
        "Could not find threshold data in NoiseHeightThresholdInterpreter. Keyword: Thresholds";
    pub const ERROR_NO_KEYS: &str =
        "Could not find key data in NoiseHeightThresholdInterpreter. Keyword: Keys";
}

/// Java `NoiseHeightThresholdInterpreterJsonLoader`.
pub struct NoiseHeightThresholdInterpreterJsonLoader {
    /// Java `extends JsonLoader<K, NoiseHeightThresholdInterpreter>`.
    pub base: JsonLoaderBase,
    /// Java `protected final int length`.
    pub length: i32,
}

impl NoiseHeightThresholdInterpreterJsonLoader {
    /// Java `NoiseHeightThresholdInterpreterJsonLoader(SeedString<K> seed,
    /// Path dataFolder, JsonElement json, int length)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>, length: i32) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(".NoiseHeightThresholdInterpreter"),
                data_folder,
                json,
            ),
            length,
        }
    }

    /// Java `NoiseHeightThresholdInterpreter load()`.
    pub fn load(&self) -> NoiseHeightThresholdInterpreter {
        let interpreters = self.load_interpreters();
        let keys = self.load_keys();
        if keys.len() != interpreters.len() {
            panic!("Keys and Thresholds array do not have the same length!");
        }
        NoiseHeightThresholdInterpreter::new(self.load_noise(), keys, interpreters)
    }

    /// Java `@Nullable NoiseProperty loadNoise()` — throws when missing (the
    /// Java `@Nullable` is vacuous here).
    pub fn load_noise(&self) -> Rc<dyn NoiseProperty> {
        if !self.base.has(constants::KEY_NOISE) {
            panic!("{}", constants::ERROR_NO_NOISE);
        }
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE),
        )
        .load()
    }

    /// Java `IHeightThresholdInterpreter[] loadInterpreters()`.
    pub fn load_interpreters(&self) -> Vec<Rc<dyn IHeightThresholdInterpreter>> {
        if !self.base.has(constants::KEY_THRESHOLDS) {
            panic!("{}", constants::ERROR_NO_THRESHOLDS);
        }
        let array = self.base.get(constants::KEY_THRESHOLDS).unwrap();
        let array = match array.as_array() {
            Some(array) => array,
            None => panic!("Thresholds must be a JSON array, got: {array:?}"),
        };
        let mut interpreters = Vec::with_capacity(array.len());
        for (i, element) in array.iter().enumerate() {
            interpreters.push(
                HeightThresholdInterpreterJsonLoader::new(
                    self.base.seed.append(&format!("-{i}")),
                    &self.base.data_folder,
                    Some(element.clone()),
                    self.length,
                )
                .load(),
            );
        }
        interpreters
    }

    /// Java `float[] loadKeys()`.
    pub fn load_keys(&self) -> Vec<f32> {
        if !self.base.has(constants::KEY_KEYS) {
            panic!("{}", constants::ERROR_NO_KEYS);
        }
        let array = self.base.get(constants::KEY_KEYS).unwrap();
        let array = match array.as_array() {
            Some(array) => array,
            None => panic!("Keys must be a JSON array, got: {array:?}"),
        };
        let mut keys = Vec::with_capacity(array.len());
        for element in array {
            // Java: `getAsFloat()` == `(float) getAsDouble()`.
            keys.push(JsonLoaderBase::get_as_double(element) as f32);
        }
        keys
    }

    /// Java `static boolean shouldHandle(@Nonnull JsonObject jsonObject)`.
    pub fn should_handle(json_object: &Value) -> bool {
        json_object.get(constants::KEY_THRESHOLDS).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value, length: i32) -> NoiseHeightThresholdInterpreterJsonLoader {
        NoiseHeightThresholdInterpreterJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
            length,
        )
    }

    fn child_json() -> Value {
        serde_json::json!({ "Positions": [25, 75], "Values": [1.0, 0.0] })
    }

    #[test]
    fn keys_thresholds_and_noise_are_loaded() {
        let l = loader(
            serde_json::json!({
                "Noise": { "NoiseType": "CONSTANT", "Value": 0.0 },
                "Keys": [0.25, 0.75],
                "Thresholds": [child_json(), child_json()]
            }),
            100,
        );
        assert_eq!(l.load_keys(), vec![0.25, 0.75]);
        assert_eq!(l.load_interpreters().len(), 2);

        let interpreter = l.load();
        // getLowestNonOne == min over children.
        assert_eq!(interpreter.get_lowest_non_one(), 26);
        assert_eq!(interpreter.get_length(), 100);
    }

    #[test]
    #[should_panic(expected = "Keys and Thresholds array do not have the same length!")]
    fn mismatched_lengths_panic_verbatim() {
        loader(
            serde_json::json!({
                "Noise": { "NoiseType": "CONSTANT" },
                "Keys": [0.25, 0.75],
                "Thresholds": [child_json()]
            }),
            100,
        )
        .load();
    }

    #[test]
    #[should_panic(
        expected = "Could not find noise map data in NoiseHeightThresholdInterpreter. Keyword: Noise"
    )]
    fn missing_noise_panics_verbatim() {
        let l = loader(
            serde_json::json!({
                "Keys": [0.5],
                "Thresholds": [child_json()]
            }),
            100,
        );
        l.load();
    }

    #[test]
    fn should_handle_checks_only_key_presence() {
        assert!(NoiseHeightThresholdInterpreterJsonLoader::should_handle(
            &serde_json::json!({ "Thresholds": [] })
        ));
        assert!(!NoiseHeightThresholdInterpreterJsonLoader::should_handle(
            &serde_json::json!({ "Positions": [] })
        ));
    }
}
