//! Block simulation: scheduled/random ticks, support, fluid flow, falling blocks
//! and transient block health.
//!
//! Ported behavior from Java's `BlockSection` tick queue, `BlockTickPlugin`,
//! `builtin/RandomTick`, `FluidTicker`/`DefaultFluidTicker`,
//! `builtin/BlockPhysics` and `builtin/FallingBlocks`.
//!
//! This is the M5 core, not the full builtin runtime: it owns the deterministic
//! single-threaded pass over loaded chunks and emits plain block/fluid changes.
//! The network/session layer is responsible for broadcasting those changes.
//!
//! Deliberate reductions (recorded in `tasks/T52-block-simulation.md`):
//! - fluid flow ignores partial-block face coverage (only cube/cube-with-model
//!   blocks block flow);
//! - support propagation is direct-face only (no `MaxSupportDistance` chain);
//! - random growth ignores light level and reduced support/filler details;
//! - falling blocks are simulated as one-cell-per-tick block movement, not ECS
//!   entities.

use std::collections::{BTreeMap, BTreeSet};

use hytale_assets::AssetRegistry;
use hytale_math::fast_random::FastRandom;
use hytale_protocol::generated::common::{BlockMaterial, DrawType, Opacity};
use serde_json::Value;

use crate::chunk::section::BlockTickStrategy;
use crate::world::World;

const DEFAULT_FLOW_RATE: f32 = 0.5;
const DEFAULT_MAX_FLUID_LEVEL: i8 = 8;
const STABLE_RANDOM_TICKS: usize = 1;
const UNSTABLE_RANDOM_TICKS: usize = 3;
const FLUID_BLOCK_DISTANCE: i32 = 5;

/// One direction used by support and growth tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dir {
    /// X offset.
    pub x: i8,
    /// Y offset.
    pub y: i8,
    /// Z offset.
    pub z: i8,
}

impl Dir {
    /// Up.
    pub const UP: Self = Self::new(0, 1, 0);
    /// Down.
    pub const DOWN: Self = Self::new(0, -1, 0);
    /// North (`-Z`).
    pub const NORTH: Self = Self::new(0, 0, -1);
    /// South (`+Z`).
    pub const SOUTH: Self = Self::new(0, 0, 1);
    /// East (`+X`).
    pub const EAST: Self = Self::new(1, 0, 0);
    /// West (`-X`).
    pub const WEST: Self = Self::new(-1, 0, 0);

    /// Construct a direction.
    pub const fn new(x: i8, y: i8, z: i8) -> Self {
        Self { x, y, z }
    }

    /// The opposite direction.
    pub const fn opposite(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }

    /// Parse a `BlockFace`/`BlockNeighbor` name.
    pub fn from_name(name: &str) -> Option<Self> {
        let name: String = name
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .map(|character| character.to_ascii_lowercase())
            .collect();
        Some(match name.as_str() {
            "up" => Self::UP,
            "down" => Self::DOWN,
            "north" => Self::NORTH,
            "south" => Self::SOUTH,
            "east" => Self::EAST,
            "west" => Self::WEST,
            "upnorth" => Self::new(0, 1, -1),
            "upsouth" => Self::new(0, 1, 1),
            "upeast" => Self::new(1, 1, 0),
            "upwest" => Self::new(-1, 1, 0),
            "downnorth" => Self::new(0, -1, -1),
            "downsouth" => Self::new(0, -1, 1),
            "downeast" => Self::new(1, -1, 0),
            "downwest" => Self::new(-1, -1, 0),
            "northeast" => Self::new(1, 0, -1),
            "southeast" => Self::new(1, 0, 1),
            "southwest" => Self::new(-1, 0, 1),
            "northwest" => Self::new(-1, 0, -1),
            _ => return None,
        })
    }

    /// Rotate by a Java `RotationTuple` index (`roll*16 + pitch*4 + yaw`).
    pub fn rotated(self, rotation_index: i32) -> Self {
        let rotation = rotation_index.rem_euclid(64) as u8;
        let yaw = rotation % 4;
        let pitch = (rotation / 4) % 4;
        let roll = rotation / 16;
        let mut out = self;
        out = out.rotate_z(roll);
        out = out.rotate_x(pitch);
        out = out.rotate_y(yaw);
        out
    }

    fn rotate_x(self, rotation: u8) -> Self {
        match rotation {
            1 => Self::new(self.x, -self.z, self.y),
            2 => Self::new(self.x, -self.y, -self.z),
            3 => Self::new(self.x, self.z, -self.y),
            _ => self,
        }
    }

    fn rotate_y(self, rotation: u8) -> Self {
        match rotation {
            1 => Self::new(self.z, self.y, -self.x),
            2 => Self::new(-self.x, self.y, -self.z),
            3 => Self::new(-self.z, self.y, self.x),
            _ => self,
        }
    }

    fn rotate_z(self, rotation: u8) -> Self {
        match rotation {
            1 => Self::new(-self.y, self.x, self.z),
            2 => Self::new(-self.x, -self.y, self.z),
            3 => Self::new(self.y, -self.x, self.z),
            _ => self,
        }
    }
}

/// Java `SupportDropType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportDropType {
    /// Break with drops.
    Break,
    /// Destroy without drops.
    Destroy,
    /// Convert into a falling block.
    Fall,
}

/// One `Support` requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportRule {
    /// Face on the block being checked.
    pub face: Dir,
    /// Rotate the face with the block.
    pub rotate: bool,
    /// Java `RequiredBlockFaceSupport.Support`.
    pub required: bool,
    /// Neighbour block type requirement.
    pub block_type: Option<i32>,
    /// Neighbour tag requirement (`TagId`).
    pub tag: Option<String>,
    /// Neighbour fluid requirement.
    pub fluid: Option<i32>,
    /// Neighbour face type (`FaceType`).
    pub face_type: Option<String>,
    /// Own face type (`SelfFaceType`).
    pub self_face_type: Option<String>,
    /// `MatchSelf` (`required`, `disallowed`, `ignored`).
    pub match_self: MatchSelf,
}

/// Java `RequiredBlockFaceSupport.MatchSelf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchSelf {
    /// Ignore.
    Ignored,
    /// Neighbour must have the same block type.
    Required,
    /// Neighbour must have a different block type.
    Disallowed,
}

/// One `Supporting` face offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportingFace {
    /// Face of the block that offers support.
    pub face: Dir,
    /// Advertised face type.
    pub face_type: String,
}

/// A parsed `Gathering` drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDrop {
    /// Item id, when the drop is a fixed item.
    pub item_id: Option<String>,
    /// Item drop-list id, when the drop is a weighted list.
    pub drop_list_id: Option<String>,
    /// Quantity for a fixed item.
    pub quantity: i32,
}

/// `BlockGathering.Breaking`/`Harvest`/`Soft`/`Physics`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockGathering {
    /// Player-breaking drops.
    pub breaking: Option<BlockDrop>,
    /// Harvest-tool drops.
    pub harvest: Option<BlockDrop>,
    /// Soft-block drops.
    pub soft: Option<BlockDrop>,
    /// Physics-break drops.
    pub physics: Option<BlockDrop>,
    /// Whether placed blocks use the default drop behavior.
    pub use_default_drop_when_placed: bool,
}

impl BlockGathering {
    /// Whether this block is soft-breakable.
    pub fn is_soft(&self) -> bool {
        self.soft.is_some()
    }

    /// Whether this block is harvestable with a tool.
    pub fn is_harvestable(&self) -> bool {
        self.harvest.is_some()
    }
}

/// One random-tick procedure (`RandomTickProcedure.CODEC`).
#[derive(Debug, Clone, PartialEq)]
pub enum RandomTickProcedure {
    /// Replace the block with `TargetBlock`.
    ChangeIntoBlock {
        /// Target block id, when it resolves.
        target: Option<i32>,
    },
    /// Spread the same block to adjacent blocks matching `AllowedTag`.
    SpreadTo {
        /// Relative offsets.
        directions: Vec<Dir>,
        /// Minimum relative Y.
        min_y: i32,
        /// Maximum relative Y.
        max_y: i32,
        /// Target block tag (`Spreadable=Grass`).
        allowed_tag: String,
        /// Whether the target's above cell must be empty.
        require_empty_above: bool,
        /// Allowed fluid names above the target (`["Empty"]` = no fluid).
        allowed_above_fluids: Option<Vec<i32>>,
        /// Revert block when covered.
        revert_block: Option<i32>,
    },
    /// Place a fixed block at an offset.
    PlaceBlock {
        /// Offset from the ticking block.
        offset: Dir,
        /// Candidate block ids.
        placements: Vec<i32>,
        /// Whether the target must be empty.
        require_empty_target: bool,
    },
    /// A procedure this core does not yet execute.
    Unsupported(String),
}

/// One scheduled tick procedure (`TickProcedure.CODEC`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickProcedure {
    /// Chance-based replacement.
    BasicChanceBlockGrowth {
        /// Minimum roll (`chanceMin`).
        chance_min: i32,
        /// Roll bound (`Chance`).
        chance: i32,
        /// Target block id.
        target: Option<i32>,
        /// Keep ticking after a change.
        next_ticking: bool,
    },
    /// A procedure this core does not yet execute.
    Unsupported(String),
}

/// Parsed collision conversion for one fluid contact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FluidCollision {
    /// Fluid that triggers the contact.
    pub other: i32,
    /// Block to place.
    pub block_to_place: Option<i32>,
    /// Whether the flowing fluid remains.
    pub place_fluid: bool,
}

/// One fluid's simulation behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct FluidBehavior {
    /// Maximum level.
    pub max_level: i8,
    /// Tick frequency in seconds.
    pub flow_rate: f32,
    /// Whether levels can drop when unsupported.
    pub can_demote: bool,
    /// Supporting source fluid.
    pub supported_by: Option<i32>,
    /// Fluid placed when spreading.
    pub spread_fluid: Option<i32>,
    /// Collision conversions.
    pub collisions: Vec<FluidCollision>,
}

/// One block's simulation behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockBehavior {
    /// Material.
    pub material: BlockMaterial,
    /// Draw type.
    pub draw_type: DrawType,
    /// Opacity.
    pub opacity: Opacity,
    /// Whether this block fills a fluid-tight cube.
    pub fluid_solid: bool,
    /// Support requirements.
    pub support: Vec<SupportRule>,
    /// Drop behavior on lost support.
    pub support_drop: SupportDropType,
    /// Java `MaxSupportDistance` (parsed; direct support only in this core).
    pub max_support_distance: i32,
    /// Whether the block has `FallingBlockSettings`.
    pub falling: bool,
    /// Gathering/drop metadata.
    pub gathering: Option<BlockGathering>,
    /// Random tick behavior.
    pub random_tick: Option<RandomTickProcedure>,
    /// Scheduled tick behavior.
    pub tick_procedure: Option<TickProcedure>,
    /// Block tags expanded from the containing item (`Type=Soil`, `Spreadable=Grass`).
    pub tags: BTreeSet<String>,
    /// Faces this block offers for `Support` checks.
    pub supporting: Vec<SupportingFace>,
}

/// Asset-derived block/fluid behavior tables.
#[derive(Debug, Clone, Default)]
pub struct SimulationTables {
    blocks: Vec<Option<BlockBehavior>>,
    fluids: Vec<Option<FluidBehavior>>,
}

impl SimulationTables {
    /// Build tables directly (test/embedding seam).
    pub fn from_parts(
        blocks: Vec<Option<BlockBehavior>>,
        fluids: Vec<Option<FluidBehavior>>,
    ) -> Self {
        Self { blocks, fluids }
    }

    /// Build the tables from the server's registry.
    pub fn from_assets(assets: &AssetRegistry) -> Self {
        let block_highest = assets
            .block_types
            .iter()
            .filter_map(|record| record.id)
            .max()
            .unwrap_or(0)
            .max(0) as usize;
        let fluid_highest = assets
            .fluids
            .iter()
            .filter_map(|record| record.id)
            .max()
            .unwrap_or(0)
            .max(0) as usize;
        let mut blocks = vec![None; block_highest + 1];
        let mut fluids = vec![None; fluid_highest + 1];
        let fluid_ids: BTreeMap<String, i32> = assets
            .fluids
            .iter()
            .filter_map(|record| record.id.map(|id| (record.key.clone(), id)))
            .collect();
        let block_ids: BTreeMap<String, i32> = assets
            .block_types
            .iter()
            .filter_map(|record| record.id.map(|id| (record.key.clone(), id)))
            .collect();

        for record in assets.block_types.iter() {
            let Some(id) = record.id else {
                continue;
            };
            let tags = block_tags(assets, record);
            let behavior = parse_block(record.value.json.clone(), &tags, &fluid_ids, &block_ids);
            if id >= 0 {
                blocks[id as usize] = Some(behavior);
            }
        }
        for record in assets.fluids.iter() {
            let Some(id) = record.id else {
                continue;
            };
            let behavior = parse_fluid(
                &record.value.json,
                record.value.max_fluid_level,
                &fluid_ids,
                &block_ids,
            );
            if id >= 0 {
                fluids[id as usize] = Some(behavior);
            }
        }
        Self { blocks, fluids }
    }

    /// One block's behavior.
    pub fn block(&self, id: i32) -> Option<&BlockBehavior> {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.blocks.get(id))
            .and_then(Option::as_ref)
    }

    /// One fluid's behavior.
    pub fn fluid(&self, id: i32) -> Option<&FluidBehavior> {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.fluids.get(id))
            .and_then(Option::as_ref)
    }

    /// Player-breaking drops for one block.
    pub fn breaking_drops(&self, id: i32) -> Vec<BlockDrop> {
        self.block(id)
            .and_then(|block| block.gathering.as_ref())
            .and_then(|gathering| gathering.breaking.clone())
            .into_iter()
            .collect()
    }

    /// Drops used when physics/support removes a block, matching Java's
    /// `naturallyRemoveBlockByPhysics` priority: physics, breaking, soft, harvest.
    pub fn natural_drops(&self, id: i32) -> Vec<BlockDrop> {
        self.block(id)
            .and_then(|block| block.gathering.as_ref())
            .and_then(|gathering| {
                gathering
                    .physics
                    .clone()
                    .or_else(|| gathering.breaking.clone())
                    .or_else(|| gathering.soft.clone())
                    .or_else(|| gathering.harvest.clone())
            })
            .into_iter()
            .collect()
    }

    /// Number of indexed block entries.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Number of indexed fluid entries.
    pub fn fluid_count(&self) -> usize {
        self.fluids.len()
    }
}

fn block_tags(
    assets: &AssetRegistry,
    record: &hytale_assets::AssetRecord<hytale_assets::BlockTypeAsset>,
) -> BTreeSet<String> {
    let source = record
        .value
        .container_item
        .as_ref()
        .and_then(|item| assets.items.get(item))
        .map(|item| &item.value)
        .or_else(|| record.value.json.get("Tags").map(|_| &record.value.json));
    let mut tags = BTreeSet::new();
    if let Some(value) = source {
        collect_tags(value, &mut tags);
    }
    collect_tags(&record.value.json, &mut tags);
    tags
}

fn collect_tags(value: &Value, tags: &mut BTreeSet<String>) {
    let Some(map) = value.get("Tags").and_then(Value::as_object) else {
        return;
    };
    for (name, values) in map {
        tags.insert(name.clone());
        if let Some(values) = values.as_array() {
            for value in values.iter().filter_map(Value::as_str) {
                tags.insert(value.to_string());
                tags.insert(format!("{name}={value}"));
            }
        }
    }
}

fn parse_block(
    json: Value,
    tags: &BTreeSet<String>,
    fluid_ids: &BTreeMap<String, i32>,
    block_ids: &BTreeMap<String, i32>,
) -> BlockBehavior {
    let material = match json.get("Material").and_then(Value::as_str) {
        Some("Solid") => BlockMaterial::Solid,
        _ => BlockMaterial::Empty,
    };
    let draw_type = match json.get("DrawType").and_then(Value::as_str) {
        Some("GizmoCube") => DrawType::GizmoCube,
        Some("Model") => DrawType::Model,
        Some("CubeWithModel") => DrawType::CubeWithModel,
        Some("Empty") => DrawType::Empty,
        _ => DrawType::Cube,
    };
    let opacity = match json.get("Opacity").and_then(Value::as_str) {
        Some("Semitransparent") => Opacity::Semitransparent,
        Some("Cutout") => Opacity::Cutout,
        Some("Transparent") => Opacity::Transparent,
        _ => Opacity::Solid,
    };
    let fluid_solid = matches!(draw_type, DrawType::Cube | DrawType::CubeWithModel);
    let support_drop = match json.get("SupportDropType").and_then(Value::as_str) {
        Some("Destroy") => SupportDropType::Destroy,
        Some("Fall") => SupportDropType::Fall,
        _ => SupportDropType::Break,
    };
    let max_support_distance = json
        .get("MaxSupportDistance")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let support = parse_support_rules(
        json.get("Support"),
        &material,
        json.get("Group").and_then(Value::as_str),
        fluid_ids,
        block_ids,
    );
    let supporting = parse_supporting(json.get("Supporting"), material, draw_type);
    let gathering = parse_gathering(json.get("Gathering"));
    let random_tick = parse_random_tick(json.get("RandomTickProcedure"), fluid_ids, block_ids);
    let tick_procedure = parse_tick_procedure(json.get("TickProcedure"), block_ids);
    BlockBehavior {
        material,
        draw_type,
        opacity,
        fluid_solid,
        support,
        support_drop,
        max_support_distance,
        falling: json.get("FallingBlockSettings").is_some(),
        gathering,
        random_tick,
        tick_procedure,
        tags: tags.clone(),
        supporting,
    }
}

fn parse_support_rules(
    value: Option<&Value>,
    material: &BlockMaterial,
    group: Option<&str>,
    fluid_ids: &BTreeMap<String, i32>,
    block_ids: &BTreeMap<String, i32>,
) -> Vec<SupportRule> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        if *material == BlockMaterial::Empty && group != Some("@Tech") {
            return vec![SupportRule {
                face: Dir::DOWN,
                rotate: true,
                required: true,
                block_type: None,
                tag: None,
                fluid: None,
                face_type: Some("Full".to_string()),
                self_face_type: None,
                match_self: MatchSelf::Ignored,
            }];
        }
        return Vec::new();
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    let mut rules = Vec::new();
    for (name, entries) in map {
        let Some(face) = Dir::from_name(name) else {
            continue;
        };
        let Some(entries) = entries.as_array() else {
            continue;
        };
        for entry in entries {
            let required = !matches!(
                entry.get("Support").and_then(Value::as_str),
                Some("Disallowed")
            );
            rules.push(SupportRule {
                face,
                rotate: entry.get("Rotate").and_then(Value::as_bool).unwrap_or(true),
                required,
                block_type: entry
                    .get("BlockType")
                    .and_then(Value::as_str)
                    .and_then(|name| block_ids.get(name).copied()),
                tag: entry
                    .get("TagId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                fluid: entry
                    .get("FluidId")
                    .and_then(Value::as_str)
                    .and_then(|name| fluid_ids.get(name).copied()),
                face_type: entry
                    .get("FaceType")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                self_face_type: entry
                    .get("SelfFaceType")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                match_self: match entry.get("MatchSelf").and_then(Value::as_str) {
                    Some("Required") => MatchSelf::Required,
                    Some("Disallowed") => MatchSelf::Disallowed,
                    _ => MatchSelf::Ignored,
                },
            });
        }
    }
    rules
}

fn parse_supporting(
    value: Option<&Value>,
    material: BlockMaterial,
    draw_type: DrawType,
) -> Vec<SupportingFace> {
    if let Some(map) = value.and_then(Value::as_object) {
        let mut faces = Vec::new();
        for (name, entries) in map {
            let Some(face) = Dir::from_name(name) else {
                continue;
            };
            let Some(entries) = entries.as_array() else {
                continue;
            };
            for entry in entries {
                let face_type = entry
                    .get("FaceType")
                    .and_then(Value::as_str)
                    .unwrap_or("Full");
                faces.push(SupportingFace {
                    face,
                    face_type: face_type.to_string(),
                });
            }
        }
        return faces;
    }
    if material == BlockMaterial::Solid
        && matches!(
            draw_type,
            DrawType::Cube | DrawType::CubeWithModel | DrawType::GizmoCube
        )
    {
        return [
            Dir::UP,
            Dir::DOWN,
            Dir::NORTH,
            Dir::SOUTH,
            Dir::EAST,
            Dir::WEST,
        ]
        .into_iter()
        .map(|face| SupportingFace {
            face,
            face_type: "Full".to_string(),
        })
        .collect();
    }
    Vec::new()
}

fn parse_gathering(value: Option<&Value>) -> Option<BlockGathering> {
    let map = value?.as_object()?;
    let drop = |value: Option<&Value>| -> Option<BlockDrop> {
        let map = value?.as_object()?;
        let item_id = map
            .get("ItemId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let drop_list_id = map
            .get("DropList")
            .and_then(Value::as_str)
            .map(str::to_string);
        if item_id.is_none() && drop_list_id.is_none() {
            return None;
        }
        Some(BlockDrop {
            item_id,
            drop_list_id,
            quantity: map.get("Quantity").and_then(Value::as_i64).unwrap_or(1) as i32,
        })
    };
    Some(BlockGathering {
        breaking: drop(map.get("Breaking")),
        harvest: drop(map.get("Harvest")),
        soft: drop(map.get("Soft")),
        physics: drop(map.get("Physics")),
        use_default_drop_when_placed: map
            .get("UseDefaultDropWhenPlaced")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_random_tick(
    value: Option<&Value>,
    fluid_ids: &BTreeMap<String, i32>,
    block_ids: &BTreeMap<String, i32>,
) -> Option<RandomTickProcedure> {
    let map = value?.as_object()?;
    let kind = map.get("Type").and_then(Value::as_str).unwrap_or_default();
    Some(match kind {
        "ChangeIntoBlock" => RandomTickProcedure::ChangeIntoBlock {
            target: map
                .get("TargetBlock")
                .and_then(Value::as_str)
                .and_then(|name| block_ids.get(name).copied()),
        },
        "SpreadTo" => RandomTickProcedure::SpreadTo {
            directions: map
                .get("SpreadDirections")
                .and_then(Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|entry| {
                            Dir::new(
                                entry.get("X").and_then(Value::as_i64).unwrap_or(0) as i8,
                                entry.get("Y").and_then(Value::as_i64).unwrap_or(0) as i8,
                                entry.get("Z").and_then(Value::as_i64).unwrap_or(0) as i8,
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
            min_y: map.get("MinY").and_then(Value::as_i64).unwrap_or(0) as i32,
            max_y: map.get("MaxY").and_then(Value::as_i64).unwrap_or(0) as i32,
            allowed_tag: map
                .get("AllowedTag")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            require_empty_above: map
                .get("RequireEmptyAboveTarget")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            allowed_above_fluids: map.get("AllowedAboveFluids").and_then(Value::as_array).map(
                |entries| {
                    entries
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|name| fluid_ids.get(name).copied().unwrap_or(0))
                        .collect()
                },
            ),
            revert_block: map
                .get("RevertBlock")
                .and_then(Value::as_str)
                .and_then(|name| block_ids.get(name).copied()),
        },
        "PlaceBlock" => {
            let offset = map.get("Offset");
            RandomTickProcedure::PlaceBlock {
                offset: Dir::new(
                    offset
                        .and_then(|offset| offset.get("X"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i8,
                    offset
                        .and_then(|offset| offset.get("Y"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i8,
                    offset
                        .and_then(|offset| offset.get("Z"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0) as i8,
                ),
                placements: map
                    .get("Placements")
                    .and_then(Value::as_array)
                    .map(|placements| {
                        placements
                            .iter()
                            .filter_map(|placement| {
                                placement
                                    .get("Block")
                                    .and_then(Value::as_str)
                                    .and_then(|name| block_ids.get(name).copied())
                                    .or_else(|| {
                                        placement
                                            .get("Blocks")
                                            .and_then(Value::as_array)
                                            .and_then(|blocks| blocks.first())
                                            .and_then(|block| block.get("Block"))
                                            .and_then(Value::as_str)
                                            .and_then(|name| block_ids.get(name).copied())
                                    })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                require_empty_target: map
                    .get("RequireEmptyTarget")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            }
        }
        other => RandomTickProcedure::Unsupported(other.to_string()),
    })
}

fn parse_tick_procedure(
    value: Option<&Value>,
    block_ids: &BTreeMap<String, i32>,
) -> Option<TickProcedure> {
    let map = value?.as_object()?;
    let kind = map.get("Type").and_then(Value::as_str).unwrap_or_default();
    Some(match kind {
        "BasicChanceBlockGrowth" => TickProcedure::BasicChanceBlockGrowth {
            chance_min: map.get("ChanceMin").and_then(Value::as_i64).unwrap_or(0) as i32,
            chance: map.get("Chance").and_then(Value::as_i64).unwrap_or(1) as i32,
            target: map
                .get("NextId")
                .and_then(Value::as_str)
                .and_then(|name| block_ids.get(name).copied()),
            next_ticking: map
                .get("NextTicking")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        other => TickProcedure::Unsupported(other.to_string()),
    })
}

fn parse_fluid(
    json: &Value,
    max_level: i8,
    fluid_ids: &BTreeMap<String, i32>,
    block_ids: &BTreeMap<String, i32>,
) -> FluidBehavior {
    let ticker = json.get("Ticker").and_then(Value::as_object);
    let flow_rate = ticker
        .and_then(|ticker| ticker.get("FlowRate"))
        .and_then(Value::as_f64)
        .map_or(DEFAULT_FLOW_RATE, |value| value as f32);
    let can_demote = ticker
        .and_then(|ticker| ticker.get("CanDemote"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let supported_by = ticker
        .and_then(|ticker| ticker.get("SupportedBy"))
        .and_then(Value::as_str)
        .and_then(|name| fluid_ids.get(name).copied());
    let spread_fluid = ticker
        .and_then(|ticker| ticker.get("SpreadFluid"))
        .and_then(Value::as_str)
        .and_then(|name| fluid_ids.get(name).copied());
    let collisions = ticker
        .and_then(|ticker| ticker.get("Collisions"))
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(name, entry)| {
                    let other = *fluid_ids.get(name)?;
                    Some(FluidCollision {
                        other,
                        block_to_place: entry
                            .get("BlockToPlace")
                            .and_then(Value::as_str)
                            .and_then(|name| block_ids.get(name).copied()),
                        place_fluid: entry
                            .get("PlaceFluid")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    FluidBehavior {
        max_level: if max_level <= 0 {
            DEFAULT_MAX_FLUID_LEVEL
        } else {
            max_level
        },
        flow_rate: flow_rate.max(0.05),
        can_demote,
        supported_by,
        spread_fluid,
        collisions,
    }
}

/// One block change emitted by the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockChange {
    /// X.
    pub x: i32,
    /// Y.
    pub y: i32,
    /// Z.
    pub z: i32,
    /// New block id (`0` = air).
    pub block_id: i32,
    /// New rotation index.
    pub rotation: i32,
    /// New filler offset.
    pub filler: i32,
}

/// One fluid change emitted by the simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FluidChange {
    /// X.
    pub x: i32,
    /// Y.
    pub y: i32,
    /// Z.
    pub z: i32,
    /// New fluid id (`0` = empty).
    pub fluid_id: i32,
    /// New level.
    pub level: i8,
}

/// Result of one simulation pass.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SimulationReport {
    /// Ticked block entries.
    pub block_ticks: usize,
    /// Random-tick attempts.
    pub random_ticks: usize,
    /// Fluid entries ticked.
    pub fluid_ticks: usize,
    /// Blocks removed or changed by lost support.
    pub support_changes: usize,
    /// Falling blocks that moved a cell.
    pub falling_moves: usize,
    /// Falling blocks that landed.
    pub falling_placed: usize,
    /// Block changes to broadcast.
    pub block_changes: Vec<BlockChange>,
    /// Fluid changes to broadcast.
    pub fluid_changes: Vec<FluidChange>,
    /// Drops produced by support/physics removal.
    pub drops: Vec<(i32, i32, i32, BlockDrop)>,
}

#[derive(Debug, Clone)]
struct FallingBlock {
    block_id: i32,
    rotation: i32,
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Debug, Clone, Copy)]
struct NeighborInfo<'a> {
    id: i32,
    behavior: Option<&'a BlockBehavior>,
    fluid: i32,
}

/// One deterministic block-simulation pass.
#[derive(Debug, Clone)]
pub struct BlockSimulation {
    tables: SimulationTables,
    random: FastRandom,
    falling: Vec<FallingBlock>,
}

impl BlockSimulation {
    /// Create a simulation over the given asset-derived tables.
    pub fn new(tables: SimulationTables, seed: i64) -> Self {
        Self {
            tables,
            random: FastRandom::new(seed),
            falling: Vec::new(),
        }
    }

    /// Read-only behavior tables.
    pub fn tables(&self) -> &SimulationTables {
        &self.tables
    }

    /// Pending falling blocks (debug/test seam).
    pub fn falling_count(&self) -> usize {
        self.falling.len()
    }

    /// Schedule an explicit block tick at `tick + delay`.
    pub fn schedule(&mut self, world: &mut World, x: i32, y: i32, z: i32, delay: u64) {
        let due = world.tick().wrapping_add(delay);
        let Some(column) = world.chunk_mut(x >> 5, z >> 5) else {
            return;
        };
        let Some(section) = column.section_mut(y) else {
            return;
        };
        section.block.schedule_tick(index(x, y, z), due);
    }

    /// Force the random-tick procedure at one block (debug/test seam).
    pub fn force_random_tick(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
    ) -> SimulationReport {
        let mut report = SimulationReport::default();
        let block_id = world.get_block(x, y, z);
        if let Some(procedure) = self
            .tables
            .block(block_id)
            .and_then(|block| block.random_tick.clone())
        {
            report.random_ticks += 1;
            self.run_random_tick(world, (x, y, z), block_id, procedure, &mut report);
        }
        report
    }

    /// Run one fixed simulation tick over every loaded chunk.
    pub fn tick(&mut self, world: &mut World, dt: f32) -> SimulationReport {
        let tick = world.advance_tick();
        let changed = world.take_changed_blocks();
        for (x, y, z) in changed {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        self.schedule(world, x + dx, y + dy, z + dz, 0);
                    }
                }
            }
        }

        let mut report = SimulationReport::default();
        let mut due = Vec::new();
        let chunks = world.loaded_chunk_coordinates();
        for (chunk_x, chunk_z) in chunks {
            let Some(column) = world.chunk_mut(chunk_x, chunk_z) else {
                continue;
            };
            for section_index in 0..10 {
                let section_y = section_index * 32;
                let Some(section) = column.section_mut(section_y) else {
                    continue;
                };
                let indices = section.block.begin_tick(tick);
                if !indices.is_empty() {
                    due.push((chunk_x, chunk_z, section_y, indices));
                }
            }
        }

        for (chunk_x, chunk_z, section_y, indices) in due {
            for index in indices {
                let x = (chunk_x << 5) + (index & 31) as i32;
                let y = section_y + ((index >> 10) & 31) as i32;
                let z = (chunk_z << 5) + ((index >> 5) & 31) as i32;
                let strategy = self.tick_block(world, x, y, z, &mut report);
                if let Some(column) = world.chunk_mut(chunk_x, chunk_z) {
                    if let Some(section) = column.section_mut(section_y) {
                        section.block.finish_tick(index, strategy);
                    }
                }
            }
        }

        self.random_ticks(world, &mut report);
        self.tick_falling(world, &mut report);
        world.tick_block_health(dt);
        report
    }

    fn tick_block(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        report: &mut SimulationReport,
    ) -> BlockTickStrategy {
        let fluid_id = world.get_fluid_id(x, y, z);
        if fluid_id != 0 {
            report.fluid_ticks += 1;
            return self.tick_fluid(world, x, y, z, fluid_id, report);
        }
        let block_id = world.get_block(x, y, z);
        if block_id == 0 {
            return BlockTickStrategy::Ignored;
        }
        let Some(block) = self.tables.block(block_id).cloned() else {
            return BlockTickStrategy::Ignored;
        };
        report.block_ticks += 1;
        if !block.support.is_empty()
            && !self.support_satisfied(
                world,
                (x, y, z),
                block_id,
                world.get_rotation(x, y, z),
                &block,
            )
        {
            report.support_changes += 1;
            match block.support_drop {
                SupportDropType::Fall => {
                    self.remove_falling(world, x, y, z, block_id, world.get_rotation(x, y, z));
                }
                SupportDropType::Break | SupportDropType::Destroy => {
                    if block.support_drop == SupportDropType::Break {
                        report.drops.extend(
                            self.tables
                                .natural_drops(block_id)
                                .into_iter()
                                .map(|drop| (x, y, z, drop)),
                        );
                    }
                    let rotation = world.get_rotation(x, y, z);
                    let filler = world.get_filler(x, y, z);
                    if world
                        .set_block_full(x, y, z, 0, rotation, filler)
                        .unwrap_or(false)
                    {
                        report.block_changes.push(BlockChange {
                            x,
                            y,
                            z,
                            block_id: 0,
                            rotation,
                            filler,
                        });
                    }
                }
            }
            return BlockTickStrategy::Sleep;
        }
        if let Some(procedure) = block.tick_procedure {
            return self.run_tick_procedure(world, x, y, z, procedure, report);
        }
        BlockTickStrategy::Sleep
    }

    fn run_tick_procedure(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        procedure: TickProcedure,
        report: &mut SimulationReport,
    ) -> BlockTickStrategy {
        match procedure {
            TickProcedure::BasicChanceBlockGrowth {
                chance_min,
                chance,
                target,
                next_ticking,
            } => {
                let roll = self.random.next_int_bound(chance.max(1));
                if roll >= chance_min {
                    return BlockTickStrategy::Continue;
                }
                let Some(target) = target else {
                    return BlockTickStrategy::Sleep;
                };
                let rotation = world.get_rotation(x, y, z);
                let filler = world.get_filler(x, y, z);
                if world
                    .set_block_full(x, y, z, target, rotation, filler)
                    .unwrap_or(false)
                {
                    report.block_changes.push(BlockChange {
                        x,
                        y,
                        z,
                        block_id: target,
                        rotation,
                        filler,
                    });
                }
                if next_ticking {
                    BlockTickStrategy::Continue
                } else {
                    BlockTickStrategy::Sleep
                }
            }
            TickProcedure::Unsupported(_) => BlockTickStrategy::Sleep,
        }
    }

    fn support_satisfied(
        &self,
        world: &World,
        position: (i32, i32, i32),
        block_id: i32,
        rotation: i32,
        block: &BlockBehavior,
    ) -> bool {
        let (x, y, z) = position;
        let mut required_seen = false;
        for rule in &block.support {
            let face = if rule.rotate {
                rule.face.rotated(rotation)
            } else {
                rule.face
            };
            let (nx, ny, nz) = (
                x + i32::from(face.x),
                y + i32::from(face.y),
                z + i32::from(face.z),
            );
            if !loaded(world, nx, ny, nz) {
                return true;
            }
            let neighbor = NeighborInfo {
                id: world.get_block(nx, ny, nz),
                behavior: self.tables.block(world.get_block(nx, ny, nz)),
                fluid: world.get_fluid_id(nx, ny, nz),
            };
            let satisfied = self.rule_satisfied(block_id, rule, face, neighbor);
            if rule.required {
                required_seen = true;
                if satisfied {
                    return true;
                }
            } else if satisfied {
                return false;
            }
        }
        !required_seen
    }

    fn rule_satisfied(
        &self,
        block_id: i32,
        rule: &SupportRule,
        face: Dir,
        neighbor: NeighborInfo<'_>,
    ) -> bool {
        if let Some(required) = rule.block_type {
            if neighbor.id != required {
                return false;
            }
        }
        if let Some(tag) = &rule.tag {
            let Some(behavior) = neighbor.behavior else {
                return false;
            };
            if !behavior.tags.contains(tag) {
                return false;
            }
        }
        if let Some(fluid) = rule.fluid {
            if neighbor.fluid != fluid {
                return false;
            }
        }
        if let Some(face_type) = &rule.face_type {
            let Some(behavior) = neighbor.behavior else {
                return false;
            };
            if !behavior
                .supporting
                .iter()
                .any(|entry| entry.face == face.opposite() && entry.face_type == *face_type)
            {
                return false;
            }
        }
        match rule.match_self {
            MatchSelf::Ignored => {}
            MatchSelf::Required => {
                if neighbor.id != block_id {
                    return false;
                }
            }
            MatchSelf::Disallowed => {
                if neighbor.id == block_id {
                    return false;
                }
            }
        }
        let _ = rule.self_face_type;
        true
    }

    fn remove_falling(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        rotation: i32,
    ) {
        let filler = world.get_filler(x, y, z);
        if world
            .set_block_full(x, y, z, 0, rotation, filler)
            .unwrap_or(false)
        {
            self.falling.push(FallingBlock {
                block_id,
                rotation,
                x,
                y,
                z,
            });
        }
    }

    fn tick_falling(&mut self, world: &mut World, report: &mut SimulationReport) {
        let mut index = 0;
        while index < self.falling.len() {
            let mut falling = self.falling[index].clone();
            if !loaded(world, falling.x, falling.y - 1, falling.z) {
                index += 1;
                continue;
            }
            let below = world.get_block(falling.x, falling.y - 1, falling.z);
            if below == 0 {
                falling.y -= 1;
                report.falling_moves += 1;
                self.falling[index] = falling;
                index += 1;
                continue;
            }
            if world
                .set_block_full(
                    falling.x,
                    falling.y,
                    falling.z,
                    falling.block_id,
                    falling.rotation,
                    0,
                )
                .unwrap_or(false)
            {
                report.falling_placed += 1;
                report.block_changes.push(BlockChange {
                    x: falling.x,
                    y: falling.y,
                    z: falling.z,
                    block_id: falling.block_id,
                    rotation: falling.rotation,
                    filler: 0,
                });
            }
            self.falling.remove(index);
        }
    }

    fn tick_fluid(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        report: &mut SimulationReport,
    ) -> BlockTickStrategy {
        let Some(fluid) = self.tables.fluid(fluid_id).cloned() else {
            return BlockTickStrategy::Ignored;
        };
        let level = world.get_fluid_level(x, y, z);
        let tick = world.tick();
        let rate = (fluid.flow_rate * 20.0).round().max(1.0) as u64;
        if !tick.is_multiple_of(rate) {
            return BlockTickStrategy::Continue;
        }

        let spread_id = fluid.spread_fluid.unwrap_or(fluid_id);
        let spread = self
            .tables
            .fluid(spread_id)
            .cloned()
            .unwrap_or_else(|| fluid.clone());

        // Java `isAlive`: a flowing fluid with no source above/side loses a level.
        if fluid.can_demote {
            let supported = self.fluid_supported(world, x, y, z, fluid_id, fluid.supported_by);
            if !supported {
                if level <= 1 {
                    world.set_fluid(x, y, z, 0, 0).ok();
                    report.fluid_changes.push(FluidChange {
                        x,
                        y,
                        z,
                        fluid_id: 0,
                        level: 0,
                    });
                } else {
                    let next = level - 1;
                    world.set_fluid(x, y, z, fluid_id, next).ok();
                    report.fluid_changes.push(FluidChange {
                        x,
                        y,
                        z,
                        fluid_id,
                        level: next,
                    });
                }
                return BlockTickStrategy::Sleep;
            }
        }

        if !loaded(world, x, y - 1, z) {
            return BlockTickStrategy::Continue;
        }
        let below_id = world.get_fluid_id(x, y - 1, z);
        let below_block = world.get_block(x, y - 1, z);
        let below_solid = self
            .tables
            .block(below_block)
            .is_some_and(|block| block.fluid_solid);
        if !below_solid && (below_id == 0 || below_id == spread_id || below_id != fluid_id) {
            if let Some(collision) = fluid
                .collisions
                .iter()
                .find(|collision| collision.other == below_id)
            {
                self.apply_fluid_collision(world, x, y - 1, z, collision, report);
            } else if (below_id == 0)
                || (below_id == spread_id && world.get_fluid_level(x, y - 1, z) < spread.max_level)
            {
                world
                    .set_fluid(x, y - 1, z, spread_id, spread.max_level)
                    .ok();
                report.fluid_changes.push(FluidChange {
                    x,
                    y: y - 1,
                    z,
                    fluid_id: spread_id,
                    level: spread.max_level,
                });
            }
            return BlockTickStrategy::Sleep;
        }

        if level == 1 && fluid.max_level != 1 {
            return BlockTickStrategy::Sleep;
        }
        let child_level = if spread_id != fluid_id {
            spread.max_level.saturating_sub(1)
        } else {
            level.saturating_sub(1)
        };
        if child_level <= 0 {
            return BlockTickStrategy::Sleep;
        }
        for direction in [Dir::NORTH, Dir::EAST, Dir::SOUTH, Dir::WEST] {
            let (tx, tz) = (x + i32::from(direction.x), z + i32::from(direction.z));
            if !loaded(world, tx, y, tz) {
                continue;
            }
            let target_block = world.get_block(tx, y, tz);
            if self
                .tables
                .block(target_block)
                .is_some_and(|block| block.fluid_solid)
            {
                continue;
            }
            let target_fluid = world.get_fluid_id(tx, y, tz);
            if let Some(collision) = fluid
                .collisions
                .iter()
                .find(|collision| collision.other == target_fluid)
            {
                self.apply_fluid_collision(world, tx, y, tz, collision, report);
                continue;
            }
            if target_fluid != 0 && target_fluid != spread_id {
                continue;
            }
            if target_fluid == spread_id && world.get_fluid_level(tx, y, tz) >= child_level {
                continue;
            }
            world.set_fluid(tx, y, tz, spread_id, child_level).ok();
            report.fluid_changes.push(FluidChange {
                x: tx,
                y,
                z: tz,
                fluid_id: spread_id,
                level: child_level,
            });
        }
        let _ = FLUID_BLOCK_DISTANCE;
        BlockTickStrategy::Sleep
    }

    fn fluid_supported(
        &self,
        world: &World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        supported_by: Option<i32>,
    ) -> bool {
        let above = world.get_fluid_id(x, y + 1, z);
        if above == fluid_id || supported_by == Some(above) {
            return true;
        }
        if let Some(source) = supported_by {
            for direction in [Dir::NORTH, Dir::EAST, Dir::SOUTH, Dir::WEST] {
                if world.get_fluid_id(x + i32::from(direction.x), y, z + i32::from(direction.z))
                    == source
                {
                    return true;
                }
            }
        }
        false
    }

    fn apply_fluid_collision(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        collision: &FluidCollision,
        report: &mut SimulationReport,
    ) {
        if let Some(block_id) = collision.block_to_place {
            if let Some(block) = self.tables.block(block_id).cloned() {
                if block.material == BlockMaterial::Solid
                    || world.get_block(x, y, z) == 0
                    || self
                        .tables
                        .block(world.get_block(x, y, z))
                        .and_then(|block| block.gathering.as_ref())
                        .is_some_and(BlockGathering::is_soft)
                {
                    let rotation = world.get_rotation(x, y, z);
                    if world
                        .set_block_full(x, y, z, block_id, rotation, 0)
                        .unwrap_or(false)
                    {
                        report.block_changes.push(BlockChange {
                            x,
                            y,
                            z,
                            block_id,
                            rotation,
                            filler: 0,
                        });
                    }
                    world.set_fluid(x, y, z, 0, 0).ok();
                }
            }
        }
        if !collision.place_fluid {
            world.set_fluid(x, y, z, 0, 0).ok();
        }
    }

    fn random_ticks(&mut self, world: &mut World, report: &mut SimulationReport) {
        let chunks = world.loaded_chunk_coordinates();
        for (chunk_x, chunk_z) in chunks {
            for section_index in 0..10 {
                let section_y = section_index * 32;
                let attempts = STABLE_RANDOM_TICKS + UNSTABLE_RANDOM_TICKS;
                for _ in 0..attempts {
                    let index = self.random.next_int_bound(32 * 32 * 32).max(0) as usize;
                    let x = (chunk_x << 5) + (index & 31) as i32;
                    let y = section_y + ((index >> 10) & 31) as i32;
                    let z = (chunk_z << 5) + ((index >> 5) & 31) as i32;
                    let block_id = world.get_block(x, y, z);
                    if block_id == 0 {
                        continue;
                    }
                    let Some(procedure) = self
                        .tables
                        .block(block_id)
                        .and_then(|block| block.random_tick.clone())
                    else {
                        continue;
                    };
                    report.random_ticks += 1;
                    self.run_random_tick(world, (x, y, z), block_id, procedure, report);
                }
            }
        }
    }

    fn run_random_tick(
        &mut self,
        world: &mut World,
        position: (i32, i32, i32),
        block_id: i32,
        procedure: RandomTickProcedure,
        report: &mut SimulationReport,
    ) {
        let (x, y, z) = position;
        match procedure {
            RandomTickProcedure::ChangeIntoBlock { target } => {
                if let Some(target) = target {
                    let rotation = world.get_rotation(x, y, z);
                    if world
                        .set_block_full(x, y, z, target, rotation, 0)
                        .unwrap_or(false)
                    {
                        report.block_changes.push(BlockChange {
                            x,
                            y,
                            z,
                            block_id: target,
                            rotation,
                            filler: 0,
                        });
                    }
                }
            }
            RandomTickProcedure::SpreadTo {
                directions,
                min_y,
                max_y,
                allowed_tag,
                require_empty_above,
                allowed_above_fluids,
                revert_block,
            } => {
                if let Some(revert) = revert_block {
                    let above = world.get_block(x, y + 1, z);
                    if self.tables.block(above).is_some_and(|block| {
                        block.material == BlockMaterial::Solid
                            && block.opacity == Opacity::Solid
                            && matches!(block.draw_type, DrawType::Cube | DrawType::CubeWithModel)
                    }) {
                        let rotation = world.get_rotation(x, y, z);
                        if world
                            .set_block_full(x, y, z, revert, rotation, 0)
                            .unwrap_or(false)
                        {
                            report.block_changes.push(BlockChange {
                                x,
                                y,
                                z,
                                block_id: revert,
                                rotation,
                                filler: 0,
                            });
                        }
                        return;
                    }
                }
                for dy in min_y..=max_y {
                    for direction in &directions {
                        let target_x = x + i32::from(direction.x);
                        let target_y = y + i32::from(direction.y) + dy;
                        let target_z = z + i32::from(direction.z);
                        let target_id = world.get_block(target_x, target_y, target_z);
                        let Some(target) = self.tables.block(target_id) else {
                            continue;
                        };
                        if !target.tags.contains(&allowed_tag) {
                            continue;
                        }
                        if require_empty_above {
                            let above = world.get_block(target_x, target_y + 1, target_z);
                            if self
                                .tables
                                .block(above)
                                .is_none_or(|block| block.material != BlockMaterial::Empty)
                            {
                                continue;
                            }
                        }
                        if let Some(allowed) = &allowed_above_fluids {
                            let above = world.get_fluid_id(target_x, target_y + 1, target_z);
                            if !allowed.contains(&above) {
                                continue;
                            }
                        }
                        if world
                            .set_block_full(target_x, target_y, target_z, block_id, 0, 0)
                            .unwrap_or(false)
                        {
                            report.block_changes.push(BlockChange {
                                x: target_x,
                                y: target_y,
                                z: target_z,
                                block_id,
                                rotation: 0,
                                filler: 0,
                            });
                        }
                    }
                }
            }
            RandomTickProcedure::PlaceBlock {
                offset,
                placements,
                require_empty_target,
            } => {
                let target_x = x + i32::from(offset.x);
                let target_y = y + i32::from(offset.y);
                let target_z = z + i32::from(offset.z);
                let target_id = world.get_block(target_x, target_y, target_z);
                if require_empty_target
                    && self
                        .tables
                        .block(target_id)
                        .is_some_and(|block| block.material != BlockMaterial::Empty)
                {
                    return;
                }
                if let Some(block_id) = placements.first() {
                    if world
                        .set_block_full(target_x, target_y, target_z, *block_id, 0, 0)
                        .unwrap_or(false)
                    {
                        report.block_changes.push(BlockChange {
                            x: target_x,
                            y: target_y,
                            z: target_z,
                            block_id: *block_id,
                            rotation: 0,
                            filler: 0,
                        });
                    }
                }
            }
            RandomTickProcedure::Unsupported(_) => {}
        }
    }
}

fn index(x: i32, y: i32, z: i32) -> usize {
    (((y & 31) << 10) | ((z & 31) << 5) | (x & 31)) as usize
}

fn loaded(world: &World, x: i32, y: i32, z: i32) -> bool {
    (0..320).contains(&y) && world.chunk(x >> 5, z >> 5).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::column::ChunkColumn;
    use crate::world::WorldStorage;

    fn cube() -> BlockBehavior {
        BlockBehavior {
            material: BlockMaterial::Solid,
            draw_type: DrawType::Cube,
            opacity: Opacity::Solid,
            fluid_solid: true,
            support: Vec::new(),
            support_drop: SupportDropType::Break,
            max_support_distance: 0,
            falling: false,
            gathering: None,
            random_tick: None,
            tick_procedure: None,
            tags: BTreeSet::new(),
            supporting: [
                Dir::UP,
                Dir::DOWN,
                Dir::NORTH,
                Dir::SOUTH,
                Dir::EAST,
                Dir::WEST,
            ]
            .into_iter()
            .map(|face| SupportingFace {
                face,
                face_type: "Full".to_string(),
            })
            .collect(),
        }
    }

    fn plant(support_drop: SupportDropType) -> BlockBehavior {
        BlockBehavior {
            material: BlockMaterial::Empty,
            draw_type: DrawType::Model,
            opacity: Opacity::Transparent,
            fluid_solid: false,
            support: vec![SupportRule {
                face: Dir::DOWN,
                rotate: true,
                required: true,
                block_type: None,
                tag: None,
                fluid: None,
                face_type: Some("Full".to_string()),
                self_face_type: None,
                match_self: MatchSelf::Ignored,
            }],
            support_drop,
            max_support_distance: 0,
            falling: support_drop == SupportDropType::Fall,
            gathering: Some(BlockGathering {
                breaking: Some(BlockDrop {
                    item_id: Some("Plant_Drop".to_string()),
                    drop_list_id: None,
                    quantity: 1,
                }),
                ..BlockGathering::default()
            }),
            random_tick: None,
            tick_procedure: None,
            tags: BTreeSet::new(),
            supporting: Vec::new(),
        }
    }

    fn world_with_cube() -> World {
        let mut world = World::new(WorldStorage::new("/tmp/hytale-simulation-test"));
        world.insert_chunk(ChunkColumn::new(0, 0));
        world
    }

    fn tables(block: BlockBehavior, support_drop: SupportDropType) -> SimulationTables {
        SimulationTables::from_parts(
            vec![
                None,
                Some(block),
                Some(cube()),
                Some(plant(support_drop)),
                None,
                None,
            ],
            Vec::new(),
        )
    }

    #[test]
    fn support_breaks_and_drops_when_the_floor_is_removed() {
        let mut world = world_with_cube();
        world.set_block(0, 0, 0, 2).unwrap();
        world.set_block(0, 1, 0, 1).unwrap();
        let mut simulation = BlockSimulation::new(
            tables(plant(SupportDropType::Break), SupportDropType::Break),
            1,
        );
        simulation.tick(&mut world, 0.05);
        assert_eq!(world.get_block(0, 1, 0), 1, "supported plant stays");

        world.set_block(0, 0, 0, 0).unwrap();
        let report = simulation.tick(&mut world, 0.05);
        assert_eq!(world.get_block(0, 1, 0), 0, "plant is removed");
        assert!(
            report
                .drops
                .iter()
                .any(|(_, _, _, drop)| drop.item_id.as_deref() == Some("Plant_Drop")),
            "{report:?}"
        );
    }

    #[test]
    fn a_falling_block_moves_one_cell_per_tick_and_lands() {
        let mut world = world_with_cube();
        world.set_block(0, 0, 0, 2).unwrap();
        world.set_block(0, 3, 0, 3).unwrap();
        let mut simulation = BlockSimulation::new(
            tables(plant(SupportDropType::Fall), SupportDropType::Fall),
            2,
        );
        simulation.tick(&mut world, 0.05);
        assert_eq!(world.get_block(0, 3, 0), 0);
        assert_eq!(simulation.falling_count(), 1);

        for _ in 0..4 {
            simulation.tick(&mut world, 0.05);
        }
        assert_eq!(simulation.falling_count(), 0);
        assert_eq!(world.get_block(0, 1, 0), 3, "lands on the floor");
    }

    #[test]
    fn water_source_spreads_one_level_down_and_out() {
        let fluid_source = FluidBehavior {
            max_level: 1,
            flow_rate: 0.05,
            can_demote: false,
            supported_by: None,
            spread_fluid: Some(2),
            collisions: Vec::new(),
        };
        let fluid_flow = FluidBehavior {
            max_level: 8,
            flow_rate: 0.05,
            can_demote: true,
            supported_by: Some(1),
            spread_fluid: Some(2),
            collisions: Vec::new(),
        };
        let tables = SimulationTables::from_parts(
            vec![None, Some(cube()), Some(cube()), Some(cube()), Some(cube())],
            vec![None, Some(fluid_source), Some(fluid_flow)],
        );
        let mut world = world_with_cube();
        world.set_block(0, 0, 0, 1).unwrap();
        world.set_fluid(0, 1, 0, 1, 1).unwrap();
        let mut simulation = BlockSimulation::new(tables, 3);
        let report = simulation.tick(&mut world, 0.05);
        assert_eq!(world.get_fluid_id(0, 1, 0), 1);
        assert_eq!(world.get_fluid_id(1, 1, 0), 2, "{report:?}");
        assert_eq!(world.get_fluid_level(1, 1, 0), 7, "{report:?}");
    }

    #[test]
    fn damage_regenerates_after_the_java_delay() {
        let mut world = world_with_cube();
        world.set_block(0, 1, 0, 2).unwrap();
        let remaining = world.damage_block(0, 1, 0, 0.25);
        assert!((remaining - 0.75).abs() < 1e-6);
        assert!((world.block_health(0, 1, 0) - 0.75).abs() < 1e-6);

        let mut simulation = BlockSimulation::new(
            SimulationTables::from_parts(vec![None, Some(cube())], Vec::new()),
            4,
        );
        for _ in 0..100 {
            simulation.tick(&mut world, 0.05);
        }
        assert!(
            world.block_health(0, 1, 0) > 0.75,
            "health must regenerate after five seconds"
        );
        world.damage_block(0, 1, 0, 1.0);
        assert!((world.block_health(0, 1, 0) - 1.0).abs() < 1e-6);
    }
}
