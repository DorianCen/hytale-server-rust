// Ported from: `.hytale-source/.../worldgen/loader/ZonesJsonLoader.java`
//! Java `loader.ZonesJsonLoader` — loads every zone of a world's loading
//! context (in registry order) from its `Zone.json`.

use std::path::Path;
use std::rc::Rc;

use worldgen_model::registry::Registries;

use crate::loader::context::file_loading_context::FileLoadingContext;
use crate::loader::load_json_object;
use crate::loader::seed_string_resource::SeedStringResource;
use crate::loader::zone::zone_json_loader::ZoneJsonLoader;
use crate::zone::zone::Zone;
use procedural::json::SeedString;

/// Java `ZonesJsonLoader.Constants`.
pub mod constants {
    pub const PATH_ZONES: &str = "Zones";
    pub const FILE_ZONE_MAIN_FILE: &str = "Zone.json";
    pub const ERROR_LOADING_ZONE: &str =
        "Error while loading zone \"%s\" for world generator from file.";
}

/// Java `loader.ZonesJsonLoader`.
pub struct ZonesJsonLoader {
    /// Java `extends Loader<SeedStringResource, Zone[]>`.
    pub seed: SeedString,
    /// Java `protected final Path dataFolder`.
    pub data_folder: std::path::PathBuf,
    /// Java `protected final FileLoadingContext loadingContext`.
    pub loading_context: Rc<FileLoadingContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    pub resource: Rc<SeedStringResource>,
}

impl ZonesJsonLoader {
    /// Java `ZonesJsonLoader(SeedString<SeedStringResource> seed, Path dataFolder, FileLoadingContext loadingContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        loading_context: Rc<FileLoadingContext>,
        registries: Rc<Registries>,
        resource: Rc<SeedStringResource>,
    ) -> Self {
        Self {
            seed: seed.append(".Zones"),
            data_folder: data_folder.to_path_buf(),
            loading_context,
            registries,
            resource,
        }
    }

    /// Java `Zone[] load()`.
    pub fn load(&self) -> Vec<Rc<Zone>> {
        let zone_registry = self.loading_context.get_zones().borrow();
        let mut zones = Vec::with_capacity(zone_registry.size());

        for (_, zone_context) in zone_registry.iter() {
            let path = zone_context.base.get_path().to_path_buf();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let zone_json = load_json_object(&path.join(constants::FILE_ZONE_MAIN_FILE));
                ZoneJsonLoader::new(
                    self.seed.clone(),
                    &self.data_folder,
                    Some(zone_json),
                    Rc::clone(zone_context),
                    Rc::clone(&self.registries),
                    Rc::clone(&self.resource),
                )
                .load()
            }));
            match result {
                Ok(zone) => zones.push(zone),
                Err(_) => panic!(
                    "{}",
                    constants::ERROR_LOADING_ZONE.replacen("%s", &path.display().to_string(), 1)
                ),
            }
        }

        zones
    }
}
