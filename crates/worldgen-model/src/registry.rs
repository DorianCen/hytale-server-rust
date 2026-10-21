// Ported from: `.hytale-source/.../Protocol/.../DrawType.java`,
// `.hytale-source/.../CoreServer/.../asset/type/blocktype/config/BlockType.java`,
// `.hytale-source/.../CoreServer/.../asset/type/fluid/Fluid.java` and
// `.hytale-source/.../CoreServer/.../asset/type/fluid/FluidTicker.java`
//! The block / fluid registry seam.
//!
//! The Java world generator resolves block and fluid *names* to integer ids
//! through the CoreServer asset maps (`BlockType.getAssetMap().getIndex(name)`
//! / `Fluid.getAssetMap().getIndex(name)`). The base-game asset pack defines
//! the block types as the `"BlockType"` objects embedded in the
//! `Server/Item/Items/**` item assets, and the numeric ids are handed out by
//! the asset pipeline in load order — so they can only be obtained by running
//! the real Java pipeline (see `tools/java-dumper`, which dumps them to
//! `testdata/golden/registries.json`).
//!
//! The Rust port therefore carries a *data-driven* seam: the registries below
//! are plain lookup tables that [`Registries::from_golden`] fills with the
//! real assets. Until then [`Registries::empty`] is used, in which every block
//! lookup misses (`BlockRegistry::get` → `None`) and every fluid lookup falls
//! back to the default max fluid level — exactly the Java semantics of
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

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::Deserialize;

/// Java `CaseInsensitiveHashStrategy` key normalization — the asset maps look
/// names up case-insensitively. Asset ids are ASCII, so the ASCII lowercase
/// form is an exact key (and avoids allocating for already-lowercase keys in
/// the common case).
fn name_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

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

    /// Java `DrawType.valueOf(String)` (the enum constant name).
    #[inline]
    pub fn from_name(name: &str) -> Option<DrawType> {
        match name {
            "Empty" => Some(DrawType::Empty),
            "GizmoCube" => Some(DrawType::GizmoCube),
            "Cube" => Some(DrawType::Cube),
            "Model" => Some(DrawType::Model),
            "CubeWithModel" => Some(DrawType::CubeWithModel),
            _ => None,
        }
    }
}

/// The part of Java `BlockType.getSupport(int rotationIndex)` that the world
/// generator observes.
///
/// Java returns `@Nullable Map<BlockFace, RequiredBlockFaceSupport[]>` and
/// `CaveNodeShapeUtils.invalidateCover` only asks `containsKey(BlockFace.DOWN)`
/// (a `FLOOR` cover) and `containsKey(BlockFace.UP)` (a `CEILING` cover), so
/// the map is reduced to those two membership bits.
///
/// The rotation is part of the Java lookup: `getSupport(rotationIndex)` maps
/// every face key through `BlockFace.rotate(yaw, pitch, roll)` of the
/// `RotationTuple` (and the required support through the matching
/// `RequiredBlockFaceSupport.rotate`), so a face may appear / disappear under
/// rotation. The golden registry dump therefore carries one map per rotation
/// index (see [`BlockTypeInfo::get_support`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockSupport {
    /// Java `supportsMap.containsKey(BlockFace.DOWN)`.
    pub down: bool,
    /// Java `supportsMap.containsKey(BlockFace.UP)`.
    pub up: bool,
}

/// Java `RotationTuple.VALUES.length` — the number of distinct block rotation
/// indices (`Rotation.values().length³`; `Rotation` has 4 constants, so 64).
/// Index `0` is Java `RotationTuple.NONE_INDEX` (the unrotated base).
pub const SUPPORT_ROTATION_COUNT: usize = 64;

/// Java `com.hypixel.hytale.protocol.Opacity` — the block opacity enum of
/// `BlockType.getOpacity()` (protocol values `0..=3`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Opacity {
    /// Java `Solid` = `0` (the `BlockType` field default).
    #[default]
    Solid = 0,
    /// Java `Semitransparent` = `1`.
    Semitransparent = 1,
    /// Java `Cutout` = `2`.
    Cutout = 2,
    /// Java `Transparent` = `3`.
    Transparent = 3,
}

impl Opacity {
    /// Java `Opacity.valueOf(String)` — `None` for an unknown name.
    pub fn from_name(name: &str) -> Option<Opacity> {
        match name {
            "Solid" => Some(Opacity::Solid),
            "Semitransparent" => Some(Opacity::Semitransparent),
            "Cutout" => Some(Opacity::Cutout),
            "Transparent" => Some(Opacity::Transparent),
            _ => None,
        }
    }

    /// Java `getOpacity() != Opacity.Transparent` (the
    /// `GeneratedBlockChunk.getHeight` opacity test).
    #[inline]
    pub const fn is_transparent(self) -> bool {
        matches!(self, Opacity::Transparent)
    }
}

/// The subset of the Java `BlockType` asset that the world generator needs.
///
/// Java `com.hypixel.hytale.server.core.asset.type.blocktype.config.BlockType`
/// is a full asset (textures, hitboxes, materials, …); only `getDrawType()`
/// is observable for the chunk bytes (through `FluidTicker.isSolid`),
/// `getOpacity()` for the surface-limited cave height and `getSupport()` for
/// the cave covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockTypeInfo {
    /// Java `BlockType.drawType`.
    pub draw_type: DrawType,
    /// Java `BlockType.opacity` (`GeneratedBlockChunk.getHeight`).
    pub opacity: Opacity,
    /// Java `BlockType.support` — the base map (rotation index `0`, Java
    /// `RotationTuple.NONE_INDEX`). `None` is the Java `null` map, i.e. the
    /// block requires no support.
    pub support: Option<BlockSupport>,
    /// The rotated support maps of the golden dump — the full per-rotation
    /// table (index `0` duplicates [`BlockTypeInfo::support`], Java
    /// `RotationTuple.NONE_INDEX` is table entry 0). Empty when
    /// [`BlockTypeInfo::support`] is `None`, which is the Java base-null
    /// short circuit.
    pub rotated_support: Vec<BlockSupport>,
}

impl BlockTypeInfo {
    /// Java `new BlockType()` — the field default is `DrawType.Cube`,
    /// `Opacity.Solid` and a `null` support map.
    pub const fn new(draw_type: DrawType) -> Self {
        Self {
            draw_type,
            opacity: Opacity::Solid,
            support: None,
            rotated_support: Vec::new(),
        }
    }

    /// The same block type with an `Opacity` (Java: the `Opacity` asset
    /// field).
    pub const fn with_opacity(mut self, opacity: Opacity) -> Self {
        self.opacity = opacity;
        self
    }

    /// The same block type with a support map (Java: the `Support` asset
    /// field) that does not depend on the rotation.
    pub const fn with_support(mut self, support: BlockSupport) -> Self {
        self.support = Some(support);
        self
    }

    /// The same block type with the per-rotation support maps of the golden
    /// dump (index `0` = the base map, Java `RotationTuple.NONE_INDEX`; the
    /// table must have [`SUPPORT_ROTATION_COUNT`] entries and entry `0` is
    /// also kept in [`BlockTypeInfo::rotated_support`] so the lookup stays a
    /// plain index).
    pub fn with_support_rotations(mut self, supports: Vec<BlockSupport>) -> Self {
        self.support = supports.first().copied();
        self.rotated_support = supports;
        self
    }

    /// Java `@Nullable Map<BlockFace, RequiredBlockFaceSupport[]>
    /// getSupport(int rotationIndex)`.
    ///
    /// The dump carries the `containsKey(BlockFace.DOWN/UP)` result of every
    /// rotation index (see the dump `meta.supportField`), so the lookup is a
    /// plain index. Java guarantees a non-`null` map for every in-range index
    /// when the base map is non-`null`; an out-of-range rotation index (which
    /// cannot occur — the chunk rotation storage is bounded by the Java table)
    /// falls back to the base map. Java `RotationTuple.NONE_INDEX` is `0`.
    #[inline]
    pub fn get_support(&self, rotation_index: i32) -> Option<BlockSupport> {
        let base = self.support?;
        if rotation_index <= 0 {
            return Some(base);
        }
        self.rotated_support
            .get(rotation_index as usize)
            .copied()
            .or(Some(base))
    }
}

/// Java `BlockType.drawType` field default.
pub const DEFAULT_BLOCK_TYPE: BlockTypeInfo = BlockTypeInfo {
    draw_type: DrawType::DEFAULT,
    opacity: Opacity::Solid,
    support: None,
    rotated_support: Vec::new(),
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
    /// The block type key of every indexed slot (`None` for a hole).
    names: Vec<Option<String>>,
    /// Java `BlockTypeAssetMap.keyToIndex` — the case-insensitive name → id
    /// direction (`BlockType.getAssetMap().getIndex(name)`).
    name_to_index: HashMap<String, i32>,
}

impl BlockRegistry {
    /// Java `BlockTypeAssetMap` — the indexed asset table (without names; the
    /// name → id direction stays empty, matching a table built without keys).
    pub fn new(assets: Vec<Option<BlockTypeInfo>>) -> Self {
        let names = vec![None; assets.len()];
        Self {
            assets,
            names,
            name_to_index: HashMap::new(),
        }
    }

    /// The indexed asset table plus the block type key of every slot.
    ///
    /// Java `BlockTypeAssetMap.putAll` fills both directions from the same
    /// loaded assets; this constructor mirrors that for the golden dump.
    pub fn with_names(assets: Vec<Option<BlockTypeInfo>>, names: Vec<Option<String>>) -> Self {
        assert_eq!(
            assets.len(),
            names.len(),
            "the block asset and name tables must have the same length"
        );
        let mut name_to_index = HashMap::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            if let Some(name) = name {
                name_to_index.entry(name_key(name)).or_insert(index as i32);
            }
        }
        Self {
            assets,
            names,
            name_to_index,
        }
    }

    /// An empty registry — every lookup misses.
    pub fn empty() -> Self {
        Self {
            assets: Vec::new(),
            names: Vec::new(),
            name_to_index: HashMap::new(),
        }
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

    /// The block type key at `index` (`None` for a hole or out of range).
    #[inline]
    pub fn name(&self, index: i32) -> Option<&str> {
        if index < 0 {
            return None;
        }
        self.names
            .get(index as usize)
            .and_then(|name| name.as_deref())
    }

    /// The numeric id of a block name (Java: `getIndex(name)`).
    ///
    /// The Java map compares keys case-insensitively
    /// (`CaseInsensitiveHashStrategy`), so the lookup is case-insensitive.
    #[inline]
    pub fn index_of(&self, name: &str) -> Option<i32> {
        self.name_to_index.get(&name_key(name)).copied()
    }

    /// The child variants of `base` (Java `BlockTypeAssetMap.getSubKeys`).
    ///
    /// Fallback when the dump carries no parent/child data: Java returns
    /// `ObjectSets.singleton(key)` for a key without children, i.e. the block
    /// itself. The golden dump has no child-variant data, so this is the only
    /// branch representable today.
    pub fn sub_keys(&self, base: &str) -> Vec<String> {
        if self.index_of(base).is_some() {
            vec![base.to_string()]
        } else {
            Vec::new()
        }
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
    /// The fluid key of every indexed slot (`None` for a hole).
    names: Vec<Option<String>>,
    /// Java `IndexedLookupTableAssetMap.keyToIndex` — the case-insensitive
    /// name → id direction (`Fluid.getAssetMap().getIndex(name)`).
    name_to_index: HashMap<String, i32>,
}

impl FluidRegistry {
    /// Java `IndexedLookupTableAssetMap` — the indexed fluid table (without
    /// names).
    pub fn new(max_levels: Vec<i8>) -> Self {
        let names = vec![None; max_levels.len()];
        Self {
            max_levels,
            names,
            name_to_index: HashMap::new(),
        }
    }

    /// The indexed fluid table plus the fluid key of every slot.
    pub fn with_names(max_levels: Vec<i8>, names: Vec<Option<String>>) -> Self {
        assert_eq!(
            max_levels.len(),
            names.len(),
            "the fluid asset and name tables must have the same length"
        );
        let mut name_to_index = HashMap::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            if let Some(name) = name {
                name_to_index.entry(name_key(name)).or_insert(index as i32);
            }
        }
        Self {
            max_levels,
            names,
            name_to_index,
        }
    }

    /// An empty registry — every lookup falls back to the default level.
    pub fn empty() -> Self {
        Self {
            max_levels: Vec::new(),
            names: Vec::new(),
            name_to_index: HashMap::new(),
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

    /// The fluid key at `index` (`None` for a hole or out of range).
    #[inline]
    pub fn name(&self, index: i32) -> Option<&str> {
        if index < 0 {
            return None;
        }
        self.names
            .get(index as usize)
            .and_then(|name| name.as_deref())
    }

    /// The numeric id of a fluid name (Java: `getIndex(name)`),
    /// case-insensitively.
    #[inline]
    pub fn index_of(&self, name: &str) -> Option<i32> {
        self.name_to_index.get(&name_key(name)).copied()
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

/// Java `IndexedLookupTableAssetMap<String, Environment>` — the id →
/// environment asset table.
///
/// The world generator resolves the environment *names* of the biome
/// environment containers and of the prefab contents to their numeric ids
/// (`Environment.getAssetMap().getIndex(name)`), so the table is kept as
/// name-per-index. The environment assets themselves carry no data the
/// generator observes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvironmentRegistry {
    /// Java `private final Environment[] array` — the indexed asset table.
    names: Vec<String>,
    /// Java `IndexedLookupTableAssetMap.keyToIndex` — the case-insensitive
    /// name → id direction (`Environment.getAssetMap().getIndex(name)`).
    name_to_index: HashMap<String, i32>,
}

impl EnvironmentRegistry {
    /// Java `IndexedLookupTableAssetMap` — the indexed environment table.
    pub fn new(names: Vec<String>) -> Self {
        let mut name_to_index = HashMap::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            if !name.is_empty() {
                name_to_index.entry(name_key(name)).or_insert(index as i32);
            }
        }
        Self {
            names,
            name_to_index,
        }
    }

    /// An empty registry — every lookup misses.
    pub fn empty() -> Self {
        Self {
            names: Vec::new(),
            name_to_index: HashMap::new(),
        }
    }

    /// The environment name at `index` (Java: `getAsset(index).getId()`).
    #[inline]
    pub fn name(&self, index: i32) -> Option<&str> {
        if index < 0 {
            return None;
        }
        self.names.get(index as usize).map(String::as_str)
    }

    /// The numeric id of an environment name (Java: `getIndex(name)`).
    ///
    /// The Java map compares the keys case-insensitively
    /// (`CaseInsensitiveHashStrategy`), so the lookup is case-insensitive too.
    #[inline]
    pub fn index_of(&self, name: &str) -> Option<i32> {
        self.name_to_index.get(&name_key(name)).copied()
    }

    /// The indexed environment table.
    #[inline]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The number of indexed slots.
    #[inline]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the registry holds no slot at all.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// The block, fluid and environment registries of a world.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registries {
    /// Java `BlockType.getAssetMap()`.
    pub blocks: BlockRegistry,
    /// Java `Fluid.getAssetMap()`.
    pub fluids: FluidRegistry,
    /// Java `Environment.getAssetMap()`.
    pub environments: EnvironmentRegistry,
}

impl Registries {
    /// A registry pair.
    pub fn new(blocks: BlockRegistry, fluids: FluidRegistry) -> Self {
        Self {
            blocks,
            fluids,
            environments: EnvironmentRegistry::empty(),
        }
    }

    /// A registry triple (the environment table is empty in [`Registries::new`]).
    pub fn with_environments(
        blocks: BlockRegistry,
        fluids: FluidRegistry,
        environments: EnvironmentRegistry,
    ) -> Self {
        Self {
            blocks,
            fluids,
            environments,
        }
    }

    /// An empty registry pair — every block lookup misses and every fluid
    /// lookup falls back to the default max fluid level.
    pub fn empty() -> Self {
        Self {
            blocks: BlockRegistry::empty(),
            fluids: FluidRegistry::empty(),
            environments: EnvironmentRegistry::empty(),
        }
    }

    /// Loads the registries from the golden dump the Java dumper produced
    /// (`testdata/golden/registries.json`).
    ///
    /// The dump is the output of the *real* Java asset pipeline (the vanilla
    /// `AssetRegistryLoader` running over the base-game asset pack), so the
    /// ids it contains are the ids the Java world generator uses. The Rust
    /// side never re-derives them.
    pub fn from_golden(path: impl AsRef<Path>) -> Result<Registries, RegistryLoadError> {
        let text = std::fs::read_to_string(path.as_ref()).map_err(RegistryLoadError::Io)?;
        let dump: GoldenDump = serde_json::from_str(&text).map_err(RegistryLoadError::Json)?;
        dump.into_registries()
    }

    /// Loads the golden registries from the repository default location
    /// (`testdata/golden/registries.json`, resolved relative to this crate's
    /// manifest directory).
    ///
    /// World loading is a one-time fatal operation in Java as well, so a
    /// missing/invalid dump panics with the underlying cause instead of
    /// returning an error.
    pub fn load() -> Registries {
        let path = Self::default_golden_path();
        Self::load_from(&path).unwrap_or_else(|error| {
            panic!(
                "Failed to load the world generator registries from '{}': {error}",
                path.display()
            )
        })
    }

    /// Loads the golden registries from `path`.
    pub fn load_from(path: impl AsRef<Path>) -> Result<Registries, RegistryLoadError> {
        Self::from_golden(path)
    }

    /// The repository default of the golden registry dump.
    ///
    /// `CARGO_MANIFEST_DIR` is compile-time metadata, so the path works no
    /// matter which directory the test/binary runs from.
    pub fn default_golden_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/golden/registries.json")
    }
}

/// The error of [`Registries::from_golden`].
#[derive(Debug)]
pub enum RegistryLoadError {
    /// The golden file could not be read.
    Io(std::io::Error),
    /// The golden file is not valid JSON (or does not have the expected shape).
    Json(serde_json::Error),
    /// The golden file contains inconsistent data.
    Invalid(String),
}

impl fmt::Display for RegistryLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryLoadError::Io(error) => write!(f, "cannot read the registry dump: {error}"),
            RegistryLoadError::Json(error) => write!(f, "cannot parse the registry dump: {error}"),
            RegistryLoadError::Invalid(message) => {
                write!(f, "invalid registry dump: {message}")
            }
        }
    }
}

impl std::error::Error for RegistryLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RegistryLoadError::Io(error) => Some(error),
            RegistryLoadError::Json(error) => Some(error),
            RegistryLoadError::Invalid(_) => None,
        }
    }
}

// ---------------------------------------------------------------------
// The golden dump format
// ---------------------------------------------------------------------

/// The root of `testdata/golden/registries.json`.
#[derive(Debug, Deserialize)]
struct GoldenDump {
    blocks: GoldenBlocks,
    fluids: GoldenFluids,
    #[serde(default)]
    environments: Option<GoldenEnvironments>,
}

#[derive(Debug, Deserialize)]
struct GoldenBlocks {
    #[serde(default)]
    assets: Vec<GoldenBlock>,
}

#[derive(Debug, Deserialize)]
struct GoldenBlock {
    name: String,
    index: i32,
    #[serde(rename = "drawType")]
    draw_type: String,
    #[serde(rename = "drawTypeId")]
    draw_type_id: i32,
    /// The Java `BlockType.getOpacity()` name (`Solid`, `Semitransparent`,
    /// `Cutout`, `Transparent`). Defaults to `Solid` so a dump produced
    /// before the opacity field existed still loads.
    #[serde(default = "default_opacity_name")]
    opacity: String,
    /// The per-rotation support probes (`meta.supportField` of the dump) —
    /// absent when the Java support map is `null`.

    #[serde(default)]
    support: Option<Vec<GoldenBlockSupport>>,
}

/// Java `BlockType` field default for `opacity`.
fn default_opacity_name() -> String {
    "Solid".to_string()
}

/// One entry of the dump's per-rotation `support` array.
#[derive(Debug, Deserialize)]
struct GoldenBlockSupport {
    #[serde(default)]
    down: bool,
    #[serde(default)]
    up: bool,
}

#[derive(Debug, Deserialize)]
struct GoldenFluids {
    #[serde(default)]
    assets: Vec<GoldenFluid>,
}

#[derive(Debug, Deserialize)]
struct GoldenFluid {
    name: String,
    index: i32,
    #[serde(rename = "maxFluidLevel")]
    max_fluid_level: i32,
}

#[derive(Debug, Deserialize)]
struct GoldenEnvironments {
    #[serde(default)]
    assets: Vec<GoldenEnvironment>,
}

#[derive(Debug, Deserialize)]
struct GoldenEnvironment {
    name: String,
    index: i32,
}

impl GoldenDump {
    fn into_registries(self) -> Result<Registries, RegistryLoadError> {
        // --- blocks ------------------------------------------------------
        let mut blocks: Vec<Option<BlockTypeInfo>> = Vec::new();
        let mut block_names: Vec<Option<String>> = Vec::new();
        for asset in &self.blocks.assets {
            if asset.index < 0 {
                return Err(RegistryLoadError::Invalid(format!(
                    "the block '{}' has a negative index",
                    asset.name
                )));
            }
            let draw_type = DrawType::from_name(&asset.draw_type).ok_or_else(|| {
                RegistryLoadError::Invalid(format!(
                    "the block '{}' has the unknown draw type '{}'",
                    asset.name, asset.draw_type
                ))
            })?;
            if draw_type.get_value() != asset.draw_type_id {
                return Err(RegistryLoadError::Invalid(format!(
                    "the block '{}' has the draw type '{}' (id {}) but the dump says id {}",
                    asset.name,
                    asset.draw_type,
                    draw_type.get_value(),
                    asset.draw_type_id
                )));
            }
            let index = asset.index as usize;
            if blocks.len() <= index {
                blocks.resize(index + 1, None);
                block_names.resize(index + 1, None);
            }
            let opacity = Opacity::from_name(&asset.opacity).ok_or_else(|| {
                RegistryLoadError::Invalid(format!(
                    "the block '{}' has the unknown opacity '{}'",
                    asset.name, asset.opacity
                ))
            })?;
            let block_type = match asset.support.as_ref() {
                Some(entries) => {
                    if entries.len() != SUPPORT_ROTATION_COUNT {
                        return Err(RegistryLoadError::Invalid(format!(
                            "the block '{}' has {} support entries but {} are expected (one per Java RotationTuple)",
                            asset.name,
                            entries.len(),
                            SUPPORT_ROTATION_COUNT
                        )));
                    }
                    BlockTypeInfo::new(draw_type).with_opacity(opacity).with_support_rotations(
                        entries
                            .iter()
                            .map(|entry| BlockSupport {
                                down: entry.down,
                                up: entry.up,
                            })
                            .collect(),
                    )
                }
                None => BlockTypeInfo::new(draw_type).with_opacity(opacity),
            };
            blocks[index] = Some(block_type);
            block_names[index] = Some(asset.name.clone());
        }

        // --- fluids ------------------------------------------------------
        let mut fluids: Vec<i8> = Vec::new();
        let mut fluid_names: Vec<Option<String>> = Vec::new();
        for asset in &self.fluids.assets {
            if asset.index < 0 {
                return Err(RegistryLoadError::Invalid(format!(
                    "the fluid '{}' has a negative index",
                    asset.name
                )));
            }
            let index = asset.index as usize;
            if fluids.len() <= index {
                fluids.resize(index + 1, DEFAULT_MAX_FLUID_LEVEL);
                fluid_names.resize(index + 1, None);
            }
            fluids[index] = asset.max_fluid_level as i8;
            fluid_names[index] = Some(asset.name.clone());
        }

        // --- environments ------------------------------------------------
        let mut environments: Vec<String> = Vec::new();
        if let Some(section) = &self.environments {
            for asset in &section.assets {
                if asset.index < 0 {
                    return Err(RegistryLoadError::Invalid(format!(
                        "the environment '{}' has a negative index",
                        asset.name
                    )));
                }
                let index = asset.index as usize;
                if environments.len() <= index {
                    environments.resize(index + 1, String::new());
                }
                environments[index] = asset.name.clone();
            }
        }

        Ok(Registries::with_environments(
            BlockRegistry::with_names(blocks, block_names),
            FluidRegistry::with_names(fluids, fluid_names),
            EnvironmentRegistry::new(environments),
        ))
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
    fn the_support_map_is_null_by_default() {
        // Java: `BlockType.support` is `null` unless the asset sets it.
        let block = BlockTypeInfo::new(DrawType::Cube);
        assert_eq!(block.get_support(0), None);
        assert_eq!(block.get_support(7), None);
        assert_eq!(DEFAULT_BLOCK_TYPE.get_support(0), None);
    }

    #[test]
    fn the_support_map_carries_the_two_observed_faces() {
        // Java: `getSupport(rotationIndex)` returns the face map; the world
        // generator asks for `BlockFace.DOWN` (a floor cover) and `BlockFace.UP`
        // (a ceiling cover).
        let block = BlockTypeInfo::new(DrawType::Cube).with_support(BlockSupport {
            down: true,
            up: false,
        });
        let support = block.get_support(0).expect("the block has a support map");
        assert!(support.down);
        assert!(!support.up);
    }

    #[test]
    fn the_per_rotation_support_maps_follow_the_rotation_index() {
        // Java: the rotated support maps of `getSupport(1..)`.
        let rotations = (0..SUPPORT_ROTATION_COUNT as i32)
            .map(|index| BlockSupport {
                down: index % 4 == 0,
                up: index % 8 == 0,
            })
            .collect();
        let block = BlockTypeInfo::new(DrawType::Cube).with_support_rotations(rotations);
        // Index 0 is the base map (Java `RotationTuple.NONE_INDEX`).
        assert_eq!(
            block.get_support(0),
            Some(BlockSupport {
                down: true,
                up: true
            })
        );
        // The rotated maps are indexed verbatim.
        assert_eq!(
            block.get_support(1),
            Some(BlockSupport {
                down: false,
                up: false
            })
        );
        assert_eq!(
            block.get_support(8),
            Some(BlockSupport {
                down: true,
                up: true
            })
        );
        assert_eq!(
            block.support,
            Some(BlockSupport {
                down: true,
                up: true
            })
        );
        assert_eq!(block.rotated_support.len(), SUPPORT_ROTATION_COUNT);
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

    #[test]
    fn the_environment_registry_resolves_names_case_insensitively() {
        let registry = EnvironmentRegistry::new(vec![
            "Unknown".to_string(),
            "Default".to_string(),
            "Env_Zone0".to_string(),
        ]);

        assert_eq!(registry.name(0), Some("Unknown"));
        assert_eq!(registry.name(2), Some("Env_Zone0"));
        assert_eq!(registry.name(3), None);
        assert_eq!(registry.name(-1), None);

        assert_eq!(registry.index_of("Default"), Some(1));
        assert_eq!(registry.index_of("default"), Some(1));
        assert_eq!(registry.index_of("Missing"), None);
        assert_eq!(registry.len(), 3);
    }

    #[test]
    fn the_golden_loader_fills_the_registries() {
        let json = r#"{
            "blocks": {
                "count": 3,
                "assets": [
                    {"name": "Empty", "index": 0, "drawType": "Empty", "drawTypeId": 0, "unknown": false},
                    {"name": "Unknown", "index": 1, "drawType": "Cube", "drawTypeId": 2, "unknown": true},
                    {"name": "Rock_Stone", "index": 7, "drawType": "Cube", "drawTypeId": 2, "unknown": false}
                ]
            },
            "fluids": {
                "count": 2,
                "assets": [
                    {"name": "Empty", "index": 0, "maxFluidLevel": 8},
                    {"name": "Water_Source", "index": 1, "maxFluidLevel": 1}
                ]
            },
            "environments": {
                "count": 2,
                "assets": [
                    {"name": "Unknown", "index": 0},
                    {"name": "Default", "index": 1}
                ]
            }
        }"#;

        let path = std::env::temp_dir().join("worldgen-registry-golden-test.json");
        std::fs::write(&path, json).expect("write the temporary dump");

        let registries = Registries::from_golden(&path).expect("load the dump");
        let _ = std::fs::remove_file(&path);

        // The blocks keep their index (the gap between 1 and 7 is a hole).
        assert_eq!(registries.blocks.len(), 8);
        assert_eq!(
            registries.blocks.get(0).map(|b| b.draw_type),
            Some(DrawType::Empty)
        );
        assert_eq!(
            registries.blocks.get(1).map(|b| b.draw_type),
            Some(DrawType::Cube)
        );
        assert!(registries.blocks.get(2).is_none());
        assert_eq!(
            registries.blocks.get(7).map(|b| b.draw_type),
            Some(DrawType::Cube)
        );

        // The fluids keep their max fluid level per index.
        assert_eq!(registries.fluids.get_max_fluid_level(0), 8);
        assert_eq!(registries.fluids.get_max_fluid_level(1), 1);
        assert_eq!(registries.fluids.get_max_fluid_level(2), 8);

        // The environments keep their name per index.
        assert_eq!(registries.environments.name(1), Some("Default"));
        assert_eq!(registries.environments.index_of("default"), Some(1));

        // The name → id direction is filled from the same dump, and the
        // lookup is case-insensitive (Java `CaseInsensitiveHashStrategy`).
        assert_eq!(registries.blocks.name(7), Some("Rock_Stone"));
        assert_eq!(registries.blocks.index_of("Rock_Stone"), Some(7));
        assert_eq!(registries.blocks.index_of("rock_stone"), Some(7));
        assert_eq!(registries.blocks.index_of("Missing"), None);
        assert_eq!(registries.fluids.name(1), Some("Water_Source"));
        assert_eq!(registries.fluids.index_of("water_source"), Some(1));
        assert_eq!(registries.fluids.index_of("Missing"), None);
    }

    #[test]
    fn the_default_registry_load_reads_the_repository_dump() {
        let registries = Registries::load();

        // Spot checks against the real Java asset pipeline dump: a few ids
        // the world JSONs reference on every path.
        assert_eq!(registries.blocks.index_of("Rock_Bedrock"), Some(445));
        assert_eq!(registries.blocks.index_of("Rock_Stone"), Some(1527));
        assert_eq!(registries.blocks.index_of("Soil_Grass"), Some(1250));
        assert_eq!(registries.blocks.name(1527), Some("Rock_Stone"));
        assert_eq!(registries.fluids.index_of("Water_Source"), Some(2));
        assert_eq!(registries.environments.index_of("Default"), Some(58));
        assert!(!registries.blocks.is_empty());
        assert!(!registries.fluids.is_empty());
        assert!(!registries.environments.is_empty());
    }

    #[test]
    fn the_default_registry_dump_carries_the_support_maps() {
        let registries = Registries::load();

        // Blocks without a support map (the Java `null`) — e.g. `Rock_Stone`.
        let rock = registries
            .blocks
            .index_of("Rock_Stone")
            .and_then(|index| registries.blocks.get(index))
            .expect("Rock_Stone is in the dump");
        assert_eq!(rock.support, None, "Rock_Stone has no support map");
        assert_eq!(rock.get_support(0), None);
        assert_eq!(rock.rotated_support.len(), 0);

        // `Debug_Model` requires support below when unrotated (rotation index
        // 0) but not under every rotation — the dump stores one entry per
        // Java `RotationTuple`.
        let debug_model = registries
            .blocks
            .index_of("Debug_Model")
            .and_then(|index| registries.blocks.get(index))
            .expect("Debug_Model is in the dump");
        let base = debug_model.support.expect("Debug_Model has a support map");
        assert!(base.down, "the unrotated model requires support below");
        assert_eq!(debug_model.rotated_support.len(), SUPPORT_ROTATION_COUNT);
        // Some rotation of the model faces another way, so its support map
        // differs from the base map at least once.
        assert!(
            debug_model
                .rotated_support
                .iter()
                .any(|support| *support != base),
            "the rotated support maps differ from the base map somewhere"
        );
    }

    #[test]
    fn the_default_golden_path_points_at_the_repository_dump() {
        let path = Registries::default_golden_path();
        assert!(
            path.ends_with("testdata/golden/registries.json"),
            "{}",
            path.display()
        );
        assert!(path.is_file(), "{}", path.display());
    }

    #[test]
    fn the_golden_loader_rejects_an_unknown_draw_type() {
        let json = r#"{
            "blocks": {"assets": [
                {"name": "Broken", "index": 0, "drawType": "Nope", "drawTypeId": 9, "unknown": false}
            ]},
            "fluids": {"assets": []}
        }"#;

        let path = std::env::temp_dir().join("worldgen-registry-golden-broken.json");
        std::fs::write(&path, json).expect("write the temporary dump");

        let error = Registries::from_golden(&path).expect_err("the dump must be rejected");
        let _ = std::fs::remove_file(&path);

        assert!(matches!(error, RegistryLoadError::Invalid(_)));
    }
}
