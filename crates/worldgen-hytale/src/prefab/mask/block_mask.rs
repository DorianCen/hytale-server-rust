// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/mask/BlockMask.java`
//! Java `com.hypixel.hytale.server.core.prefab.selection.mask.BlockMask` — a
//! set of [`BlockFilter`]s that are combined with a logical AND.

use std::collections::HashMap;

use crate::prefab::mask::block_filter::{BlockFilter, BlockReader, FilterType};

/// Java `public class BlockMask`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockMask {
    /// Java `private final BlockFilter[] filters`.
    pub filters: Vec<BlockFilter>,
    /// Java `private boolean inverted`.
    pub inverted: bool,
}

impl BlockMask {
    /// Java `public static final BlockMask EMPTY` — the mask without filters.
    pub fn empty() -> Self {
        Self {
            filters: Vec::new(),
            inverted: false,
        }
    }

    /// Java `public BlockMask(BlockFilter[] filters)`.
    pub fn new(filters: Vec<BlockFilter>) -> Self {
        Self {
            filters,
            inverted: false,
        }
    }

    /// Java `public static final String MASK_SEPARATOR` = `","`.
    pub const MASK_SEPARATOR: char = ',';
    /// Java `public static final String ALT_MASK_SEPARATOR` = `";"`.
    pub const ALT_MASK_SEPARATOR: char = ';';
    /// Java `public static final String EMPTY_MASK_CHARACTER` = `"-"`.
    pub const EMPTY_MASK_CHARACTER: &'static str = "-";
    /// Java `public static final String OR_MASK_SEPARATOR` = `"|"`.
    pub const OR_MASK_SEPARATOR: char = '|';

    /// Java `public BlockMask withOptions(@Nonnull BlockFilter.FilterType
    /// filterType, boolean inverted)`.
    pub fn with_options(&self, filter_type: FilterType, inverted: bool) -> Self {
        if self.filters.is_empty() && self.inverted == inverted {
            // Java: `if (this == BlockMask.EMPTY) return this;`
            return self.clone();
        }
        let mut changed = false;
        for filter in &self.filters {
            if filter.block_filter_type != filter_type || filter.inverted != inverted {
                changed = true;
                break;
            }
        }
        if !changed {
            return self.clone();
        }
        let filters = self
            .filters
            .iter()
            .map(|filter| {
                if filter.block_filter_type != filter_type || filter.inverted != inverted {
                    BlockFilter::new(filter_type, filter.blocks.clone(), inverted)
                } else {
                    filter.clone()
                }
            })
            .collect();
        Self {
            filters,
            inverted: self.inverted,
        }
    }

    /// Java `public void setInverted(boolean inverted)`.
    pub fn set_inverted(&mut self, inverted: bool) {
        self.inverted = inverted;
    }

    /// Java `public boolean isInverted()`.
    pub fn is_inverted(&self) -> bool {
        self.inverted
    }

    /// Java `public boolean hasInvalidBlocks()`.
    pub fn has_invalid_blocks(&self) -> bool {
        self.filters
            .iter()
            .any(|filter| filter.has_invalid_blocks())
    }

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
        for filter in &self.filters {
            if filter.is_excluded(
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

    /// Java `public static BlockMask parse(@Nonnull String masks)`.
    pub fn parse(masks: &str) -> Self {
        if masks.is_empty() || masks == Self::EMPTY_MASK_CHARACTER {
            return Self::empty();
        }
        let masks = masks.replace(Self::ALT_MASK_SEPARATOR, &Self::MASK_SEPARATOR.to_string());
        let parts: Vec<&str> = masks.split(Self::MASK_SEPARATOR).collect();
        Self::parse_parts(&parts)
    }

    /// Java `public static BlockMask parse(@Nonnull String[] masks)`.
    pub fn parse_parts(masks: &[&str]) -> Self {
        if masks.is_empty() {
            return Self::empty();
        }
        if masks.len() == 1 {
            return Self::new(vec![BlockFilter::parse(masks[0])]);
        }
        let filters = masks.iter().map(|mask| BlockFilter::parse(mask)).collect();
        Self::group_filters(filters)
    }

    /// Java `public static BlockMask combine(@Nullable BlockMask... masks)`.
    pub fn combine(masks: &[Option<BlockMask>]) -> Self {
        let mut total = 0usize;
        for mask in masks.iter().flatten() {
            if !mask.filters.is_empty() {
                total += mask.filters.len();
            }
        }
        if total == 0 {
            return Self::empty();
        }
        let mut all = Vec::with_capacity(total);
        for mask in masks.iter().flatten() {
            if mask.filters.is_empty() {
                continue;
            }
            all.extend(mask.filters.iter().cloned());
        }
        Self::group_filters(all)
    }

    /// Java `private static BlockMask groupFilters(@Nonnull BlockFilter[]
    /// inputFilters)`.
    ///
    /// The filters are grouped by their type and their inversion, so that
    /// two filters of the same kind share one filter with the union of their
    /// blocks. The group order follows the insertion order of the groups.
    fn group_filters(input_filters: Vec<BlockFilter>) -> Self {
        if input_filters.is_empty() {
            return Self::empty();
        }
        if input_filters.len() == 1 {
            return Self::new(input_filters);
        }

        let mut groups: Vec<(i32, Vec<String>)> = Vec::new();
        let mut group_index: HashMap<i32, usize> = HashMap::new();
        for filter in &input_filters {
            let key = (filter.block_filter_type as i32) << 1 | i32::from(filter.inverted);
            let index = match group_index.get(&key) {
                Some(index) => *index,
                None => {
                    group_index.insert(key, groups.len());
                    groups.push((key, Vec::new()));
                    groups.len() - 1
                }
            };
            groups[index].1.extend(filter.blocks.iter().cloned());
        }

        if groups.len() == input_filters.len() {
            return Self::new(input_filters);
        }

        let filters = groups
            .into_iter()
            .map(|(key, blocks)| {
                let filter_type = FilterType::VALUES[(key >> 1) as usize];
                let inverted = (key & 1) != 0;
                BlockFilter::new(filter_type, blocks, inverted)
            })
            .collect();
        Self::new(filters)
    }
}

impl std::fmt::Display for BlockMask {
    /// Java `public String toString()`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.filters.is_empty() {
            return write!(f, "{}", Self::EMPTY_MASK_CHARACTER);
        }
        let base = self
            .filters
            .iter()
            .map(filter_to_string)
            .collect::<Vec<_>>()
            .join(&Self::MASK_SEPARATOR.to_string());
        if self.inverted {
            write!(f, "!{base}")
        } else {
            write!(f, "{base}")
        }
    }
}

/// Java `BlockFilter.toString()`.
pub(crate) fn filter_to_string(filter: &BlockFilter) -> String {
    let prefix = if filter.inverted {
        FilterType::INVERT_PREFIX
    } else {
        ""
    };
    format!(
        "{}{}{}",
        prefix,
        filter.block_filter_type.get_prefix(),
        filter
            .blocks
            .join(&BlockFilter::BLOCK_SEPARATOR.to_string())
    )
}
