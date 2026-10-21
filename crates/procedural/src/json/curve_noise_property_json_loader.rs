// Ported from: `.hytale-source/.../procedurallib/json/CurveNoisePropertyJsonLoader.java`
//! Java `CurveNoisePropertyJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::noise_property_json_loader::NoisePropertyJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::property::curve_noise_property::{
    power_curve, CurveNoiseProperty, DoubleUnaryOperator,
};
use crate::noise::support::NoiseProperty;

/// Java `CurveNoisePropertyJsonLoader`.
pub struct CurveNoisePropertyJsonLoader {
    /// Java `extends JsonLoader<K, CurveNoiseProperty>`.
    pub base: JsonLoaderBase,
    /// Java `@Nullable protected final NoiseProperty noise`.
    noise: Option<Rc<dyn NoiseProperty>>,
}

impl CurveNoisePropertyJsonLoader {
    /// Java `CurveNoisePropertyJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, NoiseProperty noise)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        noise: Option<Rc<dyn NoiseProperty>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            noise,
        }
    }

    /// Java `CurveNoiseProperty load()`.
    pub fn load(&self) -> CurveNoiseProperty {
        CurveNoiseProperty::new(self.load_noise(), self.load_d_curve())
    }

    /// Java `NoiseProperty loadNoise()`.
    fn load_noise(&self) -> Rc<dyn NoiseProperty> {
        const KEY_NOISE: &str = "Noise";
        if let Some(noise) = &self.noise {
            return noise.clone();
        }
        if self.base.has(KEY_NOISE) {
            return NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(KEY_NOISE),
            )
            .load();
        }
        panic!("Missing Noise entry!");
    }

    /// Java `DoubleUnaryOperator loadDCurve()`.
    fn load_d_curve(&self) -> DoubleUnaryOperator {
        const KEY_CONST_A: &str = "A";
        const KEY_CONST_B: &str = "B";
        const DEFAULT_A: f64 = 2.0;
        const DEFAULT_B: f64 = -DEFAULT_A;
        let a = self.load_value(KEY_CONST_A, DEFAULT_A);
        let b = self.load_value(KEY_CONST_B, DEFAULT_B);
        power_curve(a, b)
    }

    /// Java `double loadValue(String key, double def)`.
    fn load_value(&self, key: &str, default_value: f64) -> f64 {
        if self.base.has(key) {
            JsonLoaderBase::get_as_double(&self.base.get(key).unwrap())
        } else {
            default_value
        }
    }
}
