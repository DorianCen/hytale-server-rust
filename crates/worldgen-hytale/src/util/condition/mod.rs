// Ported from: `.hytale-source/.../worldgen/util/condition/*.java`
//! Java `com.hypixel.hytale.server.worldgen.util.condition` package — the
//! conditions used by the world generator.
//!
//! | Java class                     | Rust item                       |
//! |--------------------------------|---------------------------------|
//! | `HashSetIntCondition`          | [`HashSetIntCondition`]         |
//! | `HashSetBlockFluidCondition`   | [`HashSetBlockFluidCondition`]  |
//! | `RandomCoordinateCondition`    | [`RandomCoordinateCondition`]   |
//! | `FilteredBlockFluidCondition`  | [`FilteredBlockFluidCondition`] |
//! | `IntConditionBuilder`          | [`IntConditionBuilder`]         |
//! | `BlockMaskCondition`           | [`BlockMaskCondition`]          |

pub mod flag;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use hytale_math::hash_util;
use hytale_math::math_util;
use procedural::noise::condition::{IBlockFluidCondition, ICoordinateCondition};
use procedural::noise::support::IIntCondition;

use crate::util::resolved_block_array::ResolvedBlockArray;

/// Java `class HashSetIntCondition implements IIntCondition`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashSetIntCondition {
    /// Java `protected final IntSet set`.
    pub set: HashSet<i32>,
}

impl HashSetIntCondition {
    /// Java `HashSetIntCondition(IntSet set)`.
    pub fn new(set: HashSet<i32>) -> Self {
        Self { set }
    }

    /// Java `IntSet getSet()`.
    pub fn get_set(&self) -> &HashSet<i32> {
        &self.set
    }
}

impl IIntCondition for HashSetIntCondition {
    /// Java `boolean eval(int i)`.
    #[inline]
    fn eval(&self, value: i32) -> bool {
        self.set.contains(&value)
    }
}

/// Java `class HashSetBlockFluidCondition implements IBlockFluidCondition`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashSetBlockFluidCondition {
    /// Java `protected final LongSet set` — the packed `(block, fluid)` keys.
    pub set: HashSet<i64>,
}

impl HashSetBlockFluidCondition {
    /// Java `HashSetBlockFluidCondition(LongSet set)`.
    pub fn new(set: HashSet<i64>) -> Self {
        Self { set }
    }

    /// Java `LongSet getSet()`.
    pub fn get_set(&self) -> &HashSet<i64> {
        &self.set
    }
}

impl IBlockFluidCondition for HashSetBlockFluidCondition {
    /// Java `boolean eval(int block, int fluid)`.
    #[inline]
    fn eval(&self, block: i32, fluid: i32) -> bool {
        self.set.contains(&math_util::pack_long(block, fluid))
    }
}

/// Java `class RandomCoordinateCondition implements ICoordinateCondition`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomCoordinateCondition {
    /// Java `private final double chance`.
    pub chance: f64,
}

impl RandomCoordinateCondition {
    /// Java `RandomCoordinateCondition(double chance)`.
    pub fn new(chance: f64) -> Self {
        Self { chance }
    }
}

impl ICoordinateCondition for RandomCoordinateCondition {
    /// Java `HashUtil.random(seed, x, y) <= chance`.
    ///
    /// Java's `HashUtil.random(long, long, long)` is selected because the
    /// `int` arguments widen to `long`.
    fn eval2(&self, seed: i32, x: i32, y: i32) -> bool {
        hash_util::random3(seed as i64, x as i64, y as i64) <= self.chance
    }

    /// Java `HashUtil.random(seed, x, y, z) <= chance`.
    fn eval3(&self, seed: i32, x: i32, y: i32, z: i32) -> bool {
        hash_util::random4(seed as i64, x as i64, y as i64, z as i64) <= self.chance
    }
}

/// Java `class FilteredBlockFluidCondition implements IBlockFluidCondition`.
pub struct FilteredBlockFluidCondition {
    /// Java `private final IBlockFluidCondition filter`.
    pub filter: Rc<dyn IBlockFluidCondition>,
    /// Java `private final IBlockFluidCondition condition`.
    pub condition: Rc<dyn IBlockFluidCondition>,
}

impl FilteredBlockFluidCondition {
    /// Java `FilteredBlockFluidCondition(int blockId, IBlockFluidCondition
    /// condition)`.
    ///
    /// The filter is the Java lambda `(block, fluid) -> block == blockId &&
    /// fluid == 0`.
    pub fn with_block_id(block_id: i32, condition: Rc<dyn IBlockFluidCondition>) -> Self {
        Self {
            filter: Rc::new(BlockIdFluidFilter { block_id }),
            condition,
        }
    }

    /// Java `FilteredBlockFluidCondition(IBlockFluidCondition filter,
    /// IBlockFluidCondition condition)`.
    pub fn new(
        filter: Rc<dyn IBlockFluidCondition>,
        condition: Rc<dyn IBlockFluidCondition>,
    ) -> Self {
        Self { filter, condition }
    }
}

impl IBlockFluidCondition for FilteredBlockFluidCondition {
    /// Java `if (filter.eval(block, fluid)) return false; return
    /// condition.eval(block, fluid);`
    fn eval(&self, block: i32, fluid: i32) -> bool {
        if self.filter.eval(block, fluid) {
            return false;
        }
        self.condition.eval(block, fluid)
    }
}

/// The Java lambda `(block, fluid) -> block == blockId && fluid == 0` used by
/// [`FilteredBlockFluidCondition::with_block_id`].
struct BlockIdFluidFilter {
    block_id: i32,
}

impl IBlockFluidCondition for BlockIdFluidFilter {
    #[inline]
    fn eval(&self, block: i32, fluid: i32) -> bool {
        block == self.block_id && fluid == 0
    }
}

/// Java `class IntConditionBuilder implements IntConsumer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntConditionBuilder {
    /// Java `private int first`.
    first: i32,
    /// Java `private final int nullValue`.
    null_value: i32,
    /// Java `@Nullable private IntSet set`.
    set: Option<HashSet<i32>>,
}

impl IntConditionBuilder {
    /// Java `IntConditionBuilder(Supplier<IntSet> setSupplier, int nullValue)`.
    ///
    /// Deviation: Java's `setSupplier` only picks the `IntSet` implementation
    /// (fastutil); the Rust port always uses `std::collections::HashSet` and
    /// therefore does not need the supplier.
    pub fn new(null_value: i32) -> Self {
        Self {
            first: null_value,
            null_value,
            set: None,
        }
    }

    /// Java `void accept(int value)` (the `IntConsumer` method).
    pub fn accept(&mut self, value: i32) {
        self.add(value);
    }

    /// Java `boolean add(int value)`.
    pub fn add(&mut self, value: i32) -> bool {
        if value == self.first || value == self.null_value {
            return false;
        }
        if self.first == self.null_value {
            self.first = value;
            return true;
        }
        if self.set.is_none() {
            let mut set = HashSet::new();
            set.insert(self.first);
            self.set = Some(set);
        }
        self.set
            .as_mut()
            .expect("set was just created")
            .insert(value)
    }

    /// Java `IIntCondition buildOrDefault(IIntCondition defaultCondition)`.
    pub fn build_or_default(
        &self,
        default_condition: Rc<dyn IIntCondition>,
    ) -> Rc<dyn IIntCondition> {
        if self.first == self.null_value {
            return default_condition;
        }
        let set = match &self.set {
            Some(set) => set.clone(),
            // Java: if (set == null) set = IntSets.singleton(first);
            None => {
                let mut set = HashSet::new();
                set.insert(self.first);
                set
            }
        };
        Rc::new(HashSetIntCondition::new(set))
    }

    /// The `HashSetIntCondition` set this builder would produce, or `None`
    /// when the builder is empty (Java `buildOrDefault` returns the default
    /// then). Exposed so loaders that need the id set (the biome-mask
    /// `instanceof HashSetIntCondition` check) can read it without a
    /// downcast.
    pub fn build_hash_set(&self) -> Option<HashSet<i32>> {
        if self.first == self.null_value {
            return None;
        }
        let set = match &self.set {
            Some(set) => set.clone(),
            None => {
                let mut set = HashSet::new();
                set.insert(self.first);
                set
            }
        };
        Some(set)
    }
}

/// Java `class BlockMaskCondition`.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockMaskCondition {
    /// Java `@Nonnull private Mask defaultMask`.
    default_mask: Mask,
    /// Java `@Nonnull private Long2ObjectMap<Mask> specificMasks` — keyed by
    /// the packed `(block, fluid)` pair.
    specific_masks: HashMap<i64, Mask>,
}

impl BlockMaskCondition {
    /// Java `public BlockMaskCondition()` — the default condition.
    ///
    /// Java's `DEFAULT_TRUE` and `DEFAULT_FALSE` are both `new
    /// BlockMaskCondition()` (the same instance type with the default mask).
    pub fn new() -> Self {
        Self {
            default_mask: Mask::new_match_empty(true, Vec::new()),
            specific_masks: HashMap::new(),
        }
    }

    /// Java `void set(Mask defaultMask, Long2ObjectMap<Mask> specificMasks)`.
    pub fn set(&mut self, default_mask: Mask, specific_masks: HashMap<i64, Mask>) {
        self.default_mask = default_mask;
        self.specific_masks = specific_masks;
    }

    /// Java `Mask getDefaultMask()`.
    pub fn get_default_mask(&self) -> &Mask {
        &self.default_mask
    }

    /// Java `Long2ObjectMap<Mask> getSpecificMasks()`.
    pub fn get_specific_masks(&self) -> &HashMap<i64, Mask> {
        &self.specific_masks
    }

    /// Java `boolean eval(int currentBlock, int currentFluid, int nextBlockId,
    /// int nextFluidId)`.
    pub fn eval(
        &self,
        current_block: i32,
        current_fluid: i32,
        next_block_id: i32,
        next_fluid_id: i32,
    ) -> bool {
        // Java: specificMasks.getOrDefault(MathUtil.packLong(nextBlockId, nextFluidId), defaultMask)
        let mask = self
            .specific_masks
            .get(&math_util::pack_long(next_block_id, next_fluid_id))
            .unwrap_or(&self.default_mask);
        mask.should_replace(current_block, current_fluid)
    }

    /// The Java `hashCode()` of the condition — `31 * defaultMask.hashCode() +
    /// specificMasks.hashCode()`.
    ///
    /// The fastutil map hash is the sum of `key.hashCode() ^ value.hashCode()`
    /// over its entries, which is order independent.
    pub fn hash_value(&self) -> i32 {
        let mut specific: i32 = 0;
        for (key, mask) in &self.specific_masks {
            let key_hash = (*key ^ ((*key as u64 >> 32) as i64)) as i32;
            specific = specific.wrapping_add(key_hash ^ mask.hash_value());
        }
        31i32
            .wrapping_mul(self.default_mask.hash_value())
            .wrapping_add(specific)
    }
}

impl Default for BlockMaskCondition {
    fn default() -> Self {
        Self::new()
    }
}

/// Java `static class BlockMaskCondition.Mask`.
///
/// Note: Java's `Mask.equals` only compares the `entries` array and ignores
/// the `matchEmpty` flag; the derived Rust `PartialEq` compares both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    /// Java `private final boolean matchEmpty`.
    match_empty: bool,
    /// Java `private final MaskEntry[] entries`.
    entries: Vec<MaskEntry>,
}

impl Mask {
    /// Java `Mask(MaskEntry[] entries)` — `matchEmpty` is `false`.
    pub fn new(entries: Vec<MaskEntry>) -> Self {
        Self {
            match_empty: false,
            entries,
        }
    }

    /// Java `private Mask(boolean matchEmpty, MaskEntry[] entries)`.
    ///
    /// Java's `DEFAULT_MASK` uses this constructor; it is private in Java
    /// because only the class itself creates it.
    pub fn new_match_empty(match_empty: bool, entries: Vec<MaskEntry>) -> Self {
        Self {
            match_empty,
            entries,
        }
    }

    /// Java `boolean shouldReplace(int current, int fluid)`.
    pub fn should_replace(&self, current: i32, fluid: i32) -> bool {
        for entry in &self.entries {
            if entry.should_handle(current, fluid) {
                return entry.should_replace();
            }
        }
        // Java: return matchEmpty && (current == 0 || fluid == 0);
        self.match_empty && (current == 0 || fluid == 0)
    }

    /// The Java `hashCode()` of the mask — `Arrays.hashCode(entries)`.
    pub fn hash_value(&self) -> i32 {
        let mut result: i32 = 1;
        for entry in &self.entries {
            result = 31i32.wrapping_mul(result).wrapping_add(entry.hash_value());
        }
        result
    }
}

/// Java `static class BlockMaskCondition.MaskEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaskEntry {
    /// Java `private ResolvedBlockArray blocks`.
    blocks: ResolvedBlockArray,
    /// Java `private final boolean any`.
    any: bool,
    /// Java `private boolean replace`.
    replace: bool,
}

impl MaskEntry {
    /// Java `MaskEntry()` — `any = false`, `replace = false`.
    pub fn default_entry() -> Self {
        Self {
            blocks: ResolvedBlockArray::empty(),
            any: false,
            replace: false,
        }
    }

    /// Java `MaskEntry(boolean any, boolean replace)`.
    pub fn new(any: bool, replace: bool) -> Self {
        Self {
            blocks: ResolvedBlockArray::empty(),
            any,
            replace,
        }
    }

    /// Java `MaskEntry.WILDCARD_TRUE` — `any = true`, `replace = true`.
    ///
    /// Deviation: Java declares the wildcards as `static final` constants. The
    /// Rust `MaskEntry` holds a `ResolvedBlockArray`, which cannot be built in
    /// a `const` context (its `HashSet` has no `const` constructor), so the
    /// wildcards are provided as constructors.
    pub fn wildcard_true() -> Self {
        Self::new(true, true)
    }

    /// Java `MaskEntry.WILDCARD_FALSE` — `any = true`, `replace = false`.
    pub fn wildcard_false() -> Self {
        Self::new(true, false)
    }

    /// Java `void set(ResolvedBlockArray blocks, boolean replace)`.
    pub fn set(&mut self, blocks: ResolvedBlockArray, replace: bool) {
        self.blocks = blocks;
        self.replace = replace;
    }

    /// Java `boolean shouldHandle(int current, int fluid)`.
    pub fn should_handle(&self, current: i32, fluid: i32) -> bool {
        self.any || self.blocks.contains(current, fluid)
    }

    /// Java `boolean shouldReplace()`.
    pub fn should_replace(&self) -> bool {
        self.replace
    }

    /// The Java `hashCode()` of the entry — `31 * (31 * blocks.hashCode() +
    /// (any ? 1 : 0)) + (replace ? 1 : 0)`.
    pub fn hash_value(&self) -> i32 {
        let mut result = self.blocks.hash_value();
        result = 31i32.wrapping_mul(result).wrapping_add(i32::from(self.any));
        31i32
            .wrapping_mul(result)
            .wrapping_add(i32::from(self.replace))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::block_fluid_entry::BlockFluidEntry;
    use procedural::noise::condition::ConstantIntCondition;

    #[test]
    fn hash_set_int_condition_checks_membership() {
        let mut set = HashSet::new();
        set.insert(1);
        set.insert(5);
        let condition = HashSetIntCondition::new(set);
        assert!(condition.eval(1));
        assert!(condition.eval(5));
        assert!(!condition.eval(2));
    }

    #[test]
    fn hash_set_block_fluid_condition_packs_the_pair() {
        let mut set = HashSet::new();
        set.insert(math_util::pack_long(3, 0));
        let condition = HashSetBlockFluidCondition::new(set);
        assert!(condition.eval(3, 0));
        assert!(!condition.eval(3, 1));
        assert!(!condition.eval(4, 0));
    }

    #[test]
    fn random_coordinate_condition_is_deterministic() {
        let condition = RandomCoordinateCondition::new(0.5);
        let first = condition.eval2(1, 2, 3);
        assert_eq!(first, condition.eval2(1, 2, 3));
        // A chance of 1.0 always matches, a chance of -1.0 never does.
        assert!(RandomCoordinateCondition::new(1.0).eval2(1, 2, 3));
        assert!(RandomCoordinateCondition::new(1.0).eval3(1, 2, 3, 4));
        assert!(!RandomCoordinateCondition::new(-1.0).eval2(1, 2, 3));
        assert!(!RandomCoordinateCondition::new(-1.0).eval3(1, 2, 3, 4));
    }

    #[test]
    fn filtered_condition_rejects_the_filtered_pair() {
        let condition = FilteredBlockFluidCondition::with_block_id(
            7,
            Rc::new(HashSetBlockFluidCondition::new(HashSet::new())),
        );
        // The filter matches block 7 with fluid 0 → the condition is skipped.
        assert!(!condition.eval(7, 0));
        // Everything else is passed through to the (empty) condition.
        assert!(!condition.eval(8, 0));
    }

    #[test]
    fn filtered_condition_with_a_custom_filter() {
        let filter: Rc<dyn IBlockFluidCondition> = Rc::new(HashSetBlockFluidCondition::new({
            let mut set = HashSet::new();
            set.insert(math_util::pack_long(1, 0));
            set
        }));
        let inner: Rc<dyn IBlockFluidCondition> = Rc::new(HashSetBlockFluidCondition::new({
            let mut set = HashSet::new();
            set.insert(math_util::pack_long(2, 0));
            set
        }));
        let condition = FilteredBlockFluidCondition::new(filter, inner);
        assert!(!condition.eval(1, 0), "filtered out");
        assert!(condition.eval(2, 0), "passes the filter and matches");
        assert!(
            !condition.eval(3, 0),
            "passes the filter but does not match"
        );
    }

    #[test]
    fn int_condition_builder_collects_values() {
        let mut builder = IntConditionBuilder::new(-1);
        // The first value replaces the null value.
        assert!(builder.add(4));
        // The same value is rejected.
        assert!(!builder.add(4));
        // The null value is always rejected.
        assert!(!builder.add(-1));
        // A second value creates the set.
        assert!(builder.add(6));

        let condition = builder.build_or_default(Rc::new(ConstantIntCondition::DEFAULT_TRUE));
        // The built condition is a set of {4, 6}.
        assert!(condition.eval(4));
        assert!(condition.eval(6));
        assert!(!condition.eval(5));
    }

    #[test]
    fn int_condition_builder_without_values_returns_the_default() {
        let builder = IntConditionBuilder::new(-1);
        let condition = builder.build_or_default(Rc::new(ConstantIntCondition::DEFAULT_FALSE));
        // The default condition is returned unchanged.
        assert!(!condition.eval(0));
        assert!(!condition.eval(42));
    }

    #[test]
    fn int_condition_builder_singleton() {
        let mut builder = IntConditionBuilder::new(-1);
        assert!(builder.add(4));
        let condition = builder.build_or_default(Rc::new(ConstantIntCondition::DEFAULT_TRUE));
        assert!(condition.eval(4));
        assert!(!condition.eval(5));
    }

    #[test]
    fn block_mask_condition_uses_the_specific_mask() {
        let mut specific = HashMap::new();
        specific.insert(
            math_util::pack_long(2, 0),
            Mask::new(vec![MaskEntry::wildcard_true()]),
        );
        let mut condition = BlockMaskCondition::new();
        condition.set(Mask::new(vec![MaskEntry::wildcard_false()]), specific);

        // The specific mask matches → replace.
        assert!(condition.eval(0, 0, 2, 0));
        // The default mask is used for other blocks → no replace.
        assert!(!condition.eval(0, 0, 3, 0));
    }

    #[test]
    fn block_mask_default_mask_matches_empty() {
        let condition = BlockMaskCondition::new();
        // The default mask has `matchEmpty = true` and no entries: it matches
        // when the current block *or* the current fluid is empty.
        assert!(condition.eval(0, 0, 1, 0), "empty current block matches");
        assert!(condition.eval(5, 0, 1, 0), "empty current fluid matches");
        assert!(!condition.eval(5, 7, 1, 0), "neither is empty");
    }

    #[test]
    fn mask_entry_handles_its_blocks() {
        let blocks = ResolvedBlockArray::new(vec![BlockFluidEntry::new(4, 0, 0)]);
        let mut entry = MaskEntry::new(false, true);
        entry.set(blocks, true);
        assert!(entry.should_handle(4, 0));
        assert!(!entry.should_handle(5, 0));
        assert!(entry.should_replace());
    }

    #[test]
    fn wildcard_mask_entries_handle_everything() {
        let entry = MaskEntry::wildcard_true();
        assert!(entry.should_handle(0, 0));
        assert!(entry.should_handle(123, 456));
        assert!(entry.should_replace());

        let entry = MaskEntry::wildcard_false();
        assert!(entry.should_handle(0, 0));
        assert!(!entry.should_replace());
    }
}
