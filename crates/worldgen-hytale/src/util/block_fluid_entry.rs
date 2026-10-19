// Ported from: `.hytale-source/.../worldgen/util/BlockFluidEntry.java`
//! Java `com.hypixel.hytale.server.worldgen.util.BlockFluidEntry` — helper for
//! the transition with the split of block types and fluids.
//!
//! Java declares this as a `record`:
//! `public record BlockFluidEntry(int blockId, int rotation, int fluidId)`.

/// Java `record BlockFluidEntry(int blockId, int rotation, int fluidId)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct BlockFluidEntry {
    /// Java `blockId()` — the block type id.
    pub block_id: i32,
    /// Java `rotation()` — the `RotationTuple` index of the block.
    pub rotation: i32,
    /// Java `fluidId()` — the fluid id.
    pub fluid_id: i32,
}

impl BlockFluidEntry {
    /// Java `BlockFluidEntry.EMPTY_ARRAY`.
    pub const EMPTY_ARRAY: [BlockFluidEntry; 0] = [];

    /// Java `BlockFluidEntry.EMPTY` =
    /// `new BlockFluidEntry(BlockType.EMPTY_ID, RotationTuple.NONE_INDEX, Fluid.EMPTY_ID)`.
    ///
    /// The three constants come from CoreServer:
    /// - `BlockType.EMPTY_ID` = `0`
    /// - `RotationTuple.NONE_INDEX` = `0`
    /// - `Fluid.EMPTY_ID` = `0`
    pub const EMPTY: BlockFluidEntry = BlockFluidEntry {
        block_id: 0,
        rotation: 0,
        fluid_id: 0,
    };

    /// Java `new BlockFluidEntry(int blockId, int rotation, int fluidId)`.
    pub const fn new(block_id: i32, rotation: i32, fluid_id: i32) -> Self {
        Self {
            block_id,
            rotation,
            fluid_id,
        }
    }
}

impl Default for BlockFluidEntry {
    /// Java has no default constructor; the empty entry is the closest
    /// equivalent (`BlockFluidEntry.EMPTY`).
    fn default() -> Self {
        Self::EMPTY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_entry_uses_the_coreserver_empty_constants() {
        // BlockType.EMPTY_ID == 0, RotationTuple.NONE_INDEX == 0, Fluid.EMPTY_ID == 0
        assert_eq!(BlockFluidEntry::EMPTY.block_id, 0);
        assert_eq!(BlockFluidEntry::EMPTY.rotation, 0);
        assert_eq!(BlockFluidEntry::EMPTY.fluid_id, 0);
    }

    #[test]
    fn empty_array_is_empty() {
        assert_eq!(BlockFluidEntry::EMPTY_ARRAY.len(), 0);
        assert!(BlockFluidEntry::EMPTY_ARRAY.is_empty());
    }

    #[test]
    fn entries_compare_by_value() {
        let a = BlockFluidEntry::new(4, 1, 9);
        let b = BlockFluidEntry::new(4, 1, 9);
        let c = BlockFluidEntry::new(4, 2, 9);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
