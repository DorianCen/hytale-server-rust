// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/CoverContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.CoverContainer` — the
//! per-column cover (grass/flower/… decoration) rules of a zone.
//!
//! Every entry owns a weighted map of *cover parts*; the part is picked with
//! a random draw. The entry additionally carries the conditions that decide
//! whether the cover is placed at all and how dense it is.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::noise::condition::{
    IBlockFluidCondition, ICoordinateCondition, ICoordinateRndCondition,
};

use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::list_pool::ListPool;
use crate::weighted_map::IWeightedMap;

thread_local! {
    /// Java `public static final ListPool<CoverContainerEntry> ENTY_POOL` =
    /// `new ListPool<>(10, new CoverContainerEntry[0])`.
    ///
    /// Deviation: the Java field name is misspelled (`ENTY_POOL`); the Rust
    /// port uses the corrected `ENTRY_POOL`. The pool is thread-local
    /// because the pooled entries hold `Rc<dyn …>` trait objects (not
    /// `Send`) — see [`crate::container::tint_container::ENTRY_POOL`].
    pub static ENTRY_POOL: &'static ListPool<CoverContainerEntry> =
        Box::leak(Box::new(ListPool::new(10)));
}

/// Java `class CoverContainer`.
pub struct CoverContainer {
    /// Java `protected final CoverContainerEntry[] entries`.
    entries: Vec<CoverContainerEntry>,
}

impl CoverContainer {
    /// Java `CoverContainer(CoverContainerEntry[] entries)`.
    pub fn new(entries: Vec<CoverContainerEntry>) -> Self {
        Self { entries }
    }

    /// Java `CoverContainerEntry[] getEntries()`.
    pub fn get_entries(&self) -> &[CoverContainerEntry] {
        &self.entries
    }
}

/// Java `static class CoverContainer.CoverContainerEntry`.
pub struct CoverContainerEntry {
    /// Java `protected final IWeightedMap<CoverContainerEntryPart> entries`.
    entries: Rc<dyn IWeightedMap<CoverContainerEntryPart>>,
    /// Java `protected final ICoordinateCondition mapCondition`.
    map_condition: Rc<dyn ICoordinateCondition>,
    /// Java `protected final ICoordinateRndCondition heightCondition`.
    height_condition: Rc<dyn ICoordinateRndCondition>,
    /// Java `protected final IBlockFluidCondition parentCondition`.
    parent_condition: Rc<dyn IBlockFluidCondition>,
    /// Java `protected final double coverDensity`.
    cover_density: f64,
    /// Java `protected final boolean onWater`.
    on_water: bool,
}

impl CoverContainerEntry {
    /// Java `CoverContainerEntry(IWeightedMap<CoverContainerEntryPart> entries,
    /// ICoordinateCondition mapCondition, ICoordinateRndCondition
    /// heightCondition, IBlockFluidCondition parentCondition, double
    /// coverDensity, boolean onWater)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        entries: Rc<dyn IWeightedMap<CoverContainerEntryPart>>,
        map_condition: Rc<dyn ICoordinateCondition>,
        height_condition: Rc<dyn ICoordinateRndCondition>,
        parent_condition: Rc<dyn IBlockFluidCondition>,
        cover_density: f64,
        on_water: bool,
    ) -> Self {
        Self {
            entries,
            map_condition,
            height_condition,
            parent_condition,
            cover_density,
            on_water,
        }
    }

    /// Java `CoverContainerEntryPart get(Random random)`.
    ///
    /// Deviation: Java takes a `java.util.Random`; the world generator uses
    /// a `FastRandom`, which *is* a `java.util.Random` subclass.
    pub fn get(&self, random: &mut FastRandom) -> Option<&CoverContainerEntryPart> {
        self.entries.get_random(random)
    }

    /// Java `IWeightedMap<CoverContainerEntryPart> getEntries()`.
    ///
    /// Deviation: Java has no getter; the Rust port exposes the weighted map
    /// so the loaders/tests can inspect it.
    pub fn get_entries(&self) -> &Rc<dyn IWeightedMap<CoverContainerEntryPart>> {
        &self.entries
    }

    /// Java `IBlockFluidCondition getParentCondition()`.
    pub fn get_parent_condition(&self) -> &Rc<dyn IBlockFluidCondition> {
        &self.parent_condition
    }

    /// Java `ICoordinateCondition getMapCondition()`.
    pub fn get_map_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.map_condition
    }

    /// Java `double getCoverDensity()`.
    pub const fn get_cover_density(&self) -> f64 {
        self.cover_density
    }

    /// Java `ICoordinateRndCondition getHeightCondition()`.
    pub fn get_height_condition(&self) -> &Rc<dyn ICoordinateRndCondition> {
        &self.height_condition
    }

    /// Java `boolean isOnWater()`.
    pub const fn is_on_water(&self) -> bool {
        self.on_water
    }
}

/// Java `static class CoverContainer.CoverContainerEntry.CoverContainerEntryPart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CoverContainerEntryPart {
    /// Java `protected final BlockFluidEntry entry`.
    entry: BlockFluidEntry,
    /// Java `protected final int offset`.
    offset: i32,
}

impl CoverContainerEntryPart {
    /// Java `public static final CoverContainerEntryPart[] EMPTY_ARRAY`.
    pub const EMPTY_ARRAY: [CoverContainerEntryPart; 0] = [];

    /// Java `CoverContainerEntryPart(BlockFluidEntry entry, int offset)`.
    pub const fn new(entry: BlockFluidEntry, offset: i32) -> Self {
        Self { entry, offset }
    }

    /// Java `BlockFluidEntry getEntry()`.
    pub const fn get_entry(&self) -> BlockFluidEntry {
        self.entry
    }

    /// Java `int getOffset()`.
    pub const fn get_offset(&self) -> i32 {
        self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weighted_map::WeightedMap;
    use procedural::noise::condition::DefaultCoordinateCondition;

    /// Test stub: a block/fluid condition matching a single block id.
    struct BlockEquals(i32);

    impl IBlockFluidCondition for BlockEquals {
        fn eval(&self, block: i32, _fluid: i32) -> bool {
            block == self.0
        }
    }

    /// Test stub: a height condition that always matches.
    struct AlwaysTrueHeight;

    impl ICoordinateRndCondition for AlwaysTrueHeight {
        fn eval(&self, _seed: i32, _x: i32, _z: i32, _y: i32, _random: &mut FastRandom) -> bool {
            true
        }
    }

    fn part_map(
        parts: &[(CoverContainerEntryPart, f64)],
    ) -> Rc<dyn IWeightedMap<CoverContainerEntryPart>> {
        let mut builder = WeightedMap::<CoverContainerEntryPart>::builder();
        for (part, weight) in parts {
            builder.put(*part, *weight);
        }
        Rc::new(builder.build())
    }

    fn entry(parts: &[(CoverContainerEntryPart, f64)]) -> CoverContainerEntry {
        CoverContainerEntry::new(
            part_map(parts),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(AlwaysTrueHeight),
            Rc::new(BlockEquals(3)),
            0.25,
            true,
        )
    }

    #[test]
    fn a_part_exposes_its_block_and_offset() {
        let part = CoverContainerEntryPart::new(BlockFluidEntry::new(7, 1, 2), 5);
        assert_eq!(part.get_entry(), BlockFluidEntry::new(7, 1, 2));
        assert_eq!(part.get_offset(), 5);
    }

    #[test]
    fn the_weighted_pick_selects_a_part() {
        let first = CoverContainerEntryPart::new(BlockFluidEntry::new(1, 0, 0), 0);
        let second = CoverContainerEntryPart::new(BlockFluidEntry::new(2, 0, 0), 1);
        let entry = entry(&[(first, 1.0), (second, 1.0)]);
        let mut random = FastRandom::new(1234);
        let picked = entry.get(&mut random).expect("a part is picked");
        assert!(picked == &first || picked == &second);
    }

    #[test]
    fn the_entry_exposes_its_conditions() {
        let part = CoverContainerEntryPart::new(BlockFluidEntry::new(1, 0, 0), 0);
        let entry = entry(&[(part, 1.0)]);
        assert_eq!(entry.get_cover_density(), 0.25);
        assert!(entry.is_on_water());
        assert!(entry.get_parent_condition().eval(3, 0));
        assert!(!entry.get_parent_condition().eval(4, 0));
        assert!(entry.get_map_condition().eval2(0, 0, 0));
    }

    #[test]
    fn the_container_exposes_its_entries() {
        let part = CoverContainerEntryPart::new(BlockFluidEntry::new(1, 0, 0), 0);
        let container = CoverContainer::new(vec![entry(&[(part, 1.0)])]);
        assert_eq!(container.get_entries().len(), 1);
    }

    #[test]
    fn the_entry_pool_yields_a_usable_list() {
        let part = CoverContainerEntryPart::new(BlockFluidEntry::new(1, 0, 0), 0);
        let mut resource = ENTRY_POOL.with(|pool| pool.acquire());
        resource.push(entry(&[(part, 1.0)]));
        assert_eq!(resource.len(), 1);
    }
}
