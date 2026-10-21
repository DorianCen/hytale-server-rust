// Ported from: `.hytale-source/.../worldgen/util/NoiseBlockArray.java`
//! Java `com.hypixel.hytale.server.worldgen.util.NoiseBlockArray` — an ordered
//! list of block entries whose repetition count is driven by noise.
//!
//! ⚠ **Not ported**: Java's `NoiseBlockArray.equals`/`hashCode` compare the
//! `Entry[]` arrays, and `Entry.equals`/`hashCode` include the `IDoubleRange`
//! and the `NoiseProperty` / `IDoubleCoordinateSupplier2d` fields. Rust has
//! no equivalent equality for `Rc<dyn IDoubleRange>` / `Rc<dyn NoiseProperty>`,
//! so the Rust port does not implement `PartialEq` for `NoiseBlockArray` or
//! `Entry`.

use std::rc::Rc;

use hytale_math::math_util;
use procedural::noise::property::NoiseProperty;
use procedural::noise::supplier::{IDoubleRange, NoisePropertySupplier2d};

use crate::util::block_fluid_entry::BlockFluidEntry;

/// Java `class NoiseBlockArray`.
pub struct NoiseBlockArray {
    /// Java `protected final Entry[] entries`.
    entries: Vec<Entry>,
}

impl NoiseBlockArray {
    /// Java `public static final NoiseBlockArray EMPTY` — the empty instance.
    ///
    /// Deviation: Java declares this as a `static final` constant. The Rust
    /// [`Entry`] holds `Rc` fields (Java holds references to the noise
    /// property / range), so no `const` instance can be built; the equivalent
    /// instance is created by this constructor.
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Java `NoiseBlockArray(Entry[] entries)`.
    pub fn new(entries: Vec<Entry>) -> Self {
        Self { entries }
    }

    /// Java `Entry[] getEntries()`.
    pub fn get_entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Java `BlockFluidEntry getTopBlockAt(int seed, double x, double z)`.
    pub fn get_top_block_at(&self, seed: i32, x: f64, z: f64) -> BlockFluidEntry {
        for entry in &self.entries {
            let repetitions = entry.get_repetitions(seed, x, z);
            if repetitions > 0 {
                return entry.block_entry;
            }
        }
        BlockFluidEntry::EMPTY
    }

    /// Java `BlockFluidEntry getBottomBlockAt(int seed, double x, double z)`.
    pub fn get_bottom_block_at(&self, seed: i32, x: f64, z: f64) -> BlockFluidEntry {
        for entry in self.entries.iter().rev() {
            let repetitions = entry.get_repetitions(seed, x, z);
            if repetitions > 0 {
                return entry.block_entry;
            }
        }
        BlockFluidEntry::EMPTY
    }
}

/// Java `static class NoiseBlockArray.Entry`.
pub struct Entry {
    /// Java `protected final String blockName`.
    pub block_name: String,
    /// Java `protected final BlockFluidEntry blockEntry`.
    pub block_entry: BlockFluidEntry,
    /// Java `protected final IDoubleRange repetitions`.
    pub repetitions: Rc<dyn IDoubleRange>,
    /// Java `@Nonnull protected final NoiseProperty noise`.
    pub noise: Rc<dyn NoiseProperty>,
}

impl Entry {
    /// Java `Entry(String blockName, BlockFluidEntry blockEntry, IDoubleRange
    /// repetitions, NoiseProperty noise)`.
    ///
    /// Deviation: Java additionally stores
    /// `@Nonnull protected final IDoubleCoordinateSupplier2d noiseSupplier`
    /// (the `noise::get` method reference). The Rust port creates the
    /// equivalent adapter on demand in [`Entry::get_repetitions`] — the
    /// adapter only borrows the noise property, so storing it as a field would
    /// make the struct self-referential.
    pub fn new(
        block_name: String,
        block_entry: BlockFluidEntry,
        repetitions: Rc<dyn IDoubleRange>,
        noise: Rc<dyn NoiseProperty>,
    ) -> Self {
        Self {
            block_name,
            block_entry,
            repetitions,
            noise,
        }
    }

    /// Java `String getBlockName()`.
    pub fn get_block_name(&self) -> &str {
        &self.block_name
    }

    /// Java `BlockFluidEntry getBlockEntry()`.
    pub fn get_block_entry(&self) -> BlockFluidEntry {
        self.block_entry
    }

    /// Java `int getRepetitions(int seed, double x, double z)`.
    pub fn get_repetitions(&self, seed: i32, x: f64, z: f64) -> i32 {
        // Java: MathUtil.floor(repetitions.getValue(seed, x, z, noiseSupplier));
        math_util::floor(self.repetitions.get_value_2d(
            seed,
            x,
            z,
            &NoisePropertySupplier2d(self.noise.as_ref()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::constant_noise::ConstantNoise;
    use procedural::noise::property::single_noise_property::SingleNoiseProperty;
    use procedural::noise::supplier::DoubleRange;

    fn constant_property(value: f64) -> Rc<dyn NoiseProperty> {
        Rc::new(SingleNoiseProperty::new(
            0,
            Rc::new(ConstantNoise::new(value)),
        ))
    }

    fn entry(
        name: &str,
        block_id: i32,
        repetitions: Rc<dyn IDoubleRange>,
        noise: Rc<dyn NoiseProperty>,
    ) -> Entry {
        Entry::new(
            name.to_string(),
            BlockFluidEntry::new(block_id, 0, 0),
            repetitions,
            noise,
        )
    }

    #[test]
    fn top_block_is_the_first_entry_with_repetitions() {
        // ConstantNoise(0) remapped by SingleNoiseProperty is 0.5, so a
        // Normal(0, 4) range yields 2 repetitions.
        let noise = constant_property(0.0);
        let array = NoiseBlockArray::new(vec![
            entry(
                "stone",
                1,
                Rc::new(DoubleRange::normal(0.0, 4.0)),
                noise.clone(),
            ),
            entry(
                "dirt",
                2,
                Rc::new(DoubleRange::normal(10.0, 20.0)),
                noise.clone(),
            ),
        ]);
        assert_eq!(array.get_top_block_at(1, 0.0, 0.0).block_id, 1);
        // The bottom block is the *last* entry with repetitions, which is the
        // "dirt" entry (15 repetitions).
        assert_eq!(array.get_bottom_block_at(1, 0.0, 0.0).block_id, 2);
    }

    #[test]
    fn bottom_block_is_the_last_entry_with_repetitions() {
        // ConstantNoise(1) remapped by SingleNoiseProperty is 1.0, so a
        // Normal(0, 4) range yields 4 repetitions and a Normal(0, 2) range
        // yields 2 repetitions.
        let noise = constant_property(1.0);
        let array = NoiseBlockArray::new(vec![
            entry(
                "stone",
                1,
                Rc::new(DoubleRange::normal(0.0, 2.0)),
                noise.clone(),
            ),
            entry(
                "dirt",
                2,
                Rc::new(DoubleRange::normal(0.0, 4.0)),
                noise.clone(),
            ),
        ]);
        assert_eq!(array.get_top_block_at(1, 0.0, 0.0).block_id, 1);
        assert_eq!(array.get_bottom_block_at(1, 0.0, 0.0).block_id, 2);
    }

    #[test]
    fn empty_array_yields_the_empty_entry() {
        let array = NoiseBlockArray::empty();
        assert_eq!(array.get_top_block_at(1, 0.0, 0.0), BlockFluidEntry::EMPTY);
        assert_eq!(
            array.get_bottom_block_at(1, 0.0, 0.0),
            BlockFluidEntry::EMPTY
        );
        assert!(array.get_entries().is_empty());
    }

    #[test]
    fn entries_without_repetitions_fall_through_to_empty() {
        // Normal(-10, -5) never reaches 0 repetitions.
        let noise = constant_property(0.0);
        let array = NoiseBlockArray::new(vec![entry(
            "stone",
            1,
            Rc::new(DoubleRange::normal(-10.0, -5.0)),
            noise,
        )]);
        assert_eq!(array.get_top_block_at(1, 0.0, 0.0), BlockFluidEntry::EMPTY);
        assert_eq!(
            array.get_bottom_block_at(1, 0.0, 0.0),
            BlockFluidEntry::EMPTY
        );
    }

    #[test]
    fn repetitions_are_floored() {
        let noise = constant_property(0.0); // → 0.5 after the remap
        let entry = entry("stone", 1, Rc::new(DoubleRange::normal(0.0, 5.0)), noise);
        // 0 + 5 * 0.5 = 2.5 → floor → 2
        assert_eq!(entry.get_repetitions(1, 0.0, 0.0), 2);
    }
}
