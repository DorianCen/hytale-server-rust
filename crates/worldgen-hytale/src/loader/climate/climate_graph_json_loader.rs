// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimateGraphJsonLoader.java`
//! Java `loader.climate.ClimateGraphJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::climate::climate_graph::{ClimateGraph, FadeMode, RESOLUTION};
use crate::climate::climate_type::ClimateType;
use crate::loader::climate::climate_type_json_loader::ClimateTypeJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ClimateGraphJsonLoader.Constants.KEY_CLIMATE`.
pub const KEY_CLIMATE: &str = "Climate";
/// Java `ClimateGraphJsonLoader.Constants.KEY_FADE_MODE`.
pub const KEY_FADE_MODE: &str = "FadeMode";
/// Java `ClimateGraphJsonLoader.Constants.KEY_FADE_RADIUS`.
pub const KEY_FADE_RADIUS: &str = "FadeRadius";
/// Java `ClimateGraphJsonLoader.Constants.KEY_FADE_DISTANCE`.
pub const KEY_FADE_DISTANCE: &str = "FadeDistance";
/// Java `ClimateGraphJsonLoader.Constants.KEY_CLIMATES`.
pub const KEY_CLIMATES: &str = "Climates";

/// Java `ClimateGraphJsonLoader.Constants.DEFAULT_FADE_RADIUS`.
pub const DEFAULT_FADE_RADIUS: f64 = 50.0;
/// Java `ClimateGraphJsonLoader.Constants.DEFAULT_FADE_DISTANCE`.
pub const DEFAULT_FADE_DISTANCE: f64 = 100.0;
/// Java `ClimateGraphJsonLoader.Constants.DEFAULT_FADE_MODE`.
pub const DEFAULT_FADE_MODE: &str = "CHILDREN";

/// Java `ClimateGraphJsonLoader`.
pub struct ClimateGraphJsonLoader {
    /// Java `extends JsonLoader<K, ClimateGraph>`.
    pub base: JsonLoaderBase,
}

impl ClimateGraphJsonLoader {
    /// Java `ClimateGraphJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `ClimateGraph load()`.
    pub fn load(&self) -> ClimateGraph {
        let climates = self.load_climates();
        let fade_mode = self.load_fade_mode();
        let fade_radius = self.load_fade_radius();
        let fade_distance = self.load_fade_distance();
        ClimateGraph::new(RESOLUTION, climates, fade_mode, fade_radius, fade_distance)
    }

    /// Java `ClimateGraph.FadeMode loadFadeMode()`.
    pub fn load_fade_mode(&self) -> FadeMode {
        let fade_mode = self
            .base
            .must_get_string(KEY_FADE_MODE, Some(DEFAULT_FADE_MODE));
        Self::fade_mode_value_of(&fade_mode)
    }

    /// Java `double loadFadeRadius()`.
    pub fn load_fade_radius(&self) -> f64 {
        self.base
            .must_get_number(KEY_FADE_RADIUS, Some(DEFAULT_FADE_RADIUS))
    }

    /// Java `double loadFadeDistance()`.
    pub fn load_fade_distance(&self) -> f64 {
        self.base
            .must_get_number(KEY_FADE_DISTANCE, Some(DEFAULT_FADE_DISTANCE))
    }

    /// Java `ClimateType[] loadClimates()`.
    pub fn load_climates(&self) -> Vec<Rc<ClimateType>> {
        let array = self
            .base
            .must_get_array(KEY_CLIMATES, Some(serde_json::json!([])));
        let Some(items) = array.as_array() else {
            unreachable!()
        };
        let mut climates = Vec::with_capacity(items.len());
        for item in items.iter() {
            climates.push(
                ClimateTypeJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(item.clone()),
                    None,
                )
                .load(),
            );
        }
        climates
    }

    /// Java `ClimateGraph.FadeMode.valueOf(String)`.
    pub fn fade_mode_value_of(name: &str) -> FadeMode {
        match name {
            "PARENTS" => FadeMode::Parents,
            "CHILDREN" => FadeMode::Children,
            other => panic!("No enum constant ClimateGraph.FadeMode.{other}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracts the message of a caught panic.
    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            return (*message).to_string();
        }
        "<non-string panic>".to_string()
    }

    fn loader(json: Value) -> ClimateGraphJsonLoader {
        ClimateGraphJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json))
    }

    #[test]
    fn defaults_are_applied() {
        let graph = loader(serde_json::json!({
            "Climates": [{ "Name": "Zone 1", "Points": [{ "Temperature": 0.5, "Intensity": 0.5 }] }]
        }))
        .load();

        assert_eq!(graph.fade_mode(), FadeMode::Children);
        assert_eq!(graph.fade_radius(), 50.0);
        assert_eq!(graph.fade_distance(), 100.0);
        assert_eq!(graph.get_parents().len(), 1);
        assert_eq!(graph.get_parents()[0].name, "Zone 1");
    }

    #[test]
    fn values_are_read() {
        let graph = loader(serde_json::json!({
            "FadeMode": "PARENTS",
            "FadeRadius": 12.5,
            "FadeDistance": 42.0,
            "Climates": []
        }))
        .load();

        assert_eq!(graph.fade_mode(), FadeMode::Parents);
        assert_eq!(graph.fade_radius(), 12.5);
        assert_eq!(graph.fade_distance(), 42.0);
        assert!(graph.get_parents().is_empty());
    }

    #[test]
    fn unknown_fade_mode_panics() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(serde_json::json!({ "FadeMode": "NOPE" })).load()
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert_eq!(message, "No enum constant ClimateGraph.FadeMode.NOPE");
    }
}
