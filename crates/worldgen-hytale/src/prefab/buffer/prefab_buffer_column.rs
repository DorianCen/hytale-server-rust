// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/buffer/impl/PrefabBufferColumn.java`
//! Java `PrefabBufferColumn` — the blocks of one `(x, z)` column of a
//! prefab.
//!
//! # Layout deviation
//!
//! The Java column stores its blocks in a packed byte segment (a `short`
//! mask per block followed by the variable-width id / offset / chance /
//! rotation / filler / fluid fields). The Rust port stores the same
//! information in a plain [`Vec`] of [`PrefabBufferBlockEntry`] — the
//! *values* are identical, only the memory layout differs. Every height and
//! index computation of the Java buffer is reproduced exactly (see
//! [`PrefabBufferAccessor`](crate::prefab::buffer::prefab_buffer::PrefabBufferAccessor)).

use super::prefab_buffer_block_entry::PrefabBufferBlockEntry;

/// Java `public class PrefabBufferColumn`.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabBufferColumn {
    /// Java `private final int blockCount`.
    pub block_count: usize,
    /// The blocks of the column, ordered by ascending `y` (the Java
    /// column stores them in that order as well).
    pub entries: Vec<PrefabBufferBlockEntry>,
}

impl PrefabBufferColumn {
    /// Java `PrefabBufferColumn(int blockCount, MemorySegment
    /// memorySegment, Holder<EntityStore>[] entityHolders, Int2ObjectMap
    /// <Holder<ChunkStore>> blockComponents)`.
    ///
    /// Deviation: the entity holders and the block components (the ECS
    /// state) are not ported (see `REMAINING.md` § 2) — the column only
    /// carries its blocks.
    pub fn new(entries: Vec<PrefabBufferBlockEntry>) -> Self {
        Self {
            block_count: entries.len(),
            entries,
        }
    }

    /// Java `int getBlockCount()`.
    pub fn get_block_count(&self) -> usize {
        self.block_count
    }
}
