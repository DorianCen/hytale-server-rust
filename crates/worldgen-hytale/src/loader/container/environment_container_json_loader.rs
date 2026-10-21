// Ported from: `.hytale-source/.../worldgen/loader/container/EnvironmentContainerJsonLoader.java`
//! Java `loader.container.EnvironmentContainerJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::container::environment_container::{
    DefaultEnvironmentContainerEntry, EnvironmentContainer, EnvironmentContainerEntry,
};
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::util::constant_noise_property::default_zero;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::ICoordinateCondition;
use procedural::noise::support::NoiseProperty;

/// Java `EnvironmentContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_DEFAULT: &str = "Default";
    pub const KEY_ENTRIES: &str = "Entries";
    pub const KEY_NAMES: &str = "Names";
    pub const KEY_WEIGHTS: &str = "Weights";
    pub const KEY_ENTRY_NOISE: &str = "Noise";
    pub const KEY_NOISE_MASK: &str = "NoiseMask";

    pub const ERROR_NAMES_NOT_FOUND: &str = "Could not find names. Keyword: Names";
    /// Java reuses the tint message verbatim.
    pub const ERROR_WEIGHT_SIZE: &str = "Tint weights array size does not fit color array size.";
    pub const ERROR_NO_VALUE_NOISE: &str = "Could not find value noise. Keyword: Noise";
    /// Java reuses the tint message verbatim.
    pub const ERROR_LOADING_ENTRY: &str = "Failed to load TintContainerEntry #%s";

    pub const SEED_INDEX_SUFFIX: &str = "-%s";
}

/// Java `EnvironmentContainerJsonLoader`.
pub struct EnvironmentContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, EnvironmentContainer>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull protected final BiomeFileContext biomeContext` — the
    /// Java `ModifyEvents` hooks are not ported.
    pub biome_context: Rc<BiomeFileContext>,
    /// The resolved environment registry (`Environment.getAssetMap()`).
    pub registries: Rc<Registries>,
}

impl EnvironmentContainerJsonLoader {
    /// Java `EnvironmentContainerJsonLoader(SeedString seed, Path dataFolder, JsonElement json, BiomeFileContext biomeContext)`.
    ///
    /// Note: the top-level loader appends **no** seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            biome_context,
            registries,
        }
    }

    /// Java `EnvironmentContainer load()`.
    pub fn load(&self) -> EnvironmentContainer {
        EnvironmentContainer::new(self.load_default(), self.load_entries())
    }

    /// Java `DefaultEnvironmentContainerEntry loadDefault()`.
    fn load_default(&self) -> DefaultEnvironmentContainerEntry {
        self.default_entry_loader().load_default()
    }

    /// The `EnvironmentContainerEntryJsonLoader` Java constructs for the
    /// default entry (`DefaultEnvironmentContainerEntryLoader extends
    /// EnvironmentContainerEntryJsonLoader` and reuses its constructor).
    fn default_entry_loader(&self) -> EnvironmentContainerEntryJsonLoader {
        let element = match self.base.json.as_ref() {
            None | Some(Value::Null) | Some(Value::Array(_)) => None,
            Some(Value::Object(_)) if self.base.has(constants::KEY_DEFAULT) => {
                self.base.get(constants::KEY_DEFAULT)
            }
            Some(Value::String(_))
            | Some(Value::Number(_))
            | Some(Value::Bool(_))
            | Some(Value::Object(_)) => self.base.json.clone(),
        };
        EnvironmentContainerEntryJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            element,
            Rc::clone(&self.registries),
        )
    }

    /// Java `EnvironmentContainerEntry[] loadEntries()`.
    fn load_entries(&self) -> Vec<EnvironmentContainerEntry> {
        let entries_value = self
            .base
            .must_get_array(constants::KEY_ENTRIES, Some(Value::Array(Vec::new())));
        let entries_array = entries_value.as_array().unwrap();
        let mut entries = Vec::with_capacity(entries_array.len());
        for (index, element) in entries_array.iter().enumerate() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                EnvironmentContainerEntryJsonLoader::new(
                    self.base.seed.append(&format!("-{index}")),
                    &self.base.data_folder,
                    Some(element.clone()),
                    Rc::clone(&self.registries),
                )
                .load()
            }));
            match result {
                Ok(entry) => entries.push(entry),
                Err(_) => panic!(
                    "{}",
                    constants::ERROR_LOADING_ENTRY.replacen("%s", &index.to_string(), 1)
                ),
            }
        }
        entries
    }
}

/// Java `EnvironmentContainerJsonLoader.EnvironmentContainerEntryJsonLoader`.
struct EnvironmentContainerEntryJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl EnvironmentContainerEntryJsonLoader {
    /// Java `EnvironmentContainerEntryJsonLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".EnvironmentContainer"), data_folder, json),
            registries,
        }
    }

    /// Java `EnvironmentContainerEntry load()`.
    fn load(&self) -> EnvironmentContainerEntry {
        let id_mapping = self.load_id_mapping();
        EnvironmentContainerEntry::new(
            Rc::clone(&id_mapping),
            if id_mapping.size() > 1 {
                self.load_value_noise()
            } else {
                default_zero()
            },
            self.load_map_condition(),
        )
    }

    /// Java `IWeightedMap<Integer> loadIdMapping()`.
    fn load_id_mapping(&self) -> Rc<dyn IWeightedMap<i32>> {
        let mut builder = WeightedMap::<i32>::builder();
        match self.base.json.as_ref() {
            None | Some(Value::Null) => {
                builder.put(0, 1.0);
            }
            Some(Value::Object(_)) => {
                if !self.base.has(constants::KEY_NAMES) {
                    panic!("{}", constants::ERROR_NAMES_NOT_FOUND);
                }
                let names_element = self.base.get(constants::KEY_NAMES).unwrap();
                if let Some(names) = names_element.as_array() {
                    let weights = if self.base.has(constants::KEY_WEIGHTS) {
                        Some(self.base.get(constants::KEY_WEIGHTS).unwrap())
                    } else {
                        None
                    };
                    if let Some(weights) = &weights {
                        if weights.as_array().map(Vec::len) != Some(names.len()) {
                            panic!("{}", constants::ERROR_WEIGHT_SIZE);
                        }
                    }
                    for (index, name) in names.iter().enumerate() {
                        let key = JsonLoaderBase::get_as_string(name);
                        let environment = self
                            .registries
                            .environments
                            .index_of(&key)
                            .unwrap_or_else(|| panic!("Unknown key! {key}"));
                        let weight = match &weights {
                            Some(weights) => {
                                JsonLoaderBase::get_as_double(&weights.as_array().unwrap()[index])
                            }
                            None => 1.0,
                        };
                        builder.put(environment, weight);
                    }
                } else {
                    let key = JsonLoaderBase::get_as_string(&names_element);
                    let environment = self
                        .registries
                        .environments
                        .index_of(&key)
                        .unwrap_or_else(|| panic!("Unknown key! {key}"));
                    builder.put(environment, 1.0);
                }
            }
            Some(primitive) => {
                let key = JsonLoaderBase::get_as_string(primitive);
                let environment = self
                    .registries
                    .environments
                    .index_of(&key)
                    .unwrap_or_else(|| panic!("Unknown key! {key}"));
                builder.put(environment, 1.0);
            }
        }
        Rc::new(builder.build())
    }

    /// Java `NoiseProperty loadValueNoise()`.
    fn load_value_noise(&self) -> Rc<dyn NoiseProperty> {
        if !self.base.has(constants::KEY_ENTRY_NOISE) {
            panic!("{}", constants::ERROR_NO_VALUE_NOISE);
        }
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_NOISE),
        )
        .load()
    }

    /// Java `ICoordinateCondition loadMapCondition()`.
    fn load_map_condition(&self) -> Rc<dyn ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE_MASK),
        )
        .load()
    }

    /// Java `DefaultEnvironmentContainerEntry load()` (the subclass override).
    fn load_default(&self) -> DefaultEnvironmentContainerEntry {
        let id_mapping: Rc<dyn IWeightedMap<i32>> = match self.base.json.as_ref() {
            None | Some(Value::Null) => {
                let mut builder = WeightedMap::<i32>::builder();
                builder.put(0, 1.0);
                Rc::new(builder.build())
            }
            Some(primitive) if !primitive.is_object() && !primitive.is_array() => {
                let mut builder = WeightedMap::<i32>::builder();
                let key = JsonLoaderBase::get_as_string(primitive);
                let environment = self
                    .registries
                    .environments
                    .index_of(&key)
                    .unwrap_or_else(|| panic!("Unknown key! {key}"));
                builder.put(environment, 1.0);
                Rc::new(builder.build())
            }
            _ => self.load_id_mapping(),
        };
        DefaultEnvironmentContainerEntry::new(
            Rc::clone(&id_mapping),
            if id_mapping.size() > 1 {
                self.load_value_noise()
            } else {
                default_zero()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::loader::context::biome_file_context::BiomeType;
    use crate::loader::context::file_loading_context::FileLoadingContext;

    fn biome_context() -> Rc<BiomeFileContext> {
        let loading = FileLoadingContext::new("test", PathBuf::from("."));
        let zone = loading.create_zone("Zone", PathBuf::from("Zones/Zone"));
        zone.create_biome("Plains", PathBuf::from("Tile.Plains.json"), BiomeType::Tile)
    }

    #[test]
    fn default_entry_seed_is_the_environment_container_suffix() {
        let loader = EnvironmentContainerJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Default": "Default"})),
            biome_context(),
            Rc::new(Registries::empty()),
        );
        // Java `DefaultEnvironmentContainerEntryLoader` calls the
        // `EnvironmentContainerEntryJsonLoader` constructor, which appends
        // ".EnvironmentContainer" (the top-level container appends nothing).
        assert_eq!(
            loader.default_entry_loader().base.seed.seed(),
            "0.EnvironmentContainer"
        );
    }

    #[test]
    fn the_default_entry_resolves_the_environment_name() {
        let loader = EnvironmentContainerJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Default": "Default", "Entries": []})),
            biome_context(),
            Rc::new(Registries::load()),
        );
        let entry = loader.load_default();
        assert_eq!(
            entry.get_environment_mapping().get(0.0).copied(),
            Some(58),
            "environment Default"
        );
    }
}
