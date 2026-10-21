// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveNodeTypeStorage.java`
//! Java `loader.cave.CaveNodeTypeStorage` — the name → node type registry
//! of one cave type.
//!
//! The Java storage is created *per cave type* (the
//! `CaveTypeJsonLoader.loadEntryNodeType` method builds a fresh one), so
//! every cave type has its own node-type namespace and the "already added"
//! error can only fire inside one node-type graph.
//!
//! The storage resolves a node type name to the file
//! `<cave folder>/<name with dots as separators>.node.json` and rejects a
//! name that would escape the cave folder (the Java
//! `PathUtil.resolvePathWithinDir` guard).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::cave::cave_node_type::CaveNodeType;
use crate::loader::cave::cave_node_type_json_loader::CaveNodeTypeJsonLoader;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::seed_string_resource::SeedStringResource;
use crate::loader::util::path_util::resolve_path_within_dir;
use procedural::json::SeedString;

/// Java `CaveNodeTypeStorage.Constants`.
pub mod constants {
    pub const ERROR_ALREADY_ADDED: &str =
        "CaveNodeType (%s) has already been added to CaveNodeTypeStorage!";
    pub const ERROR_LOADING_CAVE_NODE_TYPE: &str =
        "Error while loading CaveNodeType %s for world generator from %s";

    pub const FILE_SUFFIX: &str = "%s.node.json";
}

/// Java `loader.cave.CaveNodeTypeStorage`.
pub struct CaveNodeTypeStorage {
    /// Java `protected final SeedString<SeedStringResource> seed`.
    seed: SeedString,
    /// Java `protected final Path dataFolder`.
    data_folder: PathBuf,
    /// Java `protected final Path caveFolder`.
    cave_folder: PathBuf,
    /// Java `protected final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
    /// Java `@Nonnull protected final Map<String, CaveNodeType>
    /// caveNodeTypes`.
    cave_node_types: RefCell<HashMap<String, Rc<CaveNodeType>>>,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    pub resource: Rc<SeedStringResource>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl CaveNodeTypeStorage {
    /// Java `CaveNodeTypeStorage(SeedString seed, Path dataFolder, Path
    /// caveFolder, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        cave_folder: PathBuf,
        zone_context: Rc<ZoneFileContext>,
        resource: Rc<SeedStringResource>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            seed,
            data_folder: data_folder.to_path_buf(),
            cave_folder,
            zone_context,
            cave_node_types: RefCell::new(HashMap::new()),
            resource,
            registries,
        }
    }

    /// Java `SeedString<SeedStringResource> getSeed()`.
    pub fn get_seed(&self) -> &SeedString {
        &self.seed
    }

    /// Java `void add(String name, CaveNodeType caveNodeType)`.
    ///
    /// Panics with the Java `Error` message when the name is already
    /// registered.
    pub fn add(&self, name: &str, cave_node_type: Rc<CaveNodeType>) {
        let mut cave_node_types = self.cave_node_types.borrow_mut();
        if cave_node_types.contains_key(name) {
            panic!("{}", constants::ERROR_ALREADY_ADDED.replace("%s", name));
        }
        cave_node_types.insert(name.to_string(), cave_node_type);
    }

    /// Java `CaveNodeType getCaveNodeType(String name)` — `null` when the
    /// name is unknown.
    pub fn get_cave_node_type(&self, name: &str) -> Option<Rc<CaveNodeType>> {
        self.cave_node_types.borrow().get(name).cloned()
    }

    /// Java `CaveNodeType getOrLoadCaveNodeType(String name)`.
    pub fn get_or_load_cave_node_type(&self, name: &str) -> Rc<CaveNodeType> {
        match self.get_cave_node_type(name) {
            Some(cave_node_type) => cave_node_type,
            None => self.load_cave_node_type(name),
        }
    }

    /// Java `CaveNodeType loadCaveNodeType(String name)`.
    ///
    /// The name is resolved to `<cave folder>/<name>.node.json`; a name that
    /// escapes the cave folder is rejected with the Java error.
    pub fn load_cave_node_type(&self, name: &str) -> Rc<CaveNodeType> {
        let relative_path =
            constants::FILE_SUFFIX.replace("%s", &name.replace('.', std::path::MAIN_SEPARATOR_STR));
        let Some(file) = resolve_path_within_dir(&self.cave_folder, &relative_path) else {
            panic!("Invalid cave node type name: {name}");
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let json = crate::loader::load_json_object(&file);
            self.load_cave_node_type_with_json(name, json)
        }));
        match result {
            Ok(cave_node_type) => cave_node_type,
            Err(_) => panic!(
                "{}",
                constants::ERROR_LOADING_CAVE_NODE_TYPE
                    .replace("%s", name)
                    .replacen("%s", &file.display().to_string(), 1)
            ),
        }
    }

    /// Java `CaveNodeType loadCaveNodeType(String name, JsonObject json)`.
    pub fn load_cave_node_type_with_json(&self, name: &str, json: Value) -> Rc<CaveNodeType> {
        CaveNodeTypeJsonLoader::new(
            self.seed.clone(),
            &self.data_folder,
            Some(json),
            name,
            self,
            Rc::clone(&self.zone_context),
            Rc::clone(&self.resource),
        )
        .load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;

    /// A storage rooted at a temporary cave folder.
    fn storage(root: &std::path::Path) -> CaveNodeTypeStorage {
        let loading = FileLoadingContext::new("test", root.to_path_buf());
        let zone = loading.create_zone("Oceans", root.join("Zones/Oceans"));
        CaveNodeTypeStorage::new(
            SeedString::new("0"),
            root,
            root.join("Zones/Oceans/Cave"),
            zone,
            Rc::new(SeedStringResource::new()),
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn an_unknown_name_is_not_registered() {
        let storage = storage(std::path::Path::new("/tmp/world"));
        assert!(storage.get_cave_node_type("Missing").is_none());
    }

    #[test]
    fn a_name_that_escapes_the_cave_folder_is_rejected() {
        let storage = storage(std::path::Path::new("/tmp/world"));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            storage.load_cave_node_type("../../etc/passwd")
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "Invalid cave node type name: ../../etc/passwd");
    }

    #[test]
    fn an_absolute_name_is_rejected() {
        let storage = storage(std::path::Path::new("/tmp/world"));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            storage.load_cave_node_type("/etc/passwd")
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "Invalid cave node type name: /etc/passwd");
    }
}
