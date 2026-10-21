// Ported from: `.hytale-source/.../worldgen/loader/climate/UniqueClimateGeneratorJsonLoader.java`
//! Java `loader.climate.UniqueClimateGeneratorJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::climate::unique_climate_generator::UniqueClimateGenerator;
use crate::loader::climate::unique_climate_json_loader::UniqueClimateJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `UniqueClimateGeneratorJsonLoader`.
pub struct UniqueClimateGeneratorJsonLoader {
    /// Java `extends JsonLoader<K, UniqueClimateGenerator>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull private final JsonArray array`.
    array: Vec<Value>,
}

impl UniqueClimateGeneratorJsonLoader {
    /// Java `UniqueClimateGeneratorJsonLoader(SeedString<K> seed, Path dataFolder, JsonArray json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        let array = match &json {
            Some(Value::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            array,
        }
    }

    /// Java `UniqueClimateGenerator load()`.
    pub fn load(&self) -> UniqueClimateGenerator {
        if self.array.is_empty() {
            return UniqueClimateGenerator::empty();
        }
        UniqueClimateGenerator::from_entries(self.load_entries())
    }

    /// Java `UniqueClimateGenerator.Entry[] loadEntries()`.
    pub fn load_entries(&self) -> Vec<crate::climate::unique_climate_generator::Entry> {
        let mut entries = Vec::with_capacity(self.array.len());
        for item in self.array.iter() {
            entries.push(
                UniqueClimateJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(item.clone()),
                )
                .load(),
            );
        }
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> UniqueClimateGeneratorJsonLoader {
        UniqueClimateGeneratorJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json))
    }

    #[test]
    fn empty_array_yields_the_empty_generator() {
        let generator = loader(serde_json::json!([])).load();
        assert!(generator.entries.is_empty());
        assert!(generator.zones.is_empty());
    }

    #[test]
    fn entries_are_loaded() {
        let generator = loader(serde_json::json!([
            { "Name": "Zone1_Spawn", "Color": "#ff0000", "Rule": {} },
            { "Name": "Zone1_Temple", "Parent": "Zone1_Spawn", "Color": "#ffff00", "Rule": {} }
        ]))
        .load();

        assert_eq!(generator.entries.len(), 2);
        assert_eq!(generator.entries[0].zone, "Zone1_Spawn");
        assert_eq!(generator.entries[1].parent, "Zone1_Spawn");
        assert_eq!(generator.entries[1].color, 0xFFFF00);
    }
}
