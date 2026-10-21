// Ported from: `.hytale-source/.../worldgen/prefab/PrefabPatternGenerator.java`
//! Java `com.hypixel.hytale.server.worldgen.prefab.PrefabPatternGenerator` —
//! the description of *where* and *how* a biome places its prefabs.

use std::rc::Rc;

use hytale_math::math_util;
use procedural::noise::condition::{
    IBlockFluidCondition, ICoordinateCondition, ICoordinateRndCondition,
    IHeightThresholdInterpreter,
};
use procedural::noise::point::IPointGenerator;

use crate::prefab::prefab_base_check::PrefabBaseCheck;
use crate::prefab::prefab_category::PrefabCategory;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::condition::BlockMaskCondition;
use crate::util::function::ICoordinateDoubleSupplier;

/// Java `public class PrefabPatternGenerator`.
pub struct PrefabPatternGenerator {
    /// Java `protected final int seedOffset`.
    pub seed_offset: i32,
    /// Java `protected final PrefabCategory category`.
    pub category: PrefabCategory,
    /// Java `protected final IPointGenerator gridGenerator`.
    pub grid_generator: Option<Rc<dyn IPointGenerator>>,
    /// Java `protected final ICoordinateRndCondition heightCondition`.
    pub height_condition: Rc<dyn ICoordinateRndCondition>,
    /// Java `protected final IHeightThresholdInterpreter
    /// heightThresholdInterpreter`.
    pub height_threshold_interpreter: Option<Rc<dyn IHeightThresholdInterpreter>>,
    /// Java `protected final BlockMaskCondition prefabPlacementConfiguration`.
    pub prefab_placement_configuration: BlockMaskCondition,
    /// Java `protected final ICoordinateCondition mapCondition`.
    pub map_condition: Rc<dyn ICoordinateCondition>,
    /// Java `protected final IBlockFluidCondition parentCondition`.
    pub parent_condition: Rc<dyn IBlockFluidCondition>,
    /// Java `protected final PrefabRotation[] rotations`.
    pub rotations: Option<Vec<PrefabRotation>>,
    /// Java `protected final ICoordinateDoubleSupplier displacement`.
    pub displacement: Rc<dyn ICoordinateDoubleSupplier>,
    /// Java `protected final boolean fitHeightmap`.
    pub fit_heightmap: bool,
    /// Java `protected final boolean onWater`.
    pub on_water: bool,
    /// Java `protected final boolean deepSearch`.
    pub deep_search: bool,
    /// Java `protected final boolean submerge`.
    pub submerge: bool,
    /// Java `protected final int maxSize`.
    pub max_size: i32,
    /// Java `protected final int exclusionRadius`.
    pub exclusion_radius: i32,
    /// Java `protected final PrefabBaseCheck[] baseChecks`.
    pub base_checks: Vec<PrefabBaseCheck>,
}

impl PrefabPatternGenerator {
    /// Java `public PrefabPatternGenerator(...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        seed_offset: i32,
        category: PrefabCategory,
        grid_generator: Option<Rc<dyn IPointGenerator>>,
        height_condition: Rc<dyn ICoordinateRndCondition>,
        height_threshold_interpreter: Option<Rc<dyn IHeightThresholdInterpreter>>,
        prefab_placement_configuration: BlockMaskCondition,
        map_condition: Rc<dyn ICoordinateCondition>,
        parent_condition: Rc<dyn IBlockFluidCondition>,
        rotations: Option<Vec<PrefabRotation>>,
        displacement: Rc<dyn ICoordinateDoubleSupplier>,
        fit_heightmap: bool,
        on_water: bool,
        deep_search: bool,
        submerge: bool,
        max_size: i32,
        exclusion_radius: i32,
        base_checks: Vec<PrefabBaseCheck>,
    ) -> Self {
        Self {
            seed_offset,
            category,
            grid_generator,
            height_condition,
            height_threshold_interpreter,
            prefab_placement_configuration,
            map_condition,
            parent_condition,
            rotations,
            displacement,
            fit_heightmap,
            on_water,
            deep_search,
            submerge,
            max_size,
            exclusion_radius,
            base_checks,
        }
    }

    /// Java `public PrefabCategory getCategory()`.
    pub fn get_category(&self) -> &PrefabCategory {
        &self.category
    }

    /// Java `public IPointGenerator getGridGenerator()`.
    pub fn get_grid_generator(&self) -> Option<&Rc<dyn IPointGenerator>> {
        self.grid_generator.as_ref()
    }

    /// Java `public ICoordinateCondition getMapCondition()`.
    pub fn get_map_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.map_condition
    }

    /// Java `public BlockMaskCondition getPrefabPlacementConfiguration()`.
    pub fn get_prefab_placement_configuration(&self) -> &BlockMaskCondition {
        &self.prefab_placement_configuration
    }

    /// Java `public boolean isFitHeightmap()`.
    pub fn is_fit_heightmap(&self) -> bool {
        self.fit_heightmap
    }

    /// Java `public IBlockFluidCondition getParentCondition()`.
    pub fn get_parent_condition(&self) -> &Rc<dyn IBlockFluidCondition> {
        &self.parent_condition
    }

    /// Java `public ICoordinateRndCondition getHeightCondition()`.
    pub fn get_height_condition(&self) -> &Rc<dyn ICoordinateRndCondition> {
        &self.height_condition
    }

    /// Java `public IHeightThresholdInterpreter getHeightThresholdInterpreter()`.
    pub fn get_height_threshold_interpreter(&self) -> Option<&Rc<dyn IHeightThresholdInterpreter>> {
        self.height_threshold_interpreter.as_ref()
    }

    /// Java `public PrefabRotation[] getRotations()`.
    pub fn get_rotations(&self) -> Option<&Vec<PrefabRotation>> {
        self.rotations.as_ref()
    }

    /// Java `public int getDisplacement(int seed, int x, int z)`.
    pub fn get_displacement(&self, seed: i32, x: i32, z: i32) -> i32 {
        math_util::floor(self.displacement.apply2(seed + self.seed_offset, x, z))
    }

    /// Java `public boolean isOnWater()`.
    pub fn is_on_water(&self) -> bool {
        self.on_water
    }

    /// Java `public boolean isDeepSearch()`.
    pub fn is_deep_search(&self) -> bool {
        self.deep_search
    }

    /// Java `public boolean isSubmerge()`.
    pub fn is_submerge(&self) -> bool {
        self.submerge
    }

    /// Java `public int getMaxSize()`.
    pub fn get_max_size(&self) -> i32 {
        self.max_size
    }

    /// Java `public int getExclusionRadius()`.
    pub fn get_exclusion_radius(&self) -> i32 {
        self.exclusion_radius
    }

    /// Java `@Nonnull public PrefabBaseCheck[] getBaseChecks()`.
    pub fn get_base_checks(&self) -> &[PrefabBaseCheck] {
        &self.base_checks
    }
}
