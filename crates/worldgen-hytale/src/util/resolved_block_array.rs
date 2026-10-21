// Ported from: `.hytale-source/.../worldgen/util/ResolvedBlockArray.java`
//! Java `com.hypixel.hytale.server.worldgen.util.ResolvedBlockArray` — a set
//! of resolved `(block, fluid)` pairs.
//!
//! ⚠ **Not ported**: Java's global registries
//! `ResolvedBlockArray.RESOLVED_BLOCKS` and
//! `ResolvedBlockArray.RESOLVED_BLOCKS_WITH_VARIANTS`
//! (`Long2ObjectMap<ResolvedBlockArray>`) are loader-time globals that are
//! populated by the world/block JSON loaders. They arrive with the Phase-2
//! JSON loaders (see `plan.md`, "serde loaders"); until then the resolved
//! arrays are created explicitly by the callers.

use std::collections::HashSet;
use std::sync::LazyLock;

use hytale_math::math_util;

use crate::util::block_fluid_entry::BlockFluidEntry;

/// Java `final class ResolvedBlockArray`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBlockArray {
    /// Java `private final LongSet entrySet` — the packed `(block, fluid)` keys.
    entry_set: HashSet<i64>,
    /// Java `private final BlockFluidEntry[] entries`.
    entries: Vec<BlockFluidEntry>,
}

impl ResolvedBlockArray {
    /// Java `ResolvedBlockArray(BlockFluidEntry[] entries)`.
    pub fn new(entries: Vec<BlockFluidEntry>) -> Self {
        let mut entry_set = HashSet::with_capacity(entries.len());
        for entry in &entries {
            // Java: entrySet.add(MathUtil.packLong(entry.blockId(), entry.fluidId()));
            entry_set.insert(math_util::pack_long(entry.block_id, entry.fluid_id));
        }
        Self { entry_set, entries }
    }

    /// Constructor for the empty instance (see [`EMPTY`]).
    pub fn empty() -> Self {
        ResolvedBlockArray::new(Vec::new())
    }

    /// Java `BlockFluidEntry[] getEntries()`.
    pub fn get_entries(&self) -> &[BlockFluidEntry] {
        &self.entries
    }

    /// Java `LongSet getEntrySet()`.
    pub fn get_entry_set(&self) -> &HashSet<i64> {
        &self.entry_set
    }

    /// Java `int size()`.
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// Java `boolean contains(int block, int fluidId)`.
    pub fn contains(&self, block: i32, fluid_id: i32) -> bool {
        self.entry_set
            .contains(&math_util::pack_long(block, fluid_id))
    }

    /// The Java `hashCode()` of the array — `31 * entrySet.hashCode() +
    /// Arrays.hashCode(entries)`.
    ///
    /// `LongSet.hashCode()` is the sum of the element hash codes and
    /// `Long.hashCode(v)` is `(int) (v ^ (v >>> 32))`.
    pub fn hash_value(&self) -> i32 {
        let mut set_hash: i32 = 0;
        for value in &self.entry_set {
            set_hash = set_hash.wrapping_add((value ^ ((*value as u64 >> 32) as i64)) as i32);
        }

        let mut array_hash: i32 = 1;
        for entry in &self.entries {
            array_hash = 31i32
                .wrapping_mul(array_hash)
                .wrapping_add(entry.hash_value());
        }

        31i32.wrapping_mul(set_hash).wrapping_add(array_hash)
    }
}

impl Default for ResolvedBlockArray {
    fn default() -> Self {
        ResolvedBlockArray::empty()
    }
}

/// Java `public static final ResolvedBlockArray EMPTY` — the empty instance.
///
/// Deviation: Java declares this as a `static final` constant. Rust's
/// `HashSet::new()` is not a `const fn`, so a `const` instance cannot be
/// built. The equivalent shared instance is exposed as this lazily initialized
/// static (it derefs to a `ResolvedBlockArray`) and as the
/// [`ResolvedBlockArray::empty`] constructor.
pub static EMPTY: LazyLock<ResolvedBlockArray> =
    LazyLock::new(|| ResolvedBlockArray::new(Vec::new()));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_matches_block_and_fluid_pairs() {
        let array = ResolvedBlockArray::new(vec![
            BlockFluidEntry::new(1, 0, 0),
            BlockFluidEntry::new(2, 0, 5),
        ]);
        assert!(array.contains(1, 0));
        assert!(array.contains(2, 5));
        assert!(!array.contains(1, 5), "fluid is part of the key");
        assert!(!array.contains(2, 0));
        assert!(!array.contains(3, 0));
    }

    #[test]
    fn size_and_entries_are_exposed() {
        let array = ResolvedBlockArray::new(vec![
            BlockFluidEntry::new(1, 0, 0),
            BlockFluidEntry::new(2, 0, 5),
        ]);
        assert_eq!(array.size(), 2);
        assert_eq!(array.get_entries().len(), 2);
        assert_eq!(array.get_entry_set().len(), 2);
    }

    #[test]
    fn empty_array_contains_nothing() {
        let array = ResolvedBlockArray::empty();
        assert_eq!(array.size(), 0);
        assert!(!array.contains(0, 0));
        assert!(!EMPTY.contains(0, 0));
    }

    #[test]
    fn equality_uses_entries_and_set() {
        let a = ResolvedBlockArray::new(vec![BlockFluidEntry::new(1, 0, 0)]);
        let b = ResolvedBlockArray::new(vec![BlockFluidEntry::new(1, 0, 0)]);
        let c = ResolvedBlockArray::new(vec![BlockFluidEntry::new(1, 0, 1)]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
