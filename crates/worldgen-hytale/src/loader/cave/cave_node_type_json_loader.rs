// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveNodeTypeJsonLoader.java`
//! Java `loader.cave.CaveNodeTypeJsonLoader` — the loader of one cave node
//! type (the shape it grows into, the children it spawns, the covers it
//! places and the filling it fills its volume with).
//!
//! # The Java → Rust shape of the node-type graph
//!
//! The Java loader creates the node type, registers it in the storage and
//! *then* loads the children:
//!
//! ```java
//! final CaveNodeType caveNodeType = new CaveNodeType(…);
//! storage.add(name, caveNodeType);
//! caveNodeType.setChildren(loadChildren());
//! ```
//!
//! The children may reference the node type itself, so the node type has to
//! exist before they are loaded — and it has to be mutable afterwards. The
//! Rust port therefore hands the children to the shared node type through a
//! [`RefCell`](std::cell::RefCell) (see [`CaveNodeType::set_children`]).
//!
//! # Seams
//!
//! * The `ModifyEvents.CaveCovers` dispatch (the modifier / event plugin
//!   system) is **deferred** — see `REMAINING.md` §1. The port loads the
//!   covers of the JSON file and skips the event hook.
//! * The prefab containers of a node type are parsed (the names, the
//!   rotations, the conditions); the prefab *files* are Phase 5.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::block_key::BlockEntry;
use worldgen_model::registry::Registries;

use crate::cave::cave_node_type::{CaveNodeCoverEntry, CaveNodeType};
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::loader::cave::cave_node_child_entry_json_loader::CaveNodeChildEntryJsonLoader;
use crate::loader::cave::cave_node_cover_entry_json_loader::CaveNodeCoverEntryJsonLoader;
use crate::loader::cave::cave_node_type_storage::CaveNodeTypeStorage;
use crate::loader::cave::cave_prefab_container_json_loader::CavePrefabContainerJsonLoader;
use crate::loader::context::cave_file_context::CaveFileContext;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::seed_string_resource::SeedStringResource;
use crate::loader::util::resolved_block_array_json_loader::ResolvedBlockArrayJsonLoader;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::{
    DefaultCoordinateCondition, HeightThresholdCoordinateCondition, ICoordinateCondition,
};
use procedural::noise::supplier::IDoubleRange;

/// Java `CaveNodeTypeJsonLoader.Constants`.
pub mod constants {
    pub const KEY_CHILDREN: &str = "Children";
    pub const KEY_PREFABS: &str = "Prefabs";
    pub const KEY_FILLING: &str = "Filling";
    pub const KEY_FILLING_TYPES: &str = "Types";
    pub const KEY_FILLING_WEIGHT: &str = "Weight";
    pub const KEY_TYPE: &str = "Type";
    pub const KEY_HEIGHT_THRESHOLDS: &str = "HeightThreshold";
    pub const KEY_CHILD_COUNT_BOUNDS: &str = "ChildCountBounds";
    pub const KEY_COVER: &str = "Cover";
    pub const KEY_PRIORITY: &str = "Priority";
    pub const KEY_ENVIRONMENT: &str = "Environment";

    pub const SEED_COVER_SUFFIX: &str = "-cover#%s";
    pub const SEED_CHILD_ENTRY_SUFFIX: &str = ".Child-%s";

    pub const ERROR_UNKNOWN_SHAPE_NAME: &str = "Could not find Shape by the name %s in Json: %s";
    pub const ERROR_UNKOWN_CONSTRUCTOR_NODE_SHAPE: &str =
        "Could not find Constructor for %s CaveNodeShape";
    pub const ERROR_LOADING_ENVIRONMENT: &str = "Error while looking up environment \"%s\"!";
}

/// Java `loader.cave.CaveNodeTypeJsonLoader`.
pub struct CaveNodeTypeJsonLoader<'a> {
    /// Java `extends JsonLoader<SeedStringResource, CaveNodeType>`.
    pub base: JsonLoaderBase,
    /// Java `protected final String name`.
    name: String,
    /// Java `protected final CaveNodeTypeStorage storage`.
    storage: &'a CaveNodeTypeStorage,
    /// Java `protected final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
    /// Java `protected final CaveFileContext caveContext`.
    cave_context: Rc<CaveFileContext>,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    resource: Rc<SeedStringResource>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl<'a> CaveNodeTypeJsonLoader<'a> {
    /// Java `CaveNodeTypeJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, String name, CaveNodeTypeStorage storage,
    /// ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        name: &str,
        storage: &'a CaveNodeTypeStorage,
        zone_context: Rc<ZoneFileContext>,
        resource: Rc<SeedStringResource>,
    ) -> Self {
        let seed = seed.append(&format!(".CaveNodeType-{name}"));
        let cave_context = CaveFileContext::new(name, Rc::clone(&zone_context));
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            name: name.to_string(),
            storage,
            zone_context,
            cave_context,
            resource,
            registries: Rc::clone(&storage.registries),
        }
    }

    /// Java `CaveNodeType load()`.
    pub fn load(&self) -> Rc<CaveNodeType> {
        // Java evaluates the constructor arguments left-to-right.
        let prefab_container = self.load_prefabs();
        let fillings = self.load_fillings();
        let shape_generator = self.load_shape_generator();
        let height_condition = self.load_height_condition();
        let children_count_bounds = self.load_child_count_bounds();
        let covers = self.load_covers();
        let priority = self.load_priority();
        let environment = self.load_environment();

        let cave_node_type = Rc::new(CaveNodeType::new(
            self.name.clone(),
            prefab_container,
            fillings,
            shape_generator,
            height_condition,
            children_count_bounds,
            covers,
            priority,
            environment,
        ));

        self.storage.add(&self.name, Rc::clone(&cave_node_type));
        cave_node_type.set_children(self.load_children());
        cave_node_type
    }

    /// Java `CaveNodeType.CaveNodeChildEntry[] loadChildren()`.
    fn load_children(&self) -> Vec<crate::cave::cave_node_type::CaveNodeChildEntry> {
        let Some(children_element) = self.base.get(constants::KEY_CHILDREN) else {
            return Vec::new();
        };

        match &children_element {
            Value::Array(children_array) => {
                let mut entries = Vec::with_capacity(children_array.len());
                for (index, child) in children_array.iter().enumerate() {
                    entries.push(
                        CaveNodeChildEntryJsonLoader::new(
                            self.base.seed.append(&format!(".Child-{index}")),
                            &self.base.data_folder,
                            Some(self.base.get_or_load(child)),
                            self.storage,
                            Rc::clone(&self.resource),
                        )
                        .load(),
                    );
                }
                entries
            }
            _ => vec![CaveNodeChildEntryJsonLoader::new(
                self.base.seed.append(".Child-0"),
                &self.base.data_folder,
                Some(self.base.get_or_load(&children_element)),
                self.storage,
                Rc::clone(&self.resource),
            )
            .load()],
        }
    }

    /// Java `CavePrefabContainer loadPrefabs()`.
    fn load_prefabs(
        &self,
    ) -> Option<Rc<crate::cave::prefab::cave_prefab_container::CavePrefabContainer>> {
        if !self.base.has(constants::KEY_PREFABS) {
            return None;
        }
        // The prefab definition may redirect to a file in a different zone
        // folder, so the parent context may change.
        let context = self
            .zone_context
            .match_context_json(self.base.json.as_ref(), constants::KEY_PREFABS);
        let cave_context = CaveFileContext::with_path(
            &self.name,
            self.cave_context.base.get_path().to_path_buf(),
            context,
        );
        Some(Rc::new(
            CavePrefabContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_PREFABS),
                cave_context,
                Rc::clone(&self.registries),
            )
            .load(),
        ))
    }

    /// Java `IWeightedMap<BlockFluidEntry> loadFillings()`.
    fn load_fillings(&self) -> Rc<dyn IWeightedMap<BlockFluidEntry>> {
        let mut builder = WeightedMap::<BlockFluidEntry>::builder();
        let filling_element = self.base.get(constants::KEY_FILLING);

        match filling_element {
            None | Some(Value::Null) => {
                builder.put(BlockFluidEntry::EMPTY, 1.0);
            }
            Some(Value::Object(filling_object)) => {
                let block_array = filling_object
                    .get(constants::KEY_FILLING_TYPES)
                    .and_then(|value| value.as_array().cloned())
                    .unwrap_or_else(|| {
                        panic!(
                            "Not a JSON Array: {}",
                            filling_object
                                .get(constants::KEY_FILLING_TYPES)
                                .unwrap_or(&Value::Null)
                        )
                    });
                let weight_array = filling_object
                    .get(constants::KEY_FILLING_WEIGHT)
                    .and_then(|value| value.as_array().cloned())
                    .unwrap_or_else(|| {
                        panic!(
                            "Not a JSON Array: {}",
                            filling_object
                                .get(constants::KEY_FILLING_WEIGHT)
                                .unwrap_or(&Value::Null)
                        )
                    });
                let blocks = ResolvedBlockArrayJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(Value::Array(block_array.clone())),
                    Rc::clone(&self.registries),
                )
                .load();

                for (index, block) in blocks.get_entries().iter().enumerate() {
                    let weight = JsonLoaderBase::get_as_double(
                        weight_array.get(index).unwrap_or(&Value::Null),
                    );
                    builder.put(*block, weight);
                }
            }
            Some(Value::Array(block_array)) => {
                let blocks = ResolvedBlockArrayJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    Some(Value::Array(block_array.clone())),
                    Rc::clone(&self.registries),
                )
                .load();
                for block in blocks.get_entries() {
                    builder.put(*block, 1.0);
                }
            }
            Some(other) => {
                // A plain block name (with the optional rotation metadata).
                let key = BlockEntry::decode(&JsonLoaderBase::get_as_string(&other));
                let index = self
                    .registries
                    .blocks
                    .index_of(&key.block_type_key)
                    .unwrap_or_else(|| panic!("Unknown key! {key}"));
                builder.put(BlockFluidEntry::new(index, key.rotation, 0), 1.0);
            }
        }

        Rc::new(builder.build())
    }

    /// Java `CaveNodeShapeEnum.CaveNodeShapeGenerator loadShapeGenerator()`.
    fn load_shape_generator(&self) -> Rc<dyn CaveNodeShapeGenerator> {
        if !self.base.has(constants::KEY_TYPE) {
            return Rc::new(
                super::shape::pipe_cave_node_shape_generator_json_loader::PipeCaveNodeShapeGeneratorJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                )
                .load(),
            );
        }

        let type_element = self.base.get(constants::KEY_TYPE).unwrap();
        let type_string = JsonLoaderBase::get_as_string(&type_element);
        let upper = type_string.to_uppercase();

        // Java wraps the whole switch in a `catch (Throwable)` and reports
        // every failure as "Could not find Shape by the name …" — the
        // original cause is the `Error`'s cause (the panic hook still prints
        // it).
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.load_shape_generator_of(&upper)
        }));
        let generator: Option<Rc<dyn CaveNodeShapeGenerator>> = match result {
            Ok(generator) => generator,
            Err(_) => panic!(
                "{}",
                constants::ERROR_UNKNOWN_SHAPE_NAME
                    .replace("%s", &type_string)
                    .replacen("%s", &type_element.to_string(), 1)
            ),
        };

        match generator {
            Some(generator) => generator,
            None => panic!(
                "{}",
                constants::ERROR_UNKNOWN_SHAPE_NAME
                    .replace("%s", &type_string)
                    .replacen("%s", &type_element.to_string(), 1)
            ),
        }
    }

    /// The Java `switch (CaveNodeShapeEnum.valueOf(typeString.toUpperCase()))`
    /// — the shape generator of the given (upper case) shape name.
    fn load_shape_generator_of(&self, upper: &str) -> Option<Rc<dyn CaveNodeShapeGenerator>> {
        match upper {
            "PIPE" => Some(Rc::new(
                super::shape::pipe_cave_node_shape_generator_json_loader::PipeCaveNodeShapeGeneratorJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                )
                .load(),
            )),
            "CYLINDER" => Some(Rc::new(
                super::shape::cylinder_cave_node_shape_generator_json_loader::CylinderCaveNodeShapeGeneratorJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                )
                .load(),
            )),
            "PREFAB" => Some(Rc::new(
                super::shape::prefab_cave_node_shape_generator_json_loader::PrefabCaveNodeShapeGeneratorJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                    Rc::clone(&self.registries),
                    self.zone_context.get_parent_context(),
                )
                .load(),
            )),
            "ELLIPSOID" => Some(Rc::new(
                super::shape::ellipsoid_cave_node_shape_generator_json_loader::EllipsoidCaveNodeShapeGeneratorJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                )
                .load(),
            )),
            "EMPTY_LINE" => Some(Rc::new(
                super::shape::empty_line_cave_node_shape_generator_json_loader::EmptyLineCaveNodeShapeGeneratorJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                )
                .load(),
            )),
            "DISTORTED" => Some(Rc::new(
                super::shape::distorted_cave_node_shape_generator_json_loader::DistortedCaveNodeShapeGeneratorJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                )
                .load(),
            )),
            _ => None,
        }
    }

    /// Java `ICoordinateCondition loadHeightCondition()`.
    fn load_height_condition(&self) -> Rc<dyn ICoordinateCondition> {
        if !self.base.has(constants::KEY_HEIGHT_THRESHOLDS) {
            return Rc::new(DefaultCoordinateCondition::True);
        }
        let interpreter = HeightThresholdInterpreterJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_HEIGHT_THRESHOLDS),
            #[allow(deprecated)] // deprecated in hytale-math but this is its exact Java value
            hytale_math::chunk_util::HEIGHT,
        )
        .load();
        Rc::new(HeightThresholdCoordinateCondition::new(interpreter))
    }

    /// Java `IDoubleRange loadChildCountBounds()`.
    fn load_child_count_bounds(&self) -> Option<Rc<dyn IDoubleRange>> {
        if !self.base.has(constants::KEY_CHILD_COUNT_BOUNDS) {
            return None;
        }
        Some(Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_CHILD_COUNT_BOUNDS),
                0.0,
            )
            .load(),
        ))
    }

    /// Java `CaveNodeType.CaveNodeCoverEntry[] loadCovers()`.
    fn load_covers(&self) -> Vec<CaveNodeCoverEntry> {
        let Some(cover_element) = self.base.get(constants::KEY_COVER) else {
            return Vec::new();
        };

        let covers = match &cover_element {
            Value::Array(cover_array) => {
                let mut covers = Vec::with_capacity(cover_array.len());
                for (index, cover) in cover_array.iter().enumerate() {
                    covers.push(
                        CaveNodeCoverEntryJsonLoader::new(
                            self.base.seed.append(
                                &constants::SEED_COVER_SUFFIX.replace("%s", &index.to_string()),
                            ),
                            &self.base.data_folder,
                            Some(cover.clone()),
                            Rc::clone(&self.registries),
                        )
                        .load(),
                    );
                }
                covers
            }
            _ => vec![CaveNodeCoverEntryJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                Some(cover_element.clone()),
                Rc::clone(&self.registries),
            )
            .load()],
        };

        // The Java loader dispatches the `ModifyEvents.CaveCovers` event
        // here; the modifier / event plugin system is deferred (see
        // `REMAINING.md` §1), so the port skips the dispatch.
        covers
    }

    /// Java `int loadPriority()`.
    fn load_priority(&self) -> i32 {
        if !self.base.has(constants::KEY_PRIORITY) {
            return 0;
        }
        JsonLoaderBase::get_as_int(&self.base.get(constants::KEY_PRIORITY).unwrap())
    }

    /// Java `int loadEnvironment()`.
    fn load_environment(&self) -> i32 {
        if !self.base.has(constants::KEY_ENVIRONMENT) {
            return worldgen_model::chunk::ENVIRONMENT_NOT_SET;
        }
        let environment_id =
            JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_ENVIRONMENT).unwrap());
        self.registries
            .environments
            .index_of(&environment_id)
            .unwrap_or_else(|| {
                panic!(
                    "{}",
                    constants::ERROR_LOADING_ENVIRONMENT.replace("%s", &environment_id)
                )
            })
    }
}
