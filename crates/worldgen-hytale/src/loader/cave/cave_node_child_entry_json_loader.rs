// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveNodeChildEntryJsonLoader.java`
//! Java `loader.cave.CaveNodeChildEntryJsonLoader` — one child entry of a
//! cave node type: the node types it may spawn, where they are attached,
//! how they are rotated and how likely they are.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::cave_node_type::{CaveNodeChildEntry, CaveNodeType, OrientationModifier};
use crate::cave::cave_yaw_mode::CaveYawMode;
use crate::loader::cave::cave_node_type_storage::CaveNodeTypeStorage;
use crate::loader::seed_string_resource::SeedStringResource;
use crate::loader::util::float_range::load_float_range_with_default;
use crate::loader::util::Vector3d;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::supplier::IFloatRange;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::supplier::IDoubleRange;

/// Java `CaveNodeChildEntryJsonLoader.Constants`.
pub mod constants {
    pub const KEY_NODE: &str = "Node";
    pub const KEY_SEED: &str = "Seed";
    pub const KEY_WEIGHTS: &str = "Weights";
    pub const KEY_ANCHOR: &str = "Anchor";
    pub const KEY_OFFSET: &str = "Offset";
    pub const KEY_ROTATION: &str = "Rotation";
    pub const KEY_CHILDREN_LIMIT: &str = "ChildrenLimit";
    pub const KEY_REPEAT: &str = "Repeat";
    pub const KEY_PITCH_ADD: &str = "PitchAdd";
    pub const KEY_PITCH_SET: &str = "PitchSet";
    pub const KEY_YAW_ADD: &str = "YawAdd";
    pub const KEY_YAW_SET: &str = "YawSet";
    pub const KEY_CHANCE: &str = "Chance";
    pub const KEY_YAW_MODE: &str = "YawMode";

    pub const ERROR_WEIGHTS_ARRAY: &str = "'Weights' must be an array if set";
    pub const ERROR_ENTRY_WEIGHT_SIZE: &str =
        "Weight array size is different from node name array.";
    pub const ERROR_NO_NODES: &str = "There are no valid nodes in this child entry!";
}

/// Java `loader.cave.CaveNodeChildEntryJsonLoader`.
pub struct CaveNodeChildEntryJsonLoader<'a> {
    /// Java `extends JsonLoader<SeedStringResource,
    /// CaveNodeType.CaveNodeChildEntry>`.
    pub base: JsonLoaderBase,
    /// Java `protected final CaveNodeTypeStorage storage`.
    storage: &'a CaveNodeTypeStorage,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    resource: Rc<SeedStringResource>,
}

impl<'a> CaveNodeChildEntryJsonLoader<'a> {
    /// Java `CaveNodeChildEntryJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, CaveNodeTypeStorage storage)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        storage: &'a CaveNodeTypeStorage,
        resource: Rc<SeedStringResource>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CaveNodeChildEntry"), data_folder, json),
            storage,
            resource,
        }
    }

    /// Java `CaveNodeType.CaveNodeChildEntry load()`.
    pub fn load(&self) -> CaveNodeChildEntry {
        // Java evaluates the constructor arguments left-to-right.
        let types = self.load_nodes();
        let anchor = self.load_anchor();
        let offset = self.load_offset();
        let rotations = self.load_rotations();
        let children_limit = self.load_children_limit();
        let repeat = self.load_repeat();
        let pitch_modifier = self.load_pitch_modifier();
        let yaw_modifier = self.load_yaw_modifier();
        let chance = self.load_chance();
        let yaw_mode = self.load_yaw_mode();

        CaveNodeChildEntry::new(
            types,
            anchor,
            offset,
            rotations,
            children_limit,
            repeat,
            pitch_modifier,
            yaw_modifier,
            chance,
            yaw_mode,
        )
    }

    /// Java `IWeightedMap<CaveNodeType> loadNodes()`.
    fn load_nodes(&self) -> Rc<dyn IWeightedMap<Rc<CaveNodeType>>> {
        let mut builder = WeightedMap::<Rc<CaveNodeType>>::builder();
        let node_element = self.base.get(constants::KEY_NODE).unwrap_or_else(|| {
            panic!(
                "Cannot invoke \"com.google.gson.JsonElement.isJsonArray()\" because \"nodeElement\" is null"
            )
        });

        match &node_element {
            Value::Array(node_array) => {
                let weights_array = if self.base.has(constants::KEY_WEIGHTS) {
                    let weights_element = self.base.get(constants::KEY_WEIGHTS).unwrap();
                    let weights_array = weights_element
                        .as_array()
                        .cloned()
                        .unwrap_or_else(|| panic!("{}", constants::ERROR_WEIGHTS_ARRAY));
                    if weights_array.len() != node_array.len() {
                        panic!("{}", constants::ERROR_ENTRY_WEIGHT_SIZE);
                    }
                    Some(weights_array)
                } else {
                    None
                };

                for (index, node) in node_array.iter().enumerate() {
                    let node_entry_element = self.base.get_or_load(node);
                    let cave_node_type = self.load_cave_node_type(index, &node_entry_element);
                    let weight = match &weights_array {
                        Some(weights_array) => JsonLoaderBase::get_as_double(&weights_array[index]),
                        None => 1.0,
                    };
                    builder.put(cave_node_type, weight);
                }
            }
            Value::Object(_) | Value::String(_) | Value::Number(_) | Value::Bool(_) => {
                let cave_node_type = self.load_cave_node_type(0, &node_element);
                builder.put(cave_node_type, 1.0);
            }
            Value::Null => {}
        }

        if builder.size() == 0 {
            panic!("{}", constants::ERROR_NO_NODES);
        }
        Rc::new(builder.build())
    }

    /// Java `CaveNodeType loadCaveNodeType(int index, JsonElement element)`.
    fn load_cave_node_type(&self, index: usize, element: &Value) -> Rc<CaveNodeType> {
        match element {
            Value::Object(object) => {
                // An inline node type: it gets a unique name.
                let cave_node_type_name = self.resource.get_unique_name("ChildCaveType#");
                self.storage.load_cave_node_type_with_json(
                    &cave_node_type_name,
                    Value::Object(object.clone()),
                )
            }
            Value::String(name) => self.storage.get_or_load_cave_node_type(name),
            _ => panic!("Invalid cave node type entry: {index}"),
        }
    }

    /// Java `Vector3d loadAnchor()`.
    fn load_anchor(&self) -> Vector3d {
        if !self.base.has(constants::KEY_ANCHOR) {
            return Vector3d::ZERO;
        }
        self.load_vector(&self.base.get(constants::KEY_ANCHOR).unwrap())
    }

    /// Java `Vector3d loadOffset()`.
    fn load_offset(&self) -> Vector3d {
        if !self.base.has(constants::KEY_OFFSET) {
            return Vector3d::ZERO;
        }
        self.load_vector(&self.base.get(constants::KEY_OFFSET).unwrap())
    }

    /// Java `Vector3d loadVector(Vector3d vector, JsonElement jsonElement)`.
    fn load_vector(&self, element: &Value) -> Vector3d {
        let array = element
            .as_array()
            .cloned()
            .unwrap_or_else(|| panic!("Not a JSON Array: {element}"));
        Vector3d::new(
            JsonLoaderBase::get_as_double(array.first().unwrap_or(&Value::Null)),
            JsonLoaderBase::get_as_double(array.get(1).unwrap_or(&Value::Null)),
            JsonLoaderBase::get_as_double(array.get(2).unwrap_or(&Value::Null)),
        )
    }

    /// Java `PrefabRotation[] loadRotations()`.
    fn load_rotations(&self) -> Vec<PrefabRotation> {
        if !self.base.has(constants::KEY_ROTATION) {
            return vec![PrefabRotation::Rotation0];
        }
        let rotation_element = self.base.get(constants::KEY_ROTATION).unwrap();
        match &rotation_element {
            Value::Array(rotation_array) => rotation_array
                .iter()
                .map(|element| Self::value_of_extended(&JsonLoaderBase::get_as_string(element)))
                .collect(),
            other => vec![Self::value_of_extended(&JsonLoaderBase::get_as_string(
                other,
            ))],
        }
    }

    /// Java `PrefabRotation.valueOfExtended(String)`.
    fn value_of_extended(name: &str) -> PrefabRotation {
        PrefabRotation::value_of_extended(name).unwrap_or_else(|| {
            panic!("No enum constant com.hypixel.hytale.server.core.prefab.PrefabRotation.{name}")
        })
    }

    /// Java `IDoubleRange loadChildrenLimit()`.
    fn load_children_limit(&self) -> Option<Rc<dyn IDoubleRange>> {
        if !self.base.has(constants::KEY_CHILDREN_LIMIT) {
            return None;
        }
        Some(Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_CHILDREN_LIMIT),
                0.0,
            )
            .load(),
        ))
    }

    /// Java `IDoubleRange loadRepeat()`.
    fn load_repeat(&self) -> Rc<dyn IDoubleRange> {
        if !self.base.has(constants::KEY_REPEAT) {
            // Java `DoubleRange.ONE`.
            return Rc::new(procedural::noise::supplier::DoubleRange::constant(1.0));
        }
        Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_REPEAT),
                0.0,
            )
            .load(),
        )
    }

    /// Java `OrientationModifier loadYawModifier()`.
    fn load_yaw_modifier(&self) -> OrientationModifier {
        if let Some(yaw_add) = self.load_yaw_add() {
            return Rc::new(
                move |current: f32, random: &mut hytale_math::fast_random::FastRandom| {
                    current + yaw_add.get_value_random(random)
                },
            );
        }
        if let Some(yaw_set) = self.load_yaw_set() {
            return Rc::new(
                move |_current: f32, random: &mut hytale_math::fast_random::FastRandom| {
                    yaw_set.get_value_random(random)
                },
            );
        }
        Rc::new(|current: f32, _random: &mut hytale_math::fast_random::FastRandom| current)
    }

    /// Java `OrientationModifier loadPitchModifier()`.
    fn load_pitch_modifier(&self) -> OrientationModifier {
        if let Some(pitch_add) = self.load_pitch_add() {
            return Rc::new(
                move |current: f32, random: &mut hytale_math::fast_random::FastRandom| {
                    current + pitch_add.get_value_random(random)
                },
            );
        }
        if let Some(pitch_set) = self.load_pitch_set() {
            return Rc::new(
                move |_current: f32, random: &mut hytale_math::fast_random::FastRandom| {
                    pitch_set.get_value_random(random)
                },
            );
        }
        Rc::new(|current: f32, _random: &mut hytale_math::fast_random::FastRandom| current)
    }

    /// Java `IFloatRange loadYawAdd()`.
    fn load_yaw_add(&self) -> Option<Rc<dyn IFloatRange>> {
        if !self.base.has(constants::KEY_YAW_ADD) {
            return None;
        }
        Some(load_float_range_with_default(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_YAW_ADD),
            0.0,
            degrees_to_radians,
        ))
    }

    /// Java `IFloatRange loadPitchAdd()`.
    fn load_pitch_add(&self) -> Option<Rc<dyn IFloatRange>> {
        if !self.base.has(constants::KEY_PITCH_ADD) {
            return None;
        }
        Some(load_float_range_with_default(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_PITCH_ADD),
            0.0,
            degrees_to_radians,
        ))
    }

    /// Java `IFloatRange loadYawSet()`.
    fn load_yaw_set(&self) -> Option<Rc<dyn IFloatRange>> {
        if !self.base.has(constants::KEY_YAW_SET) {
            return None;
        }
        Some(load_float_range_with_default(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_YAW_SET),
            0.0,
            degrees_to_radians,
        ))
    }

    /// Java `IFloatRange loadPitchSet()`.
    fn load_pitch_set(&self) -> Option<Rc<dyn IFloatRange>> {
        if !self.base.has(constants::KEY_PITCH_SET) {
            return None;
        }
        Some(load_float_range_with_default(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_PITCH_SET),
            0.0,
            degrees_to_radians,
        ))
    }

    /// Java `double loadChance()`.
    fn load_chance(&self) -> f64 {
        if !self.base.has(constants::KEY_CHANCE) {
            return 1.0;
        }
        JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_CHANCE).unwrap())
    }

    /// Java `CaveYawMode loadYawMode()`.
    fn load_yaw_mode(&self) -> CaveYawMode {
        if !self.base.has(constants::KEY_YAW_MODE) {
            return CaveYawMode::Node;
        }
        let name = JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_YAW_MODE).unwrap());
        match name.as_str() {
            "NODE" => CaveYawMode::Node,
            "SUM" => CaveYawMode::Sum,
            "PREFAB" => CaveYawMode::Prefab,
            other => panic!(
                "No enum constant com.hypixel.hytale.server.worldgen.cave.CaveYawMode.{other}"
            ),
        }
    }
}

/// Java `deg -> deg * TrigMathUtil.degToRad`.
fn degrees_to_radians(degrees: f32) -> f32 {
    degrees * hytale_math::trig_math_util::DEG_TO_RAD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;
    use crate::loader::util::path_util::resolve_path_within_dir;
    use worldgen_model::registry::Registries;

    /// A storage rooted at a temporary cave folder.
    fn storage(root: &Path) -> CaveNodeTypeStorage {
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

    fn loader<'a>(
        storage: &'a CaveNodeTypeStorage,
        json: Option<Value>,
    ) -> CaveNodeChildEntryJsonLoader<'a> {
        CaveNodeChildEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            storage,
            Rc::new(SeedStringResource::new()),
        )
    }

    #[test]
    fn the_seed_appends_the_child_entry_suffix() {
        let storage = storage(Path::new("/tmp/world"));
        let loader = loader(&storage, None);
        assert_eq!(loader.base.seed.seed(), "0.CaveNodeChildEntry");
    }

    #[test]
    fn a_missing_node_key_panics_like_the_java_npe() {
        let storage = storage(Path::new("/tmp/world"));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(&storage, None).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert!(
            message.contains("because \"nodeElement\" is null"),
            "{message}"
        );
    }

    #[test]
    fn the_defaults_are_the_java_ones() {
        let storage = storage(Path::new("/tmp/world"));
        // A node that does not exist -> the storage fails to load it.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(&storage, Some(serde_json::json!({"Node": "Missing"}))).load()
        }));
        assert!(result.is_err());
    }

    #[test]
    fn the_weights_must_have_the_same_size_as_the_nodes() {
        let storage = storage(Path::new("/tmp/world"));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(
                &storage,
                Some(serde_json::json!({"Node": ["A", "B"], "Weights": [1.0]})),
            )
            .load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Weight array size is different from node name array."
        );
    }

    #[test]
    fn the_weights_must_be_an_array() {
        let storage = storage(Path::new("/tmp/world"));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(
                &storage,
                Some(serde_json::json!({"Node": ["A"], "Weights": 1.0})),
            )
            .load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "'Weights' must be an array if set");
    }

    #[test]
    fn the_path_traversal_guard_rejects_a_escaping_node_name() {
        let root = std::env::temp_dir().join(format!("hytale-cave-storage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Zones/Oceans/Cave")).unwrap();
        let storage = storage(&root);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            storage.load_cave_node_type("../../escape")
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "Invalid cave node type name: ../../escape");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_node_type_inside_the_cave_folder_is_resolved() {
        let root =
            std::env::temp_dir().join(format!("hytale-cave-storage-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Zones/Oceans/Cave/Type_0")).unwrap();
        std::fs::write(
            root.join("Zones/Oceans/Cave/Type_0/Entry.node.json"),
            r#"{"Type": "EMPTY_LINE", "Length": [0]}"#,
        )
        .unwrap();
        let storage = storage(&root);

        let node_type = storage.load_cave_node_type("Type_0.Entry");
        assert_eq!(node_type.get_name(), "Type_0.Entry");
        // The file was resolved below the cave folder.
        assert!(
            resolve_path_within_dir(&root.join("Zones/Oceans/Cave"), "Type_0/Entry.node.json")
                .is_some()
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
