// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/WaterContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.WaterContainer` — the
//! per-column water level of a zone.
//!
//! Every entry describes a water *band* (a minimum and a maximum height) and
//! the block/fluid pair used for it. The container reports the highest
//! water level of all entries that cover the column.

use std::rc::Rc;

use hytale_math::math_util;
use procedural::noise::condition::ICoordinateCondition;
use procedural::noise::supplier::IDoubleCoordinateSupplier;

use crate::util::list_pool::ListPool;

thread_local! {
    /// Java `public static final ListPool<Entry> ENTRY_POOL` =
    /// `new ListPool<>(10, new WaterContainer.Entry[0])`.
    ///
    /// Deviation: see [`crate::container::tint_container::ENTRY_POOL`] — the
    /// pooled entries hold `Rc<dyn …>` trait objects (not `Send`), so the
    /// pool is thread-local rather than a process-wide `static`.
    pub static ENTRY_POOL: &'static ListPool<Entry> = Box::leak(Box::new(ListPool::new(10)));
}

/// Java `class WaterContainer`.
pub struct WaterContainer {
    /// Java `private final Entry[] entries`.
    entries: Vec<Entry>,
}

impl WaterContainer {
    /// Java `public static final int NO_WATER_AT_COORDINATED` =
    /// `Integer.MIN_VALUE`.
    pub const NO_WATER_AT_COORDINATED: i32 = i32::MIN;

    /// Java `public static boolean isValidWaterHeight(int height)`.
    pub const fn is_valid_water_height(height: i32) -> bool {
        height > 0
    }

    /// Java `WaterContainer(Entry[] entries)`.
    pub fn new(entries: Vec<Entry>) -> Self {
        Self { entries }
    }

    /// Java `boolean hasEntries()`.
    pub fn has_entries(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Java `Entry[] getEntries()`.
    pub fn get_entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Java `int getMaxHeight(int seed, int x, int z)`.
    pub fn get_max_height(&self, seed: i32, x: i32, z: i32) -> i32 {
        let mut total_max = Self::NO_WATER_AT_COORDINATED;
        for entry in &self.entries {
            let min = entry.get_min(seed, x, z);
            let max = entry.get_max(seed, x, z);
            if min > max {
                continue;
            }
            if max > total_max {
                total_max = max;
            }
        }
        if total_max < 0 {
            return Self::NO_WATER_AT_COORDINATED;
        }
        total_max
    }
}

/// Java `static class WaterContainer.Entry`.
pub struct Entry {
    /// Java `private final int block`.
    block: i32,
    /// Java `private final int fluid`.
    fluid: i32,
    /// Java `private final IDoubleCoordinateSupplier min`.
    min: Rc<dyn IDoubleCoordinateSupplier>,
    /// Java `private final IDoubleCoordinateSupplier max`.
    max: Rc<dyn IDoubleCoordinateSupplier>,
    /// Java `private final ICoordinateCondition mask`.
    mask: Rc<dyn ICoordinateCondition>,
}

impl Entry {
    /// Java `public static final Entry[] EMPTY_ARRAY`.
    pub const EMPTY_ARRAY: [Entry; 0] = [];

    /// Java `Entry(int block, int fluid, IDoubleCoordinateSupplier min,
    /// IDoubleCoordinateSupplier max, ICoordinateCondition mask)`.
    pub fn new(
        block: i32,
        fluid: i32,
        min: Rc<dyn IDoubleCoordinateSupplier>,
        max: Rc<dyn IDoubleCoordinateSupplier>,
        mask: Rc<dyn ICoordinateCondition>,
    ) -> Self {
        Self {
            block,
            fluid,
            min,
            max,
            mask,
        }
    }

    /// Java `int getBlock()`.
    pub const fn get_block(&self) -> i32 {
        self.block
    }

    /// Java `int getFluid()`.
    pub const fn get_fluid(&self) -> i32 {
        self.fluid
    }

    /// Java `int getMax(int seed, int x, int z)`.
    pub fn get_max(&self, seed: i32, x: i32, z: i32) -> i32 {
        math_util::floor(self.max.get2(seed, x as f64, z as f64))
    }

    /// Java `int getMin(int seed, int x, int z)`.
    pub fn get_min(&self, seed: i32, x: i32, z: i32) -> i32 {
        math_util::floor(self.min.get2(seed, x as f64, z as f64))
    }

    /// Java `boolean shouldPopulate(int seed, int x, int z)`.
    pub fn should_populate(&self, seed: i32, x: i32, z: i32) -> bool {
        // Java: mask.eval(seed, x, z) → the 3-argument (2d) overload.
        self.mask.eval2(seed, x, z)
    }

    /// Java `IDoubleCoordinateSupplier getMax()`.
    ///
    /// Deviation: Java overloads `getMax()` (the supplier) and `getMax(int,
    /// int, int)` (the floored height). Rust has no method overloading, so
    /// the supplier accessor is named `get_max_supplier`.
    pub fn get_max_supplier(&self) -> &Rc<dyn IDoubleCoordinateSupplier> {
        &self.max
    }

    /// Java `IDoubleCoordinateSupplier getMin()`.
    ///
    /// Deviation: see [`Self::get_max_supplier`] — the supplier accessor is
    /// named `get_min_supplier`.
    pub fn get_min_supplier(&self) -> &Rc<dyn IDoubleCoordinateSupplier> {
        &self.min
    }

    /// Java `ICoordinateCondition getMask()`.
    ///
    /// Deviation: Java has no getter; the Rust port exposes the mask so the
    /// loaders/tests can inspect it.
    pub fn get_mask(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::condition::DefaultCoordinateCondition;

    /// Test stub: a constant double supplier.
    struct ConstantSupplier(f64);

    impl IDoubleCoordinateSupplier for ConstantSupplier {
        fn get2(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }
        fn get3(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    fn entry(block: i32, fluid: i32, min: f64, max: f64) -> Entry {
        Entry::new(
            block,
            fluid,
            Rc::new(ConstantSupplier(min)),
            Rc::new(ConstantSupplier(max)),
            Rc::new(DefaultCoordinateCondition::True),
        )
    }

    #[test]
    fn the_max_height_is_the_highest_valid_band() {
        let container = WaterContainer::new(vec![
            entry(1, 0, 10.0, 20.0),
            entry(2, 3, 5.0, 30.0),
            // Inverted band (min > max) → skipped by `getMaxHeight`.
            entry(4, 0, 50.0, 40.0),
        ]);
        assert_eq!(container.get_max_height(0, 0, 0), 30);
    }

    #[test]
    fn inverted_bands_are_skipped() {
        // min > max → the entry is skipped entirely.
        let container = WaterContainer::new(vec![entry(1, 0, 30.0, 20.0)]);
        assert_eq!(
            container.get_max_height(0, 0, 0),
            WaterContainer::NO_WATER_AT_COORDINATED
        );
    }

    #[test]
    fn an_empty_container_has_no_water() {
        let container = WaterContainer::new(Vec::new());
        assert!(!container.has_entries());
        assert_eq!(
            container.get_max_height(0, 0, 0),
            WaterContainer::NO_WATER_AT_COORDINATED
        );
        assert!(container.get_entries().is_empty());
    }

    #[test]
    fn a_zero_height_band_is_reported_as_zero() {
        // Java: `if (totalMax < 0) return NO_WATER_AT_COORDINATED;` — a band
        // that ends at 0 is *not* negative, so 0 is returned.
        let container = WaterContainer::new(vec![entry(1, 0, -5.0, 0.0)]);
        assert_eq!(container.get_max_height(0, 0, 0), 0);
    }

    #[test]
    fn the_band_heights_are_floored() {
        let entry = entry(1, 2, 10.9, 20.9);
        assert_eq!(entry.get_min(0, 0, 0), 10);
        assert_eq!(entry.get_max(0, 0, 0), 20);
        assert_eq!(entry.get_block(), 1);
        assert_eq!(entry.get_fluid(), 2);
    }

    #[test]
    fn only_positive_heights_are_valid_water_heights() {
        assert!(WaterContainer::is_valid_water_height(1));
        assert!(!WaterContainer::is_valid_water_height(0));
        assert!(!WaterContainer::is_valid_water_height(-1));
    }

    #[test]
    fn the_empty_array_constant_is_empty() {
        assert!(Entry::EMPTY_ARRAY.is_empty());
    }

    #[test]
    fn the_entry_pool_yields_a_usable_list() {
        let mut resource = ENTRY_POOL.with(|pool| pool.acquire());
        resource.push(entry(1, 0, 0.0, 10.0));
        assert_eq!(resource.len(), 1);
    }
}
