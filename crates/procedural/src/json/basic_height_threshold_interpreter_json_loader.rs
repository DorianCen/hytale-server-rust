// Ported from: `.hytale-source/.../procedurallib/json/BasicHeightThresholdInterpreterJsonLoader.java`
//! Java `BasicHeightThresholdInterpreterJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::condition::BasicHeightThresholdInterpreter;

/// Java `BasicHeightThresholdInterpreterJsonLoader.Constants`.
mod constants {
    pub const KEY_POSITIONS: &str = "Positions";
    pub const KEY_VALUES: &str = "Values";

    pub const ERROR_NO_POSITIONS: &str =
        "Could not find position data in HeightThresholdInterpreter. Keyword: Positions";
    pub const ERROR_NO_VALUES: &str =
        "Could not find value data in HeightThresholdInterpreter. Keyword: Values";
}

/// Java `BasicHeightThresholdInterpreterJsonLoader`.
pub struct BasicHeightThresholdInterpreterJsonLoader {
    /// Java `extends JsonLoader<K, BasicHeightThresholdInterpreter>`.
    pub base: JsonLoaderBase,
    /// Java `protected final int length`.
    pub length: i32,
}

impl BasicHeightThresholdInterpreterJsonLoader {
    /// Java `BasicHeightThresholdInterpreterJsonLoader(SeedString<K> seed,
    /// Path dataFolder, JsonElement json, int length)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>, length: i32) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(".BasicHeightThresholdInterpreter"),
                data_folder,
                json,
            ),
            length,
        }
    }

    /// Java `BasicHeightThresholdInterpreter load()`.
    pub fn load(&self) -> BasicHeightThresholdInterpreter {
        BasicHeightThresholdInterpreter::new(
            &self.load_positions(),
            &self.load_values(),
            self.length,
        )
    }

    /// Java `int[] loadPositions()`.
    pub fn load_positions(&self) -> Vec<i32> {
        if !self.base.has(constants::KEY_POSITIONS) {
            panic!("{}", constants::ERROR_NO_POSITIONS);
        }
        let array = self.base.get(constants::KEY_POSITIONS).unwrap();
        let array = match array.as_array() {
            Some(array) => array,
            None => panic!("Positions must be a JSON array, got: {array:?}"),
        };
        let mut positions = Vec::with_capacity(array.len());
        for element in array {
            positions.push(JsonLoaderBase::get_as_int(element));
        }
        positions
    }

    /// Java `float[] loadValues()`.
    pub fn load_values(&self) -> Vec<f32> {
        if !self.base.has(constants::KEY_VALUES) {
            panic!("{}", constants::ERROR_NO_VALUES);
        }
        let array = self.base.get(constants::KEY_VALUES).unwrap();
        let array = match array.as_array() {
            Some(array) => array,
            None => panic!("Values must be a JSON array, got: {array:?}"),
        };
        let mut values = Vec::with_capacity(array.len());
        for element in array {
            // Java: `getAsFloat()` == `(float) getAsDouble()`.
            values.push(JsonLoaderBase::get_as_double(element) as f32);
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::condition::IHeightThresholdInterpreter;

    fn loader(json: Value, length: i32) -> BasicHeightThresholdInterpreterJsonLoader {
        BasicHeightThresholdInterpreterJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
            length,
        )
    }

    #[test]
    fn positions_and_values_are_loaded() {
        let l = loader(
            serde_json::json!({ "Positions": [25, 75], "Values": [1.0, 0.0] }),
            100,
        );
        assert_eq!(l.load_positions(), vec![25, 75]);
        assert_eq!(l.load_values(), vec![1.0, 0.0]);

        let interpreter = l.load();
        assert_eq!(interpreter.get_lowest_non_one(), 26);
        // y=75 interpolates to exactly 0.0 (not > 0), so the highest
        // non-zero slot is 74.
        assert_eq!(interpreter.get_highest_non_zero(), 74);
    }

    #[test]
    #[should_panic(
        expected = "Could not find position data in HeightThresholdInterpreter. Keyword: Positions"
    )]
    fn missing_positions_panics_verbatim() {
        loader(serde_json::json!({ "Values": [1.0] }), 100).load();
    }

    #[test]
    #[should_panic(
        expected = "Could not find value data in HeightThresholdInterpreter. Keyword: Values"
    )]
    fn missing_values_panics_verbatim() {
        loader(serde_json::json!({ "Positions": [25] }), 100).load();
    }

    #[test]
    #[should_panic(expected = "Mismatching array lengths! positions: 2, thresholds: 1")]
    fn mismatched_lengths_panic_like_java() {
        loader(
            serde_json::json!({ "Positions": [25, 75], "Values": [1.0] }),
            100,
        )
        .load();
    }
}
