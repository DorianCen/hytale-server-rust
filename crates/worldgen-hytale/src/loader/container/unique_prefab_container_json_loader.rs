// Ported from: `.hytale-source/.../worldgen/loader/container/UniquePrefabContainerJsonLoader.java`
//! Java `loader.container.UniquePrefabContainerJsonLoader`.
//!
//! The container holds the *unique* prefab generators of a zone: the spawn
//! points, the landmarks, the temples, …
//!
//! # Seed chain
//!
//! ```java
//! public UniquePrefabContainerJsonLoader(…) {
//!     super(seed.append(".UniquePrefabContainer"), dataFolder, json);
//! }
//! ```
//!
//! and every entry:
//!
//! ```java
//! generators[i] = new UniquePrefabGeneratorJsonLoader(
//!     seed.append(String.format(SEED_INDEX_SUFFIX, i)), dataFolder, jsonArray.get(i), zoneContext)
//!     .load();
//! ```
//!
//! The generator loader appends `".UniquePrefabGenerator"`, so the chain of
//! the *n*-th entry is
//! `…UniquePrefabContainer-<n>.UniquePrefabGenerator`. The configuration
//! loader does **not** append anything (it shares the seed of the
//! generator), and the weighted prefab map appends `".WeightedPrefabMap"`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::container::unique_prefab_container::{UniquePrefabContainer, UniquePrefabGenerator};
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::prefab::unique::unique_prefab_configuration_json_loader::UniquePrefabConfigurationJsonLoader;
use crate::loader::prefab::weighted_prefab_map_json_loader::WeightedPrefabMapJsonLoader;
use crate::prefab::prefab_category::PrefabCategory;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::weighted_map::IWeightedMap;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `UniquePrefabContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_ENTRIES: &str = "Entries";
    pub const KEY_CONFIG: &str = "Config";
    pub const KEY_PREFAB: &str = "Prefab";
    pub const KEY_WEIGHTS: &str = "Weights";
    pub const KEY_ENTRY_NAME: &str = "Name";

    pub const NO_NAME: &str = "NO_NAME_GIVEN";

    pub const SEED_INDEX_SUFFIX: &str = "-%s";

    /// Java `PrefabPatternGeneratorJsonLoader.Constants.KEY_CATEGORY`.
    pub const KEY_CATEGORY: &str = "Category";
}

/// Java `UniquePrefabContainerJsonLoader`.
pub struct UniquePrefabContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, UniquePrefabContainer>`.
    pub base: JsonLoaderBase,
    /// Java `protected final ZoneFileContext zoneContext`.
    pub zone_context: Rc<ZoneFileContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl UniquePrefabContainerJsonLoader {
    /// Java `UniquePrefabContainerJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".UniquePrefabContainer"), data_folder, json),
            zone_context,
            registries,
        }
    }

    /// Java `UniquePrefabContainer load()`.
    ///
    /// ```java
    /// final UniquePrefabGenerator[] generators;
    /// if (json == null || json.isJsonNull()) {
    ///     generators = EMPTY_GENERATORS;
    /// } else {
    ///     final JsonArray jsonArray = get(KEY_ENTRIES).getAsJsonArray();
    ///     generators = new UniquePrefabGenerator[jsonArray.size()];
    ///     for (int i = 0; i < jsonArray.size(); i++) {
    ///         generators[i] = new UniquePrefabGeneratorJsonLoader(seed.append(String.format(SEED_INDEX_SUFFIX, i)), dataFolder, jsonArray.get(i), zoneContext).load();
    ///     }
    /// }
    /// return new UniquePrefabContainer(seed.hashCode(), generators);
    /// ```
    pub fn load(&self) -> UniquePrefabContainer {
        let generators: Vec<UniquePrefabGenerator> = match self.base.json.as_ref() {
            None | Some(Value::Null) => Vec::new(),
            Some(_) => {
                let json_array = self
                    .base
                    .get(constants::KEY_ENTRIES)
                    .expect("Cannot invoke \"com.google.gson.JsonElement.getAsJsonArray()\" because \"this.get(KEY_ENTRIES)\" is null")
                    .as_array()
                    .cloned()
                    .expect("Not a JSON array");

                let mut generators = Vec::with_capacity(json_array.len());
                for (index, element) in json_array.iter().enumerate() {
                    let seed = self
                        .base
                        .seed
                        .append(&format!("-{index}"))
                        .append(".UniquePrefabGenerator");
                    generators.push(self.load_generator(seed, element.clone()));
                }
                generators
            }
        };
        UniquePrefabContainer::new(self.base.seed.hash_code(), generators)
    }

    /// Java `UniquePrefabContainerJsonLoader.UniquePrefabGeneratorJsonLoader.load()`.
    fn load_generator(&self, seed: SeedString, json: Value) -> UniquePrefabGenerator {
        let name = self.load_name(&json);
        let category = self.load_category(&json);
        let prefabs = self.load_prefabs(&seed, &json);
        let configuration = self.load_configuration(&seed, &json);
        UniquePrefabGenerator::new(
            name,
            category,
            prefabs,
            configuration,
            self.zone_context.base.get_id(),
        )
    }

    /// Java `public String loadName()`.
    fn load_name(&self, json: &Value) -> String {
        match json.get(constants::KEY_ENTRY_NAME) {
            Some(name) => JsonLoaderBase::get_as_string(name),
            None => constants::NO_NAME.to_string(),
        }
    }

    /// Java `protected PrefabCategory loadCategory()`.
    ///
    /// ```java
    /// final var category = mustGetString(KEY_CATEGORY, "");
    /// if (category.isEmpty()) {
    ///     return PrefabCategory.UNIQUE;
    /// }
    /// if (!zoneContext.getParentContext().getPrefabCategories().contains(category)) {
    ///     LogUtil.getLogger().at(Level.WARNING).log("Could not find prefab category: %s, defaulting to None", category);
    ///     return PrefabCategory.UNIQUE;
    /// }
    /// return zoneContext.getParentContext().getPrefabCategories().get(category);
    /// ```
    fn load_category(&self, json: &Value) -> PrefabCategory {
        // Java: `mustGetString(KEY_CATEGORY, "")` — a missing key gives the
        // empty string, a non-string value is an error.
        let category = match json.get(constants::KEY_CATEGORY) {
            None => String::new(),
            Some(value) if value.is_string() => value.as_str().unwrap_or_default().to_string(),
            Some(_) => panic!(
                "Property '{}' must be of type 'String'",
                constants::KEY_CATEGORY
            ),
        };

        if category.is_empty() {
            return PrefabCategory::unique();
        }

        let parent_context = self.zone_context.get_parent_context();
        let categories = parent_context.get_prefab_categories();
        let categories = categories.borrow();
        if !categories.contains(&category) {
            crate::util::log_util::warn(&format!(
                "Could not find prefab category: {category}, defaulting to None"
            ));
            return PrefabCategory::unique();
        }

        categories.get(&category).as_ref().clone()
    }

    /// Java `@Nonnull UniquePrefabConfiguration loadConfiguration()`.
    fn load_configuration(
        &self,
        seed: &SeedString,
        json: &Value,
    ) -> crate::prefab::unique::unique_prefab_configuration::UniquePrefabConfiguration {
        UniquePrefabConfigurationJsonLoader::new(
            seed.clone(),
            &self.base.data_folder,
            json.get(constants::KEY_CONFIG).cloned(),
            Rc::clone(&self.zone_context),
            Rc::clone(&self.registries),
        )
        .load()
    }

    /// Java `@Nullable IWeightedMap<WorldGenPrefabSupplier> loadPrefabs()`.
    fn load_prefabs(
        &self,
        seed: &SeedString,
        json: &Value,
    ) -> Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
        let loader = self
            .zone_context
            .get_parent_context()
            .get_prefab_loader()
            .unwrap_or_else(|| {
                panic!(
                    "// SEAM(prefab-loader): the prefab loader is not configured (the world generator was \
                     not loaded through the loader chain)"
                )
            });
        let map = WeightedPrefabMapJsonLoader::new(
            seed.clone(),
            &self.base.data_folder,
            Some(json.clone()),
            constants::KEY_PREFAB,
            constants::KEY_WEIGHTS,
            loader,
        )
        .load();
        Rc::new(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;
    use crate::loader::test_prefab_store::test_prefab_loader;
    use std::path::PathBuf;

    /// A zone context whose root holds the test prefab loader.
    fn zone_context() -> Rc<ZoneFileContext> {
        let loading = FileLoadingContext::new("test", PathBuf::from("."));
        loading.set_prefab_loader(test_prefab_loader());
        loading.create_zone("Zone", PathBuf::from("Zones/Zone"))
    }

    /// A loader over the given container JSON.
    fn loader(json: Option<Value>) -> UniquePrefabContainerJsonLoader {
        UniquePrefabContainerJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            zone_context(),
            Rc::new(Registries::load()),
        )
    }

    #[test]
    fn the_seed_appends_the_unique_prefab_container_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.UniquePrefabContainer");
    }

    #[test]
    fn an_absent_container_loads_an_empty_one() {
        let container = loader(None).load();
        assert!(container.get_generators().is_empty());
    }

    #[test]
    fn a_null_container_loads_an_empty_one() {
        let container = loader(Some(Value::Null)).load();
        assert!(container.get_generators().is_empty());
    }

    #[test]
    fn the_entries_become_generators() {
        let container = loader(Some(serde_json::json!({
            "Entries": [
                {
                    "Name": "Spawn",
                    "Prefab": ["A"],
                    "Config": { "Anchor": [0, 0] }
                }
            ]
        })))
        .load();
        assert_eq!(container.get_generators().len(), 1);
        let generator = &container.get_generators()[0];
        assert_eq!(generator.get_name(), "Spawn");
        // The default category of a unique prefab is `UNIQUE`.
        assert_eq!(generator.get_category().name, "Unique");
        assert_eq!(generator.get_zone_index(), 0);
        assert_eq!(
            generator.get_configuration().get_anchor(),
            crate::loader::util::Vector2d::new(0.0, 0.0)
        );
    }

    #[test]
    fn an_entry_without_a_name_gets_the_java_placeholder() {
        let container = loader(Some(serde_json::json!({
            "Entries": [
                {
                    "Prefab": ["A"],
                    "Config": { "Anchor": [0, 0] }
                }
            ]
        })))
        .load();
        assert_eq!(container.get_generators()[0].get_name(), "NO_NAME_GIVEN");
    }
}
