// Ported from: `.hytale-source/.../procedurallib/json/NoiseMaskConditionJsonLoader.java`
//! Java `NoiseMaskConditionJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::double_condition_json_loader::DoubleConditionJsonLoader;
use crate::json::noise_property_json_loader::NoisePropertyJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::condition::{
    DefaultCoordinateCondition, ICoordinateCondition, NoiseMaskCondition,
};

/// Java `NoiseMaskConditionJsonLoader.Constants`.
pub mod constants {
    pub const KEY_THRESHOLD: &str = "Threshold";
    pub const ERROR_THRESHOLD: &str =
        "Could not find threshold data in noise mask. Keyword: Threshold";
}

/// Java `NoiseMaskConditionJsonLoader`.
pub struct NoiseMaskConditionJsonLoader {
    /// Java `extends JsonLoader<K, ICoordinateCondition>`.
    pub base: JsonLoaderBase,
    /// Java `protected final boolean defaultValue`.
    pub default_value: bool,
}

impl NoiseMaskConditionJsonLoader {
    /// Java `NoiseMaskConditionJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self::with_default_value(seed.append(".NoiseMaskCondition"), data_folder, json, true)
    }

    /// Java `NoiseMaskConditionJsonLoader(SeedString<K> seed, Path dataFolder,
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

    /// Java `ICoordinateCondition load()`.
    pub fn load(&self) -> Rc<dyn ICoordinateCondition> {
        let mut map_condition: Rc<dyn ICoordinateCondition> = if self.default_value {
            Rc::new(DefaultCoordinateCondition::True)
        } else {
            Rc::new(DefaultCoordinateCondition::False)
        };

        // Java: `json != null && !json.isJsonNull()`.
        if !matches!(&self.base.json, None | Some(Value::Null)) {
            if !self.base.has(constants::KEY_THRESHOLD) {
                panic!("{}", constants::ERROR_THRESHOLD);
            }
            // Java quirk: the FULL json (Threshold key included) feeds the
            // noise property loader — extra keys are simply ignored.
            let noise = NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.json.clone(),
            )
            .load();
            let threshold = DoubleConditionJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_THRESHOLD),
            )
            .load();
            map_condition = Rc::new(NoiseMaskCondition::new(noise, threshold));
        }
        map_condition
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> NoiseMaskConditionJsonLoader {
        NoiseMaskConditionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    #[should_panic(expected = "Could not find threshold data in noise mask. Keyword: Threshold")]
    fn missing_threshold_panics_verbatim() {
        loader(serde_json::json!({ "NoiseType": "CONSTANT" })).load();
    }

    #[test]
    fn null_json_gives_the_default_condition() {
        let condition = NoiseMaskConditionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            None,
        );
        assert!(condition.load().eval2(1, 0, 0));
        assert!(condition.load().eval3(1, 0, 0, 0));

        let condition = NoiseMaskConditionJsonLoader::with_default_value(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(Value::Null),
            false,
        );
        assert!(!condition.load().eval2(1, 0, 0));
    }

    #[test]
    fn mask_condition_evaluates_the_noise_against_the_threshold() {
        // Constant noise (Value 0.5) through SingleNoiseProperty → 0.75;
        // threshold `< 0.8` → true.
        let condition = loader(serde_json::json!({
            "NoiseType": "CONSTANT",
            "Value": 0.5,
            "Threshold": 0.8
        }));
        let condition = condition.load();
        assert!(condition.eval2(3, 10, 20));
        assert!(condition.eval3(3, 10, 20, 30));
        // threshold `< 0.7` → false for the same constant noise.
        let condition = loader(serde_json::json!({
            "NoiseType": "CONSTANT",
            "Value": 0.5,
            "Threshold": 0.7
        }));
        assert!(!condition.load().eval2(3, 10, 20));
    }
}
