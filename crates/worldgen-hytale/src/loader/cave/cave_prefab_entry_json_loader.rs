// Ported from: `.hytale-source/.../worldgen/loader/cave/CavePrefabEntryJsonLoader.java`
//! Java `loader.cave.CavePrefabEntryJsonLoader` — one entry of a cave prefab
//! container: the prefabs it spawns and their configuration.
//!
//! The prefab names are resolved through the prefab store (the Java
//! `WeightedPrefabMapJsonLoader`), so a name that matches several prefab
//! files contributes one weighted entry per file.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::cave::prefab::cave_prefab_container::{CavePrefabConfig, CavePrefabEntry};
use crate::cave::prefab::cave_prefab_supplier::{CavePrefabKey, CavePrefabSupplier};
use crate::loader::cave::cave_prefab_config_json_loader::CavePrefabConfigJsonLoader;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::prefab::weighted_prefab_map_json_loader::WeightedPrefabMapJsonLoader;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `CavePrefabEntryJsonLoader.Constants`.
pub mod constants {
    pub const KEY_PREFAB: &str = "Prefab";
    pub const KEY_WEIGHT: &str = "Weight";
    /// Java `CavePrefabConfigJsonLoader`.
    pub const KEY_CONFIG: &str = "Config";
}

/// Java `loader.cave.CavePrefabEntryJsonLoader`.
pub struct CavePrefabEntryJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource,
    /// CavePrefabContainer.CavePrefabEntry>`.
    pub base: JsonLoaderBase,
    /// Java `private final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl CavePrefabEntryJsonLoader {
    /// Java `CavePrefabEntryJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CavePrefabEntry"), data_folder, json),
            zone_context,
            registries,
        }
    }

    /// Java `CavePrefabContainer.CavePrefabEntry load()`.
    pub fn load(&self) -> CavePrefabEntry {
        // Java evaluates the constructor arguments left-to-right.
        let prefabs = self.load_prefabs();
        let config = self.load_config();
        CavePrefabEntry::new(prefabs, config)
    }

    /// Java `IWeightedMap<WorldGenPrefabSupplier> loadPrefabs()`.
    fn load_prefabs(&self) -> Rc<dyn IWeightedMap<CavePrefabKey>> {
        self.load_weighted_prefab_map(constants::KEY_PREFAB, constants::KEY_WEIGHT)
    }

    /// Java `WeightedPrefabMapJsonLoader.load()`.
    fn load_weighted_prefab_map(
        &self,
        prefabs_key: &str,
        weights_key: &str,
    ) -> Rc<dyn IWeightedMap<CavePrefabKey>> {
        let loader = self
            .zone_context
            .get_parent_context()
            .get_prefab_loader()
            .unwrap_or_else(|| {
                panic!(
                "// SEAM(prefab-loader): the prefab loader is not configured (the world generator was \\
                 not loaded through the loader chain)"
            )
            });
        let map = WeightedPrefabMapJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
            prefabs_key,
            weights_key,
            loader,
        )
        .load();

        // The Java map is keyed by the suppliers; the Rust cave map is keyed
        // by the `CavePrefabKey` wrapper (the weighted map needs `Eq`).
        let mut builder = WeightedMap::<CavePrefabKey>::builder();
        map.for_each_entry(&mut |supplier: &WorldGenPrefabSupplier, weight: f64| {
            builder.put(
                CavePrefabKey::new(Rc::new(supplier.clone()) as Rc<dyn CavePrefabSupplier>),
                weight,
            );
        });
        Rc::new(builder.build())
    }

    /// Java `CavePrefabContainer.CavePrefabEntry.CavePrefabConfig
    /// loadConfig()`.
    fn load_config(&self) -> CavePrefabConfig {
        CavePrefabConfigJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_CONFIG),
            Rc::clone(&self.zone_context),
            Rc::clone(&self.registries),
        )
        .load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;
    use std::path::PathBuf;

    fn loader(json: Option<Value>) -> CavePrefabEntryJsonLoader {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        loading.set_prefab_loader(crate::loader::test_prefab_store::test_prefab_loader());
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        CavePrefabEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            zone,
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_appends_the_prefab_entry_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CavePrefabEntry");
    }

    #[test]
    fn a_missing_prefab_key_panics_with_the_key_name() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(None).load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "Prefab");
    }

    #[test]
    fn the_weight_array_size_must_match() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({
                "Prefab": ["A", "B"],
                "Weight": [1.0]
            })))
            .load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Weight array size is different from prefab name array."
        );
    }
}
