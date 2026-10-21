// Ported from: `.hytale-source/.../procedurallib/json/GradientNoisePropertyJsonLoader.java`
//! Java `GradientNoisePropertyJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::property::gradient_noise_property::{GradientMode, GradientNoiseProperty};
use crate::noise::property::NoiseProperty;

/// Java `GradientNoisePropertyJsonLoader`.
pub struct GradientNoisePropertyJsonLoader {
    /// Java `extends JsonLoader<K, GradientNoiseProperty>`.
    pub base: JsonLoaderBase,
    /// Java `protected final NoiseProperty noise`.
    noise: Rc<dyn NoiseProperty>,
}

impl GradientNoisePropertyJsonLoader {
    /// Java `GradientNoisePropertyJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, NoiseProperty noise)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        noise: Rc<dyn NoiseProperty>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            noise,
        }
    }

    /// Java `GradientNoiseProperty load()`.
    pub fn load(&self) -> GradientNoiseProperty {
        GradientNoiseProperty::new(
            self.noise.clone(),
            self.load_mode(),
            self.load_distance(),
            self.load_normalization(),
        )
    }

    /// Java `GradientNoiseProperty.GradientMode loadMode()`.
    fn load_mode(&self) -> GradientMode {
        const KEY_MODE: &str = "Mode";
        let mut mode = GradientMode::Magnitude;
        if self.base.has(KEY_MODE) {
            mode = Self::gradient_mode_value_of(&JsonLoaderBase::get_as_string(
                &self.base.get(KEY_MODE).unwrap(),
            ));
        }
        mode
    }

    /// Java `GradientNoiseProperty.GradientMode.valueOf(String)`.
    pub fn gradient_mode_value_of(name: &str) -> GradientMode {
        match name {
            "MAGNITUDE" => GradientMode::Magnitude,
            "ANGLE" => GradientMode::Angle,
            "ANGLE_ABS" => GradientMode::AngleAbs,
            other => panic!("No enum constant GradientNoiseProperty.GradientMode.{other}"),
        }
    }

    /// Java `double loadDistance()`.
    fn load_distance(&self) -> f64 {
        const KEY_DISTANCE: &str = "Distance";
        if self.base.has(KEY_DISTANCE) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_DISTANCE).unwrap())
        } else {
            5.0
        }
    }

    /// Java `double loadNormalization()`.
    fn load_normalization(&self) -> f64 {
        const KEY_NORMALIZE: &str = "Normalize";
        if self.base.has(KEY_NORMALIZE) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_NORMALIZE).unwrap())
        } else {
            0.1
        }
    }
}
