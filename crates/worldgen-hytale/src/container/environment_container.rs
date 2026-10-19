// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/EnvironmentContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.EnvironmentContainer` —
//! the per-column environment (light/weather) mapping of a zone.
//!
//! The container holds an ordered list of entries; the first entry whose
//! *map condition* matches the column wins. When no entry matches, the
//! *default* entry (which always matches) is used.

use std::ops::Deref;
use std::rc::Rc;

use procedural::noise::condition::{DefaultCoordinateCondition, ICoordinateCondition};
use procedural::noise::support::NoiseProperty;

use crate::util::list_pool::ListPool;
use crate::weighted_map::IWeightedMap;

thread_local! {
    /// Java `public static final ListPool<EnvironmentContainerEntry> ENTRY_POOL`
    /// = `new ListPool<>(10, new EnvironmentContainerEntry[0])`.
    ///
    /// Deviation: see [`crate::container::tint_container::ENTRY_POOL`] — the
    /// pooled entries hold `Rc<dyn …>` trait objects (not `Send`), so the
    /// pool is thread-local rather than a process-wide `static`.
    pub static ENTRY_POOL: &'static ListPool<EnvironmentContainerEntry> =
        Box::leak(Box::new(ListPool::new(10)));
}

/// Java `class EnvironmentContainer`.
pub struct EnvironmentContainer {
    /// Java `protected final DefaultEnvironmentContainerEntry defaultEntry`.
    default_entry: DefaultEnvironmentContainerEntry,
    /// Java `protected final EnvironmentContainerEntry[] entries`.
    entries: Vec<EnvironmentContainerEntry>,
}

impl EnvironmentContainer {
    /// Java `EnvironmentContainer(DefaultEnvironmentContainerEntry defaultEntry,
    /// EnvironmentContainerEntry[] entries)`.
    pub fn new(
        default_entry: DefaultEnvironmentContainerEntry,
        entries: Vec<EnvironmentContainerEntry>,
    ) -> Self {
        Self {
            default_entry,
            entries,
        }
    }

    /// Java `DefaultEnvironmentContainerEntry getDefaultEntry()`.
    ///
    /// Deviation: Java has no getter for the default entry; the Rust port
    /// exposes it because the default entry is part of the container's
    /// observable state (the JSON loaders build it).
    pub fn get_default_entry(&self) -> &DefaultEnvironmentContainerEntry {
        &self.default_entry
    }

    /// Java `EnvironmentContainerEntry[] getEntries()`.
    ///
    /// Deviation: Java has no getter for the entry array; the Rust port
    /// exposes it for the same reason as [`Self::get_default_entry`].
    pub fn get_entries(&self) -> &[EnvironmentContainerEntry] {
        &self.entries
    }

    /// Java `int getEnvironmentAt(int seed, int x, int z)`.
    pub fn get_environment_at(&self, seed: i32, x: i32, z: i32) -> i32 {
        for entry in &self.entries {
            if entry.should_generate(seed, x, z) {
                return entry.get_environment_at(seed, x, z);
            }
        }
        self.default_entry.get_environment_at(seed, x, z)
    }
}

/// Java `static class EnvironmentContainer.EnvironmentContainerEntry`.
pub struct EnvironmentContainerEntry {
    /// Java `protected final IWeightedMap<Integer> environmentMapping`.
    environment_mapping: Rc<dyn IWeightedMap<i32>>,
    /// Java `protected final NoiseProperty valueNoise`.
    value_noise: Rc<dyn NoiseProperty>,
    /// Java `protected final ICoordinateCondition mapCondition`.
    map_condition: Rc<dyn ICoordinateCondition>,
}

impl EnvironmentContainerEntry {
    /// Java `public static final EnvironmentContainerEntry[] EMPTY_ARRAY`.
    pub const EMPTY_ARRAY: [EnvironmentContainerEntry; 0] = [];

    /// Java `EnvironmentContainerEntry(IWeightedMap<Integer> environmentMapping,
    /// NoiseProperty valueNoise, ICoordinateCondition mapCondition)`.
    pub fn new(
        environment_mapping: Rc<dyn IWeightedMap<i32>>,
        value_noise: Rc<dyn NoiseProperty>,
        map_condition: Rc<dyn ICoordinateCondition>,
    ) -> Self {
        Self {
            environment_mapping,
            value_noise,
            map_condition,
        }
    }

    /// Java `IWeightedMap<Integer> getEnvironmentMapping()`.
    ///
    /// Deviation: Java has no getter; the Rust port exposes the mapping so
    /// the loaders/tests can inspect it.
    pub fn get_environment_mapping(&self) -> &Rc<dyn IWeightedMap<i32>> {
        &self.environment_mapping
    }

    /// Java `NoiseProperty getValueNoise()`.
    ///
    /// Deviation: Java has no getter; see [`Self::get_environment_mapping`].
    pub fn get_value_noise(&self) -> &Rc<dyn NoiseProperty> {
        &self.value_noise
    }

    /// Java `ICoordinateCondition getMapCondition()`.
    ///
    /// Deviation: Java has no getter; see [`Self::get_environment_mapping`].
    pub fn get_map_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.map_condition
    }

    /// Java `boolean shouldGenerate(int seed, int x, int z)`.
    pub fn should_generate(&self, seed: i32, x: i32, z: i32) -> bool {
        // Java: mapCondition.eval(seed, x, z) → the 3-argument (2d) overload.
        self.map_condition.eval2(seed, x, z)
    }

    /// Java `int getEnvironmentAt(int seed, int x, int z)`.
    ///
    /// ```java
    /// return environmentMapping.get(seed, x, z, (iSeed, ix, iz, entry) -> entry.valueNoise.get(iSeed, ix, iz), this);
    /// ```
    ///
    /// The Java 5-argument `IWeightedMap.get` collapses to
    /// `get(supplier.apply(seed, x, z, k))`, i.e. the weighted map is queried
    /// with the raw noise value.
    pub fn get_environment_at(&self, seed: i32, x: i32, z: i32) -> i32 {
        let value = self.value_noise.get(seed, x as f64, z as f64);
        // Java auto-unboxes the `@Nullable Integer`; an empty mapping would
        // raise a NullPointerException there. The Rust port substitutes the
        // neutral environment id `0` instead of panicking.
        self.environment_mapping.get(value).copied().unwrap_or(0)
    }
}

/// Java `static class EnvironmentContainer.DefaultEnvironmentContainerEntry
/// extends EnvironmentContainerEntry`.
pub struct DefaultEnvironmentContainerEntry {
    /// Java `super(...)` — the inherited entry state.
    pub inner: EnvironmentContainerEntry,
}

impl DefaultEnvironmentContainerEntry {
    /// Java `DefaultEnvironmentContainerEntry(IWeightedMap<Integer>
    /// environmentMapping, NoiseProperty valueNoise)`.
    ///
    /// The map condition is Java's
    /// `DefaultCoordinateCondition.DEFAULT_TRUE`.
    pub fn new(
        environment_mapping: Rc<dyn IWeightedMap<i32>>,
        value_noise: Rc<dyn NoiseProperty>,
    ) -> Self {
        Self {
            inner: EnvironmentContainerEntry::new(
                environment_mapping,
                value_noise,
                Rc::new(DefaultCoordinateCondition::True),
            ),
        }
    }
}

impl Deref for DefaultEnvironmentContainerEntry {
    type Target = EnvironmentContainerEntry;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::constant_noise_property::{default_one, default_zero};
    use crate::weighted_map::WeightedMap;

    /// Test stub: matches when `x` is strictly greater than the threshold.
    struct XGreaterThan(i32);

    impl ICoordinateCondition for XGreaterThan {
        fn eval2(&self, _seed: i32, x: i32, _y: i32) -> bool {
            x > self.0
        }
        fn eval3(&self, _seed: i32, x: i32, _y: i32, _z: i32) -> bool {
            x > self.0
        }
    }

    fn environment_map(environments: &[(i32, f64)]) -> Rc<dyn IWeightedMap<i32>> {
        let mut builder = WeightedMap::<i32>::builder();
        for (environment, weight) in environments {
            builder.put(*environment, *weight);
        }
        Rc::new(builder.build())
    }

    fn entry(
        environments: &[(i32, f64)],
        noise: Rc<dyn NoiseProperty>,
        condition: Rc<dyn ICoordinateCondition>,
    ) -> EnvironmentContainerEntry {
        EnvironmentContainerEntry::new(environment_map(environments), noise, condition)
    }

    #[test]
    fn the_first_matching_entry_wins() {
        let default_entry =
            DefaultEnvironmentContainerEntry::new(environment_map(&[(7, 1.0)]), default_zero());
        let container = EnvironmentContainer::new(
            default_entry,
            vec![
                entry(&[(1, 1.0)], default_zero(), Rc::new(XGreaterThan(100))),
                entry(&[(2, 1.0)], default_zero(), Rc::new(XGreaterThan(10))),
                entry(&[(3, 1.0)], default_zero(), Rc::new(XGreaterThan(0))),
            ],
        );
        assert_eq!(container.get_environment_at(0, 5, 0), 3);
        assert_eq!(container.get_environment_at(0, 50, 0), 2);
        assert_eq!(container.get_environment_at(0, 500, 0), 1);
    }

    #[test]
    fn the_default_entry_is_used_when_no_entry_matches() {
        let default_entry =
            DefaultEnvironmentContainerEntry::new(environment_map(&[(9, 1.0)]), default_zero());
        let container = EnvironmentContainer::new(
            default_entry,
            vec![entry(
                &[(1, 1.0)],
                default_zero(),
                Rc::new(XGreaterThan(100)),
            )],
        );
        assert_eq!(container.get_environment_at(0, 0, 0), 9);
    }

    #[test]
    fn the_sampled_noise_value_selects_the_environment() {
        let mapping = environment_map(&[(1, 1.0), (2, 1.0)]);
        let entry = EnvironmentContainerEntry::new(
            mapping,
            default_zero(), // evaluates to 0.5
            Rc::new(DefaultCoordinateCondition::True),
        );
        assert_eq!(entry.get_environment_at(0, 0, 0), 1);

        let mapping = environment_map(&[(1, 1.0), (2, 1.0)]);
        let entry = EnvironmentContainerEntry::new(
            mapping,
            default_one(), // evaluates to 1.0
            Rc::new(DefaultCoordinateCondition::True),
        );
        assert_eq!(entry.get_environment_at(0, 0, 0), 2);
    }

    #[test]
    fn the_default_entry_always_matches() {
        let default_entry =
            DefaultEnvironmentContainerEntry::new(environment_map(&[(5, 1.0)]), default_zero());
        assert!(default_entry.should_generate(0, -100, -100));
        assert_eq!(default_entry.get_environment_at(0, 0, 0), 5);
    }

    #[test]
    fn the_empty_array_constant_is_empty() {
        assert!(EnvironmentContainerEntry::EMPTY_ARRAY.is_empty());
    }

    #[test]
    fn the_entry_pool_yields_a_usable_list() {
        let mut resource = ENTRY_POOL.with(|pool| pool.acquire());
        resource.push(EnvironmentContainerEntry::new(
            environment_map(&[(1, 1.0)]),
            default_zero(),
            Rc::new(DefaultCoordinateCondition::True),
        ));
        assert_eq!(resource.len(), 1);
    }
}
