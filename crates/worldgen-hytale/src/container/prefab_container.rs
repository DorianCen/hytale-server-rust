// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/PrefabContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.PrefabContainer` — the
//! prefab catalogue of a biome.
//!
//! # Phase 5
//!
//! The prefab package landed: the entries carry the real
//! [`PrefabPatternGenerator`](crate::prefab::prefab_pattern_generator::PrefabPatternGenerator)
//! and the real prefab suppliers (the Java `WorldGenPrefabSupplier`).

use std::cell::Cell;
use std::rc::Rc;

use hytale_math::math_util;

use crate::prefab::prefab_pattern_generator::PrefabPatternGenerator;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::util::bounds::IChunkBounds;
use crate::util::list_pool::ListPool;
use crate::weighted_map::IWeightedMap;

thread_local! {
    /// Java `public static final ListPool<PrefabContainerEntry> ENTRY_POOL` =
    /// `new ListPool<>(10, new PrefabContainerEntry[0])`.
    ///
    /// Deviation: see [`crate::container::tint_container::ENTRY_POOL`] — the
    /// pooled entries hold `Rc<dyn …>` trait objects (not `Send`), so the
    /// pool is thread-local rather than a process-wide `static`.
    pub static ENTRY_POOL: &'static ListPool<PrefabContainerEntry> =
        Box::leak(Box::new(ListPool::new(10)));
}

/// Java `static class PrefabContainer.PrefabContainerEntry`.
pub struct PrefabContainerEntry {
    /// Java `protected final IWeightedMap<WorldGenPrefabSupplier> prefabs`.
    prefabs: Rc<dyn IWeightedMap<WorldGenPrefabSupplier>>,
    /// Java `protected final PrefabPatternGenerator prefabPatternGenerator`.
    prefab_pattern_generator: Rc<PrefabPatternGenerator>,
    /// Java `protected final int environmentId`.
    environment_id: i32,
    /// Java `protected int extend = -1` — the lazily computed extents of the
    /// prefabs of the entry (`-1` = "not computed yet").
    extend: Cell<i32>,
}

impl PrefabContainerEntry {
    /// Java `PrefabContainerEntry(IWeightedMap<WorldGenPrefabSupplier> prefabs,
    /// PrefabPatternGenerator prefabPatternGenerator, int environmentId)`.
    pub fn new(
        prefabs: Rc<dyn IWeightedMap<WorldGenPrefabSupplier>>,
        prefab_pattern_generator: Rc<PrefabPatternGenerator>,
        environment_id: i32,
    ) -> Self {
        Self {
            prefabs,
            prefab_pattern_generator,
            environment_id,
            extend: Cell::new(-1),
        }
    }

    /// Java `IWeightedMap<WorldGenPrefabSupplier> getPrefabs()`.
    pub fn get_prefabs(&self) -> &Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
        &self.prefabs
    }

    /// Java `int getEnvironmentId()`.
    pub const fn get_environment_id(&self) -> i32 {
        self.environment_id
    }

    /// Java `PrefabPatternGenerator getPrefabPatternGenerator()`.
    pub fn get_prefab_pattern_generator(&self) -> &Rc<PrefabPatternGenerator> {
        &self.prefab_pattern_generator
    }

    /// Java `PrefabPatternGenerator.getMaxSize()`.
    pub fn get_pattern_generator_max_size(&self) -> i32 {
        self.prefab_pattern_generator.get_max_size()
    }

    /// Java `int getExtents()` — the maximum extent of the prefabs of the
    /// entry.
    ///
    /// ```java
    /// public int getExtents() {
    ///     if (extend == -1) {
    ///         int max = 0;
    ///         for (var supplier : prefabs.internalKeys()) {
    ///             var bounds = supplier.getBounds(supplier.get());
    ///             int lengthX = bounds.getHighBoundX() - bounds.getLowBoundX();
    ///             int lengthZ = bounds.getHighBoundZ() - bounds.getLowBoundZ();
    ///             max = MathUtil.maxValue(max, lengthX, lengthZ);
    ///         }
    ///         extend = max;
    ///     }
    ///     return extend;
    /// }
    /// ```
    ///
    /// Deviation: the Java caches the value in the `extend` field; the Rust
    /// port caches it in a `Cell` (the entry is shared through an `Rc`).
    pub fn get_extents(&self) -> i32 {
        let cached = self.extend.get();
        if cached != -1 {
            return cached;
        }
        let mut max = 0;
        for supplier in self.prefabs.internal_keys() {
            let bounds = supplier.get_bounds();
            let length_x = bounds.get_high_bound_x() - bounds.get_low_bound_x();
            let length_z = bounds.get_high_bound_z() - bounds.get_low_bound_z();
            max = math_util::max_value3_i32(max, length_x, length_z);
        }
        self.extend.set(max);
        max
    }
}

/// Java `class PrefabContainer`.
pub struct PrefabContainer {
    /// Java `private final PrefabContainerEntry[] entries`.
    ///
    /// Deviation: the Java array holds *references* to the entries; the Rust
    /// port holds `Rc`s, so that the entries stay alive when a candidate
    /// refers to them (the Java `PrefabPopulator.Candidate` keeps the entry
    /// alive the same way).
    entries: Vec<Rc<PrefabContainerEntry>>,
    /// Java `private final int maxSize`.
    max_size: i32,
}

impl PrefabContainer {
    /// Java `PrefabContainer(PrefabContainerEntry[] entries)`.
    pub fn new(entries: Vec<Rc<PrefabContainerEntry>>) -> Self {
        let max_size = Self::compute_max_size(&entries);
        Self { entries, max_size }
    }

    /// Java `PrefabContainerEntry[] getEntries()`.
    pub fn get_entries(&self) -> &[Rc<PrefabContainerEntry>] {
        &self.entries
    }

    /// Java `int getMaxSize()`.
    pub const fn get_max_size(&self) -> i32 {
        self.max_size
    }

    /// Java `private static int getMaxSize(PrefabContainerEntry[] entries)`.
    fn compute_max_size(entries: &[Rc<PrefabContainerEntry>]) -> i32 {
        let mut max = 0;
        for entry in entries {
            max = max.max(entry.get_pattern_generator_max_size());
        }
        max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weighted_map::WeightedMap;

    /// A prefab pattern generator with the given maximum size.
    fn pattern_generator(max_size: i32) -> Rc<PrefabPatternGenerator> {
        Rc::new(PrefabPatternGenerator::new(
            0,
            crate::prefab::prefab_category::PrefabCategory::none(),
            None,
            Rc::new(procedural::noise::condition::DefaultCoordinateRndCondition::True),
            None,
            crate::util::condition::BlockMaskCondition::new(),
            Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True),
            Rc::new(procedural::noise::condition::ConstantBlockFluidCondition::True),
            None,
            Rc::new(crate::util::function::ConstantCoordinateDoubleSupplier::DEFAULT_ZERO),
            false,
            false,
            false,
            false,
            max_size,
            0,
            Vec::new(),
        ))
    }

    fn prefab_map() -> Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
        let mut builder = WeightedMap::<WorldGenPrefabSupplier>::builder();
        builder.put(
            WorldGenPrefabSupplier::new(
                Rc::new(crate::prefab::prefab_store::PrefabStore::new(
                    std::path::PathBuf::from("/tmp"),
                    Rc::new(worldgen_model::registry::Registries::empty()),
                )),
                "key",
                std::path::PathBuf::from("/tmp/Foo.prefab.json"),
            ),
            1.0,
        );
        Rc::new(builder.build())
    }

    fn entry(max_size: i32, environment_id: i32) -> Rc<PrefabContainerEntry> {
        Rc::new(PrefabContainerEntry::new(
            prefab_map(),
            pattern_generator(max_size),
            environment_id,
        ))
    }

    #[test]
    fn the_max_size_folds_over_the_entries() {
        let container = PrefabContainer::new(vec![entry(3, 0), entry(11, 1), entry(7, 2)]);
        assert_eq!(container.get_max_size(), 11);
    }

    #[test]
    fn an_empty_container_has_a_max_size_of_zero() {
        let container = PrefabContainer::new(Vec::new());
        assert_eq!(container.get_max_size(), 0);
        assert!(container.get_entries().is_empty());
    }

    #[test]
    fn negative_sizes_do_not_lower_the_fold() {
        // Java: `max = Math.max(max, entry.getMaxSize())` starting at 0.
        let container = PrefabContainer::new(vec![entry(-5, 0)]);
        assert_eq!(container.get_max_size(), 0);
    }

    #[test]
    fn the_entry_exposes_its_environment_and_prefabs() {
        let entry = entry(4, 9);
        assert_eq!(entry.get_environment_id(), 9);
        assert_eq!(entry.get_pattern_generator_max_size(), 4);
        assert_eq!(entry.get_prefabs().size(), 1);
    }

    #[test]
    fn the_entry_pool_yields_a_usable_list() {
        let mut resource = ENTRY_POOL.with(|pool| pool.acquire());
        resource.push(PrefabContainerEntry::new(
            prefab_map(),
            pattern_generator(1),
            0,
        ));
        assert_eq!(resource.len(), 1);
    }
}
