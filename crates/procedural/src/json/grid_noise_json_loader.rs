// Ported from: `.hytale-source/.../procedurallib/json/GridNoiseJsonLoader.java`
//! Java `GridNoiseJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::grid_noise::GridNoise;

/// Java `GridNoiseJsonLoader.Constants`.
mod constants {
    pub const DEFAULT_NO_THICKNESS: f64 = f64::NAN;
    pub const DEFAULT_THICKNESS_Z: f64 = 0.0;

    pub const KEY_THICKNESS: &str = "Thickness";
    pub const KEY_THICKNESS_X: &str = "ThicknessX";
    pub const KEY_THICKNESS_Y: &str = "ThicknessY";
    pub const KEY_THICKNESS_Z: &str = "ThicknessZ";
    pub const ERROR_NO_THICKNESS: &str =
        "Could not find thickness '%s' and no default 'Thickness' value defined!";
}

/// Java `GridNoiseJsonLoader`.
pub struct GridNoiseJsonLoader {
    /// Java `extends JsonLoader<K, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl GridNoiseJsonLoader {
    /// Java `GridNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".GridNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> GridNoise {
        let default_thickness = self.load_default_thickness();
        GridNoise::new(
            self.load_thickness_x(default_thickness),
            self.load_thickness_y(default_thickness),
            self.load_thickness_z(default_thickness),
        )
    }

    /// Java `double loadDefaultThickness()`.
    pub fn load_default_thickness(&self) -> f64 {
        if !self.base.has(constants::KEY_THICKNESS) {
            return constants::DEFAULT_NO_THICKNESS;
        }
        JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_THICKNESS).unwrap())
    }

    /// Java `double loadThicknessX(double defaultThickness)`.
    pub fn load_thickness_x(&self, default_thickness: f64) -> f64 {
        self.load_thickness(constants::KEY_THICKNESS_X, default_thickness)
    }

    /// Java `double loadThicknessY(double defaultThickness)`.
    pub fn load_thickness_y(&self, default_thickness: f64) -> f64 {
        self.load_thickness(constants::KEY_THICKNESS_Y, default_thickness)
    }

    /// Java `double loadThicknessZ(double defaultThickness)`.
    ///
    /// Note: Special handling to allow z to be omitted as we mostly use these
    /// noises in 2D.
    pub fn load_thickness_z(&self, default_thickness: f64) -> f64 {
        let default_thickness = if default_thickness.is_nan() {
            constants::DEFAULT_THICKNESS_Z
        } else {
            default_thickness
        };
        self.load_thickness(constants::KEY_THICKNESS_Z, default_thickness)
    }

    /// Java `double loadThickness(String key, double defaultThickness)`.
    pub fn load_thickness(&self, key: &str, default_thickness: f64) -> f64 {
        let mut value = default_thickness;

        if self.base.has(key) {
            value = JsonLoaderBase::get_as_double(&self.base.get(key).unwrap());
        }

        if value.is_nan() {
            // Java: `throw new Error(String.format(ERROR_NO_THICKNESS, key))`.
            panic!("{}", constants::ERROR_NO_THICKNESS.replace("%s", key));
        }

        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> GridNoiseJsonLoader {
        GridNoiseJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn uniform_thickness_applies_to_all_axes() {
        let noise = loader(serde_json::json!({ "Thickness": 0.2 })).load();
        assert_eq!(noise.thickness_x, 0.2);
        assert_eq!(noise.thickness_y, 0.2);
        assert_eq!(noise.thickness_z, 0.2);
    }

    #[test]
    fn per_axis_overrides_and_the_2d_z_omission() {
        let noise = loader(serde_json::json!({ "Thickness": 0.2, "ThicknessY": 0.3 })).load();
        assert_eq!(noise.thickness_x, 0.2);
        assert_eq!(noise.thickness_y, 0.3);
        // z follows the uniform default (the 0.0 special case only applies
        // when NO default thickness is defined).
        assert_eq!(noise.thickness_z, 0.2);
    }

    #[test]
    fn z_defaults_to_zero_when_no_thickness_is_defined() {
        let noise = loader(serde_json::json!({ "ThicknessX": 0.1, "ThicknessY": 0.3 })).load();
        assert_eq!(noise.thickness_x, 0.1);
        assert_eq!(noise.thickness_y, 0.3);
        // The z-omission special case: a NaN default becomes 0.0.
        assert_eq!(noise.thickness_z, 0.0);
    }

    #[test]
    #[should_panic(
        expected = "Could not find thickness 'ThicknessY' and no default 'Thickness' value defined!"
    )]
    fn unresolved_y_thickness_panics_verbatim() {
        loader(serde_json::json!({ "ThicknessX": 0.1 })).load();
    }
}
