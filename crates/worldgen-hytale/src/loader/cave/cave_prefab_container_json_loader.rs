// Ported from: `.hytale-source/.../worldgen/loader/cave/CavePrefabContainerJsonLoader.java`
//! Java `loader.cave.CavePrefabContainerJsonLoader` — the prefabs a cave node
//! type can spawn.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::cave::prefab::cave_prefab_container::CavePrefabContainer;
use crate::loader::cave::cave_prefab_entry_json_loader::CavePrefabEntryJsonLoader;
use crate::loader::context::cave_file_context::CaveFileContext;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `CavePrefabContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_ENTRIES: &str = "Entries";

    pub const SEED_ENTRY_SUFFIX: &str = "-%s";

    pub const ERROR_NO_ENTRIES: &str =
        "Could not find entries in prefab container. Keyword: Entries";
}

/// Java `loader.cave.CavePrefabContainerJsonLoader`.
pub struct CavePrefabContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CavePrefabContainer>`.
    pub base: JsonLoaderBase,
    /// Java `protected final CaveFileContext context`.
    context: Rc<CaveFileContext>,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl CavePrefabContainerJsonLoader {
    /// Java `CavePrefabContainerJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, CaveFileContext context)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        context: Rc<CaveFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CavePrefabContainer"), data_folder, json),
            context,
            registries,
        }
    }

    /// Java `CavePrefabContainer load()`.
    pub fn load(&self) -> CavePrefabContainer {
        CavePrefabContainer::new(self.load_entries())
    }

    /// Java `CavePrefabContainer.CavePrefabEntry[] loadEntries()`.
    fn load_entries(&self) -> Vec<crate::cave::prefab::cave_prefab_container::CavePrefabEntry> {
        if self.base.json.is_none() {
            return Vec::new();
        }

        let prefab_array = self.base.must_get_array(constants::KEY_ENTRIES, None);
        let prefab_array = prefab_array.as_array().cloned().unwrap_or_default();

        let mut entries = Vec::with_capacity(prefab_array.len());
        for (index, element) in prefab_array.iter().enumerate() {
            entries.push(
                CavePrefabEntryJsonLoader::new(
                    self.base
                        .seed
                        .append(&constants::SEED_ENTRY_SUFFIX.replace("%s", &index.to_string())),
                    &self.base.data_folder,
                    Some(element.clone()),
                    self.context.get_parent_context(),
                    Rc::clone(&self.registries),
                )
                .load(),
            );
        }

        // The Java loader dispatches the `ModifyEvents.CavePrefabs` event
        // here; the modifier / event plugin system is deferred (see
        // `REMAINING.md` §1), so the port skips the dispatch.
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;
    use std::path::PathBuf;

    fn context() -> Rc<CaveFileContext> {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        loading.set_prefab_loader(crate::loader::test_prefab_store::test_prefab_loader());
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        CaveFileContext::new("Caldera", zone)
    }

    fn loader(json: Option<Value>) -> CavePrefabContainerJsonLoader {
        CavePrefabContainerJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            context(),
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_appends_the_prefab_container_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CavePrefabContainer");
    }

    #[test]
    fn an_absent_json_loads_an_empty_container() {
        let container = loader(None).load();
        assert!(container.get_entries().is_empty());
    }

    #[test]
    fn a_missing_entries_key_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({}))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "Missing property 'Entries'");
    }

    #[test]
    fn the_entries_are_loaded() {
        let container = loader(Some(serde_json::json!({
            "Entries": [{"Prefab": ["A"]}]
        })))
        .load();
        assert_eq!(container.get_entries().len(), 1);
    }
}
