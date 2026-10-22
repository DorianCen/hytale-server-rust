//! P0 asset stores and the deterministic registry snapshot.
//!
//! Ported behavior from `CoreServer/.../asset/**` and
//! `AssetStore/src/main/java/com/hypixel/hytale/assetstore/**`.
//!
//! The loader reads the raw JSON documents from an [`AssetPack`], resolves
//! `Parent` chains with the Java `AssetBuilderCodec` rule (a child field
//! replaces the inherited field, absent fields are inherited), and assigns
//! numeric ids once. The resulting [`AssetRegistry`] is the server's snapshot:
//! world generation, network encoders and storage all consume the same ids.
//!
//! ## Id assignment
//!
//! Java hands out ids in asset-load order. For stores whose assets live in
//! ordinary files the order is reproducible: the pack's files are walked in
//! reverse zip order, non-parented assets are committed first, and parented
//! assets are resolved in waves. The waiting-asset wave follows the iteration
//! order of Java's `ConcurrentHashMap` (spread hash slot, then insertion order).
//! The Rust port models that order, so `BlockBoundingBoxes`, `Fluid`,
//! `Environment` and `PhysicalMaterial` ids match the committed Java golden.
//!
//! `BlockType` assets are mostly *contained* in `Server/Item/Items` documents.
//! Java's contained-asset loader groups them through `HashMap`s, so the
//! contained-block order is Java-implementation detail rather than a stable
//! content order. Those ids are Rust-owned: the same keys, fields and semantic
//! values as Java, but the numeric order after the four preloaded blocks and
//! the standalone `Server/Item/Block/Blocks` assets is the port's own order.
//! The registry is never re-derived from a golden dump at runtime.

use std::collections::{HashMap, HashSet};

use hytale_protocol::generated::common::{BlockMaterial, DrawType, Opacity};
use serde_json::{json, Value};

use crate::pack::{AssetError, AssetPack, ParentProvider};
use crate::parent::{MergeHook, ParentResolver};

/// How a store maps keys to numeric ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapKind {
    /// `DefaultAssetMap`: key → asset only, no numeric ids.
    Default,
    /// `IndexedAssetMap`: key → id, no id-indexed array.
    Indexed,
    /// `IndexedLookupTableAssetMap`: key → id plus an id-indexed array.
    IndexedLookupTable,
    /// `BlockTypeAssetMap`: like the lookup table, with sub-key tracking.
    BlockType,
}

/// One P0 store registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreSpec {
    /// Store name (the Java asset class simple name).
    pub name: &'static str,
    /// Pack-relative directory (`Server/...`).
    pub path: &'static str,
    /// Id mapping kind.
    pub map: MapKind,
}

/// P0 store specs, in the inventory order.
pub const P0_STORES: &[StoreSpec] = &[
    StoreSpec {
        name: "BlockBoundingBoxes",
        path: "Server/Item/Block/Hitboxes",
        map: MapKind::IndexedLookupTable,
    },
    StoreSpec {
        name: "BlockType",
        path: "Server/Item/Block/Blocks",
        map: MapKind::BlockType,
    },
    StoreSpec {
        name: "BlockTypeListAsset",
        path: "Server/BlockTypeList",
        map: MapKind::Default,
    },
    StoreSpec {
        name: "Environment",
        path: "Server/Environments",
        map: MapKind::IndexedLookupTable,
    },
    StoreSpec {
        name: "Fluid",
        path: "Server/Item/Block/Fluids",
        map: MapKind::IndexedLookupTable,
    },
    StoreSpec {
        name: "Item",
        path: "Server/Item/Items",
        map: MapKind::Default,
    },
    StoreSpec {
        name: "PhysicalMaterial",
        path: "Server/Item/Block/PhysicalMaterials",
        map: MapKind::IndexedLookupTable,
    },
    StoreSpec {
        name: "PrefabListAsset",
        path: "Server/PrefabList",
        map: MapKind::Default,
    },
];

const BLOCK_BOUNDING_BOXES: StoreSpec = P0_STORES[0];
const BLOCK_TYPE: StoreSpec = P0_STORES[1];
const BLOCK_TYPE_LIST: StoreSpec = P0_STORES[2];
const ENVIRONMENT: StoreSpec = P0_STORES[3];
const FLUID: StoreSpec = P0_STORES[4];
const ITEM: StoreSpec = P0_STORES[5];
const PHYSICAL_MATERIAL: StoreSpec = P0_STORES[6];
const PREFAB_LIST: StoreSpec = P0_STORES[7];

/// One loaded asset with its assigned id and provenance.
#[derive(Debug, Clone)]
pub struct AssetRecord<T> {
    /// Asset key (case-insensitive lookup uses the ASCII lowercase form).
    pub key: String,
    /// Numeric id for indexed stores.
    pub id: Option<i32>,
    /// Pack-relative source path, or `<preloaded>`.
    pub source: String,
    /// True for a code-defined asset that has no pack file.
    pub preloaded: bool,
    /// Decoded asset.
    pub value: T,
}

/// An ordered asset table with key and id indexes.
#[derive(Debug, Clone)]
pub struct AssetCollection<T> {
    records: Vec<AssetRecord<T>>,
    by_key: HashMap<String, usize>,
    by_id: Vec<Option<usize>>,
}

impl<T> AssetCollection<T> {
    fn new(store: &str, records: Vec<AssetRecord<T>>) -> Result<Self, AssetError> {
        let mut by_key = HashMap::with_capacity(records.len());
        for (index, record) in records.iter().enumerate() {
            let key = record.key.to_ascii_lowercase();
            if by_key.insert(key, index).is_some() {
                return Err(AssetError::DuplicateAsset {
                    store: store.to_string(),
                    key: record.key.clone(),
                });
            }
        }

        let mut by_id: Vec<Option<usize>> = Vec::new();
        for (index, record) in records.iter().enumerate() {
            let Some(id) = record.id else { continue };
            if id < 0 {
                return Err(AssetError::InvalidAsset {
                    store: store.to_string(),
                    key: record.key.clone(),
                    message: format!("negative asset id {id}"),
                });
            }
            let id = id as usize;
            if by_id.len() <= id {
                by_id.resize(id + 1, None);
            }
            if by_id[id].is_some() {
                return Err(AssetError::DuplicateAssetId {
                    store: store.to_string(),
                    id: id as i32,
                });
            }
            by_id[id] = Some(index);
        }

        Ok(Self {
            records,
            by_key,
            by_id,
        })
    }

    /// All records in load order.
    pub fn iter(&self) -> impl Iterator<Item = &AssetRecord<T>> {
        self.records.iter()
    }

    /// Look up an asset by key, case-insensitively.
    pub fn get(&self, key: &str) -> Option<&AssetRecord<T>> {
        self.by_key
            .get(&key.to_ascii_lowercase())
            .map(|index| &self.records[*index])
    }

    /// Look up an indexed asset by numeric id.
    pub fn get_by_id(&self, id: i32) -> Option<&AssetRecord<T>> {
        if id < 0 {
            return None;
        }
        self.by_id
            .get(id as usize)
            .and_then(|index| *index)
            .map(|index| &self.records[index])
    }

    /// Resolve a key to its id.
    pub fn id_of(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(|record| record.id)
    }

    /// Number of assets (including preloaded ones).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Java `BlockType.getDrawType()`, `getOpacity()`, `getMaterial()` and the two
/// support probes the world generator observes.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockTypeAsset {
    /// Draw type.
    pub draw_type: DrawType,
    /// Opacity.
    pub opacity: Opacity,
    /// Material.
    pub material: BlockMaterial,
    /// Support requirements, or `None` for the Java `null` map.
    pub support: Option<BlockTypeSupport>,
    /// Merged asset document (parents resolved).
    pub json: Value,
}

impl BlockTypeAsset {
    /// Java `getSupport(RotationTuple.NONE_INDEX)` reduced to the two faces the
    /// world generator probes.
    pub fn base_support(&self) -> Option<BlockSupport> {
        self.support.as_ref().map(|support| support.at(0))
    }

    /// Java `getSupport(rotationIndex)` reduced to `DOWN` / `UP` membership.
    pub fn support_at(&self, rotation_index: u8) -> Option<BlockSupport> {
        self.support
            .as_ref()
            .map(|support| support.at(rotation_index))
    }
}

/// Java `Fluid.maxFluidLevel`.
#[derive(Debug, Clone, PartialEq)]
pub struct FluidAsset {
    /// Maximum fluid level (`0..=15`).
    pub max_fluid_level: i8,
    /// Merged asset document.
    pub json: Value,
}

/// The two support probes `CaveNodeShapeUtils` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSupport {
    /// `getSupport(index).containsKey(BlockFace.DOWN)`.
    pub down: bool,
    /// `getSupport(index).containsKey(BlockFace.UP)`.
    pub up: bool,
}

/// Java `Rotation` ordinal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Rotation {
    None = 0,
    Ninety = 1,
    OneEighty = 2,
    TwoSeventy = 3,
}

impl Rotation {
    fn from_ordinal(ordinal: u8) -> Self {
        match ordinal & 3 {
            0 => Self::None,
            1 => Self::Ninety,
            2 => Self::OneEighty,
            _ => Self::TwoSeventy,
        }
    }
}

/// A `BlockFace` / `BlockNeighbor` direction vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Face(i8, i8, i8);

const FACE_UP: Face = Face(0, 1, 0);
const FACE_DOWN: Face = Face(0, -1, 0);
const FACE_NORTH: Face = Face(0, 0, -1);
const FACE_SOUTH: Face = Face(0, 0, 1);
const FACE_EAST: Face = Face(1, 0, 0);
const FACE_WEST: Face = Face(-1, 0, 0);

const ALL_FACES: [Face; 26] = [
    FACE_UP,
    FACE_DOWN,
    FACE_NORTH,
    FACE_EAST,
    FACE_SOUTH,
    FACE_WEST,
    Face(0, 1, -1),
    Face(0, 1, 1),
    Face(1, 1, 0),
    Face(-1, 1, 0),
    Face(0, -1, -1),
    Face(0, -1, 1),
    Face(1, -1, 0),
    Face(-1, -1, 0),
    Face(1, 0, -1),
    Face(1, 0, 1),
    Face(-1, 0, 1),
    Face(-1, 0, -1),
    Face(1, 1, -1),
    Face(1, 1, 1),
    Face(-1, 1, 1),
    Face(-1, 1, -1),
    Face(1, -1, -1),
    Face(1, -1, 1),
    Face(-1, -1, 1),
    Face(-1, -1, -1),
];

const BLOCK_SIDES: [Face; 6] = [
    FACE_UP, FACE_DOWN, FACE_NORTH, FACE_EAST, FACE_SOUTH, FACE_WEST,
];
const CARDINAL_DIRECTIONS: [Face; 4] = [FACE_NORTH, FACE_EAST, FACE_SOUTH, FACE_WEST];
const HORIZONTAL: [Face; 8] = [
    FACE_NORTH,
    Face(1, 0, -1),
    FACE_EAST,
    Face(1, 0, 1),
    FACE_SOUTH,
    Face(-1, 0, 1),
    FACE_WEST,
    Face(-1, 0, -1),
];
const UP_CARDINAL_DIRECTIONS: [Face; 4] =
    [Face(0, 1, -1), Face(1, 1, 0), Face(0, 1, 1), Face(-1, 1, 0)];
const DOWN_CARDINAL_DIRECTIONS: [Face; 4] = [
    Face(0, -1, -1),
    Face(1, -1, 0),
    Face(0, -1, 1),
    Face(-1, -1, 0),
];

impl Face {
    fn normalize(name: &str) -> String {
        name.chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .map(|character| character.to_ascii_lowercase())
            .collect()
    }

    fn from_name(name: &str) -> Option<Self> {
        let name = Self::normalize(name);
        Some(match name.as_str() {
            "up" => FACE_UP,
            "down" => FACE_DOWN,
            "north" => FACE_NORTH,
            "east" => FACE_EAST,
            "south" => FACE_SOUTH,
            "west" => FACE_WEST,
            "upnorth" => Face(0, 1, -1),
            "upsouth" => Face(0, 1, 1),
            "upeast" => Face(1, 1, 0),
            "upwest" => Face(-1, 1, 0),
            "downnorth" => Face(0, -1, -1),
            "downsouth" => Face(0, -1, 1),
            "downeast" => Face(1, -1, 0),
            "downwest" => Face(-1, -1, 0),
            "northeast" => Face(1, 0, -1),
            "southeast" => Face(1, 0, 1),
            "southwest" => Face(-1, 0, 1),
            "northwest" => Face(-1, 0, -1),
            "upnortheast" => Face(1, 1, -1),
            "upsoutheast" => Face(1, 1, 1),
            "upsouthwest" => Face(-1, 1, 1),
            "upnorthwest" => Face(-1, 1, -1),
            "downnortheast" => Face(1, -1, -1),
            "downsoutheast" => Face(1, -1, 1),
            "downsouthwest" => Face(-1, -1, 1),
            "downnorthwest" => Face(-1, -1, -1),
            _ => return None,
        })
    }

    fn expand(name: &str) -> Option<Vec<Self>> {
        let normalized = Self::normalize(name);
        Some(match normalized.as_str() {
            "all" => ALL_FACES.to_vec(),
            "blocksides" => BLOCK_SIDES.to_vec(),
            "cardinaldirections" => CARDINAL_DIRECTIONS.to_vec(),
            "horizontal" => HORIZONTAL.to_vec(),
            "upcardinaldirections" => UP_CARDINAL_DIRECTIONS.to_vec(),
            "downcardinaldirections" => DOWN_CARDINAL_DIRECTIONS.to_vec(),
            _ => vec![Self::from_name(&normalized)?],
        })
    }

    /// Java `Rotation.rotate(Vector3ic, yaw, pitch, roll)`.
    fn rotate(self, yaw: Rotation, pitch: Rotation, roll: Rotation) -> Self {
        let (mut x, mut y, mut z) = (self.0, self.1, self.2);
        match roll {
            Rotation::None => {}
            Rotation::Ninety => (x, y, z) = (-y, x, z),
            Rotation::OneEighty => (x, y, z) = (-x, -y, z),
            Rotation::TwoSeventy => (x, y, z) = (y, -x, z),
        }
        match pitch {
            Rotation::None => {}
            Rotation::Ninety => (x, y, z) = (x, -z, y),
            Rotation::OneEighty => (x, y, z) = (x, -y, -z),
            Rotation::TwoSeventy => (x, y, z) = (x, z, -y),
        }
        match yaw {
            Rotation::None => {}
            Rotation::Ninety => (x, y, z) = (z, y, -x),
            Rotation::OneEighty => (x, y, z) = (-x, y, -z),
            Rotation::TwoSeventy => (x, y, z) = (-z, y, x),
        }
        Self(x, y, z)
    }
}

/// One `RequiredBlockFaceSupport` entry after expanding a `MergedBlockFaces`
/// alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportRequirement {
    /// Face the requirement is attached to.
    pub face: Face,
    /// Java `RequiredBlockFaceSupport.isRotated()`.
    pub rotate: bool,
}

/// Java `BlockType.support` reduced to the data the world generator and the
/// packet encoders need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTypeSupport {
    /// Faces present in the base support map. A face with an empty requirement
    /// list still counts for `containsKey`, matching Java's decoded map.
    pub faces: Vec<Face>,
    /// Requirement entries with their rotation flag.
    pub requirements: Vec<SupportRequirement>,
}

impl BlockTypeSupport {
    /// Java `BlockType.getSupport(rotationIndex)` reduced to `DOWN` / `UP`
    /// membership.
    pub fn at(&self, rotation_index: u8) -> BlockSupport {
        if rotation_index == 0 {
            return BlockSupport {
                down: self.faces.contains(&FACE_DOWN),
                up: self.faces.contains(&FACE_UP),
            };
        }
        let roll = Rotation::from_ordinal(rotation_index / 16);
        let pitch = Rotation::from_ordinal((rotation_index / 4) % 4);
        let yaw = Rotation::from_ordinal(rotation_index % 4);
        let mut down = false;
        let mut up = false;
        for requirement in &self.requirements {
            let face = if requirement.rotate {
                requirement.face.rotate(yaw, pitch, roll)
            } else {
                requirement.face
            };
            down |= face == FACE_DOWN;
            up |= face == FACE_UP;
        }
        BlockSupport { down, up }
    }
}

/// The P0 asset registry snapshot.
#[derive(Debug, Clone)]
pub struct AssetRegistry {
    /// `BlockType`.
    pub block_types: AssetCollection<BlockTypeAsset>,
    /// `Fluid`.
    pub fluids: AssetCollection<FluidAsset>,
    /// `Environment`.
    pub environments: AssetCollection<Value>,
    /// `Item`.
    pub items: AssetCollection<Value>,
    /// `PhysicalMaterial`.
    pub physical_materials: AssetCollection<Value>,
    /// `BlockBoundingBoxes`.
    pub block_bounding_boxes: AssetCollection<Value>,
    /// `BlockTypeListAsset`.
    pub block_type_lists: AssetCollection<Value>,
    /// `PrefabListAsset`.
    pub prefab_lists: AssetCollection<Value>,
}

impl AssetRegistry {
    /// Load the P0 stores from `pack`.
    pub fn load_p0(pack: &AssetPack) -> Result<Self, AssetError> {
        let (items, contained_block_types) = load_items(pack)?;
        let block_types = load_block_types(pack, contained_block_types)?;
        let fluids = load_indexed_store(pack, &FLUID, fluid_preloads(), parse_fluid)?;
        let environments =
            load_indexed_store(pack, &ENVIRONMENT, environment_preloads(), parse_key_asset)?;
        let physical_materials = load_indexed_store(
            pack,
            &PHYSICAL_MATERIAL,
            physical_material_preloads(),
            parse_key_asset,
        )?;
        let block_bounding_boxes = load_indexed_store(
            pack,
            &BLOCK_BOUNDING_BOXES,
            block_bounding_box_preloads(),
            parse_key_asset,
        )?;
        let block_type_lists = load_default_store(pack, &BLOCK_TYPE_LIST, Vec::new())?;
        let prefab_lists = load_default_store(pack, &PREFAB_LIST, Vec::new())?;

        Ok(Self {
            block_types,
            fluids,
            environments,
            items,
            physical_materials,
            block_bounding_boxes,
            block_type_lists,
            prefab_lists,
        })
    }

    /// Java `BlockType.getAssetMap().getAsset(key)`.
    pub fn block_type(&self, key: &str) -> Option<&BlockTypeAsset> {
        self.block_types.get(key).map(|record| &record.value)
    }

    /// Java `Fluid.getAssetMap().getAsset(key)`.
    pub fn fluid(&self, key: &str) -> Option<&FluidAsset> {
        self.fluids.get(key).map(|record| &record.value)
    }
}

// ---------------------------------------------------------------------------
// Raw loading
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct RawAsset {
    key: String,
    parent: Option<String>,
    source: String,
    preloaded: bool,
    value: Value,
}

#[derive(Debug, Clone)]
struct LoadedAsset {
    raw: RawAsset,
    merged: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MissingParentPolicy {
    Error,
    Drop,
}

struct ValueProvider<'a> {
    values: &'a HashMap<String, Value>,
}

impl ParentProvider for ValueProvider<'_> {
    fn load(&self, key: &str) -> Option<Value> {
        self.values.get(&key.to_ascii_lowercase()).cloned()
    }
}

fn read_store_files(pack: &AssetPack, path: &str) -> Result<Vec<RawAsset>, AssetError> {
    let prefix = format!("{path}/");
    let mut files: Vec<String> = pack
        .list_in_storage_order()?
        .into_iter()
        .filter(|file| file.starts_with(&prefix) && file.ends_with(".json"))
        .collect();
    // Java's zip `Files.walkFileTree` hands the files to the store in the
    // reverse of the zip central-directory order on this pack.
    files.reverse();

    let contents = pack.read_many(&files)?;
    let mut assets = Vec::with_capacity(files.len());
    for (path, bytes) in files.into_iter().zip(contents) {
        let key = asset_key(&path);
        let value: Value = serde_json::from_slice(&bytes).map_err(|source| AssetError::Json {
            key: path.clone(),
            source,
        })?;
        let parent = value
            .get("Parent")
            .and_then(Value::as_str)
            .map(str::to_string);
        assets.push(RawAsset {
            key,
            parent,
            source: path,
            preloaded: false,
            value,
        });
    }
    Ok(assets)
}

fn asset_key(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .strip_suffix(".json")
        .unwrap_or(path)
        .to_string()
}

fn load_indexed_store<T>(
    pack: &AssetPack,
    spec: &StoreSpec,
    preloads: Vec<(&'static str, Value)>,
    parse: fn(&Value) -> Result<T, String>,
) -> Result<AssetCollection<T>, AssetError> {
    let preloaded = preloaded_assets_with(preloads);
    let mut loaded: HashSet<String> = preloaded
        .iter()
        .map(|asset| asset.key.to_ascii_lowercase())
        .collect();
    let files = read_store_files(pack, spec.path)?;
    let ordered = assign_phase(files, &mut loaded, MissingParentPolicy::Error, spec.name)?;
    let mut all = preloaded;
    all.extend(ordered);
    let resolved = resolve_assets(all, spec.name)?;
    let records = resolved
        .into_iter()
        .enumerate()
        .map(|(index, asset)| {
            let id = i32::try_from(index).map_err(|_| AssetError::InvalidAsset {
                store: spec.name.to_string(),
                key: asset.raw.key.clone(),
                message: "asset id exceeds i32".to_string(),
            })?;
            let value = parse(&asset.merged).map_err(|message| AssetError::InvalidAsset {
                store: spec.name.to_string(),
                key: asset.raw.key.clone(),
                message,
            })?;
            Ok(AssetRecord {
                key: asset.raw.key,
                id: Some(id),
                source: asset.raw.source,
                preloaded: asset.raw.preloaded,
                value,
            })
        })
        .collect::<Result<Vec<_>, AssetError>>()?;
    AssetCollection::new(spec.name, records)
}

fn load_default_store(
    pack: &AssetPack,
    spec: &StoreSpec,
    preloads: Vec<(&'static str, Value)>,
) -> Result<AssetCollection<Value>, AssetError> {
    let preloaded = preloaded_assets_with(preloads);
    let mut loaded: HashSet<String> = preloaded
        .iter()
        .map(|asset| asset.key.to_ascii_lowercase())
        .collect();
    let files = read_store_files(pack, spec.path)?;
    let ordered = assign_phase(files, &mut loaded, MissingParentPolicy::Error, spec.name)?;
    let mut all = preloaded;
    all.extend(ordered);
    let resolved = resolve_assets(all, spec.name)?;
    let records = resolved
        .into_iter()
        .map(|asset| AssetRecord {
            key: asset.raw.key,
            id: None,
            source: asset.raw.source,
            preloaded: asset.raw.preloaded,
            value: asset.merged,
        })
        .collect();
    AssetCollection::new(spec.name, records)
}

fn preloaded_assets_with(preloads: Vec<(&'static str, Value)>) -> Vec<RawAsset> {
    preloads
        .into_iter()
        .map(|(key, value)| RawAsset {
            key: key.to_string(),
            parent: None,
            source: "<preloaded>".to_string(),
            preloaded: true,
            value,
        })
        .collect()
}

/// Java `ConcurrentHashMap` table capacity for `size` insertions.
fn chm_capacity(size: usize) -> u32 {
    let mut capacity = 16u32;
    while size as u64 >= capacity as u64 * 3 / 4 {
        capacity <<= 1;
    }
    capacity
}

/// Java `ConcurrentHashMap` iteration slot (`spread(hash) & (capacity - 1)`).
fn chm_slot(key: &str, capacity: u32) -> u32 {
    let mut hash: i32 = 0;
    for unit in key.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    let hash = hash as u32;
    (hash ^ (hash >> 16)) & (capacity - 1)
}

/// Apply the Java `loadedAssets` / `waitingForParent` two-phase commit order.
fn assign_phase(
    raw: Vec<RawAsset>,
    loaded: &mut HashSet<String>,
    policy: MissingParentPolicy,
    store: &str,
) -> Result<Vec<RawAsset>, AssetError> {
    let mut ordered = Vec::with_capacity(raw.len());
    let mut waiting = Vec::new();
    let mut seen = HashSet::new();

    for asset in raw {
        let key = asset.key.to_ascii_lowercase();
        if !seen.insert(key.clone()) || loaded.contains(&key) {
            continue;
        }
        if asset.parent.is_none() {
            loaded.insert(key);
            ordered.push(asset);
        } else {
            waiting.push(asset);
        }
    }

    let parented = waiting.len();
    let capacity = chm_capacity(parented);
    while !waiting.is_empty() {
        let mut ready = Vec::new();
        for (index, asset) in waiting.iter().enumerate() {
            if asset
                .parent
                .as_ref()
                .is_some_and(|parent| loaded.contains(&parent.to_ascii_lowercase()))
            {
                ready.push(index);
            }
        }

        if ready.is_empty() {
            match policy {
                MissingParentPolicy::Error => {
                    let asset = &waiting[0];
                    return Err(AssetError::MissingAssetParent {
                        store: store.to_string(),
                        key: asset.key.clone(),
                        parent: asset.parent.clone().unwrap_or_default(),
                    });
                }
                MissingParentPolicy::Drop => break,
            }
        }

        // Java iterates ConcurrentHashMap bins in ascending slot order and the
        // nodes inside one bin in insertion order.
        let mut order: Vec<(u32, usize)> = ready
            .iter()
            .map(|index| (chm_slot(&waiting[*index].key, capacity), *index))
            .collect();
        order.sort_unstable();

        let mut keep = vec![true; waiting.len()];
        for (_, index) in order {
            let asset = &waiting[index];
            loaded.insert(asset.key.to_ascii_lowercase());
            ordered.push(asset.clone());
            keep[index] = false;
        }
        waiting = waiting
            .into_iter()
            .enumerate()
            .filter_map(|(index, asset)| keep[index].then_some(asset))
            .collect();
    }

    Ok(ordered)
}

fn resolve_assets(raw: Vec<RawAsset>, store: &str) -> Result<Vec<LoadedAsset>, AssetError> {
    let mut values = HashMap::with_capacity(raw.len());
    for asset in &raw {
        let mut value = asset.value.clone();
        // The parent may come from the containing item (Java's
        // `INHERIT_ID_AND_PARENT` mode) rather than from the block's own JSON.
        // Annotate the provider document so the shared resolver follows it.
        if let (Some(parent), Value::Object(object)) = (&asset.parent, &mut value) {
            object
                .entry("Parent")
                .or_insert_with(|| Value::String(parent.clone()));
        }
        values
            .entry(asset.key.to_ascii_lowercase())
            .or_insert(value);
    }
    let provider = ValueProvider { values: &values };
    let resolver = ParentResolver::with_hook(&provider, MergeHook::Shallow);

    raw.into_iter()
        .map(|asset| {
            let merged =
                resolver
                    .resolve(&asset.key)
                    .map_err(|error| AssetError::InvalidAsset {
                        store: store.to_string(),
                        key: asset.key.clone(),
                        message: error.to_string(),
                    })?;
            Ok(LoadedAsset { raw: asset, merged })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Store specific loading and typed decoding
// ---------------------------------------------------------------------------

fn load_items(pack: &AssetPack) -> Result<(AssetCollection<Value>, Vec<RawAsset>), AssetError> {
    let preloaded = preloaded_assets_with(item_preloads());
    let mut loaded: HashSet<String> = preloaded
        .iter()
        .map(|asset| asset.key.to_ascii_lowercase())
        .collect();
    let files = read_store_files(pack, ITEM.path)?;
    let ordered = assign_phase(files, &mut loaded, MissingParentPolicy::Error, ITEM.name)?;

    let mut all = preloaded;
    all.extend(ordered);

    let records = all
        .iter()
        .map(|asset| AssetRecord {
            key: asset.key.clone(),
            id: None,
            source: asset.source.clone(),
            preloaded: asset.preloaded,
            value: asset.value.clone(),
        })
        .collect();
    let items = AssetCollection::new(ITEM.name, records)?;

    let mut contained = Vec::new();
    for asset in &all {
        extract_item_block_types(
            &asset.value,
            &asset.key,
            None,
            &asset.source,
            &mut contained,
        );
    }

    Ok((items, contained))
}

fn load_block_types(
    pack: &AssetPack,
    contained: Vec<RawAsset>,
) -> Result<AssetCollection<BlockTypeAsset>, AssetError> {
    let preloaded = preloaded_assets_with(block_type_preloads());
    let mut loaded: HashSet<String> = preloaded
        .iter()
        .map(|asset| asset.key.to_ascii_lowercase())
        .collect();
    let files = read_store_files(pack, BLOCK_TYPE.path)?;
    let ordered = assign_phase(
        files,
        &mut loaded,
        MissingParentPolicy::Error,
        BLOCK_TYPE.name,
    )?;

    let mut all = preloaded;
    all.extend(ordered);

    // A contained block type inherits the item key as its parent only when that
    // key is itself a block type. Java explicitly falls back to a standalone
    // asset for the other case (the injected item parent is not a block key).
    let mut known: HashSet<String> = all
        .iter()
        .map(|asset| asset.key.to_ascii_lowercase())
        .collect();
    known.extend(contained.iter().map(|asset| asset.key.to_ascii_lowercase()));
    let contained: Vec<RawAsset> = contained
        .into_iter()
        .map(|mut asset| {
            if asset
                .parent
                .as_ref()
                .is_some_and(|parent| !known.contains(&parent.to_ascii_lowercase()))
            {
                asset.parent = None;
            }
            asset
        })
        .collect();

    let contained = assign_phase(
        contained,
        &mut loaded,
        MissingParentPolicy::Drop,
        BLOCK_TYPE.name,
    )?;
    all.extend(contained);

    let resolved = resolve_assets(all, BLOCK_TYPE.name)?;
    let records = resolved
        .into_iter()
        .enumerate()
        .map(|(index, asset)| {
            let id = i32::try_from(index).map_err(|_| AssetError::InvalidAsset {
                store: BLOCK_TYPE.name.to_string(),
                key: asset.raw.key.clone(),
                message: "asset id exceeds i32".to_string(),
            })?;
            let value =
                parse_block_type(&asset.merged).map_err(|message| AssetError::InvalidAsset {
                    store: BLOCK_TYPE.name.to_string(),
                    key: asset.raw.key.clone(),
                    message,
                })?;
            Ok(AssetRecord {
                key: asset.raw.key,
                id: Some(id),
                source: asset.raw.source,
                preloaded: asset.raw.preloaded,
                value,
            })
        })
        .collect::<Result<Vec<_>, AssetError>>()?;
    AssetCollection::new(BLOCK_TYPE.name, records)
}

/// Extract the `BlockType` contained in an item, plus its nested state block
/// types and item-state block types. Keys follow Java `AssetExtraInfo`: a
/// contained asset key is `*<container>_<path>` and each nesting level adds a
/// leading `*`.
fn extract_item_block_types(
    item: &Value,
    key: &str,
    inherited_parent: Option<&str>,
    source: &str,
    out: &mut Vec<RawAsset>,
) {
    let item_parent = item
        .get("Parent")
        .and_then(Value::as_str)
        .or(inherited_parent);

    if let Some(block_type) = item.get("BlockType").filter(|value| value.is_object()) {
        let parent = block_type
            .get("Parent")
            .and_then(Value::as_str)
            .or(item_parent)
            .map(str::to_string);
        out.push(RawAsset {
            key: key.to_string(),
            parent,
            source: source.to_string(),
            preloaded: false,
            value: block_type.clone(),
        });
        extract_block_type_definitions(block_type, key, source, out);
    }

    if let Some(states) = item.get("State").and_then(Value::as_object) {
        for (state, value) in states {
            if !value.is_object() {
                continue;
            }
            let state_key = format!("*{key}_State_{state}");
            extract_item_block_types(value, &state_key, Some(key), source, out);
        }
    }
}

fn extract_block_type_definitions(
    block_type: &Value,
    key: &str,
    source: &str,
    out: &mut Vec<RawAsset>,
) {
    let Some(definitions) = block_type
        .get("State")
        .and_then(|state| state.get("Definitions"))
        .and_then(Value::as_object)
    else {
        return;
    };
    for (definition, value) in definitions {
        // A string is a reference to an existing block type, not a new asset.
        if !value.is_object() {
            continue;
        }
        let definition_key = format!("*{key}_State_Definitions_{definition}");
        let parent = value
            .get("Parent")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string();
        out.push(RawAsset {
            key: definition_key.clone(),
            parent: Some(parent),
            source: source.to_string(),
            preloaded: false,
            value: value.clone(),
        });
        extract_block_type_definitions(value, &definition_key, source, out);
    }
}

fn parse_key_asset(merged: &Value) -> Result<Value, String> {
    Ok(merged.clone())
}

fn parse_fluid(merged: &Value) -> Result<FluidAsset, String> {
    let max_fluid_level = match merged.get("MaxFluidLevel") {
        None | Some(Value::Null) => 8,
        Some(value) => value
            .as_i64()
            .ok_or_else(|| "MaxFluidLevel must be an integer".to_string())?,
    };
    if !(0..=15).contains(&max_fluid_level) {
        return Err(format!(
            "MaxFluidLevel {max_fluid_level} is outside Java's 0..=15 range"
        ));
    }
    Ok(FluidAsset {
        max_fluid_level: max_fluid_level as i8,
        json: merged.clone(),
    })
}

fn parse_block_type(merged: &Value) -> Result<BlockTypeAsset, String> {
    let draw_type = parse_draw_type(merged.get("DrawType"))?;
    let opacity = parse_opacity(merged.get("Opacity"))?;
    let material = parse_material(merged.get("Material"))?;
    let group = merged.get("Group").and_then(Value::as_str);
    let support = parse_support(merged.get("Support"), material, group)?;
    Ok(BlockTypeAsset {
        draw_type,
        opacity,
        material,
        support,
        json: merged.clone(),
    })
}

fn parse_draw_type(value: Option<&Value>) -> Result<DrawType, String> {
    match value {
        None | Some(Value::Null) => Ok(DrawType::Cube),
        Some(Value::String(name)) => match name.as_str() {
            "Empty" => Ok(DrawType::Empty),
            "GizmoCube" => Ok(DrawType::GizmoCube),
            "Cube" => Ok(DrawType::Cube),
            "Model" => Ok(DrawType::Model),
            "CubeWithModel" => Ok(DrawType::CubeWithModel),
            other => Err(format!("unknown DrawType `{other}`")),
        },
        Some(_) => Err("DrawType must be a string".to_string()),
    }
}

fn parse_opacity(value: Option<&Value>) -> Result<Opacity, String> {
    match value {
        None | Some(Value::Null) => Ok(Opacity::Solid),
        Some(Value::String(name)) => match name.as_str() {
            "Solid" => Ok(Opacity::Solid),
            "Semitransparent" => Ok(Opacity::Semitransparent),
            "Cutout" => Ok(Opacity::Cutout),
            "Transparent" => Ok(Opacity::Transparent),
            other => Err(format!("unknown Opacity `{other}`")),
        },
        Some(_) => Err("Opacity must be a string".to_string()),
    }
}

fn parse_material(value: Option<&Value>) -> Result<BlockMaterial, String> {
    match value {
        None | Some(Value::Null) => Ok(BlockMaterial::Empty),
        Some(Value::String(name)) => match name.as_str() {
            "Empty" => Ok(BlockMaterial::Empty),
            "Solid" => Ok(BlockMaterial::Solid),
            other => Err(format!("unknown Material `{other}`")),
        },
        Some(_) => Err("Material must be a string".to_string()),
    }
}

fn parse_support(
    value: Option<&Value>,
    material: BlockMaterial,
    group: Option<&str>,
) -> Result<Option<BlockTypeSupport>, String> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        // Java `BlockType.processConfig`: plants require support below unless
        // the block is a technical block.
        if material == BlockMaterial::Empty && group != Some("@Tech") {
            return Ok(Some(BlockTypeSupport {
                faces: vec![FACE_DOWN],
                requirements: vec![SupportRequirement {
                    face: FACE_DOWN,
                    rotate: true,
                }],
            }));
        }
        return Ok(None);
    };
    let Some(map) = value.as_object() else {
        return Err("Support must be an object".to_string());
    };

    let mut faces = Vec::new();
    let mut requirements = Vec::new();
    for (name, entries) in map {
        let expanded =
            Face::expand(name).ok_or_else(|| format!("unknown Support face or alias `{name}`"))?;
        let Some(entries) = entries.as_array() else {
            return Err(format!("Support.{name} must be an array"));
        };
        for face in expanded {
            faces.push(face);
            for entry in entries {
                let rotate = entry.get("Rotate").and_then(Value::as_bool).unwrap_or(true);
                requirements.push(SupportRequirement { face, rotate });
            }
        }
    }

    Ok(Some(BlockTypeSupport {
        faces,
        requirements,
    }))
}

// ---------------------------------------------------------------------------
// Preloaded (code-defined) assets
// ---------------------------------------------------------------------------

fn block_type_preloads() -> Vec<(&'static str, Value)> {
    vec![
        (
            "Empty",
            json!({
                "DrawType": "Empty",
                "Material": "Empty",
                "Opacity": "Transparent",
                "Support": {}
            }),
        ),
        ("Unknown", json!({"DrawType": "Cube", "Material": "Solid"})),
        (
            "Debug_Cube",
            json!({"DrawType": "Cube", "Material": "Solid"}),
        ),
        (
            "Debug_Model",
            json!({"DrawType": "Model", "Material": "Empty"}),
        ),
    ]
}

fn fluid_preloads() -> Vec<(&'static str, Value)> {
    vec![("Empty", json!({})), ("Unknown", json!({}))]
}

fn environment_preloads() -> Vec<(&'static str, Value)> {
    vec![("Unknown", json!({}))]
}

fn physical_material_preloads() -> Vec<(&'static str, Value)> {
    vec![("EMPTY", json!({}))]
}

fn block_bounding_box_preloads() -> Vec<(&'static str, Value)> {
    vec![(
        "Full",
        json!({
            "Boxes": [{
                "Min": {"X": 0.0, "Y": 0.0, "Z": 0.0},
                "Max": {"X": 1.0, "Y": 1.0, "Z": 1.0}
            }]
        }),
    )]
}

fn item_preloads() -> Vec<(&'static str, Value)> {
    vec![("Unknown", json!({}))]
}
