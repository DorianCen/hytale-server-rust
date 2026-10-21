// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveTypesJsonLoader.java`
//! Java `loader.cave.CaveTypesJsonLoader` — the list of cave types of a
//! zone.
//!
//! The entries of the list are either inline cave types or `{"File": "Some
//! .Path"}` references to a cave type file below the world folder.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;

use crate::cave::cave_type::CaveType;
use crate::loader::cave::cave_type_json_loader::CaveTypeJsonLoader;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::seed_string_resource::SeedStringResource;
use procedural::json::{JsonLoaderBase, SeedString};
use worldgen_model::registry::Registries;

/// Java `CaveTypesJsonLoader.Constants`.
pub mod constants {
    pub const KEY_NAME: &str = "Name";

    pub const SEED_CAVE_TYPE_SUFFIX: &str = "-%s";

    pub const ERROR_NOT_AN_ARRAY: &str = "CaveTypes must be a JSON array.";
}

/// Java `loader.cave.CaveTypesJsonLoader`.
pub struct CaveTypesJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CaveType[]>` — note:
    /// the Java constructor does **not** append a seed suffix.
    pub base: JsonLoaderBase,
    /// Java `protected final Path caveFolder`.
    cave_folder: PathBuf,
    /// Java `protected final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    resource: Rc<SeedStringResource>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl CaveTypesJsonLoader {
    /// Java `CaveTypesJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, Path caveFolder, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        cave_folder: PathBuf,
        zone_context: Rc<ZoneFileContext>,
        resource: Rc<SeedStringResource>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            cave_folder,
            zone_context,
            resource,
            registries,
        }
    }

    /// Java `CaveType[] load()`.
    pub fn load(&self) -> Vec<Rc<CaveType>> {
        let Some(json) = self.base.json.as_ref() else {
            panic!("{}", constants::ERROR_NOT_AN_ARRAY);
        };
        if !json.is_array() {
            panic!("{}", constants::ERROR_NOT_AN_ARRAY);
        }
        let types_array = json.as_array().unwrap();

        let mut entries = Vec::with_capacity(types_array.len());
        for (index, element) in types_array.iter().enumerate() {
            let entry = self.base.get_or_load(element);
            if !entry.is_object() {
                panic!("Expected CaveType entry to be a JsonObject at index: {index}");
            }
            entries.push(self.load_cave_type(&entry));
        }

        // The Java loader dispatches the `ModifyEvents.CaveTypes` event here;
        // the modifier / event plugin system is deferred (see `REMAINING.md`
        // §1), so the port skips the dispatch.
        entries
    }

    /// Java `CaveType loadCaveType(JsonObject json)`.
    fn load_cave_type(&self, json: &Value) -> Rc<CaveType> {
        let object = json.as_object().unwrap();
        let Some(name) = object.get(constants::KEY_NAME) else {
            panic!("CaveType is missing the 'Name' property");
        };
        if !name.is_string() {
            panic!("CaveType 'Name' property is not a string");
        }
        let name = name.as_str().unwrap();

        CaveTypeJsonLoader::new(
            self.base
                .seed
                .append(&constants::SEED_CAVE_TYPE_SUFFIX.replace("%s", name)),
            &self.base.data_folder,
            Some(json.clone()),
            self.cave_folder.clone(),
            name,
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

    fn loader(json: Option<Value>) -> CaveTypesJsonLoader {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        CaveTypesJsonLoader::new(
            SeedString::new("0"),
            &PathBuf::from("/tmp/world"),
            json,
            PathBuf::from("/tmp/world/Zones/Oceans/Cave"),
            zone,
            Rc::new(SeedStringResource::new()),
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_is_not_changed() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0");
    }

    #[test]
    fn a_missing_array_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(None).load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "CaveTypes must be a JSON array.");
    }

    #[test]
    fn a_non_object_entry_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!(["Caldera"]))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Expected CaveType entry to be a JsonObject at index: 0"
        );
    }

    #[test]
    fn a_missing_name_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!([{}]))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "CaveType is missing the 'Name' property");
    }

    #[test]
    fn a_non_string_name_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!([{"Name": 42}]))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "CaveType 'Name' property is not a string");
    }
}
