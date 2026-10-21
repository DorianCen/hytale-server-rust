// Ported from: `.hytale-source/.../procedurallib/json/HeightThresholdInterpreterJsonLoader.java`
//! Java `HeightThresholdInterpreterJsonLoader` — dispatches to the noise
//! variant when a `Thresholds` key is present, else to the basic variant.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::basic_height_threshold_interpreter_json_loader::BasicHeightThresholdInterpreterJsonLoader;
use crate::json::noise_height_threshold_interpreter_json_loader::NoiseHeightThresholdInterpreterJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::condition::IHeightThresholdInterpreter;

/// Java `HeightThresholdInterpreterJsonLoader`.
pub struct HeightThresholdInterpreterJsonLoader {
    /// Java `extends JsonLoader<K, IHeightThresholdInterpreter>`.
    pub base: JsonLoaderBase,
    /// Java `protected final int length`.
    pub length: i32,
}

impl HeightThresholdInterpreterJsonLoader {
    /// Java `HeightThresholdInterpreterJsonLoader(SeedString<K> seed, Path
    /// dataFolder, JsonElement json, int length)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>, length: i32) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(".HeightThresholdInterpreter"),
                data_folder,
                json,
            ),
            length,
        }
    }

    /// Java `IHeightThresholdInterpreter load()`.
    pub fn load(&self) -> Rc<dyn IHeightThresholdInterpreter> {
        // Java: `json.getAsJsonObject()` — a null/non-object json is fatal at
        // load time.
        let json_object = match &self.base.json {
            Some(Value::Object(_)) => self.base.json.clone().unwrap(),
            Some(other) => panic!("HeightThresholdInterpreter json must be an object, got: {other:?}"),
            None => panic!(
                "Cannot invoke \"JsonElement.getAsJsonObject()\" because the return value of \"json\" is null"
            ),
        };

        if NoiseHeightThresholdInterpreterJsonLoader::should_handle(&json_object) {
            Rc::new(
                NoiseHeightThresholdInterpreterJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(json_object),
                    self.length,
                )
                .load(),
            )
        } else {
            Rc::new(
                BasicHeightThresholdInterpreterJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(json_object),
                    self.length,
                )
                .load(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value, length: i32) -> HeightThresholdInterpreterJsonLoader {
        HeightThresholdInterpreterJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
            length,
        )
    }

    #[test]
    fn dispatches_to_the_basic_variant_without_thresholds() {
        let interpreter = loader(
            serde_json::json!({ "Positions": [25, 75], "Values": [1.0, 0.0] }),
            100,
        );
        let interpreter = interpreter.load();
        assert_eq!(interpreter.get_lowest_non_one(), 26);
        // y=75 interpolates to exactly 0.0 (not > 0).
        assert_eq!(interpreter.get_highest_non_zero(), 74);
        assert_eq!(interpreter.get_length(), 100);
    }

    #[test]
    fn dispatches_to_the_noise_variant_with_thresholds() {
        let interpreter = loader(
            serde_json::json!({
                "Noise": { "NoiseType": "CONSTANT", "Value": 0.0 },
                "Keys": [0.5],
                "Thresholds": [ { "Positions": [25, 75], "Values": [1.0, 0.0] } ]
            }),
            100,
        );
        let interpreter = interpreter.load();
        // NoiseHeightThresholdInterpreter delegates to the single child; the
        // constant noise maps to 0.5 context which is <= Keys[0] = 0.5, so
        // the child threshold applies directly.
        assert_eq!(interpreter.get_threshold(3, 1.0, 1.0, 24), 1.0);
        // y=26 interpolates between positions 25..75: lerp(1, 0, 1/50).
        assert!((interpreter.get_threshold(3, 1.0, 1.0, 26) - 0.98).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "because the return value of \"json\" is null")]
    fn null_json_panics_like_the_java_npe() {
        let loader = HeightThresholdInterpreterJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            None,
            100,
        );
        loader.load();
    }
}
