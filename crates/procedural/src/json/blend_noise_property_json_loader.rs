// Ported from: `.hytale-source/.../procedurallib/json/BlendNoisePropertyJsonLoader.java`
//! Java `BlendNoisePropertyJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::noise_property_json_loader::NoisePropertyJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::property::blend_noise_property::BlendNoiseProperty;
use crate::noise::support::NoiseProperty;

/// Java `BlendNoisePropertyJsonLoader`.
pub struct BlendNoisePropertyJsonLoader {
    /// Java `extends JsonLoader<K, BlendNoiseProperty>`.
    pub base: JsonLoaderBase,
}

impl BlendNoisePropertyJsonLoader {
    /// Java `BlendNoisePropertyJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `BlendNoiseProperty load()`.
    pub fn load(&self) -> BlendNoiseProperty {
        const KEY_ALPHA: &str = "Alpha";
        const KEY_NOISE: &str = "Noise";
        const KEY_THRESHOLDS: &str = "Thresholds";

        let alpha = self.load_alpha();
        let noise = self.load_noise();
        let thresholds = self.load_thresholds();
        Self::validate(&noise, &thresholds);
        BlendNoiseProperty::new(alpha, noise, thresholds)
    }

    /// Java `NoiseProperty loadAlpha()`.
    fn load_alpha(&self) -> Rc<dyn NoiseProperty> {
        const KEY_ALPHA: &str = "Alpha";
        let alpha_json = self.base.must_get_object(KEY_ALPHA, None);
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(alpha_json),
        )
        .load()
    }

    /// Java `NoiseProperty[] loadNoise()`.
    fn load_noise(&self) -> Vec<Rc<dyn NoiseProperty>> {
        const KEY_NOISE: &str = "Noise";
        let noise = self
            .base
            .must_get_array(KEY_NOISE, Some(serde_json::json!([])));
        let array = noise.as_array().unwrap();
        let mut noises = Vec::with_capacity(array.len());
        for element in array {
            noises.push(
                NoisePropertyJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(element.clone()),
                )
                .load() as Rc<dyn NoiseProperty>,
            );
        }
        noises
    }

    /// Java `double[] loadThresholds()`.
    fn load_thresholds(&self) -> Vec<f64> {
        const KEY_THRESHOLDS: &str = "Thresholds";
        let thresholds = self
            .base
            .must_get_array(KEY_THRESHOLDS, Some(serde_json::json!([])));
        let array = thresholds.as_array().unwrap();
        let mut values = Vec::with_capacity(array.len());
        for (i, element) in array.iter().enumerate() {
            // Java `mustGet("$" + i, ...)` with the Number predicate.
            if !JsonLoaderBase::is_number(element) {
                panic!("Property '${i}' must be of type 'Number'");
            }
            values.push(JsonLoaderBase::get_as_double(element));
        }
        values
    }

    /// Java `static void validate(NoiseProperty[] noises, double[] thresholds)`.
    fn validate(noises: &[Rc<dyn NoiseProperty>], thresholds: &[f64]) {
        if noises.len() != thresholds.len() {
            panic!("Number of noises must match number of thresholds");
        }

        let mut previous = f64::NEG_INFINITY;
        for &threshold in thresholds {
            if threshold <= previous {
                panic!("Thresholds must be in ascending order and cannot be equal");
            }
            previous = threshold;
        }
    }
}
