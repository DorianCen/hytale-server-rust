// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveTypeJsonLoader.java`
//! Java `loader.cave.CaveTypeJsonLoader` — the loader of one cave type.
//!
//! A cave type is the *kind* of cave: where its entry nodes are spawned, how
//! deep they are, how they are oriented, which biomes they may grow in and
//! which blocks they may replace.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::cave::cave_node_type::CaveNodeType;
use crate::cave::cave_type::{CaveType, FluidLevel};
use crate::loader::cave::cave_biome_mask_json_loader::CaveBiomeMaskJsonLoader;
use crate::loader::cave::cave_node_type_storage::CaveNodeTypeStorage;
use crate::loader::cave::fluid_level_json_loader::FluidLevelJsonLoader;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::prefab::block_placement_mask_json_loader::BlockPlacementMaskJsonLoader;
use crate::loader::seed_string_resource::SeedStringResource;
use crate::util::condition::BlockMaskCondition;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::float_range_json_loader::FloatRangeJsonLoader;
use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::point_generator_json_loader::PointGeneratorJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::{
    DefaultCoordinateCondition, HeightThresholdCoordinateCondition, ICoordinateCondition,
};
use procedural::noise::point::IPointGenerator;
use procedural::noise::property::NoiseProperty;
use procedural::noise::supplier::{IDoubleRange, IFloatRange};

/// Java `CaveTypeJsonLoader.Constants`.
pub mod constants {
    pub const KEY_YAW: &str = "Yaw";
    pub const KEY_PITCH: &str = "Pitch";
    pub const KEY_DEPTH: &str = "Depth";
    pub const KEY_HEIGHT_RADIUS_FACTOR: &str = "HeightRadiusFactor";
    pub const KEY_ENTRY: &str = "Entry";
    pub const KEY_ENTRY_POINTS: &str = "EntryPoints";
    pub const KEY_HEIGHT_THRESHOLDS: &str = "HeightThreshold";
    pub const KEY_BIOME_MASK: &str = "BiomeMask";
    pub const KEY_BLOCK_MASK: &str = "BlockMask";
    pub const KEY_NOISE_MASK: &str = "NoiseMask";
    pub const KEY_FIXED_ENTRY_HEIGHT: &str = "FixedEntryHeight";
    pub const KEY_FIXED_ENTRY_HEIGHT_NOISE: &str = "FixedEntryHeightNoise";
    pub const KEY_FLUID_LEVEL: &str = "FluidLevel";
    pub const KEY_SURFACE_LIMITTED: &str = "SurfaceLimited";
    pub const KEY_SUBMERGE: &str = "Submerge";
    pub const KEY_MAXIMUM_SIZE: &str = "MaximumSize";
    pub const KEY_ENVIRONMENT: &str = "Environment";

    pub const DEFAULT_SUBMERGE: bool = false;

    pub const ERROR_NO_ENTRY: &str = "\"Entry\" is not defined. Define an entry node type";
    pub const ERROR_NO_ENTRY_POINTS: &str =
        "\"EntryPoints\" is not defined, no spawn information for caves available";
    pub const ERROR_LOADING_ENVIRONMENT: &str = "Error while looking up environment \"%s\"!";
}

/// Java `ZonesJsonLoader.Constants.PATH_ZONES` (the Java cave type loader
/// imports it).
const PATH_ZONES: &str = "Zones";
/// Java `ZoneJsonLoader.Constants.PATH_CAVE`.
const PATH_CAVE: &str = "Cave";

/// Java `loader.cave.CaveTypeJsonLoader`.
pub struct CaveTypeJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CaveType>`.
    pub base: JsonLoaderBase,
    /// Java `protected final Path caveFolder`.
    cave_folder: PathBuf,
    /// Java `protected final String name`.
    name: String,
    /// Java `protected final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
    /// The world-wide unique-name counters (Java `SeedStringResource`).
    resource: Rc<SeedStringResource>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl CaveTypeJsonLoader {
    /// Java `CaveTypeJsonLoader(SeedString seed, Path dataFolder, JsonElement
    /// json, Path caveFolder, String name, ZoneFileContext zoneContext)`.
    #[allow(clippy::too_many_arguments)] // The Java constructor has seven parameters.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        cave_folder: PathBuf,
        name: &str,
        zone_context: Rc<ZoneFileContext>,
        resource: Rc<SeedStringResource>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CaveType"), data_folder, json),
            cave_folder,
            name: name.to_string(),
            zone_context,
            resource,
            registries,
        }
    }

    /// Java `CaveType load()`.
    pub fn load(&self) -> Rc<CaveType> {
        // The Java loader reads the entry point generator *before* the cave
        // type is constructed.
        let point_generator = self.load_entry_point_generator();

        // Java evaluates the constructor arguments left-to-right.
        let entry_node_type = self.load_entry_node_type();
        let yaw = self.load_yaw();
        let pitch = self.load_pitch();
        let depth = self.load_depth();
        let height_factors = self.load_height_factors();
        let biome_mask = self.load_biome_mask();
        let block_mask = self.load_block_mask();
        let map_condition = self.load_map_condition();
        let height_condition = self.load_height_condition();
        let fixed_entry_height = self.load_fixed_entry_height();
        let fixed_entry_height_noise = self.load_fixed_entry_height_noise();
        let fluid_level = self.load_fluid_level();
        let environment = self.load_environment();
        let surface_limited = self.load_surface_limited();
        let submerge = self.load_submerge();
        let maximum_size = self.load_maximum_size(&point_generator);

        Rc::new(CaveType::new(
            self.name.clone(),
            entry_node_type,
            yaw,
            pitch,
            depth,
            height_factors,
            point_generator,
            biome_mask,
            block_mask,
            map_condition,
            height_condition,
            fixed_entry_height,
            fixed_entry_height_noise,
            fluid_level,
            environment,
            surface_limited,
            submerge,
            maximum_size,
        ))
    }

    /// Java `IFloatRange loadYaw()`.
    fn load_yaw(&self) -> Rc<dyn IFloatRange> {
        FloatRangeJsonLoader::with_defaults(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_YAW),
            -180.0,
            180.0,
            degrees_to_radians,
        )
        .load()
    }

    /// Java `IFloatRange loadPitch()`.
    fn load_pitch(&self) -> Rc<dyn IFloatRange> {
        FloatRangeJsonLoader::with_default1_and_function(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_PITCH),
            -15.0,
            degrees_to_radians,
        )
        .load()
    }

    /// Java `IDoubleRange loadDepth()`.
    fn load_depth(&self) -> Rc<dyn IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_DEPTH),
                80.0,
            )
            .load(),
        )
    }

    /// Java `IHeightThresholdInterpreter loadHeightFactors()`.
    fn load_height_factors(
        &self,
    ) -> Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter> {
        HeightThresholdInterpreterJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_HEIGHT_RADIUS_FACTOR),
            #[allow(deprecated)] // deprecated in hytale-math but this is its exact Java value
            hytale_math::chunk_util::HEIGHT,
        )
        .load()
    }

    /// Java `CaveNodeType loadEntryNodeType()`.
    fn load_entry_node_type(&self) -> Rc<CaveNodeType> {
        let entry = self.base.get(constants::KEY_ENTRY).unwrap_or_else(|| {
            panic!("{}", constants::ERROR_NO_ENTRY);
        });

        if let Value::Object(object) = &entry {
            // An inline node type: it gets a unique name.
            let entry_node_type_string = self.resource.get_unique_name("CaveType#");
            return self
                .new_storage(self.cave_folder.clone(), Rc::clone(&self.zone_context))
                .load_cave_node_type_with_json(
                    &entry_node_type_string,
                    Value::Object(object.clone()),
                );
        }

        if let Value::String(entry_node_type_string) = &entry {
            let mut cave_folder = self.cave_folder.clone();
            let mut zone_context = Rc::clone(&self.zone_context);
            let mut entry_node_type_string = entry_node_type_string.clone();

            // The path may point into a different zone; then the cave folder
            // and the zone context change.
            if entry_node_type_string.starts_with(&format!("{PATH_ZONES}.")) {
                let filepath = self
                    .base
                    .data_folder
                    .join(entry_node_type_string.replace('.', std::path::MAIN_SEPARATOR_STR));
                let rel_path = Self::relativize(&filepath, &self.base.data_folder);
                if rel_path.components().count() > 1 {
                    let zone_name = rel_path
                        .components()
                        .nth(1)
                        .map(|component| component.as_os_str().to_string_lossy().to_string())
                        .unwrap_or_default();
                    let parent = zone_context.get_parent_context();
                    let zones = parent.get_zones();
                    let zone = zones.borrow().get(&zone_name);
                    zone_context = zone;
                    cave_folder = zone_context.base.get_path().join(PATH_CAVE);
                    entry_node_type_string = Self::relativize(&filepath, &cave_folder)
                        .to_string_lossy()
                        .replace(std::path::MAIN_SEPARATOR_STR, ".");
                }
            }

            return self
                .new_storage(cave_folder, zone_context)
                .load_cave_node_type(&entry_node_type_string);
        }

        panic!("Invalid entry node type definition! Expected String or JsonObject: {entry}");
    }

    /// The storage of the node-type graph of this cave type.
    fn new_storage(
        &self,
        cave_folder: PathBuf,
        zone_context: Rc<ZoneFileContext>,
    ) -> CaveNodeTypeStorage {
        CaveNodeTypeStorage::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            cave_folder,
            zone_context,
            Rc::clone(&self.resource),
            Rc::clone(&self.registries),
        )
    }

    /// Java `FileIO.relativize(Path child, Path parent)` — the child path
    /// with the parent prefix removed.
    fn relativize(child: &Path, parent: &Path) -> PathBuf {
        let child_components: Vec<_> = child.components().collect();
        let parent_components: Vec<_> = parent.components().collect();
        if child_components.len() < parent_components.len() {
            return child.to_path_buf();
        }
        if !child_components
            .iter()
            .zip(parent_components.iter())
            .all(|(child, parent)| child == parent)
        {
            return child.to_path_buf();
        }
        let mut out = PathBuf::new();
        for component in &child_components[parent_components.len()..] {
            out.push(component.as_os_str());
        }
        out
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

    /// Java `IPointGenerator loadEntryPointGenerator()`.
    fn load_entry_point_generator(&self) -> Rc<dyn IPointGenerator> {
        if !self.base.has(constants::KEY_ENTRY_POINTS) {
            panic!("{}", constants::ERROR_NO_ENTRY_POINTS);
        }
        PointGeneratorJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_POINTS),
        )
        .load()
    }

    /// Java `Int2FlagsCondition loadBiomeMask()`.
    fn load_biome_mask(&self) -> Rc<dyn crate::util::condition::flag::Int2FlagsCondition> {
        if !self.base.has(constants::KEY_BIOME_MASK) {
            return Rc::new(crate::cave::cave_biome_mask_flags::DEFAULT_ALLOW);
        }
        // The biome mask may redirect to a file in a different zone folder.
        let context = self
            .zone_context
            .match_context_json(self.base.json.as_ref(), constants::KEY_BIOME_MASK);
        CaveBiomeMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_BIOME_MASK),
            context,
        )
        .load()
    }

    /// Java `BlockMaskCondition loadBlockMask()`.
    fn load_block_mask(&self) -> BlockMaskCondition {
        if !self.base.has(constants::KEY_BLOCK_MASK) {
            return BlockMaskCondition::new();
        }
        BlockPlacementMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get_raw(constants::KEY_BLOCK_MASK),
            Rc::clone(&self.registries),
        )
        .load()
    }

    /// Java `ICoordinateCondition loadMapCondition()`.
    fn load_map_condition(&self) -> Rc<dyn ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE_MASK),
        )
        .load()
    }

    /// Java `IDoubleRange loadFixedEntryHeight()`.
    fn load_fixed_entry_height(&self) -> Option<Rc<dyn IDoubleRange>> {
        if !self.base.has(constants::KEY_FIXED_ENTRY_HEIGHT) {
            return None;
        }
        Some(Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_FIXED_ENTRY_HEIGHT),
                0.0,
            )
            .load(),
        ))
    }

    /// Java `NoiseProperty loadFixedEntryHeightNoise()`.
    fn load_fixed_entry_height_noise(&self) -> Option<Rc<dyn NoiseProperty>> {
        if !self.base.has(constants::KEY_FIXED_ENTRY_HEIGHT_NOISE) {
            // Java `ConstantNoiseProperty.DEFAULT_ZERO`.
            return Some(crate::util::constant_noise_property::default_zero());
        }
        Some(
            NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_FIXED_ENTRY_HEIGHT_NOISE),
            )
            .load(),
        )
    }

    /// Java `CaveType.FluidLevel loadFluidLevel()`.
    fn load_fluid_level(&self) -> FluidLevel {
        if !self.base.has(constants::KEY_FLUID_LEVEL) {
            return FluidLevel::EMPTY;
        }
        FluidLevelJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_FLUID_LEVEL),
            (*self.registries).clone(),
        )
        .load()
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

    /// Java `boolean loadSurfaceLimited()`.
    fn load_surface_limited(&self) -> bool {
        if !self.base.has(constants::KEY_SURFACE_LIMITTED) {
            return true;
        }
        JsonLoaderBase::get_as_bool(&self.base.get(constants::KEY_SURFACE_LIMITTED).unwrap())
    }

    /// Java `boolean loadSubmerge()`.
    fn load_submerge(&self) -> bool {
        self.base
            .must_get_bool(constants::KEY_SUBMERGE, Some(constants::DEFAULT_SUBMERGE))
    }

    /// Java `double loadMaximumSize(IPointGenerator pointGenerator)`.
    fn load_maximum_size(&self, point_generator: &Rc<dyn IPointGenerator>) -> f64 {
        if self.base.has(constants::KEY_MAXIMUM_SIZE) {
            return JsonLoaderBase::get_as_long(&self.base.get(constants::KEY_MAXIMUM_SIZE).unwrap())
                as f64;
        }
        // Java `MathUtil.fastFloor(pointGenerator.getInterval())`.
        hytale_math::math_util::fast_floor_f64(point_generator.get_interval()) as f64
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

    fn loader(json: Option<Value>) -> CaveTypeJsonLoader {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        CaveTypeJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            PathBuf::from("/tmp/world/Zones/Oceans/Cave"),
            "Caldera",
            zone,
            Rc::new(SeedStringResource::new()),
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_appends_the_cave_type_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CaveType");
    }

    #[test]
    fn a_missing_entry_points_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(None).load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "\"EntryPoints\" is not defined, no spawn information for caves available"
        );
    }

    #[test]
    fn a_missing_entry_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({
                "EntryPoints": {"Seed": "x", "Density": [0, 1], "Scale": 0.1}
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
            "\"Entry\" is not defined. Define an entry node type"
        );
    }

    #[test]
    fn the_entry_must_be_a_string_or_an_object() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({
                "Entry": 42,
                "EntryPoints": {"Seed": "x", "Density": [0, 1], "Scale": 0.1}
            })))
            .load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert!(
            message.starts_with("Invalid entry node type definition!"),
            "{message}"
        );
    }

    #[test]
    fn the_relativize_helper_matches_the_java_semantics() {
        assert_eq!(
            CaveTypeJsonLoader::relativize(Path::new("/a/b/Zones/X/Cave/Y"), Path::new("/a/b")),
            PathBuf::from("Zones/X/Cave/Y")
        );
        // A child that does not start with the parent is returned unchanged.
        assert_eq!(
            CaveTypeJsonLoader::relativize(Path::new("/a/c"), Path::new("/a/b")),
            PathBuf::from("/a/c")
        );
    }
}
