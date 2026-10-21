// Ported from: `.hytale-source/.../worldgen/loader/container/PrefabContainerJsonLoader.java`
//! Java `loader.container.PrefabContainerJsonLoader` — the prefab catalogue
//! of a biome.
//!
//! Every entry holds the weighted map of the prefab suppliers, the prefab
//! pattern generator and the environment of the placement.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::container::prefab_container::{PrefabContainer, PrefabContainerEntry};
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::loader::context::file_loading_context::FileLoadingContext;
use crate::loader::prefab::prefab_pattern_generator_json_loader::PrefabPatternGeneratorJsonLoader;
use crate::loader::prefab::weighted_prefab_map_json_loader::WeightedPrefabMapJsonLoader;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::weighted_map::WeightedMap;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `PrefabContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_ENTRIES: &str = "Entries";
    pub const KEY_ENTRY_PREFAB: &str = "Prefab";
    pub const KEY_ENTRY_WEIGHT: &str = "Weight";
    pub const KEY_ENTRY_PATTERN: &str = "Pattern";
    pub const KEY_ENVIRONMENT: &str = "Environment";

    pub const ERROR_FAIL_ENTRY: &str = "Failed to load prefab container entry #%s.";
    pub const ERROR_LOADING_ENVIRONMENT: &str = "Error while looking up environment \"%s\"!";
    pub const ERROR_ENTRY_NO_PATTERN: &str = "Could not find prefab pattern. Keyword: Pattern";
}

/// Java `loader.container.PrefabContainerJsonLoader`.
pub struct PrefabContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, PrefabContainer>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull protected final BiomeFileContext biomeContext` — the
    /// Java `ModifyEvents` hooks are not ported.
    pub biome_context: Rc<BiomeFileContext>,
    /// Java `@Nonnull protected final FileLoadingContext fileContext`.
    pub file_context: Rc<FileLoadingContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl PrefabContainerJsonLoader {
    /// Java `PrefabContainerJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, BiomeFileContext biomeContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        let file_context = biome_context.get_parent_context().get_parent_context();
        Self {
            base: JsonLoaderBase::new(seed.append(".PrefabContainer"), data_folder, json),
            biome_context,
            file_context,
            registries,
        }
    }

    /// Java `PrefabContainer load()`.
    pub fn load(&self) -> PrefabContainer {
        PrefabContainer::new(self.load_entries())
    }

    /// Java `PrefabContainer.PrefabContainerEntry[] loadEntries()`.
    fn load_entries(&self) -> Vec<Rc<PrefabContainerEntry>> {
        let prefab_array = self
            .base
            .must_get_array(constants::KEY_ENTRIES, Some(Value::Array(Vec::new())));
        let prefab_array = prefab_array.as_array().unwrap();

        let mut entries = Vec::with_capacity(prefab_array.len());
        for (index, element) in prefab_array.iter().enumerate() {
            let seed = self
                .base
                .seed
                .append(&format!("-{index}"))
                .append(".PrefabContainerEntry");
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.load_entry(seed, element.clone())
            }));
            match result {
                Ok(entry) => entries.push(Rc::new(entry)),
                Err(_) => panic!(
                    "{}",
                    constants::ERROR_FAIL_ENTRY.replace("%s", &index.to_string())
                ),
            }
        }

        // The Java loader dispatches the `ModifyEvents.BiomePrefabs` event
        // here; the modifier / event plugin system is deferred (see
        // `REMAINING.md` §1), so the port skips the dispatch.
        entries
    }

    /// Java `PrefabContainerJsonLoader.PrefabContainerEntryJsonLoader.load()`.
    fn load_entry(&self, seed: SeedString, json: Value) -> PrefabContainerEntry {
        let prefabs = self.load_prefabs(seed.clone(), json.clone());
        let pattern_generator = self.load_pattern_generator(seed, json.clone());
        let environment_id = self.load_environment(&json);
        PrefabContainerEntry::new(prefabs, pattern_generator, environment_id)
    }

    /// Java `IWeightedMap<WorldGenPrefabSupplier> prefabs = new
    /// WeightedPrefabMapJsonLoader(seed, dataFolder, json, KEY_ENTRY_PREFAB,
    /// KEY_ENTRY_WEIGHT).load()`.
    fn load_prefabs(
        &self,
        seed: SeedString,
        json: Value,
    ) -> Rc<dyn crate::weighted_map::IWeightedMap<WorldGenPrefabSupplier>> {
        let loader = self.file_context.get_prefab_loader().unwrap_or_else(|| {
            panic!(
                "// SEAM(prefab-loader): the prefab loader is not configured (the world generator was \
                     not loaded through the loader chain)"
            )
        });
        let map = WeightedPrefabMapJsonLoader::new(
            seed,
            &self.base.data_folder,
            Some(json.clone()),
            constants::KEY_ENTRY_PREFAB,
            constants::KEY_ENTRY_WEIGHT,
            loader,
        )
        .load();
        Rc::new(map)
    }

    /// Java `PrefabPatternGenerator prefabPatternGenerator = new
    /// PrefabPatternGeneratorJsonLoader(seed, dataFolder, get(KEY_ENTRY_PATTERN),
    /// context).load()`.
    fn load_pattern_generator(
        &self,
        seed: SeedString,
        json: Value,
    ) -> Rc<crate::prefab::prefab_pattern_generator::PrefabPatternGenerator> {
        let pattern = json.get(constants::KEY_ENTRY_PATTERN).cloned();
        if pattern.is_none() {
            panic!("{}", constants::ERROR_ENTRY_NO_PATTERN);
        }
        Rc::new(
            PrefabPatternGeneratorJsonLoader::new(
                seed,
                &self.base.data_folder,
                pattern,
                Rc::clone(&self.file_context),
                Rc::clone(&self.registries),
            )
            .load(),
        )
    }

    /// Java `protected int loadEnvironment()`.
    fn load_environment(&self, json: &Value) -> i32 {
        let Some(environment) = json.get(constants::KEY_ENVIRONMENT) else {
            return worldgen_model::chunk::ENVIRONMENT_NOT_SET;
        };
        let environment_id = JsonLoaderBase::get_as_string(environment);
        match self.registries.environments.index_of(&environment_id) {
            Some(index) => index,
            None => panic!(
                "{}",
                constants::ERROR_LOADING_ENVIRONMENT.replace("%s", &environment_id)
            ),
        }
    }
}

/// Java `WeightedPrefabMapJsonLoader` — the weighted map of the prefab
/// suppliers of an entry.
pub type PrefabEntryMap = WeightedMap<WorldGenPrefabSupplier>;

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

    fn loader(json: Option<Value>) -> PrefabContainerJsonLoader {
        PrefabContainerJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            biome_context(),
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_appends_the_prefab_container_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.PrefabContainer");
    }

    #[test]
    fn an_empty_entries_array_loads_an_empty_container() {
        let loader = loader(Some(serde_json::json!({"Entries": []})));
        assert!(loader.load().get_entries().is_empty());
    }

    #[test]
    fn a_missing_entries_key_loads_an_empty_container() {
        // Java: `mustGetArray(KEY_ENTRIES, EMPTY_ARRAY)` — the default is
        // the empty array, so a missing key is not an error.
        let container = loader(Some(serde_json::json!({}))).load();
        assert!(container.get_entries().is_empty());
    }
}
