// Ported from: `.hytale-source/.../worldgen/loader/cave/shape/PrefabCaveNodeShapeGeneratorJsonLoader.java`
//! Java `loader.cave.shape.PrefabCaveNodeShapeGeneratorJsonLoader` — the
//! shape of a cave node that *is* a prefab (a dungeon).
//!
//! The prefab names are resolved through the prefab store, so a name that
//! matches several prefab files contributes one supplier per file.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier;
use crate::cave::shape::prefab_cave_node_shape::PrefabCaveNodeShapeGenerator;
use crate::loader::context::file_loading_context::FileLoadingContext;
use crate::loader::prefab::block_placement_mask_json_loader::BlockPlacementMaskJsonLoader;
use crate::util::condition::BlockMaskCondition;
use procedural::json::{JsonLoaderBase, SeedString};
use worldgen_model::registry::Registries;

/// Java `PrefabCaveNodeShapeGeneratorJsonLoader.Constants`.
pub mod constants {
    pub const KEY_PREFAB: &str = "Prefab";
    pub const KEY_MASK: &str = "Mask";

    /// Java `new IllegalArgumentException("Prefabs are empty! Key: " +
    /// KEY_PREFAB)`.
    pub const ERROR_NO_PREFABS: &str = "Prefabs are empty! Key: Prefab";
}

/// Java `loader.cave.shape.PrefabCaveNodeShapeGeneratorJsonLoader`.
pub struct PrefabCaveNodeShapeGeneratorJsonLoader {
    /// Java `extends CaveNodeShapeGeneratorJsonLoader`.
    pub base: JsonLoaderBase,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
    /// The root loading context (it holds the prefab loader).
    pub context: Rc<FileLoadingContext>,
}

impl PrefabCaveNodeShapeGeneratorJsonLoader {
    /// Java `PrefabCaveNodeShapeGeneratorJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
        context: Rc<FileLoadingContext>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(".PrefabCaveNodeShapeGenerator"),
                data_folder,
                json,
            ),
            registries,
            context,
        }
    }

    /// Java `PrefabCaveNodeShape.PrefabCaveNodeShapeGenerator load()`.
    pub fn load(&self) -> PrefabCaveNodeShapeGenerator {
        PrefabCaveNodeShapeGenerator::new(self.load_prefabs(), self.load_mask())
    }

    /// Java `List<WorldGenPrefabSupplier> loadPrefabs()`.
    fn load_prefabs(&self) -> Vec<Rc<dyn CavePrefabSupplier>> {
        let loader = self.context.get_prefab_loader().unwrap_or_else(|| {
            panic!(
                "// SEAM(prefab-loader): the prefab loader is not configured (the world generator was \
                 not loaded through the loader chain)"
            )
        });
        let prefab_element = self.base.get(constants::KEY_PREFAB).unwrap_or_else(|| {
            panic!(
                "Cannot invoke \"com.google.gson.JsonElement.isJsonArray()\" because \"prefabElement\" is null"
            )
        });

        let mut prefabs: Vec<Rc<dyn CavePrefabSupplier>> = Vec::new();
        match &prefab_element {
            Value::Array(array) => {
                for element in array {
                    let name = JsonLoaderBase::get_as_string(element);
                    for supplier in loader.get(&name) {
                        prefabs.push(Rc::new(supplier) as Rc<dyn CavePrefabSupplier>);
                    }
                }
            }
            other => {
                let name = JsonLoaderBase::get_as_string(other);
                for supplier in loader.get(&name) {
                    prefabs.push(Rc::new(supplier) as Rc<dyn CavePrefabSupplier>);
                }
            }
        }

        if prefabs.is_empty() {
            panic!("{}", constants::ERROR_NO_PREFABS);
        }
        prefabs
    }

    /// Java `BlockMaskCondition loadMask()`.
    fn load_mask(&self) -> BlockMaskCondition {
        if !self.base.has(constants::KEY_MASK) {
            return BlockMaskCondition::new();
        }
        BlockPlacementMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get_raw(constants::KEY_MASK),
            Rc::clone(&self.registries),
        )
        .load()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Option<Value>) -> PrefabCaveNodeShapeGeneratorJsonLoader {
        PrefabCaveNodeShapeGeneratorJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            Rc::new(Registries::empty()),
            {
                let context = FileLoadingContext::new("test", std::path::PathBuf::from("."));
                context.set_prefab_loader(crate::loader::test_prefab_store::test_prefab_loader());
                context
            },
        )
    }

    #[test]
    fn the_seed_appends_the_prefab_shape_generator_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.PrefabCaveNodeShapeGenerator");
    }

    #[test]
    fn a_missing_prefab_key_panics_like_the_java_npe() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(None).load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert!(
            message.contains("because \"prefabElement\" is null"),
            "{message}"
        );
    }

    #[test]
    fn without_a_prefab_loader_the_names_cannot_be_resolved() {
        let loader = PrefabCaveNodeShapeGeneratorJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Prefab": ["A"]})),
            Rc::new(Registries::empty()),
            FileLoadingContext::new("test", std::path::PathBuf::from(".")),
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader.load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert!(
            message.contains("the prefab loader is not configured"),
            "{message}"
        );
    }
}
