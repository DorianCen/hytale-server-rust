// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/TintContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.TintContainer` — the
//! per-column tint (colour) mapping of a zone.
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
    /// Java `public static final ListPool<TintContainerEntry> ENTRY_POOL` =
    /// `new ListPool<>(10, new TintContainerEntry[0])`.
    ///
    /// Deviation: Java's pool is a process-wide static. The Rust entries
    /// hold `Rc<dyn …>` trait objects, which are not `Send`, so a global
    /// `static` cannot be `Sync` (a `static` requires `Sync`). The pool is
    /// therefore declared `thread_local`, matching the "one scratch pool per
    /// worker thread" model of `PORTING.md`. The pool instance itself is
    /// leaked once per thread (it holds only empty lists) so that the
    /// `ListPool::Resource` drop-guard can borrow it for an arbitrary
    /// lifetime.
    pub static ENTRY_POOL: &'static ListPool<TintContainerEntry> =
        Box::leak(Box::new(ListPool::new(10)));
}

/// Java `class TintContainer`.
pub struct TintContainer {
    /// Java `private final DefaultTintContainerEntry defaultEntry`.
    default_entry: DefaultTintContainerEntry,
    /// Java `private final List<TintContainerEntry> entries`.
    entries: Vec<TintContainerEntry>,
}

impl TintContainer {
    /// Java `TintContainer(DefaultTintContainerEntry defaultEntry, List<
    /// TintContainerEntry> entries)`.
    pub fn new(default_entry: DefaultTintContainerEntry, entries: Vec<TintContainerEntry>) -> Self {
        Self {
            default_entry,
            entries,
        }
    }

    /// Java `DefaultTintContainerEntry getDefaultEntry()`.
    ///
    /// Deviation: Java has no getter for the default entry; the Rust port
    /// exposes it because the default entry is part of the container's
    /// observable state (the JSON loaders build it).
    pub fn get_default_entry(&self) -> &DefaultTintContainerEntry {
        &self.default_entry
    }

    /// Java `List<TintContainerEntry> getEntries()`.
    ///
    /// Deviation: Java has no getter for the entry list; the Rust port
    /// exposes it for the same reason as [`Self::get_default_entry`].
    pub fn get_entries(&self) -> &[TintContainerEntry] {
        &self.entries
    }

    /// Java `int getTintColorAt(int seed, int x, int z)`.
    pub fn get_tint_color_at(&self, seed: i32, x: i32, z: i32) -> i32 {
        for entry in &self.entries {
            if entry.should_generate(seed, x, z) {
                return entry.get_tint_color_at(seed, x, z);
            }
        }
        self.default_entry.get_tint_color_at(seed, x, z)
    }
}

/// Java `static class TintContainer.TintContainerEntry`.
pub struct TintContainerEntry {
    /// Java `private final IWeightedMap<Integer> colorMapping`.
    color_mapping: Rc<dyn IWeightedMap<i32>>,
    /// Java `private final NoiseProperty valueNoise`.
    value_noise: Rc<dyn NoiseProperty>,
    /// Java `private final ICoordinateCondition mapCondition`.
    map_condition: Rc<dyn ICoordinateCondition>,
}

impl TintContainerEntry {
    /// Java `TintContainerEntry(IWeightedMap<Integer> colorMapping,
    /// NoiseProperty valueNoise, ICoordinateCondition mapCondition)`.
    pub fn new(
        color_mapping: Rc<dyn IWeightedMap<i32>>,
        value_noise: Rc<dyn NoiseProperty>,
        map_condition: Rc<dyn ICoordinateCondition>,
    ) -> Self {
        Self {
            color_mapping,
            value_noise,
            map_condition,
        }
    }

    /// Java `IWeightedMap<Integer> getColorMapping()`.
    ///
    /// Deviation: Java has no getter; the Rust port exposes the mapping so
    /// the loaders/tests can inspect it.
    pub fn get_color_mapping(&self) -> &Rc<dyn IWeightedMap<i32>> {
        &self.color_mapping
    }

    /// Java `NoiseProperty getValueNoise()`.
    ///
    /// Deviation: Java has no getter; see [`Self::get_color_mapping`].
    pub fn get_value_noise(&self) -> &Rc<dyn NoiseProperty> {
        &self.value_noise
    }

    /// Java `ICoordinateCondition getMapCondition()`.
    ///
    /// Deviation: Java has no getter; see [`Self::get_color_mapping`].
    pub fn get_map_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.map_condition
    }

    /// Java `boolean shouldGenerate(int seed, int x, int z)`.
    pub fn should_generate(&self, seed: i32, x: i32, z: i32) -> bool {
        // Java: mapCondition.eval(seed, x, z) → the 3-argument (2d) overload.
        self.map_condition.eval2(seed, x, z)
    }

    /// Java `int getTintColorAt(int seed, int x, int z)`.
    ///
    /// ```java
    /// return colorMapping.get(seed, x, z, (iSeed, ix, iz, entry) -> entry.valueNoise.get(iSeed, ix, iz), this);
    /// ```
    ///
    /// The Java 5-argument `IWeightedMap.get` collapses to
    /// `get(supplier.apply(seed, x, z, k))`, i.e. the weighted map is queried
    /// with the raw noise value.
    pub fn get_tint_color_at(&self, seed: i32, x: i32, z: i32) -> i32 {
        let value = self.value_noise.get(seed, x as f64, z as f64);
        // Java auto-unboxes the `@Nullable Integer`; an empty mapping would
        // raise a NullPointerException there. The Rust port substitutes the
        // neutral colour `0` instead of panicking.
        self.color_mapping.get(value).copied().unwrap_or(0)
    }
}

/// Java `static class TintContainer.DefaultTintContainerEntry extends
/// TintContainerEntry`.
pub struct DefaultTintContainerEntry {
    /// Java `super(...)` — the inherited entry state.
    pub inner: TintContainerEntry,
}

impl DefaultTintContainerEntry {
    /// Java `DefaultTintContainerEntry(IWeightedMap<Integer> colorMapping,
    /// NoiseProperty valueNoise)`.
    ///
    /// The map condition is Java's
    /// `DefaultCoordinateCondition.DEFAULT_TRUE`.
    pub fn new(
        color_mapping: Rc<dyn IWeightedMap<i32>>,
        value_noise: Rc<dyn NoiseProperty>,
    ) -> Self {
        Self {
            inner: TintContainerEntry::new(
                color_mapping,
                value_noise,
                Rc::new(DefaultCoordinateCondition::True),
            ),
        }
    }
}

impl Deref for DefaultTintContainerEntry {
    type Target = TintContainerEntry;

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

    fn color_map(colors: &[(i32, f64)]) -> Rc<dyn IWeightedMap<i32>> {
        let mut builder = WeightedMap::<i32>::builder();
        for (color, weight) in colors {
            builder.put(*color, *weight);
        }
        Rc::new(builder.build())
    }

    fn entry(
        colors: &[(i32, f64)],
        noise: Rc<dyn NoiseProperty>,
        condition: Rc<dyn ICoordinateCondition>,
    ) -> TintContainerEntry {
        TintContainerEntry::new(color_map(colors), noise, condition)
    }

    #[test]
    fn the_first_matching_entry_wins() {
        let default_entry = DefaultTintContainerEntry::new(color_map(&[(7, 1.0)]), default_zero());
        let container = TintContainer::new(
            default_entry,
            vec![
                entry(&[(1, 1.0)], default_zero(), Rc::new(XGreaterThan(100))),
                entry(&[(2, 1.0)], default_zero(), Rc::new(XGreaterThan(10))),
                entry(&[(3, 1.0)], default_zero(), Rc::new(XGreaterThan(0))),
            ],
        );
        // x = 5 matches only the third entry (x > 0).
        assert_eq!(container.get_tint_color_at(0, 5, 0), 3);
        // x = 50 matches the second and the third entry → the second wins.
        assert_eq!(container.get_tint_color_at(0, 50, 0), 2);
        // x = 500 matches every entry → the first wins.
        assert_eq!(container.get_tint_color_at(0, 500, 0), 1);
    }

    #[test]
    fn the_default_entry_is_used_when_no_entry_matches() {
        let default_entry = DefaultTintContainerEntry::new(color_map(&[(9, 1.0)]), default_zero());
        let container = TintContainer::new(
            default_entry,
            vec![entry(
                &[(1, 1.0)],
                default_zero(),
                Rc::new(XGreaterThan(100)),
            )],
        );
        assert_eq!(container.get_tint_color_at(0, 0, 0), 9);
    }

    #[test]
    fn the_tint_color_is_looked_up_in_the_weighted_map() {
        // A single-key map ignores the sampled value (Java's
        // SingletonWeightedMap), so the constant colour is always returned.
        let default_entry = DefaultTintContainerEntry::new(color_map(&[(4, 1.0)]), default_one());
        let container = TintContainer::new(default_entry, Vec::new());
        assert_eq!(container.get_tint_color_at(0, 0, 0), 4);
    }

    #[test]
    fn the_sampled_noise_value_selects_the_colour() {
        // Two colours with equal weights: the first one is picked while the
        // noise value is below 0.5, the second one above.
        let mapping = color_map(&[(1, 1.0), (2, 1.0)]);
        let entry = TintContainerEntry::new(
            mapping,
            default_zero(), // evaluates to 0.5
            Rc::new(DefaultCoordinateCondition::True),
        );
        assert_eq!(entry.get_tint_color_at(0, 0, 0), 1);
        // 0.5 * sum(2.0) = 1.0 → 1.0 - 1.0 = 0.0 <= ONE_MINUS_EPSILON → key 2.
        let mapping = color_map(&[(1, 1.0), (2, 1.0)]);
        let entry = TintContainerEntry::new(
            mapping,
            default_one(), // evaluates to 1.0
            Rc::new(DefaultCoordinateCondition::True),
        );
        assert_eq!(entry.get_tint_color_at(0, 0, 0), 2);
    }

    #[test]
    fn the_default_entry_always_matches() {
        let default_entry = DefaultTintContainerEntry::new(color_map(&[(5, 1.0)]), default_zero());
        assert!(default_entry.should_generate(0, -100, -100));
        assert_eq!(default_entry.get_tint_color_at(0, 0, 0), 5);
    }

    #[test]
    fn the_entry_pool_yields_a_usable_list() {
        let mut resource = ENTRY_POOL.with(|pool| pool.acquire());
        resource.push(TintContainerEntry::new(
            color_map(&[(1, 1.0)]),
            default_zero(),
            Rc::new(DefaultCoordinateCondition::True),
        ));
        assert_eq!(resource.len(), 1);
    }
}
