// Ported from: `.hytale-source/.../worldgen/prefab/unique/UniquePrefabConfiguration.java`
//! Java `com.hypixel.hytale.server.worldgen.prefab.unique.UniquePrefabConfiguration`
//! — the configuration of one *unique* prefab generator (the exclusion
//! radii, the zone-border exclusion, the spawn offset / flag, …).
//!
//! The configuration is the immutable description of *where* and *how* a
//! unique prefab may be placed. The placement itself is done by
//! [`crate::prefab::unique::unique_prefab_generator::UniquePrefabGenerator`].

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::noise::condition::IIntCondition;
use procedural::noise::condition::{
    IBlockFluidCondition, ICoordinateCondition, ICoordinateRndCondition,
};

use crate::biome::biome::Biome;
use crate::loader::util::{Vector2d, Vector3d};
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::condition::BlockMaskCondition;

/// Java `class UniquePrefabConfiguration`.
pub struct UniquePrefabConfiguration {
    /// Java `protected final ICoordinateRndCondition heightCondition` —
    /// `null` when the configuration has no height threshold.
    height_condition: Option<Rc<dyn ICoordinateRndCondition>>,
    /// Java `protected final BlockMaskCondition placementConfiguration`.
    placement_configuration: BlockMaskCondition,
    /// Java `protected final PrefabRotation[] rotations` — `null` when the
    /// configuration does not restrict the rotation.
    rotations: Option<Vec<PrefabRotation>>,
    /// Java `protected final IIntCondition biomeMask`.
    biome_mask: Rc<dyn IIntCondition>,
    /// Java `protected final ICoordinateCondition mapCondition`.
    map_condition: Rc<dyn ICoordinateCondition>,
    /// Java `protected final IBlockFluidCondition parent`.
    parent: Rc<dyn IBlockFluidCondition>,
    /// Java `protected final Vector2d anchor`.
    anchor: Vector2d,
    /// Java `protected final Vector3d spawnOffset`.
    spawn_offset: Vector3d,
    /// Java `protected final double maxDistance`.
    max_distance: f64,
    /// Java `protected final boolean fitHeightmap`.
    fit_heightmap: bool,
    /// Java `protected final boolean submerge`.
    submerge: bool,
    /// Java `protected final boolean onWater`.
    on_water: bool,
    /// Java `protected final int environmentId`.
    environment_id: i32,
    /// Java `protected final int maxAttempts`.
    max_attempts: i32,
    /// Java `protected final double exclusionRadiusSquared` — the *squared*
    /// exclusion radius (`exclusionRadius * exclusionRadius`).
    exclusion_radius_squared: f64,
    /// Java `protected final boolean spawnLocation`.
    spawn_location: bool,
    /// Java `protected final double zoneBorderExclusion`.
    zone_border_exclusion: f64,
    /// Java `protected final boolean showOnMap`.
    show_on_map: bool,
}

impl UniquePrefabConfiguration {
    /// Java `public UniquePrefabConfiguration(…)` — the 18-argument
    /// constructor.
    ///
    /// The Java constructor squares the exclusion radius itself:
    /// ```java
    /// this.exclusionRadiusSquared = exclusionRadius * exclusionRadius;
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        height_condition: Option<Rc<dyn ICoordinateRndCondition>>,
        placement_configuration: BlockMaskCondition,
        rotations: Option<Vec<PrefabRotation>>,
        biome_mask: Rc<dyn IIntCondition>,
        map_condition: Rc<dyn ICoordinateCondition>,
        parent: Rc<dyn IBlockFluidCondition>,
        anchor: Vector2d,
        spawn_offset: Vector3d,
        max_distance: f64,
        fit_heightmap: bool,
        submerge: bool,
        on_water: bool,
        environment_id: i32,
        max_attempts: i32,
        exclusion_radius: f64,
        spawn_location: bool,
        zone_border_exclusion: f64,
        show_on_map: bool,
    ) -> Self {
        Self {
            height_condition,
            placement_configuration,
            rotations,
            biome_mask,
            map_condition,
            parent,
            anchor,
            spawn_offset,
            max_distance,
            fit_heightmap,
            submerge,
            on_water,
            environment_id,
            max_attempts,
            exclusion_radius_squared: exclusion_radius * exclusion_radius,
            spawn_location,
            zone_border_exclusion,
            show_on_map,
        }
    }

    /// Java `Vector2d getAnchor()`.
    pub fn get_anchor(&self) -> Vector2d {
        self.anchor
    }

    /// Java `double getMaxDistance()`.
    pub fn get_max_distance(&self) -> f64 {
        self.max_distance
    }

    /// Java `ICoordinateCondition getMapCondition()`.
    pub fn get_map_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.map_condition
    }

    /// Java `BlockMaskCondition getPlacementConfiguration()`.
    pub fn get_placement_configuration(&self) -> &BlockMaskCondition {
        &self.placement_configuration
    }

    /// Java `Vector3d getSpawnOffset()`.
    pub fn get_spawn_offset(&self) -> Vector3d {
        self.spawn_offset
    }

    /// Java `boolean isValidParentBiome(@Nonnull Biome biome)` —
    /// `biomeMask.eval(biome.getId())`.
    pub fn is_valid_parent_biome(&self, biome: &dyn Biome) -> bool {
        self.biome_mask.eval(biome.get_id())
    }

    /// Java `boolean isFitHeightmap()`.
    pub fn is_fit_heightmap(&self) -> bool {
        self.fit_heightmap
    }

    /// Java `boolean isSubmerge()`.
    pub fn is_submerge(&self) -> bool {
        self.submerge
    }

    /// Java `boolean isValidParentBlock(int block, int fluid)` —
    /// `parent.eval(block, fluid)`.
    pub fn is_valid_parent_block(&self, block: i32, fluid: i32) -> bool {
        self.parent.eval(block, fluid)
    }

    /// Java `ICoordinateRndCondition getHeightCondition()` — `null` when the
    /// configuration has no height threshold.
    pub fn get_height_condition(&self) -> Option<&Rc<dyn ICoordinateRndCondition>> {
        self.height_condition.as_ref()
    }

    /// Java `PrefabRotation getRotation(@Nonnull Random random)`.
    ///
    /// ```java
    /// return rotations == null || rotations.length == 0
    ///     ? PrefabRotation.ROTATION_0
    ///     : rotations[random.nextInt(rotations.length)];
    /// ```
    pub fn get_rotation(&self, random: &mut FastRandom) -> PrefabRotation {
        match &self.rotations {
            None => PrefabRotation::Rotation0,
            Some(rotations) if rotations.is_empty() => PrefabRotation::Rotation0,
            Some(rotations) => rotations[random.next_int_bound(rotations.len() as i32) as usize],
        }
    }

    /// Java `boolean isOnWater()`.
    pub fn is_on_water(&self) -> bool {
        self.on_water
    }

    /// Java `int getEnvironmentId()`.
    pub fn get_environment_id(&self) -> i32 {
        self.environment_id
    }

    /// Java `int getMaxAttempts()`.
    pub fn get_max_attempts(&self) -> i32 {
        self.max_attempts
    }

    /// Java `double getExclusionRadiusSquared()`.
    pub fn get_exclusion_radius_squared(&self) -> f64 {
        self.exclusion_radius_squared
    }

    /// Java `boolean isSpawnLocation()`.
    pub fn is_spawn_location(&self) -> bool {
        self.spawn_location
    }

    /// Java `double getZoneBorderExclusion()`.
    pub fn get_zone_border_exclusion(&self) -> f64 {
        self.zone_border_exclusion
    }

    /// Java `boolean isShowOnMap()`.
    pub fn is_show_on_map(&self) -> bool {
        self.show_on_map
    }

    /// The biome mask of the configuration (`biomeMask.eval(id)`).
    ///
    /// The Java class has no getter for the mask; the Rust port exposes it so
    /// the loader tests can assert against it.
    pub fn get_biome_mask(&self) -> &Rc<dyn IIntCondition> {
        &self.biome_mask
    }

    /// The rotations of the configuration (`null` when unrestricted).
    pub fn get_rotations(&self) -> Option<&[PrefabRotation]> {
        self.rotations.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::condition::AlwaysTrueIntCondition;
    use procedural::noise::condition::{
        ConstantBlockFluidCondition, DefaultCoordinateCondition, DefaultCoordinateRndCondition,
    };

    /// A configuration with the defaults of the Java loader.
    fn configuration() -> UniquePrefabConfiguration {
        UniquePrefabConfiguration::new(
            None,
            BlockMaskCondition::new(),
            None,
            Rc::new(AlwaysTrueIntCondition),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(ConstantBlockFluidCondition::True),
            Vector2d::new(0.0, 0.0),
            Vector3d::new(0.0, -5000.0, 0.0),
            100.0,
            false,
            false,
            false,
            -1,
            5000,
            50.0,
            false,
            25.0,
            false,
        )
    }

    #[test]
    fn the_exclusion_radius_is_squared() {
        let configuration = configuration();
        assert_eq!(configuration.get_exclusion_radius_squared(), 2500.0);
    }

    #[test]
    fn the_defaults_match_the_java_loader_constants() {
        let configuration = configuration();
        assert_eq!(configuration.get_max_distance(), 100.0);
        assert_eq!(configuration.get_max_attempts(), 5000);
        assert_eq!(configuration.get_zone_border_exclusion(), 25.0);
        assert_eq!(configuration.get_environment_id(), -1);
        assert!(!configuration.is_fit_heightmap());
        assert!(!configuration.is_submerge());
        assert!(!configuration.is_on_water());
        assert!(!configuration.is_spawn_location());
        assert!(!configuration.is_show_on_map());
        assert_eq!(configuration.get_anchor(), Vector2d::new(0.0, 0.0));
        assert_eq!(
            configuration.get_spawn_offset(),
            Vector3d::new(0.0, -5000.0, 0.0)
        );
    }

    #[test]
    fn without_rotations_the_rotation_is_the_identity() {
        let mut random = FastRandom::new(7);
        assert_eq!(
            configuration().get_rotation(&mut random),
            PrefabRotation::Rotation0
        );
    }

    #[test]
    fn the_rotation_is_picked_from_the_list() {
        let mut configuration = configuration();
        configuration.rotations = Some(vec![PrefabRotation::Rotation90]);
        let mut random = FastRandom::new(7);
        assert_eq!(
            configuration.get_rotation(&mut random),
            PrefabRotation::Rotation90
        );
    }

    #[test]
    fn an_empty_rotation_list_falls_back_to_the_identity() {
        let mut configuration = configuration();
        configuration.rotations = Some(Vec::new());
        let mut random = FastRandom::new(7);
        assert_eq!(
            configuration.get_rotation(&mut random),
            PrefabRotation::Rotation0
        );
    }

    #[test]
    fn the_height_condition_is_optional() {
        assert!(configuration().get_height_condition().is_none());

        let mut configuration = configuration();
        configuration.height_condition = Some(Rc::new(DefaultCoordinateRndCondition::True));
        assert!(configuration.get_height_condition().is_some());
    }
}
