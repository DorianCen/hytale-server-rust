// Ported from: `.hytale-source/.../CoreServer/.../prefab/selection/mask/BlockFilter.java`
//! Java `com.hypixel.hytale.server.core.prefab.selection.mask.BlockFilter` —
//! one filter of a [`BlockMask`](crate::prefab::mask::block_mask::BlockMask).
//!
//! The filter matches a block (or a fluid) either at the position itself
//! (`TargetBlock`) or in one of its neighbours (`AboveBlock`,
//! `BelowBlock`, `AdjacentBlock`, `NeighborBlock`, the cardinal and the
//! diagonal directions, and the `Selection` area).

use std::collections::HashSet;

use worldgen_model::registry::Registries;

/// The world accessor the filters test against.
///
/// Java `com.hypixel.hytale.server.core.universe.world.accessor.BlockReader`.
pub trait BlockReader {
    /// Java `int getBlock(int x, int y, int z)`.
    fn get_block(&self, x: i32, y: i32, z: i32) -> i32;

    /// Java `int getFluidId(int x, int y, int z)`.
    fn get_fluid_id(&self, x: i32, y: i32, z: i32) -> i32;
}

/// Java `public class BlockFilter`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockFilter {
    /// Java `@Nonnull private final FilterType blockFilterType`.
    pub block_filter_type: FilterType,
    /// Java `@Nonnull private final String[] blocks`.
    pub blocks: Vec<String>,
    /// Java `private final boolean inverted`.
    pub inverted: bool,
    /// Java `private IntSet resolvedBlocks` — the lazily resolved block ids.
    pub resolved_blocks: Option<HashSet<i32>>,
    /// Java `private IntSet resolvedFluids` — the lazily resolved fluid ids.
    pub resolved_fluids: Option<HashSet<i32>>,
    /// Java `private boolean hasInvalidBlocks`.
    pub has_invalid_blocks: bool,
}

/// Java `BlockFilter.FilterType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterType {
    /// Java `TargetBlock` — the block at the position itself.
    TargetBlock,
    /// Java `AboveBlock` — the block above.
    AboveBlock,
    /// Java `BelowBlock` — the block below.
    BelowBlock,
    /// Java `AdjacentBlock` — one of the four horizontal neighbours.
    AdjacentBlock,
    /// Java `NeighborBlock` — one of the 26 neighbours.
    NeighborBlock,
    /// Java `NorthBlock` — the block in the -Z direction.
    NorthBlock,
    /// Java `EastBlock` — the block in the +X direction.
    EastBlock,
    /// Java `SouthBlock` — the block in the +Z direction.
    SouthBlock,
    /// Java `WestBlock` — the block in the -X direction.
    WestBlock,
    /// Java `DiagonalXy` — one of the four diagonals in the XY plane.
    DiagonalXy,
    /// Java `DiagonalXz` — one of the four diagonals in the XZ plane.
    DiagonalXz,
    /// Java `DiagonalZy` — one of the four diagonals in the ZY plane.
    DiagonalZy,
    /// Java `Selection` — the block is inside the selection area.
    Selection,
}

impl FilterType {
    /// Java `FilterType.INVERT_PREFIX` = `"!"`.
    pub const INVERT_PREFIX: &'static str = "!";
    /// Java `FilterType.TARGET_BLOCK_PREFIX` = `""`.
    pub const TARGET_BLOCK_PREFIX: &'static str = "";
    /// Java `FilterType.ABOVE_BLOCK_PREFIX` = `">"`.
    pub const ABOVE_BLOCK_PREFIX: &'static str = ">";
    /// Java `FilterType.BELOW_BLOCK_PREFIX` = `"<"`.
    pub const BELOW_BLOCK_PREFIX: &'static str = "<";
    /// Java `FilterType.ADJACENT_BLOCK_PREFIX` = `"~"`.
    pub const ADJACENT_BLOCK_PREFIX: &'static str = "~";
    /// Java `FilterType.NEIGHBOR_BLOCK_PREFIX` = `"^"`.
    pub const NEIGHBOR_BLOCK_PREFIX: &'static str = "^";
    /// Java `FilterType.SELECTION_PREFIX` = `"#"`.
    pub const SELECTION_PREFIX: &'static str = "#";
    /// Java `FilterType.CARDINAL_NORTH_PREFIX` = `"+n"`.
    pub const CARDINAL_NORTH_PREFIX: &'static str = "+n";
    /// Java `FilterType.CARDINAL_EAST_PREFIX` = `"+e"`.
    pub const CARDINAL_EAST_PREFIX: &'static str = "+e";
    /// Java `FilterType.CARDINAL_SOUTH_PREFIX` = `"+s"`.
    pub const CARDINAL_SOUTH_PREFIX: &'static str = "+s";
    /// Java `FilterType.CARDINAL_WEST_PREFIX` = `"+w"`.
    pub const CARDINAL_WEST_PREFIX: &'static str = "+w";
    /// Java `FilterType.DIAGONAL_XY_PREFIX` = `"%xy"`.
    pub const DIAGONAL_XY_PREFIX: &'static str = "%xy";
    /// Java `FilterType.DIAGONAL_XZ_PREFIX` = `"%xz"`.
    pub const DIAGONAL_XZ_PREFIX: &'static str = "%xz";
    /// Java `FilterType.DIAGONAL_ZY_PREFIX` = `"%zy"`.
    pub const DIAGONAL_ZY_PREFIX: &'static str = "%zy";

    /// Java `FilterType.VALUES`.
    pub const VALUES: [FilterType; 13] = [
        FilterType::TargetBlock,
        FilterType::AboveBlock,
        FilterType::BelowBlock,
        FilterType::AdjacentBlock,
        FilterType::NeighborBlock,
        FilterType::NorthBlock,
        FilterType::EastBlock,
        FilterType::SouthBlock,
        FilterType::WestBlock,
        FilterType::DiagonalXy,
        FilterType::DiagonalXz,
        FilterType::DiagonalZy,
        FilterType::Selection,
    ];

    /// Java `boolean hasBlocks()`.
    pub const fn has_blocks(self) -> bool {
        !matches!(self, FilterType::Selection)
    }

    /// Java `String getPrefix()`.
    pub const fn get_prefix(self) -> &'static str {
        match self {
            FilterType::TargetBlock => Self::TARGET_BLOCK_PREFIX,
            FilterType::AboveBlock => Self::ABOVE_BLOCK_PREFIX,
            FilterType::BelowBlock => Self::BELOW_BLOCK_PREFIX,
            FilterType::AdjacentBlock => Self::ADJACENT_BLOCK_PREFIX,
            FilterType::NeighborBlock => Self::NEIGHBOR_BLOCK_PREFIX,
            FilterType::NorthBlock => Self::CARDINAL_NORTH_PREFIX,
            FilterType::EastBlock => Self::CARDINAL_EAST_PREFIX,
            FilterType::SouthBlock => Self::CARDINAL_SOUTH_PREFIX,
            FilterType::WestBlock => Self::CARDINAL_WEST_PREFIX,
            FilterType::DiagonalXy => Self::DIAGONAL_XY_PREFIX,
            FilterType::DiagonalXz => Self::DIAGONAL_XZ_PREFIX,
            FilterType::DiagonalZy => Self::DIAGONAL_ZY_PREFIX,
            FilterType::Selection => Self::SELECTION_PREFIX,
        }
    }

    /// Java `static FilterType parse(@Nonnull String str, int index)`.
    ///
    /// The `TargetBlock` type has an empty prefix, so it is never matched
    /// explicitly — it is the fallback.
    pub fn parse(text: &str, index: usize) -> FilterType {
        // Java skips the `TargetBlock` entry (its prefix is empty and it
        // would match everything).
        for filter_type in FilterType::VALUES {
            if filter_type == FilterType::TargetBlock {
                continue;
            }
            if text[index..].starts_with(filter_type.get_prefix()) {
                return filter_type;
            }
        }
        FilterType::TargetBlock
    }
}

impl BlockFilter {
    /// Java `public BlockFilter(@Nonnull FilterType blockFilterType, @Nonnull
    /// String[] blocks, boolean inverted)`.
    pub fn new(block_filter_type: FilterType, blocks: Vec<String>, inverted: bool) -> Self {
        Self {
            block_filter_type,
            blocks,
            inverted,
            resolved_blocks: None,
            resolved_fluids: None,
            has_invalid_blocks: false,
        }
    }

    /// Java `public static BlockFilter parse(@Nonnull String str)`.
    pub fn parse(text: &str) -> Self {
        let parts = Self::parse_components(text);
        let blocks = if parts.filter_type.has_blocks() {
            parts
                .blocks
                .split(Self::BLOCK_SEPARATOR)
                .map(|block| block.to_string())
                .collect()
        } else {
            Vec::new()
        };
        Self::new(parts.filter_type, blocks, parts.inverted)
    }

    /// Java `BlockFilter.BLOCK_SEPARATOR` = `"|"`.
    pub const BLOCK_SEPARATOR: char = '|';

    /// Java `public static ParsedFilterParts parseComponents(@Nonnull String
    /// str)`.
    pub fn parse_components(text: &str) -> ParsedFilterParts {
        let inverted = text.starts_with(FilterType::INVERT_PREFIX);
        let mut index = if inverted { 1 } else { 0 };
        let filter_type = FilterType::parse(text, index);
        index += filter_type.get_prefix().len();
        let blocks = text[index.min(text.len())..].to_string();
        ParsedFilterParts {
            filter_type,
            inverted,
            blocks,
        }
    }

    /// Java `public void resolve()` — resolves the block and fluid names
    /// against the registries.
    ///
    /// Deviation: the Java filter also resolves *items* that place a fluid
    /// (`Item.getAssetMap()`); the Rust port has no item store, so only the
    /// fluid names resolve to fluids.
    pub fn resolve(&mut self, registries: &Registries) {
        if self.resolved_blocks.is_some() {
            return;
        }
        let result = Self::parse_blocks_and_fluids(registries, &self.blocks);
        self.resolved_blocks = Some(result.blocks);
        self.resolved_fluids = result.fluids;
        self.has_invalid_blocks = result.has_invalid_blocks;
    }

    /// Java `public boolean hasInvalidBlocks()`.
    pub fn has_invalid_blocks(&self) -> bool {
        self.has_invalid_blocks
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
        let exclude = !self.is_included(
            accessor,
            x,
            y,
            z,
            selection_min,
            selection_max,
            block_id,
            fluid_id,
        );
        self.inverted != exclude
    }

    /// Java `private boolean isIncluded(...)`.
    #[allow(clippy::too_many_arguments)]
    fn is_included(
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
        match self.block_filter_type {
            FilterType::TargetBlock => {
                let blocks = self.resolved_blocks.as_ref();
                let matches_block = blocks.is_some_and(|blocks| blocks.contains(&block_id));
                let matches_fluid = fluid_id >= 0
                    && self
                        .resolved_fluids
                        .as_ref()
                        .is_some_and(|fluids| fluids.contains(&fluid_id));
                matches_block || matches_fluid
            }
            FilterType::AboveBlock => self.matches_at(accessor, x, y - 1, z),
            FilterType::BelowBlock => self.matches_at(accessor, x, y + 1, z),
            FilterType::AdjacentBlock => {
                self.matches_at(accessor, x - 1, y, z)
                    || self.matches_at(accessor, x + 1, y, z)
                    || self.matches_at(accessor, x, y, z - 1)
                    || self.matches_at(accessor, x, y, z + 1)
            }
            FilterType::NeighborBlock => {
                for xo in -1..=1 {
                    for yo in -1..=1 {
                        for zo in -1..=1 {
                            if xo == 0 && yo == 0 && zo == 0 {
                                continue;
                            }
                            if self.matches_at(accessor, x + xo, y + yo, z + zo) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
            FilterType::NorthBlock => self.matches_at(accessor, x, y, z - 1),
            FilterType::EastBlock => self.matches_at(accessor, x, y, z + 1),
            // Java: `SouthBlock` and `WestBlock` both use `z + 1` / `z - 1`
            // in the same way as `EastBlock` / `NorthBlock` — the Java
            // switch is transcribed verbatim (including the fact that
            // `SouthBlock` looks at `z + 1`).
            FilterType::SouthBlock => self.matches_at(accessor, x, y, z + 1),
            FilterType::WestBlock => self.matches_at(accessor, x, y, z - 1),
            FilterType::DiagonalXy => {
                self.matches_at(accessor, x - 1, y + 1, z)
                    || self.matches_at(accessor, x - 1, y - 1, z)
                    || self.matches_at(accessor, x + 1, y + 1, z)
                    || self.matches_at(accessor, x + 1, y - 1, z)
            }
            FilterType::DiagonalXz => {
                self.matches_at(accessor, x - 1, y, z + 1)
                    || self.matches_at(accessor, x - 1, y, z - 1)
                    || self.matches_at(accessor, x + 1, y, z + 1)
                    || self.matches_at(accessor, x + 1, y, z - 1)
            }
            FilterType::DiagonalZy => {
                self.matches_at(accessor, x, y - 1, z + 1)
                    || self.matches_at(accessor, x, y - 1, z - 1)
                    || self.matches_at(accessor, x, y + 1, z + 1)
                    || self.matches_at(accessor, x, y + 1, z - 1)
            }
            FilterType::Selection => {
                let (Some(min), Some(max)) = (selection_min, selection_max) else {
                    return false;
                };
                x >= min.0 && y >= min.1 && z >= min.2 && x <= max.0 && y <= max.1 && z <= max.2
            }
        }
    }

    /// Java `private boolean matchesAt(@Nonnull BlockReader accessor, int x,
    /// int y, int z)`.
    fn matches_at(&self, accessor: &dyn BlockReader, x: i32, y: i32, z: i32) -> bool {
        let blocks = self.resolved_blocks.as_ref();
        if blocks.is_some_and(|blocks| blocks.contains(&accessor.get_block(x, y, z))) {
            return true;
        }
        self.resolved_fluids
            .as_ref()
            .is_some_and(|fluids| fluids.contains(&accessor.get_fluid_id(x, y, z)))
    }
}

/// Java `public record ParsedFilterParts(FilterType type, boolean inverted,
/// String blocks)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedFilterParts {
    /// The filter type.
    pub filter_type: FilterType,
    /// Whether the filter is inverted.
    pub inverted: bool,
    /// The raw block list.
    pub blocks: String,
}

/// Java `public record BlocksAndFluids(@Nonnull IntSet blocks, @Nullable IntSet
/// fluids, boolean hasInvalidBlocks)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlocksAndFluids {
    /// The resolved block ids.
    pub blocks: HashSet<i32>,
    /// The resolved fluid ids (`None` when there are none).
    pub fluids: Option<HashSet<i32>>,
    /// Whether an unknown block name was found.
    pub has_invalid_blocks: bool,
}

impl BlockFilter {
    /// Java `public static BlocksAndFluids parseBlocksAndFluids(@Nonnull
    /// String[] blocksArgs)`.
    pub fn parse_blocks_and_fluids(
        registries: &Registries,
        blocks_args: &[String],
    ) -> BlocksAndFluids {
        let mut blocks: HashSet<i32> = HashSet::new();
        let mut fluids: HashSet<i32> = HashSet::new();
        let mut invalid = false;

        for block_arg in blocks_args {
            // Java: the item store is checked first (an item that places a
            // fluid); the Rust port has no item store, so the fluid names
            // are resolved directly.
            if let Some(direct_fluid_id) = registries.fluids.index_of(block_arg) {
                if direct_fluid_id == 0 {
                    // Java `Fluid.EMPTY_ID` → the empty *block*.
                    blocks.insert(0);
                } else {
                    fluids.insert(direct_fluid_id);
                }
                continue;
            }

            let block_id = parse_block(registries, block_arg);
            if block_id == 0 && !block_arg.eq_ignore_ascii_case("Empty") {
                invalid = true;
            }
            blocks.insert(block_id);
        }

        BlocksAndFluids {
            blocks,
            fluids: if fluids.is_empty() {
                None
            } else {
                Some(fluids)
            },
            has_invalid_blocks: invalid,
        }
    }
}

/// Java `public static int parseBlock(@Nonnull String blockText)` (the
/// `BlockPattern` helper the filter uses).
pub fn parse_block(registries: &Registries, block_text: &str) -> i32 {
    if let Ok(id) = block_text.parse::<i32>() {
        if registries.blocks.get(id).is_some() {
            return id;
        }
        panic!("Block with id '{block_text}' doesn't exist!");
    }
    let mut text = block_text.replace(';', ",");
    if let Some(index) = text.find('|') {
        text = text[..index].to_string();
    }
    // Java logs a warning and uses the empty block for an unknown name.
    registries.blocks.index_of(&text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_type_is_parsed_from_its_prefix() {
        let parts = BlockFilter::parse_components(">Rock_Stone");
        assert_eq!(parts.filter_type, FilterType::AboveBlock);
        assert!(!parts.inverted);
        assert_eq!(parts.blocks, "Rock_Stone");

        let parts = BlockFilter::parse_components("!~Rock_Stone");
        assert_eq!(parts.filter_type, FilterType::AdjacentBlock);
        assert!(parts.inverted);

        // The target block has an empty prefix — it is the fallback.
        let parts = BlockFilter::parse_components("Rock_Stone");
        assert_eq!(parts.filter_type, FilterType::TargetBlock);
    }

    #[test]
    fn the_block_list_is_split_at_the_or_separator() {
        let filter = BlockFilter::parse("Rock_Stone|Rock_Sandstone");
        assert_eq!(filter.blocks, vec!["Rock_Stone", "Rock_Sandstone"]);
        assert_eq!(filter.block_filter_type, FilterType::TargetBlock);
    }

    #[test]
    fn the_fluids_resolve_to_fluid_ids() {
        let registries = Registries::load();
        let mut filter = BlockFilter::new(
            FilterType::TargetBlock,
            vec!["Water_Source".to_string()],
            false,
        );
        filter.resolve(&registries);
        let fluids = filter.resolved_fluids.as_ref().expect("the fluid resolves");
        assert!(fluids.contains(&registries.fluids.index_of("Water_Source").unwrap()));
        assert!(filter.resolved_blocks.as_ref().unwrap().is_empty());
    }
}

#[cfg(test)]
mod mask_tests {
    use super::*;
    use std::collections::HashMap;

    /// A world accessor with a fixed block and fluid.
    struct TestReader {
        blocks: HashMap<(i32, i32, i32), i32>,
        fluids: HashMap<(i32, i32, i32), i32>,
    }

    impl TestReader {
        fn new() -> Self {
            Self {
                blocks: HashMap::new(),
                fluids: HashMap::new(),
            }
        }
    }

    impl BlockReader for TestReader {
        fn get_block(&self, x: i32, y: i32, z: i32) -> i32 {
            self.blocks.get(&(x, y, z)).copied().unwrap_or(0)
        }

        fn get_fluid_id(&self, x: i32, y: i32, z: i32) -> i32 {
            self.fluids.get(&(x, y, z)).copied().unwrap_or(0)
        }
    }

    /// A filter of the given type over the given block ids.
    fn filter(filter_type: FilterType, blocks: &[i32], inverted: bool) -> BlockFilter {
        let mut filter = BlockFilter::new(
            filter_type,
            blocks.iter().map(|id| id.to_string()).collect(),
            inverted,
        );
        filter.resolved_blocks = Some(blocks.iter().copied().collect());
        filter.resolved_fluids = None;
        filter
    }

    #[test]
    fn the_target_block_filter_matches_the_block_itself() {
        let reader = TestReader::new();
        let filter = filter(FilterType::TargetBlock, &[7], false);
        assert!(!filter.is_excluded(&reader, 0, 0, 0, None, None, 7, -1));
        assert!(filter.is_excluded(&reader, 0, 0, 0, None, None, 8, -1));
    }

    #[test]
    fn the_neighbour_filters_look_at_the_neighbours() {
        let mut reader = TestReader::new();
        // Java quirk: the `AboveBlock` filter looks at `y - 1` and the
        // `BelowBlock` filter at `y + 1`.
        reader.blocks.insert((0, -1, 0), 7);
        let above = filter(FilterType::AboveBlock, &[7], false);
        assert!(!above.is_excluded(&reader, 0, 0, 0, None, None, 0, -1));
        let below = filter(FilterType::BelowBlock, &[7], false);
        assert!(below.is_excluded(&reader, 0, 0, 0, None, None, 0, -1));
        let neighbour = filter(FilterType::NeighborBlock, &[7], false);
        assert!(!neighbour.is_excluded(&reader, 0, 0, 0, None, None, 0, -1));
    }

    #[test]
    fn the_selection_filter_matches_the_selection_area() {
        let reader = TestReader::new();
        let filter = filter(FilterType::Selection, &[7], false);
        assert!(!filter.is_excluded(&reader, 1, 2, 3, Some((0, 0, 0)), Some((4, 4, 4)), 0, -1));
        assert!(filter.is_excluded(&reader, 5, 2, 3, Some((0, 0, 0)), Some((4, 4, 4)), 0, -1));
        // Without a selection the filter never matches.
        assert!(filter.is_excluded(&reader, 1, 2, 3, None, None, 0, -1));
    }

    #[test]
    fn an_inverted_filter_is_negated() {
        let reader = TestReader::new();
        let filter = filter(FilterType::TargetBlock, &[7], true);
        assert!(filter.is_excluded(&reader, 0, 0, 0, None, None, 7, -1));
        assert!(!filter.is_excluded(&reader, 0, 0, 0, None, None, 8, -1));
    }
}
