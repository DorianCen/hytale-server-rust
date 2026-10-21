// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimatePointJsonLoader.java`
//! Java `loader.climate.ClimatePointJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::climate::climate_point::ClimatePoint;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ClimatePointJsonLoader.Constants.KEY_TEMPERATURE`.
pub const KEY_TEMPERATURE: &str = "Temperature";
/// Java `ClimatePointJsonLoader.Constants.KEY_INTENSITY`.
pub const KEY_INTENSITY: &str = "Intensity";
/// Java `ClimatePointJsonLoader.Constants.KEY_MODIFIER`.
pub const KEY_MODIFIER: &str = "Modifier";

/// Java `ClimatePointJsonLoader.Constants.DEFAULT_TEMPERATURE`.
pub const DEFAULT_TEMPERATURE: f64 = 0.5;
/// Java `ClimatePointJsonLoader.Constants.DEFAULT_INTENSITY`.
pub const DEFAULT_INTENSITY: f64 = 0.5;
/// Java `ClimatePointJsonLoader.Constants.DEFAULT_MODIFIER`.
pub const DEFAULT_MODIFIER: f64 = 1.0;

/// Java `ClimatePointJsonLoader`.
pub struct ClimatePointJsonLoader {
    /// Java `extends JsonLoader<K, ClimatePoint>`.
    pub base: JsonLoaderBase,
}

impl ClimatePointJsonLoader {
    /// Java `ClimatePointJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `ClimatePoint load()`.
    pub fn load(&self) -> ClimatePoint {
        match self.try_load() {
            Ok(point) => point,
            Err(message) => panic!("{message}"),
        }
    }

    /// Java `ClimatePoint load()` — the fallible variant.
    ///
    /// Java's [`ClimateTypeJsonLoader`](super::climate_type_json_loader::ClimateTypeJsonLoader)
    /// wraps every failure of the point load into an
    /// `Invalid climate point at index: %d` error; this method returns the
    /// failure message instead of panicking so the caller can wrap it.
    pub fn try_load(&self) -> Result<ClimatePoint, String> {
        Ok(ClimatePoint::with_modifier(
            self.load_temperature()?,
            self.load_intensity()?,
            self.load_modifier()?,
        ))
    }

    /// Java `double loadTemperature()`.
    pub fn load_temperature(&self) -> Result<f64, String> {
        self.must_get_number(KEY_TEMPERATURE, DEFAULT_TEMPERATURE)
    }

    /// Java `double loadIntensity()`.
    pub fn load_intensity(&self) -> Result<f64, String> {
        self.must_get_number(KEY_INTENSITY, DEFAULT_INTENSITY)
    }

    /// Java `double loadModifier()`.
    pub fn load_modifier(&self) -> Result<f64, String> {
        self.must_get_number(KEY_MODIFIER, DEFAULT_MODIFIER)
    }

    /// Java `mustGetNumber(key, defaultValue)` — the fallible variant of the
    /// base class helper (the base class panics instead of returning an
    /// error).
    fn must_get_number(&self, key: &str, default_value: f64) -> Result<f64, String> {
        match self.base.get(key) {
            None => Ok(default_value),
            Some(element) => {
                if !element.is_number() {
                    return Err(format!("Property '{key}' must be of type 'Number'"));
                }
                Ok(JsonLoaderBase::get_as_double(&element))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> ClimatePointJsonLoader {
        ClimatePointJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json))
    }

    #[test]
    fn defaults_are_applied() {
        let point = loader(serde_json::json!({})).load();
        assert_eq!(point.temperature, 0.5);
        assert_eq!(point.intensity, 0.5);
        assert_eq!(point.modifier, 1.0);
    }

    #[test]
    fn values_are_read() {
        let point = loader(serde_json::json!({
            "Temperature": 0.25,
            "Intensity": 0.75,
            "Modifier": 0.9
        }))
        .load();
        assert_eq!(point.temperature, 0.25);
        assert_eq!(point.intensity, 0.75);
        assert_eq!(point.modifier, 0.9);
    }

    #[test]
    fn non_numeric_value_is_rejected() {
        let error = loader(serde_json::json!({ "Temperature": "hot" })).try_load();
        assert_eq!(
            error.unwrap_err(),
            "Property 'Temperature' must be of type 'Number'"
        );
    }
}
