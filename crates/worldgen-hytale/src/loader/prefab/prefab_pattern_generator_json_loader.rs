// Ported from: `.hytale-source/.../worldgen/loader/prefab/PrefabPatternGeneratorJsonLoader.java`
//! Java `loader.prefab.PrefabPatternGeneratorJsonLoader` — the description
//! of *where* and *how* a biome places its prefabs.
//!
//! The loader reads the grid generator, the height conditions, the block
//! mask, the rotations, the displacement and the base checks of the
//! `"Pattern"` object of a prefab container entry.

use std::path::Path;
use std::rc::Rc;

use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::point_generator_json_loader::PointGeneratorJsonLoader;
use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::loader::context::file_loading_context::FileLoadingContext;
use crate::loader::prefab::block_placement_mask_json_loader::BlockPlacementMaskJsonLoader;
use crate::prefab::prefab_base_check::PrefabBaseCheck;
use crate::prefab::prefab_category::PrefabCategory;
use crate::prefab::prefab_pattern_generator::PrefabPatternGenerator;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::condition::BlockMaskCondition;
use crate::util::condition::HashSetBlockFluidCondition;
use crate::util::function::{
    ConstantCoordinateDoubleSupplier, ICoordinateDoubleSupplier, RandomCoordinateDoubleSupplier,
};
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `PrefabPatternGeneratorJsonLoader.Constants`.
pub mod constants {
    /// Java `PointGeneratorJsonLoader`.
    pub const KEY_GRID_GENERATOR: &str = "GridGenerator";
    /// Java `ResolvedBlockArrayJsonLoader`.
    pub const KEY_PARENT: &str = "Parent";
    /// Java `HeightThresholdInterpreterJsonLoader`.
    pub const KEY_HEIGHT_THRESHOLD: &str = "HeightThreshold";
    /// Java `NoisePropertyJsonLoader`.
    pub const KEY_NOISE_MASK: &str = "NoiseMask";
    /// The block mask of the placement.
    pub const KEY_MASK: &str = "Mask";
    /// The rotations of the prefabs.
    pub const KEY_ROTATIONS: &str = "Rotations";
    /// Java `DoubleRangeJsonLoader`.
    pub const KEY_DISPLACEMENT: &str = "Displacement";
    /// The "fit the heightmap" flag.
    pub const KEY_FIT_HEIGHTMAP: &str = "FitHeightmap";
    /// The "may be placed on water" flag.
    pub const KEY_ON_WATER: &str = "OnWater";
    /// The "deep search" flag.
    pub const KEY_DEEP_SEARCH: &str = "DeepSearch";
    /// The "submerge" flag.
    pub const KEY_SUBMERGE: &str = "Submerge";
    /// The maximum size of the placed prefabs.
    pub const KEY_MAX_SIZE: &str = "MaxSize";
    /// The exclusion radius of the placed prefabs.
    pub const KEY_EXCLUSION_RADIUS: &str = "ExclusionRadius";
    /// The base checks of the placement.
    pub const KEY_BASE_CHECKS: &str = "BaseChecks";
    /// The category of the placed prefabs.
    pub const KEY_CATEGORY: &str = "Category";

    /// Java `ERROR_NO_GRID_GENERATOR`.
    pub const ERROR_NO_GRID_GENERATOR: &str =
        "Could not find point generator to place prefabs at! Keyword: GridGenerator";
    /// Java `ERROR_DEEP_SEARCH`.
    pub const ERROR_DEEP_SEARCH: &str = "DeepSearch is enabled but HeightThreshold is not set!";

    /// Java `Boolean DEFAULT_SUBMERGE` = `false`.
    pub const DEFAULT_SUBMERGE: bool = false;
    /// Java `Integer DEFAULT_MAX_SIZE` = `HeightThresholdInterpolator.MAX_RADIUS`.
    pub const DEFAULT_MAX_SIZE: i32 = crate::chunk::MAX_RADIUS;
    /// Java `Integer DEFAULT_EXCLUSION_RADIUS` = `0`.
    pub const DEFAULT_EXCLUSION_RADIUS: i32 = 0;
}

/// Java `loader.prefab.PrefabPatternGeneratorJsonLoader`.
pub struct PrefabPatternGeneratorJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource,
    /// PrefabPatternGenerator>`.
    pub base: JsonLoaderBase,
    /// Java `private final FileLoadingContext context`.
    context: Rc<FileLoadingContext>,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl PrefabPatternGeneratorJsonLoader {
    /// Java `PrefabPatternGeneratorJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json, FileLoadingContext context)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        context: Rc<FileLoadingContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".PrefabPatternGenerator"), data_folder, json),
            context,
            registries,
        }
    }

    /// Java `PrefabPatternGenerator load()`.
    pub fn load(&self) -> PrefabPatternGenerator {
        let height_thresholds = self.load_height_thresholds();
        let has_height_thresholds = height_thresholds.is_some();
        PrefabPatternGenerator::new(
            self.base.seed.hash_code(),
            self.load_category(),
            self.load_pattern(),
            self.load_height_condition(height_thresholds.clone()),
            height_thresholds,
            self.load_mask(),
            self.load_map_condition(),
            self.load_parent(),
            self.load_rotations(),
            self.load_displacement(),
            self.load_fit_heightmap(),
            self.load_on_water(),
            self.load_deep_search(has_height_thresholds),
            self.load_submerge(),
            self.load_max_size(),
            self.load_exclusion_radius(),
            self.load_base_checks(),
        )
    }

    /// Java `IPointGenerator loadPattern()`.
    fn load_pattern(&self) -> Option<Rc<dyn procedural::noise::point::IPointGenerator>> {
        if !self.base.has(constants::KEY_GRID_GENERATOR) {
            panic!("{}", constants::ERROR_NO_GRID_GENERATOR);
        }
        Some(
            PointGeneratorJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_GRID_GENERATOR),
            )
            .load(),
        )
    }

    /// Java `PrefabCategory loadCategory()`.
    fn load_category(&self) -> PrefabCategory {
        let category = self.base.must_get_string(constants::KEY_CATEGORY, Some(""));
        if category.is_empty() {
            return PrefabCategory::none();
        }
        let categories = self.context.get_prefab_categories().borrow();
        if !categories.contains(&category) {
            // Java logs a warning and falls back to `PrefabCategory.NONE`.
            return PrefabCategory::none();
        }
        let category = categories.get(&category);
        PrefabCategory {
            name: category.name.clone(),
            priority: category.priority,
        }
    }

    /// Java `IBlockFluidCondition loadParent()`.
    fn load_parent(&self) -> Rc<dyn procedural::noise::condition::IBlockFluidCondition> {
        if !self.base.has(constants::KEY_PARENT) {
            return Rc::new(procedural::noise::condition::ConstantBlockFluidCondition::True);
        }
        let block_array = crate::loader::util::resolved_block_array_json_loader::ResolvedBlockArrayJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_PARENT),
            Rc::clone(&self.registries),
        )
        .load();
        let mut set = std::collections::HashSet::new();
        for entry in block_array.get_entries() {
            set.insert(hytale_math::math_util::pack_long(
                entry.block_id,
                entry.fluid_id,
            ));
        }
        Rc::new(HashSetBlockFluidCondition::new(set))
    }

    /// Java `IHeightThresholdInterpreter loadHeightThresholds()`.
    fn load_height_thresholds(
        &self,
    ) -> Option<Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter>> {
        if !self.base.has(constants::KEY_HEIGHT_THRESHOLD) {
            return None;
        }
        Some(
            HeightThresholdInterpreterJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_HEIGHT_THRESHOLD),
                #[allow(deprecated)] // the Java value (the legacy chunk height)
                hytale_math::chunk_util::HEIGHT,
            )
            .load(),
        )
    }

    /// Java `ICoordinateRndCondition loadHeightCondition(@Nullable
    /// IHeightThresholdInterpreter thresholdInterpreter)`.
    fn load_height_condition(
        &self,
        threshold_interpreter: Option<
            Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter>,
        >,
    ) -> Rc<dyn procedural::noise::condition::ICoordinateRndCondition> {
        match threshold_interpreter {
            Some(interpreter) => Rc::new(procedural::noise::condition::HeightCondition::new(
                interpreter,
            )),
            None => Rc::new(procedural::noise::condition::DefaultCoordinateRndCondition::True),
        }
    }

    /// Java `ICoordinateCondition loadMapCondition()`.
    fn load_map_condition(&self) -> Rc<dyn procedural::noise::condition::ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE_MASK),
        )
        .load()
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

    /// Java `PrefabRotation[] loadRotations()`.
    fn load_rotations(&self) -> Option<Vec<PrefabRotation>> {
        if !self.base.has(constants::KEY_ROTATIONS) {
            return None;
        }
        Self::load_rotations_element(self.base.get(constants::KEY_ROTATIONS))
    }

    /// Java `public static PrefabRotation[] loadRotations(@Nullable JsonElement
    /// element)`.
    pub fn load_rotations_element(element: Option<Value>) -> Option<Vec<PrefabRotation>> {
        let element = element?;
        match &element {
            Value::Array(array) => {
                if array.is_empty() {
                    panic!("Array for rotations must be greater than 0 or left away to allow random rotation.");
                }
                let mut rotations = Vec::with_capacity(array.len());
                for value in array {
                    let name = JsonLoaderBase::get_as_string(value);
                    rotations.push(Self::parse_rotation(&name));
                }
                Some(rotations)
            }
            Value::String(name) => Some(vec![Self::parse_rotation(name)]),
            other => panic!(
                "rotations is not an array nor a string, other types are not supported! Given: {other}"
            ),
        }
    }

    /// Java `PrefabRotation.valueOf(name)` — the strict form (the
    /// `valueOfExtended` form is *not* used here).
    fn parse_rotation(name: &str) -> PrefabRotation {
        PrefabRotation::VALUES
            .into_iter()
            .find(|rotation| rotation.name() == name)
            .unwrap_or_else(|| {
                panic!(
                    "Could not find rotation \"{name}\". Allowed: {:?}",
                    PrefabRotation::VALUES
                        .iter()
                        .map(|rotation| rotation.name())
                        .collect::<Vec<_>>()
                )
            })
    }

    /// Java `ICoordinateDoubleSupplier loadDisplacement()`.
    fn load_displacement(&self) -> Rc<dyn ICoordinateDoubleSupplier> {
        if !self.base.has(constants::KEY_DISPLACEMENT) {
            return Rc::new(ConstantCoordinateDoubleSupplier::DEFAULT_ZERO);
        }
        let range = procedural::json::double_range_json_loader::DoubleRangeJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_DISPLACEMENT),
        )
        .load();
        Rc::new(RandomCoordinateDoubleSupplier::new(Rc::new(range)))
    }

    /// Java `boolean loadFitHeightmap()`.
    fn load_fit_heightmap(&self) -> bool {
        self.base.has(constants::KEY_FIT_HEIGHTMAP)
            && JsonLoaderBase::get_as_bool(
                &self
                    .base
                    .get(constants::KEY_FIT_HEIGHTMAP)
                    .expect("the key is present"),
            )
    }

    /// Java `boolean loadOnWater()`.
    fn load_on_water(&self) -> bool {
        self.base.has(constants::KEY_ON_WATER)
            && JsonLoaderBase::get_as_bool(
                &self
                    .base
                    .get(constants::KEY_ON_WATER)
                    .expect("the key is present"),
            )
    }

    /// Java `boolean loadDeepSearch(@Nonnull IHeightThresholdInterpreter
    /// interpreter)`.
    fn load_deep_search(&self, has_interpreter: bool) -> bool {
        let deep_search = self.base.has(constants::KEY_DEEP_SEARCH)
            && JsonLoaderBase::get_as_bool(
                &self
                    .base
                    .get(constants::KEY_DEEP_SEARCH)
                    .expect("the key is present"),
            );
        if deep_search && !has_interpreter {
            panic!("{}", constants::ERROR_DEEP_SEARCH);
        }
        deep_search
    }

    /// Java `boolean loadSubmerge()`.
    fn load_submerge(&self) -> bool {
        if !self.base.has(constants::KEY_SUBMERGE) {
            return constants::DEFAULT_SUBMERGE;
        }
        JsonLoaderBase::get_as_bool(
            &self
                .base
                .get(constants::KEY_SUBMERGE)
                .expect("the key is present"),
        )
    }

    /// Java `int loadMaxSize()`.
    fn load_max_size(&self) -> i32 {
        if !self.base.has(constants::KEY_MAX_SIZE) {
            return constants::DEFAULT_MAX_SIZE;
        }
        JsonLoaderBase::get_as_int(
            &self
                .base
                .get(constants::KEY_MAX_SIZE)
                .expect("the key is present"),
        )
    }

    /// Java `int loadExclusionRadius()`.
    fn load_exclusion_radius(&self) -> i32 {
        if !self.base.has(constants::KEY_EXCLUSION_RADIUS) {
            return constants::DEFAULT_EXCLUSION_RADIUS;
        }
        JsonLoaderBase::get_as_int(
            &self
                .base
                .get(constants::KEY_EXCLUSION_RADIUS)
                .expect("the key is present"),
        )
    }

    /// Java `PrefabBaseCheck[] loadBaseChecks()`.
    fn load_base_checks(&self) -> Vec<PrefabBaseCheck> {
        PrefabBaseCheck::load_all(self.base.get(constants::KEY_BASE_CHECKS).as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;

    fn loader(json: Option<Value>) -> PrefabPatternGeneratorJsonLoader {
        PrefabPatternGeneratorJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            FileLoadingContext::new("test", std::path::PathBuf::from(".")),
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_appends_the_pattern_generator_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.PrefabPatternGenerator");
    }

    #[test]
    fn a_missing_grid_generator_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(None).load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Could not find point generator to place prefabs at! Keyword: GridGenerator"
        );
    }

    #[test]
    fn the_defaults_are_the_java_ones() {
        let generator = loader(Some(serde_json::json!({
            "GridGenerator": {"Type": "GRID", "Scale": 0.1, "Seed": "x"}
        })))
        .load();
        assert_eq!(generator.get_max_size(), constants::DEFAULT_MAX_SIZE);
        assert_eq!(generator.get_exclusion_radius(), 0);
        assert!(!generator.is_submerge());
        assert!(!generator.is_fit_heightmap());
        assert!(!generator.is_on_water());
        assert!(!generator.is_deep_search());
        assert!(generator.get_base_checks().is_empty());
        // The category of an absent category is `PrefabCategory.NONE`.
        assert_eq!(generator.get_category().name, "None");
        assert_eq!(
            generator.get_category().priority,
            crate::prefab::prefab_category::PrefabCategory::MIN_PRIORITY
        );
    }

    #[test]
    fn the_rotations_are_parsed() {
        let generator = loader(Some(serde_json::json!({
            "GridGenerator": {"Type": "GRID", "Scale": 0.1, "Seed": "x"},
            "Rotations": ["ROTATION_90", "ROTATION_270"]
        })))
        .load();
        let rotations = generator.get_rotations().expect("the rotations");
        assert_eq!(rotations.len(), 2);
        assert_eq!(rotations[0], PrefabRotation::Rotation90);
        assert_eq!(rotations[1], PrefabRotation::Rotation270);
    }

    #[test]
    fn an_unknown_rotation_panics() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({
                "GridGenerator": {"Type": "GRID", "Scale": 0.1, "Seed": "x"},
                "Rotations": ["ROTATION_45"]
            })))
            .load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert!(
            message.contains("Could not find rotation \"ROTATION_45\""),
            "{message}"
        );
    }
}
