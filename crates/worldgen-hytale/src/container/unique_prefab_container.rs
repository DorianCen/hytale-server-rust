// Ported from: `.hytale-source/.../worldgen/container/UniquePrefabContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.UniquePrefabContainer`
//! — the *unique* prefab catalogue of a zone.
//!
//! The container holds a seed offset and the unique-prefab generators; the
//! [`generate`](UniquePrefabContainer::generate) method turns them into the
//! per-seed [`UniquePrefabEntry`] list (the spawn points, the landmarks, …).
//! The list is *immutable per seed* — the Java caches it in the
//! [`crate::cache::unique_prefab_cache::UniquePrefabCache`].
//!
//! **Phase 5c** landed the *generation* of the entries: the
//! [`UniquePrefabGenerator`] family, the exclusion radii, the zone-border
//! exclusion and the inverse domain warp. See
//! [`crate::chunk::chunk_generator::ChunkGenerator::generate_unique_prefabs`].

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::noise::support::Vector2i;

use crate::chunk::chunk_generator::ChunkGenerator;
use crate::loader::util::Vector3d;
use crate::prefab::prefab_category::PrefabCategory;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::util::bounds::IChunkBounds;
use crate::util::condition::BlockMaskCondition;

pub use crate::prefab::unique::unique_prefab_generator::UniquePrefabGenerator;

/// Java `class UniquePrefabContainer`.
pub struct UniquePrefabContainer {
    /// Java `protected final int seedOffset`.
    seed_offset: i32,
    /// Java `protected final UniquePrefabGenerator[] generators`.
    generators: Vec<UniquePrefabGenerator>,
}

impl UniquePrefabContainer {
    /// Java `UniquePrefabContainer(int seedOffset, UniquePrefabGenerator[] generators)`.
    pub fn new(seed_offset: i32, generators: Vec<UniquePrefabGenerator>) -> Self {
        Self {
            seed_offset,
            generators,
        }
    }

    /// Java `UniquePrefabGenerator[] getGenerators()`.
    pub fn get_generators(&self) -> &[UniquePrefabGenerator] {
        &self.generators
    }

    /// Java `protected final int seedOffset` — there is no Java getter; the
    /// Rust port exposes it so the loader result can be verified.
    pub const fn get_seed_offset(&self) -> i32 {
        self.seed_offset
    }

    /// Java `@Nonnull UniquePrefabEntry[] generate(int seed, @Nullable
    /// Vector2i position, @Nonnull ChunkGenerator chunkGenerator)`.
    ///
    /// ```java
    /// final Random random = new FastRandom(seed + seedOffset);
    /// final UniquePrefabEntry[] entries = new UniquePrefabEntry[generators.length];
    /// for (int i = 0; i < generators.length; i++) {
    ///     final UniquePrefabGenerator generator = generators[i];
    ///     final UniquePrefabConfiguration configuration = generator.getConfiguration();
    ///     final Vector3i location = generator.generate(seed, position, chunkGenerator, random, configuration.getMaxAttempts(), entries);
    ///     entries[i] = new UniquePrefabEntry(
    ///         generator.getName(),
    ///         generator.getCategory(),
    ///         location,
    ///         generator.generatePrefab(random),
    ///         configuration.getPlacementConfiguration(),
    ///         configuration.getRotation(random),
    ///         configuration.getSpawnOffset(),
    ///         configuration.getEnvironmentId(),
    ///         configuration.isFitHeightmap(),
    ///         configuration.isSubmerge(),
    ///         configuration.getExclusionRadiusSquared(),
    ///         configuration.isSpawnLocation(),
    ///         configuration.isShowOnMap()
    ///     );
    /// }
    /// return entries;
    /// ```
    ///
    /// ⚠ **Draw order**: the *placement* of a generator happens **before** its
    /// prefab pick and its rotation pick, and all three share the same random
    /// stream (`new FastRandom(seed + seedOffset)`). The prefab pick and the
    /// rotation pick therefore consume the draws that the *next* generator
    /// sees.
    ///
    /// ⚠ The Java `int` addition `seed + seedOffset` wraps; the Rust port
    /// uses the wrapping addition too.
    pub fn generate(
        &self,
        seed: i32,
        position: Option<Vector2i>,
        chunk_generator: &ChunkGenerator,
    ) -> Vec<UniquePrefabEntry> {
        let mut random = FastRandom::new(seed.wrapping_add(self.seed_offset) as i64);
        let mut entries: Vec<UniquePrefabEntry> = Vec::with_capacity(self.generators.len());

        for generator in &self.generators {
            let configuration = generator.get_configuration();
            let location = generator.generate(
                seed,
                position,
                chunk_generator,
                &mut random,
                configuration.get_max_attempts(),
                &entries,
            );

            // Java evaluates the constructor arguments left to right: the
            // prefab pick comes *before* the rotation pick.
            let prefab_supplier = generator.generate_prefab(&mut random).unwrap_or_else(|| {
                panic!(
                    "UniquePrefabGenerator '{}': the weighted prefab map is empty (Java: NPE in the \
                     UniquePrefabEntry constructor)",
                    generator.get_name()
                )
            });
            let rotation = configuration.get_rotation(&mut random);

            entries.push(UniquePrefabEntry::new(
                generator.get_name().to_string(),
                generator.get_category().clone(),
                location,
                prefab_supplier,
                configuration.get_placement_configuration().clone(),
                rotation,
                configuration.get_spawn_offset(),
                configuration.get_environment_id(),
                configuration.is_fit_heightmap(),
                configuration.is_submerge(),
                configuration.get_exclusion_radius_squared(),
                configuration.is_spawn_location(),
                configuration.is_show_on_map(),
            ));
        }

        entries
    }
}

/// Java `static class UniquePrefabContainer.UniquePrefabEntry` — one placed
/// unique prefab.
///
/// The Java constructor computes the bounds of the entry from the prefab
/// buffer (the vertical range) and from the *recursive* prefab bounds (the
/// horizontal range); the Rust port computes the same values.
#[derive(Clone)]
pub struct UniquePrefabEntry {
    /// Java `protected final String name`.
    name: String,
    /// Java `protected final PrefabCategory category`.
    category: PrefabCategory,
    /// Java `@Nonnull protected final Vector3i position` — `(x, y, z)`.
    position: (i32, i32, i32),
    /// Java `@Nonnull protected final WorldGenPrefabSupplier prefabSupplier`.
    prefab_supplier: Rc<WorldGenPrefabSupplier>,
    /// Java `protected final BlockMaskCondition configuration`.
    configuration: BlockMaskCondition,
    /// Java `@Nonnull protected final PrefabRotation rotation`.
    rotation: PrefabRotation,
    /// Java `protected final Vector3d spawnOffset`.
    spawn_offset: Vector3d,
    /// Java `protected final int lowBoundX`.
    low_bound_x: i32,
    /// Java `protected final int lowBoundY`.
    low_bound_y: i32,
    /// Java `protected final int lowBoundZ`.
    low_bound_z: i32,
    /// Java `protected final int highBoundX`.
    high_bound_x: i32,
    /// Java `protected final int highBoundY`.
    high_bound_y: i32,
    /// Java `protected final int highBoundZ`.
    high_bound_z: i32,
    /// Java `protected final int environmentId`.
    environment_id: i32,
    /// Java `protected final boolean fitHeightmap`.
    fit_heightmap: bool,
    /// Java `protected final boolean submerge`.
    submerge: bool,
    /// Java `protected final double exclusionRadiusSquared`.
    exclusion_radius_squared: f64,
    /// Java `protected final boolean spawnLocation`.
    spawn_location: bool,
    /// Java `protected final boolean showOnMap`.
    show_on_map: bool,
}

impl UniquePrefabEntry {
    /// Java `private UniquePrefabEntry(String name, PrefabCategory category,
    /// Vector3i position, WorldGenPrefabSupplier prefabSupplier,
    /// BlockMaskCondition configuration, PrefabRotation rotation, Vector3d
    /// spawnOffset, int environmentId, boolean fitHeightmap, boolean
    /// submergeable, double exclusionRadiusSquared, boolean spawnLocation,
    /// boolean showOnMap)`.
    ///
    /// The Java constructor derives the bounds of the entry:
    ///
    /// ```java
    /// final var prefab = Objects.requireNonNull(prefabSupplier.get());
    /// final var bounds = prefabSupplier.getBounds(prefab);
    /// // Note: getBounds doesn't support the y-axis so this might not be accurate if child-prefabs are tall.
    /// this.lowBoundY = position.y + prefab.getMinY();
    /// this.highBoundY = position.y + prefab.getMaxY();
    /// this.lowBoundX = position.x + bounds.getLowBoundX(rotation);
    /// this.lowBoundZ = position.z + bounds.getLowBoundZ(rotation);
    /// this.highBoundX = position.x + bounds.getHighBoundX(rotation);
    /// this.highBoundZ = position.z + bounds.getHighBoundZ(rotation);
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        category: PrefabCategory,
        position: (i32, i32, i32),
        prefab_supplier: Rc<WorldGenPrefabSupplier>,
        configuration: BlockMaskCondition,
        rotation: PrefabRotation,
        spawn_offset: Vector3d,
        environment_id: i32,
        fit_heightmap: bool,
        submerge: bool,
        exclusion_radius_squared: f64,
        spawn_location: bool,
        show_on_map: bool,
    ) -> Self {
        let prefab = prefab_supplier.get();
        let access = prefab.new_access();
        let bounds = prefab_supplier.get_bounds();

        let low_bound_y = position.1.wrapping_add(access.get_min_y());
        let high_bound_y = position.1.wrapping_add(access.get_max_y());
        let low_bound_x = position
            .0
            .wrapping_add(bounds.get_low_bound_x_rotated(rotation));
        let low_bound_z = position
            .2
            .wrapping_add(bounds.get_low_bound_z_rotated(rotation));
        let high_bound_x = position
            .0
            .wrapping_add(bounds.get_high_bound_x_rotated(rotation));
        let high_bound_z = position
            .2
            .wrapping_add(bounds.get_high_bound_z_rotated(rotation));

        Self {
            name,
            category,
            position,
            prefab_supplier,
            configuration,
            rotation,
            spawn_offset,
            low_bound_x,
            low_bound_y,
            low_bound_z,
            high_bound_x,
            high_bound_y,
            high_bound_z,
            environment_id,
            fit_heightmap,
            submerge,
            exclusion_radius_squared,
            spawn_location,
            show_on_map,
        }
    }

    /// Java `String getName()`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Java `@Nonnull PrefabCategory getCategory()`.
    pub fn get_category(&self) -> &PrefabCategory {
        &self.category
    }

    /// Java `@Nonnull Vector3i getPosition()` — `(x, y, z)`.
    pub fn get_position(&self) -> (i32, i32, i32) {
        self.position
    }

    /// Java `@Nonnull WorldGenPrefabSupplier getPrefabSupplier()`.
    pub fn get_prefab_supplier(&self) -> &Rc<WorldGenPrefabSupplier> {
        &self.prefab_supplier
    }

    /// Java `BlockMaskCondition getConfiguration()`.
    pub fn get_configuration(&self) -> &BlockMaskCondition {
        &self.configuration
    }

    /// Java `@Nonnull PrefabRotation getRotation()`.
    pub fn get_rotation(&self) -> PrefabRotation {
        self.rotation
    }

    /// Java `Vector3d getSpawnOffset()`.
    pub fn get_spawn_offset(&self) -> Vector3d {
        self.spawn_offset
    }

    /// Java `int getLowBoundX()`.
    pub const fn get_low_bound_x(&self) -> i32 {
        self.low_bound_x
    }

    /// Java `int getLowBoundY()`.
    pub const fn get_low_bound_y(&self) -> i32 {
        self.low_bound_y
    }

    /// Java `int getLowBoundZ()`.
    pub const fn get_low_bound_z(&self) -> i32 {
        self.low_bound_z
    }

    /// Java `int getHighBoundX()`.
    pub const fn get_high_bound_x(&self) -> i32 {
        self.high_bound_x
    }

    /// Java `int getHighBoundY()`.
    pub const fn get_high_bound_y(&self) -> i32 {
        self.high_bound_y
    }

    /// Java `int getHighBoundZ()`.
    pub const fn get_high_bound_z(&self) -> i32 {
        self.high_bound_z
    }

    /// Java `int getEnvironmentId()`.
    pub const fn get_environment_id(&self) -> i32 {
        self.environment_id
    }

    /// Java `boolean isFitHeightmap()`.
    pub const fn is_fit_heightmap(&self) -> bool {
        self.fit_heightmap
    }

    /// Java `boolean isSubmerge()`.
    pub const fn is_submerge(&self) -> bool {
        self.submerge
    }

    /// Java `double getExclusionRadiusSquared()`.
    pub const fn get_exclusion_radius_squared(&self) -> f64 {
        self.exclusion_radius_squared
    }

    /// Java `boolean isSpawnLocation()`.
    pub const fn is_spawn_location(&self) -> bool {
        self.spawn_location
    }

    /// Java `boolean isShowOnMap()`.
    pub const fn is_show_on_map(&self) -> bool {
        self.show_on_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::test_prefab_store::test_prefab_loader;

    /// A supplier of the test prefab store.
    fn supplier() -> Rc<WorldGenPrefabSupplier> {
        let loader = test_prefab_loader();
        let mut suppliers = loader.get("A");
        assert_eq!(suppliers.len(), 1);
        Rc::new(suppliers.remove(0))
    }

    #[test]
    fn the_container_exposes_its_seed_offset_and_generators() {
        let container = UniquePrefabContainer::new(17, Vec::new());
        assert_eq!(container.get_seed_offset(), 17);
        assert!(container.get_generators().is_empty());
    }

    #[test]
    fn the_entry_derives_its_bounds_from_the_prefab() {
        let supplier = supplier();
        let entry = UniquePrefabEntry::new(
            "Spawn".to_string(),
            PrefabCategory::unique(),
            (100, 60, 200),
            Rc::clone(&supplier),
            BlockMaskCondition::new(),
            PrefabRotation::Rotation0,
            Vector3d::new(0.0, 3.0, 16.0),
            7,
            true,
            false,
            900.0,
            true,
            false,
        );

        // The test prefab is empty → its bounds are (0,0,0)..(0,0,0).
        assert_eq!(entry.get_low_bound_y(), 60);
        assert_eq!(entry.get_high_bound_y(), 60);
        assert_eq!(entry.get_low_bound_x(), 100);
        assert_eq!(entry.get_high_bound_x(), 100);
        assert_eq!(entry.get_low_bound_z(), 200);
        assert_eq!(entry.get_high_bound_z(), 200);

        // The other fields are carried as they are.
        assert_eq!(entry.get_name(), "Spawn");
        assert_eq!(entry.get_category().priority, PrefabCategory::MAX_PRIORITY);
        assert_eq!(entry.get_position(), (100, 60, 200));
        assert_eq!(entry.get_rotation(), PrefabRotation::Rotation0);
        assert_eq!(entry.get_spawn_offset(), Vector3d::new(0.0, 3.0, 16.0));
        assert_eq!(entry.get_environment_id(), 7);
        assert!(entry.is_fit_heightmap());
        assert!(!entry.is_submerge());
        assert_eq!(entry.get_exclusion_radius_squared(), 900.0);
        assert!(entry.is_spawn_location());
        assert!(!entry.is_show_on_map());
    }

    /* ============================================================================================================ */
    /*                                              the generation                                                  */
    /* ============================================================================================================ */

    use crate::loader::util::Vector2d;
    use crate::prefab::unique::unique_prefab_configuration::UniquePrefabConfiguration;

    /// A configuration with the given anchor, max distance and spawn flag.
    fn configuration(
        anchor: Vector2d,
        max_distance: f64,
        spawn_location: bool,
    ) -> UniquePrefabConfiguration {
        UniquePrefabConfiguration::new(
            None,
            BlockMaskCondition::new(),
            Some(vec![PrefabRotation::Rotation270]),
            Rc::new(procedural::noise::condition::AlwaysTrueIntCondition),
            Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True),
            Rc::new(procedural::noise::condition::ConstantBlockFluidCondition::True),
            anchor,
            Vector3d::new(0.0, 3.0, 16.0),
            max_distance,
            false,
            false,
            false,
            worldgen_model::chunk::ENVIRONMENT_NOT_SET,
            5000,
            50.0,
            spawn_location,
            25.0,
            false,
        )
    }

    /// A weighted map with a single prefab of the test store.
    fn prefab_map() -> Rc<dyn crate::weighted_map::IWeightedMap<WorldGenPrefabSupplier>> {
        let loader = test_prefab_loader();
        let mut suppliers = loader.get("A");
        assert_eq!(suppliers.len(), 1);
        let mut builder = crate::weighted_map::WeightedMap::<WorldGenPrefabSupplier>::builder();
        builder.put(suppliers.remove(0), 1.0);
        Rc::new(builder.build())
    }

    /// A container with a single generator.
    fn container(
        seed_offset: i32,
        max_distance: f64,
        spawn_location: bool,
    ) -> UniquePrefabContainer {
        UniquePrefabContainer::new(
            seed_offset,
            vec![UniquePrefabGenerator::new(
                "Spawn".to_string(),
                PrefabCategory::unique(),
                prefab_map(),
                configuration(Vector2d::new(0.0, 0.0), max_distance, spawn_location),
                3,
            )],
        )
    }

    #[test]
    fn the_generation_produces_one_entry_per_generator() {
        let generator = crate::chunk::chunk_generator::test_stubs::stub_generator();
        let container = container(0, 0.0, true);
        let entries = container.generate(7, None, &generator);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.get_name(), "Spawn");
        assert_eq!(entry.get_rotation(), PrefabRotation::Rotation270);
        assert!(entry.is_spawn_location());
        // The anchor is (0, 0) and the max distance is 0 → the candidate is
        // the anchor itself.
        assert_eq!(entry.get_position().0, 0);
        assert_eq!(entry.get_position().2, 0);
    }

    #[test]
    fn the_seed_offset_shifts_the_placement() {
        let generator = crate::chunk::chunk_generator::test_stubs::stub_generator();
        let first = container(0, 100.0, false).generate(7, None, &generator);
        let second = container(1234, 100.0, false).generate(7, None, &generator);
        assert_ne!(
            first[0].get_position(),
            second[0].get_position(),
            "the seed offset shifts the random stream of the placement"
        );
    }

    #[test]
    fn the_generation_is_deterministic() {
        let generator = crate::chunk::chunk_generator::test_stubs::stub_generator();
        let container = container(17, 100.0, true);
        let first = container.generate(7, None, &generator);
        let second = container.generate(7, None, &generator);
        assert_eq!(first[0].get_position(), second[0].get_position());
        assert_eq!(first[0].get_rotation(), second[0].get_rotation());
    }
}
