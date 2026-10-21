// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveGeneratorJsonLoader.java`
//! Java `loader.cave.CaveGeneratorJsonLoader` — the cave generator of a zone.
//!
//! The loader reads the `Caves.json` of the zone's `Cave` folder; a zone
//! without that file has **no** cave generator (the Java loader returns
//! `null`, it does not fail).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;

use crate::cave::cave_generator::{CaveGenerator, CaveGeneratorImpl};
use crate::loader::cave::cave_types_json_loader::CaveTypesJsonLoader;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::seed_string_resource::SeedStringResource;
use procedural::json::{JsonLoaderBase, SeedString};
use worldgen_model::registry::Registries;

/// Java `CaveGeneratorJsonLoader.Constants`.
pub mod constants {
    pub const FILE_NAME: &str = "Caves";
    pub const FILE_CAVES_JSON: &str = "Caves.json";
    pub const KEY_TYPES: &str = "Types";
    pub const ERROR_LOADING_CAVES: &str = "Error while loading caves for world generator from %s";
}

/// Java `loader.cave.CaveGeneratorJsonLoader`.
pub struct CaveGeneratorJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CaveGenerator>`.
    pub base: JsonLoaderBase,
    /// Java `protected final Path caveFolder`.
    cave_folder: Option<PathBuf>,
    /// Java `protected final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    resource: Rc<SeedStringResource>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl CaveGeneratorJsonLoader {
    /// Java `CaveGeneratorJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, Path caveFolder, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        cave_folder: Option<PathBuf>,
        zone_context: Rc<ZoneFileContext>,
        resource: Rc<SeedStringResource>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CaveGenerator"), data_folder, json),
            cave_folder,
            zone_context,
            resource,
            registries,
        }
    }

    /// Java `CaveGenerator load()`.
    ///
    /// Returns `None` (the Java `null`) when the zone has no cave folder or
    /// no `Caves.json`.
    pub fn load(&self) -> Option<Rc<dyn CaveGenerator>> {
        let cave_folder = self.cave_folder.as_ref()?;

        let asset_path = cave_folder.join(constants::FILE_CAVES_JSON);
        if !asset_path.exists() {
            return None;
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let caves_json = crate::loader::load_json_object(&asset_path);
            self.load_cave_types(&caves_json)
        }));
        match result {
            Ok(cave_types) => Some(Rc::new(CaveGeneratorImpl::new(cave_types))),
            Err(_) => panic!(
                "{}",
                constants::ERROR_LOADING_CAVES.replace("%s", &asset_path.display().to_string())
            ),
        }
    }

    /// Java `CaveType[] loadCaveTypes(JsonObject jsonObject)`.
    fn load_cave_types(&self, json_object: &Value) -> Vec<Rc<crate::cave::cave_type::CaveType>> {
        let types = json_object.get(constants::KEY_TYPES).cloned();
        CaveTypesJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            types,
            self.cave_folder.clone().unwrap_or_default(),
            Rc::clone(&self.zone_context),
            Rc::clone(&self.resource),
            Rc::clone(&self.registries),
        )
        .load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;

    fn loader(cave_folder: Option<PathBuf>) -> CaveGeneratorJsonLoader {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        CaveGeneratorJsonLoader::new(
            SeedString::new("0"),
            &PathBuf::from("/tmp/world"),
            None,
            cave_folder,
            zone,
            Rc::new(SeedStringResource::new()),
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_appends_the_cave_generator_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CaveGenerator");
    }

    #[test]
    fn without_a_cave_folder_there_is_no_generator() {
        assert!(loader(None).load().is_none());
    }

    #[test]
    fn without_a_caves_json_there_is_no_generator() {
        let root = std::env::temp_dir().join(format!("hytale-cave-gen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(loader(Some(root.clone())).load().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
