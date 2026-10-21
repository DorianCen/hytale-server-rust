// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/mask/MultiBlockMask.java`
//! Java `com.hypixel.hytale.server.core.prefab.selection.mask.MultiBlockMask` —
//! a mask that holds other masks.

use crate::prefab::mask::block_filter::BlockReader;
use crate::prefab::mask::block_mask::BlockMask;

/// Java `public class MultiBlockMask extends BlockMask`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiBlockMask {
    /// Java `private final BlockMask[] masks`.
    pub masks: Vec<BlockMask>,
    /// Java `private boolean inverted` (inherited from `BlockMask`).
    pub inverted: bool,
}

impl MultiBlockMask {
    /// Java `public MultiBlockMask(BlockMask[] masks)`.
    pub fn new(masks: Vec<BlockMask>) -> Self {
        Self {
            masks,
            inverted: false,
        }
    }

    /// Java `private static final String BLOCK_MASK_SEPARATOR` = `";"`.
    pub const BLOCK_MASK_SEPARATOR: char = ';';

    /// Java `public boolean isExcluded(@Nonnull BlockReader accessor, int x,
    /// int y, int z, @Nullable Vector3i selectionMin, @Nullable Vector3i
    /// selectionMax, int blockId, int fluidId)`.
    #[allow(clippy::too_many_arguments)]
    pub fn is_excluded(
        &self,
        accessor: &dyn BlockReader,
        x: i32,
        y: i32,
        z: i32,
        selection_min: Option<(i32, i32, i32)>,
        selection_max: Option<(i32, i32, i32)>,
        block_id: i32,
        fluid_id: i32,
    ) -> bool {
        let mut excluded = false;
        for mask in &self.masks {
            if mask.is_excluded(
                accessor,
                x,
                y,
                z,
                selection_min,
                selection_max,
                block_id,
                fluid_id,
            ) {
                excluded = true;
                break;
            }
        }
        self.inverted != excluded
    }
}

impl std::fmt::Display for MultiBlockMask {
    /// Java `public String toString()`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.masks.is_empty() {
            return write!(f, "{}", BlockMask::EMPTY_MASK_CHARACTER);
        }
        let base = self
            .masks
            .iter()
            .map(|mask| mask.to_string())
            .collect::<Vec<_>>()
            .join(&Self::BLOCK_MASK_SEPARATOR.to_string());
        if self.inverted {
            write!(f, "!{base}")
        } else {
            write!(f, "{base}")
        }
    }
}
