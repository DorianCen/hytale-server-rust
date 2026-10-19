// Ported from: `.hytale-source/.../Protocol/.../DrawType.java`,
// `.hytale-source/.../CoreServer/.../asset/type/blocktype/config/BlockType.java`,
// `.hytale-source/.../CoreServer/.../asset/type/fluid/Fluid.java` and
// `.hytale-source/.../CoreServer/.../asset/type/fluid/FluidTicker.java`
//! The block / fluid registry seam.
//!
//! The Java world generator resolves block and fluid *names* to integer ids
//! through the CoreServer asset maps (`BlockType.getAssetMap().getIndex(name)`
//! / `Fluid.getAssetMap().getIndex(name)`). The asset packs themselves (the
//! `BlockType.json` / `Fluid.json` bundles) are **not** part of the shared
//! source tree, so the exact name → id maps cannot be reconstructed here (see
//! `REMAINING.md` § 6).
//!
//! The Rust port therefore carries a *data-driven* seam: the registries below
//! are plain lookup tables that the loader subtask fills with the real assets.
//! Until then [`Registries::empty`] is used, in which every block lookup
//! misses (`getAsset` → `None`) and every fluid lookup falls back to the
//! default max fluid level — exactly the Java semantics of
//! `getAssetOrDefault(index, Fluid.EMPTY)`.
//!
//! ## Bit-exactness
//!
//! The *observable* behaviour of the world generator depends on the registry
//! in exactly two places:
//!
//! * `ChunkGeneratorExecution.setFluid(x, y, z, type, fluid)` defaults the
//!   fluid level to `Fluid.getAssetMap().getAssetOrDefault(fluid,
//!   Fluid.EMPTY).getMaxFluidLevel()` — modelled by
//!   [`FluidRegistry::get_max_fluid_level`].
//! * `WaterPopulator` skips a block when `BlockType.getAssetMap().getAsset(id)`
//!   is `null` or when `FluidTicker.isSolid(blockType)` is `true` — modelled
//!   by [`BlockRegistry::get`] and [`is_solid`].
//!
//! Every other registry field is irrelevant for the generated chunk bytes.

use std::sync::LazyLock;

/// Java `com.hypixel.hytale.protocol.DrawType` — the protocol enum that
/// mirrors the CoreServer `BlockType.drawType` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawType {
    /// Java `DrawType.Empty` = `0`.
    Empty = 0,
    /// Java `DrawType.GizmoCube` = `1`.
    GizmoCube = 1,
    /// Java `DrawType.Cube` = `2`.
    Cube = 2,
    /// Java `DrawType.Model` = `3`.
    Model = 3,
    /// Java `DrawType.CubeWithModel` = `4`.
    CubeWithModel = 4,
}

impl DrawType {
    /// Java `BlockType.drawType` field default — `DrawType.Cube`.
    pub const DEFAULT: DrawType = DrawType::Cube;

    /// Java `DrawType.getValue()`.
    #[inline]
    pub const fn get_value(self) -> i32 {
        self as i32
    }

    /// Java `DrawType.fromValue(int)`.
    ///
    /// Deviation: the Java method throws a `ProtocolException` for an
    /// unknown value; the Rust port returns `None` (the caller decides).
    #[inline]
    pub const fn from_value(value: i32) -> Option<DrawType> {
        match value {
            0 => Some(DrawType::Empty),
            1 => Some(DrawType::GizmoCube),
            2 => Some(DrawType::Cube),
            3 => Some(DrawType::Model),
            4 => Some(DrawType::CubeWithModel),
            _ => None,
        }
    }
}

/// The subset of the Java `BlockType` asset that the world generator needs.
///
/// Java `com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType`
/// is a full asset (textures, hitboxes, materials, …); only `getDrawType()`
/// is observable for the generator (through `FluidTicker.isSolid`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockTypeInfo {
    /// Java `BlockType.drawType`.
    pub draw_type: DrawType,
}

impl BlockTypeInfo {
    /// Java `new BlockType()` — the field default is `DrawType.Cube`.
    pub const fn new(draw_type: DrawType) -> Self {
        Self { draw_type }
    }
}

/// Java `BlockType.drawType` field default.
pub const DEFAULT_BLOCK_TYPE: BlockTypeInfo = BlockTypeInfo {
    draw_type: DrawType::DEFAULT,
};

/// Ported from: CoreServer `.../server/core/asset/type/fluid/FluidTicker.java`
/// — `public static boolean isSolid(@Nonnull BlockType blockType)`.
///
/// ```java
/// public static boolean isSolid(@Nonnull BlockType blockType) {
///     var drawType = blockType.getDrawType();
///     return drawType == DrawType.Cube
///         || drawType == DrawType.CubeWithModel;
/// }
/// ```
///
/// Note: this is *not* `isFullySolid` (which additionally requires
/// `BlockMaterial.Solid`) — the water populator uses `isSolid`.
#[inline]
pub fn is_solid(block_type: &BlockTypeInfo) -> bool {
    block_type.draw_type == DrawType::Cube || block_type.draw_type == DrawType::CubeWithModel
}

/// Java `Fluid.maxFluidLevel` field default — `private int maxFluidLevel = 8;`.
///
/// The Java `Fluid.EMPTY` instance (`new Fluid(EMPTY_KEY)`) keeps that field
/// default, so `Fluid.EMPTY.getMaxFluidLevel()` is `8`.
pub const DEFAULT_MAX_FLUID_LEVEL: i8 = 8;

/// Java `BlockTypeAssetMap<String, BlockType>` — the id → block asset table.
///
/// Java `BlockTypeAssetMap.getAsset(int index)` returns `null` for an index
/// outside the table (and for a slot that was removed), which is exactly the
/// `Option` the Rust port returns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockRegistry {
    /// Java `private final BlockType[] array` — the indexed asset table.
    assets: Vec<Option<BlockTypeInfo>>,
}

impl BlockRegistry {
    /// Java `BlockTypeAssetMap` — the indexed asset table.
    pub fn new(assets: Vec<Option<BlockTypeInfo>>) -> Self {
        Self { assets }
    }

    /// An empty registry — every lookup misses.
    pub fn empty() -> Self {
        Self { assets: Vec::new() }
    }

    /// Java `BlockTypeAssetMap.getAsset(int index)`.
    ///
    /// * an index below zero or beyond the table → `None` (Java: `null`),
    /// * a slot that was removed → `None` (Java: `null`).
    #[inline]
    pub fn get(&self, index: i32) -> Option<&BlockTypeInfo> {
        if index < 0 {
            return None;
        }
        self.assets
            .get(index as usize)
            .and_then(|slot| slot.as_ref())
    }

    /// Java `BlockTypeAssetMap.getAssetOrDefault(int index, T def)`.
    #[inline]
    pub fn get_or_default<'a>(
        &'a self,
        index: i32,
        default: &'a BlockTypeInfo,
    ) -> &'a BlockTypeInfo {
        self.get(index).unwrap_or(default)
    }

    /// The indexed asset table.
    #[inline]
    pub fn assets(&self) -> &[Option<BlockTypeInfo>] {
        &self.assets
    }

    /// The number of indexed slots.
    #[inline]
    pub fn len(&self) -> usize {
        self.assets.len()
    }

    /// Whether the registry holds no slot at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }
}

/// Java `IndexedLookupTableAssetMap<String, Fluid>` — the id → fluid asset
/// table, reduced to the only field the generator observes:
/// `getMaxFluidLevel()`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FluidRegistry {
    /// Java `Fluid.maxFluidLevel` of every indexed fluid.
    max_levels: Vec<i8>,
}

impl FluidRegistry {
    /// Java `IndexedLookupTableAssetMap` — the indexed fluid table.
    pub fn new(max_levels: Vec<i8>) -> Self {
        Self { max_levels }
    }

    /// An empty registry — every lookup falls back to the default level.
    pub fn empty() -> Self {
        Self {
            max_levels: Vec::new(),
        }
    }

    /// Java `Fluid.getAssetMap().getAssetOrDefault(index, Fluid.EMPTY)
    /// .getMaxFluidLevel()`.
    ///
    /// An index outside the table falls back to `Fluid.EMPTY`, whose
    /// `maxFluidLevel` field default is `8`.
    #[inline]
    pub fn get_max_fluid_level(&self, index: i32) -> i8 {
        if index < 0 {
            return DEFAULT_MAX_FLUID_LEVEL;
        }
        self.max_levels
            .get(index as usize)
            .copied()
            .unwrap_or(DEFAULT_MAX_FLUID_LEVEL)
    }

    /// The indexed fluid table.
    #[inline]
    pub fn max_levels(&self) -> &[i8] {
        &self.max_levels
    }

    /// The number of indexed slots.
    #[inline]
    pub fn len(&self) -> usize {
        self.max_levels.len()
    }

    /// Whether the registry holds no slot at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.max_levels.is_empty()
    }
}

/// The block and fluid registries of a world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registries {
    /// Java `BlockType.getAssetMap()`.
    pub blocks: BlockRegistry,
    /// Java `Fluid.getAssetMap()`.
    pub fluids: FluidRegistry,
}

impl Registries {
    /// A registry pair.
    pub fn new(blocks: BlockRegistry, fluids: FluidRegistry) -> Self {
        Self { blocks, fluids }
    }

    /// An empty registry pair — every block lookup misses and every fluid
    /// lookup falls back to the default max fluid level.
    pub fn empty() -> Self {
        Self {
            blocks: BlockRegistry::empty(),
            fluids: FluidRegistry::empty(),
        }
    }
}

/// The empty registry pair — the default until the loader subtask fills the
/// registries with the real assets.
pub static EMPTY: LazyLock<Registries> = LazyLock::new(Registries::empty);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draw_type_values_match_the_protocol_enum() {
        assert_eq!(DrawType::Empty.get_value(), 0);
        assert_eq!(DrawType::GizmoCube.get_value(), 1);
        assert_eq!(DrawType::Cube.get_value(), 2);
        assert_eq!(DrawType::Model.get_value(), 3);
        assert_eq!(DrawType::CubeWithModel.get_value(), 4);

        // The declaration order is the protocol order.
        let all = [
            DrawType::Empty,
            DrawType::GizmoCube,
            DrawType::Cube,
            DrawType::Model,
            DrawType::CubeWithModel,
        ];
        for (index, draw_type) in all.iter().enumerate() {
            assert_eq!(draw_type.get_value() as usize, index);
            assert_eq!(DrawType::from_value(index as i32), Some(*draw_type));
        }
        assert_eq!(DrawType::from_value(5), None);
        assert_eq!(DrawType::from_value(-1), None);
    }

    #[test]
    fn the_default_draw_type_is_cube() {
        // Java: `protected DrawType drawType = DrawType.Cube;`
        assert_eq!(DrawType::DEFAULT, DrawType::Cube);
        assert_eq!(DEFAULT_BLOCK_TYPE.draw_type, DrawType::Cube);
    }

    #[test]
    fn only_cubes_and_cubes_with_a_model_are_solid() {
        // Java `FluidTicker.isSolid`.
        assert!(is_solid(&BlockTypeInfo::new(DrawType::Cube)));
        assert!(is_solid(&BlockTypeInfo::new(DrawType::CubeWithModel)));
        assert!(!is_solid(&BlockTypeInfo::new(DrawType::Empty)));
        assert!(!is_solid(&BlockTypeInfo::new(DrawType::GizmoCube)));
        assert!(!is_solid(&BlockTypeInfo::new(DrawType::Model)));
    }

    #[test]
    fn the_empty_registry_misses_every_lookup() {
        let registries = Registries::empty();

        assert!(registries.blocks.get(0).is_none());
        assert!(registries.blocks.get(42).is_none());
        assert!(registries.blocks.get(-1).is_none());
        assert!(registries.blocks.is_empty());

        // `getAssetOrDefault` falls back to the given default.
        assert_eq!(
            registries.blocks.get_or_default(0, &DEFAULT_BLOCK_TYPE),
            &DEFAULT_BLOCK_TYPE
        );

        // `getAssetOrDefault(fluid, Fluid.EMPTY).getMaxFluidLevel()` → 8.
        assert_eq!(registries.fluids.get_max_fluid_level(0), 8);
        assert_eq!(registries.fluids.get_max_fluid_level(42), 8);
        assert_eq!(registries.fluids.get_max_fluid_level(-1), 8);
        assert!(registries.fluids.is_empty());
    }

    #[test]
    fn the_shared_empty_registry_is_the_empty_pair() {
        assert!(EMPTY.blocks.is_empty());
        assert!(EMPTY.fluids.is_empty());
        assert_eq!(EMPTY.fluids.get_max_fluid_level(3), 8);
    }

    #[test]
    fn the_block_registry_reads_the_asset_at_its_index() {
        let registry = BlockRegistry::new(vec![
            None,
            Some(BlockTypeInfo::new(DrawType::Empty)),
            Some(BlockTypeInfo::new(DrawType::Cube)),
        ]);

        assert!(registry.get(0).is_none());
        assert_eq!(registry.get(1).map(|b| b.draw_type), Some(DrawType::Empty));
        assert_eq!(registry.get(2).map(|b| b.draw_type), Some(DrawType::Cube));
        // Out of range → `None` (Java: `null`).
        assert!(registry.get(3).is_none());
        assert!(registry.get(-1).is_none());

        // `getAssetOrDefault` only falls back for a miss.
        let fallback = BlockTypeInfo::new(DrawType::Model);
        assert_eq!(
            registry.get_or_default(3, &fallback).draw_type,
            DrawType::Model
        );
        assert_eq!(
            registry.get_or_default(2, &fallback).draw_type,
            DrawType::Cube
        );
        assert_eq!(registry.len(), 3);
    }

    #[test]
    fn the_fluid_registry_reports_the_max_level_per_index() {
        let registry = FluidRegistry::new(vec![8, 4, 0, 15]);

        assert_eq!(registry.get_max_fluid_level(0), 8);
        assert_eq!(registry.get_max_fluid_level(1), 4);
        assert_eq!(registry.get_max_fluid_level(2), 0);
        assert_eq!(registry.get_max_fluid_level(3), 15);
        // Out of range → the `Fluid.EMPTY` default.
        assert_eq!(registry.get_max_fluid_level(4), 8);
        assert_eq!(registry.get_max_fluid_level(-1), 8);
        assert_eq!(registry.len(), 4);
    }
}
