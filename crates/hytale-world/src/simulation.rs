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

use hytale_assets::{AssetRegistry, TagPatternNode};
use hytale_math::fast_random::FastRandom;
use hytale_protocol::generated::common::{BlockMaterial, DrawType, Opacity};
use serde_json::Value;

use crate::chunk::section::BlockTickStrategy;
use crate::collision::{filler_offset, Aabb, BlockShape};
use crate::world::World;

const DEFAULT_FLOW_RATE: f32 = 0.5;
const DEFAULT_MAX_FLUID_LEVEL: i8 = 8;
const FLUID_BLOCK_DISTANCE: i32 = 5;
const SPREAD_NO_PATH: i32 = i32::MAX;
const SPREAD_NO_CHUNK: i32 = SPREAD_NO_PATH - 1;
const SPREAD_NO_CHUNK_BITS: u32 = u32::MAX;
const ORTHO_OFFSETS: [Dir; 4] = [Dir::WEST, Dir::EAST, Dir::NORTH, Dir::SOUTH];
const DIAG_OFFSETS: [Dir; 4] = [
    Dir::new(-1, 0, -1),
    Dir::new(1, 0, 1),
    Dir::new(1, 0, -1),
    Dir::new(-1, 0, 1),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AliveStatus {
    Alive,
    Demote,
    WaitForChunk,
}

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
    pub support: SupportMatch,
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
    /// `AllowSupportPropagation` (default true).
    pub allow_support_propagation: bool,
    /// Filler cells this rule applies to (`Filler`), when configured.
    pub filler: Option<Vec<Dir>>,
}

/// Java `BlockSupportsRequiredForType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportsRequiredFor {
    /// Any filler cell/face may satisfy the support.
    Any,
    /// Every filler cell/face must satisfy the support.
    All,
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
    /// Filler cells this supporting face applies to, when configured.
    pub filler: Option<Vec<Dir>>,
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
        /// Minimum combined sky/block light (`RequiredLightLevel`, Java default 6).
        required_light_level: i32,
    },
    /// Place a block at an offset, optionally weighted and state-scoped.
    PlaceBlock {
        /// Offset from the ticking block.
        offset: Dir,
        /// Placement rules (`Placements`).
        rules: Vec<PlacementRule>,
        /// Whether the target must be empty.
        require_empty_target: bool,
    },
    /// A procedure this core does not yet execute.
    Unsupported(String),
}

/// One weighted placement candidate (`PlaceBlockProcedure.WeightedBlock`).
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedBlock {
    /// Block id.
    pub block: i32,
    /// Relative selection weight (Java default `1.0`).
    pub weight: f64,
}

/// One `PlaceBlockProcedure.PlacementRule`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementRule {
    /// Optional `State` scope (`None` = the stateless form).
    pub state: Option<String>,
    /// Candidate blocks.
    pub blocks: Vec<WeightedBlock>,
}

/// One scheduled tick procedure (`TickProcedure.CODEC`).
#[derive(Debug, Clone, PartialEq)]
pub enum TickProcedure {
    /// `BasicChanceBlockGrowth` / `SplitChanceBlockGrowth`.
    ChanceBlockGrowth {
        /// Minimum roll (`chanceMin`).
        chance_min: i32,
        /// Roll bound (`Chance`/`Data`).
        chance: i32,
        /// Weighted target blocks.
        targets: Vec<WeightedBlock>,
        /// `NextTicking`.
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
    /// Sound event asset name (presentation; the simulation reports it but does
    /// not play it yet).
    pub sound_event: Option<String>,
}

/// Java fluid ticker variant (`FluidTicker.CODEC` registered types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FluidTickerKind {
    /// `DefaultFluidTicker`.
    Default,
    /// `FiniteFluidTicker`.
    Finite,
    /// `FireFluidTicker`.
    Fire,
}

/// One `FireFluidTicker.FlammabilityConfig`.
#[derive(Debug, Clone, PartialEq)]
pub struct Flammability {
    /// Inline tag pattern that selects matching blocks.
    pub pattern: Option<TagPatternNode>,
    /// Higher priorities are checked first.
    pub priority: i32,
    /// Minimum fluid level required to burn.
    pub burn_level: i8,
    /// Per-tick burn probability.
    pub burn_chance: f32,
    /// Block placed after burning.
    pub resulting_block: Option<i32>,
    /// State variant applied after burning (not yet executed by the native core).
    pub resulting_state: Option<String>,
    /// Sound event asset name (presentation).
    pub sound_event: Option<String>,
}

impl Flammability {
    /// Java `TagPattern.test(tags)` over the Rust string tag set.
    pub fn matches(&self, tags: &BTreeSet<String>) -> bool {
        self.pattern
            .as_ref()
            .is_some_and(|pattern| tag_pattern_matches(pattern, tags))
    }
}

fn tag_pattern_matches(pattern: &TagPatternNode, tags: &BTreeSet<String>) -> bool {
    match pattern {
        TagPatternNode::Equals { tag } => tags.contains(tag),
        TagPatternNode::And { operands } => operands
            .iter()
            .all(|operand| tag_pattern_matches(operand, tags)),
        TagPatternNode::Or { operands } => operands
            .iter()
            .any(|operand| tag_pattern_matches(operand, tags)),
        TagPatternNode::Not { pattern } => !tag_pattern_matches(pattern, tags),
    }
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
    /// Ticker implementation.
    pub kind: FluidTickerKind,
    /// Flattened asset tags (`Fluid`, `Water`, `Fluid=Water`).
    pub tags: BTreeSet<String>,
    /// `FireFluidTicker` flammability rules.
    pub flammability: Vec<Flammability>,
}

impl FluidBehavior {
    /// Java `DefaultFluidTicker.getSpreadFluidId`.
    pub fn spread_fluid_id(&self, fluid_id: i32) -> i32 {
        self.spread_fluid.unwrap_or(fluid_id)
    }

    /// Java `FluidTicker.isSelfFluid` plus the spread-fluid override.
    pub fn is_self_fluid(&self, self_fluid_id: i32, other_fluid_id: i32) -> bool {
        other_fluid_id == self_fluid_id
            || self.supported_by == Some(other_fluid_id)
            || other_fluid_id == self.spread_fluid_id(self_fluid_id)
    }
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
    /// Java `SupportsRequiredFor` (default `All`).
    pub supports_required_for: SupportsRequiredFor,
}

/// Asset-derived block/fluid behavior tables.
#[derive(Debug, Clone, Default)]
pub struct SimulationTables {
    blocks: Vec<Option<BlockBehavior>>,
    fluids: Vec<Option<FluidBehavior>>,
    block_shapes: Vec<Option<BlockShape>>,
    block_protrudes: Vec<bool>,
}

impl SimulationTables {
    /// Build tables directly (test/embedding seam).
    pub fn from_parts(
        blocks: Vec<Option<BlockBehavior>>,
        fluids: Vec<Option<FluidBehavior>>,
    ) -> Self {
        Self {
            blocks,
            fluids,
            block_shapes: Vec::new(),
            block_protrudes: Vec::new(),
        }
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
        let mut block_shapes = vec![None; block_highest + 1];
        let mut block_protrudes = vec![false; block_highest + 1];
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
            if id < 0 {
                continue;
            }
            let index = id as usize;
            let hitbox_name = record
                .value
                .json
                .get("HitboxType")
                .and_then(Value::as_str)
                .unwrap_or("Full");
            let shape = assets
                .block_bounding_boxes
                .get(hitbox_name)
                .and_then(|record| BlockShape::from_hitbox(&record.value))
                .unwrap_or_else(BlockShape::unit);
            block_protrudes[index] = shape.protrudes_unit_box();
            block_shapes[index] = Some(shape);
            blocks[index] = Some(behavior);
        }
        for record in assets.fluids.iter() {
            let Some(id) = record.id else {
                continue;
            };
            let ticker = effective_ticker(assets, &record.key);
            let behavior = parse_fluid(
                &record.value.json,
                &ticker,
                record.value.max_fluid_level,
                &fluid_ids,
                &block_ids,
            );
            if id >= 0 {
                fluids[id as usize] = Some(behavior);
            }
        }
        Self {
            blocks,
            fluids,
            block_shapes,
            block_protrudes,
        }
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

    /// One block's hitbox shape.
    pub fn block_shape(&self, id: i32) -> Option<&BlockShape> {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.block_shapes.get(id))
            .and_then(Option::as_ref)
    }

    /// Whether a block's hitbox protrudes its unit cell.
    pub fn block_protrudes(&self, id: i32) -> bool {
        usize::try_from(id)
            .ok()
            .and_then(|id| self.block_protrudes.get(id))
            .copied()
            .unwrap_or(false)
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
        supports_required_for: match json.get("SupportsRequiredFor").and_then(Value::as_str) {
            Some("Any") => SupportsRequiredFor::Any,
            _ => SupportsRequiredFor::All,
        },
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
                support: SupportMatch::Required,
                block_type: None,
                tag: None,
                fluid: None,
                face_type: Some("Full".to_string()),
                self_face_type: None,
                match_self: MatchSelf::Ignored,
                allow_support_propagation: true,
                filler: None,
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
            rules.push(SupportRule {
                face,
                rotate: entry.get("Rotate").and_then(Value::as_bool).unwrap_or(true),
                support: match entry.get("Support").and_then(Value::as_str) {
                    Some("Disallowed") => SupportMatch::Disallowed,
                    Some("Ignored") => SupportMatch::Ignored,
                    _ => SupportMatch::Required,
                },
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
                allow_support_propagation: entry
                    .get("AllowSupportPropagation")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                filler: parse_filler_offsets(entry.get("Filler")),
            });
        }
    }
    rules
}

fn parse_filler_offsets(value: Option<&Value>) -> Option<Vec<Dir>> {
    value.and_then(Value::as_array).map(|entries| {
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
                    filler: parse_filler_offsets(entry.get("Filler")),
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
            filler: None,
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
            required_light_level: map
                .get("RequiredLightLevel")
                .and_then(Value::as_i64)
                .unwrap_or(6) as i32,
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
                rules: map
                    .get("Placements")
                    .and_then(Value::as_array)
                    .map(|placements| {
                        placements
                            .iter()
                            .filter_map(|placement| {
                                let mut blocks = Vec::new();
                                if let Some(block) = placement
                                    .get("Block")
                                    .and_then(Value::as_str)
                                    .and_then(|name| block_ids.get(name).copied())
                                {
                                    blocks.push(WeightedBlock { block, weight: 1.0 });
                                }
                                if let Some(entries) =
                                    placement.get("Blocks").and_then(Value::as_array)
                                {
                                    for entry in entries {
                                        let Some(block) = entry
                                            .get("Block")
                                            .and_then(Value::as_str)
                                            .and_then(|name| block_ids.get(name).copied())
                                        else {
                                            continue;
                                        };
                                        blocks.push(WeightedBlock {
                                            block,
                                            weight: entry
                                                .get("Weight")
                                                .and_then(Value::as_f64)
                                                .unwrap_or(1.0),
                                        });
                                    }
                                }
                                if blocks.is_empty() {
                                    return None;
                                }
                                Some(PlacementRule {
                                    state: placement
                                        .get("State")
                                        .and_then(Value::as_str)
                                        .map(str::to_string),
                                    blocks,
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
        "BasicChance" | "BasicChanceBlockGrowth" => TickProcedure::ChanceBlockGrowth {
            chance_min: map.get("ChanceMin").and_then(Value::as_i64).unwrap_or(0) as i32,
            chance: map.get("Chance").and_then(Value::as_i64).unwrap_or(1) as i32,
            targets: map
                .get("NextId")
                .and_then(Value::as_str)
                .and_then(|name| block_ids.get(name).copied())
                .map(|block| vec![WeightedBlock { block, weight: 1.0 }])
                .unwrap_or_default(),
            next_ticking: map
                .get("NextTicking")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "SplitChance" | "SplitChanceBlockGrowth" => {
            let targets = map
                .get("NextIds")
                .and_then(Value::as_object)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|(name, chance)| {
                            Some(WeightedBlock {
                                block: block_ids.get(name).copied()?,
                                weight: chance.as_f64().unwrap_or(0.0),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            TickProcedure::ChanceBlockGrowth {
                chance_min: map.get("ChanceMin").and_then(Value::as_i64).unwrap_or(0) as i32,
                chance: map.get("Data").and_then(Value::as_i64).unwrap_or(1) as i32,
                targets,
                next_ticking: map
                    .get("NextTicking")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }
        }
        other => TickProcedure::Unsupported(other.to_string()),
    })
}

fn parse_tag_pattern(value: &Value) -> Option<TagPatternNode> {
    let kind = value.get("Op").and_then(Value::as_str)?;
    Some(match kind {
        "Equals" => TagPatternNode::Equals {
            tag: value.get("Tag").and_then(Value::as_str)?.to_string(),
        },
        "And" | "Or" => {
            let operands = value
                .get("Patterns")
                .and_then(Value::as_array)
                .map(|entries| entries.iter().filter_map(parse_tag_pattern).collect())
                .unwrap_or_default();
            if kind == "And" {
                TagPatternNode::And { operands }
            } else {
                TagPatternNode::Or { operands }
            }
        }
        "Not" => TagPatternNode::Not {
            pattern: Box::new(parse_tag_pattern(value.get("Pattern")?)?),
        },
        _ => return None,
    })
}

/// Merge the `Ticker` documents along a fluid's `Parent` chain, root first.
///
/// Java decodes each `FluidTicker` subtype with inherited base fields, so a
/// child that only overrides `CanDemote`/`SupportedBy` keeps the parent's
/// `FlowRate`/`SpreadFluid`. The merged asset document does not preserve that
/// per-codec inheritance, so the ticker chain is rebuilt here.
fn effective_ticker(assets: &AssetRegistry, key: &str) -> Value {
    let mut chain = Vec::new();
    let mut current = Some(key.to_string());
    let mut seen = BTreeSet::new();
    while let Some(name) = current {
        if !seen.insert(name.clone()) {
            break;
        }
        let Some(record) = assets.fluids.get(&name) else {
            break;
        };
        chain.push(
            record
                .value
                .json
                .get("Ticker")
                .cloned()
                .unwrap_or(Value::Null),
        );
        current = record.parent.clone();
    }
    let mut merged = serde_json::Map::new();
    for ticker in chain.into_iter().rev() {
        if let Value::Object(map) = ticker {
            for (name, value) in map {
                merged.insert(name, value);
            }
        }
    }
    Value::Object(merged)
}

fn parse_fluid(
    json: &Value,
    ticker_json: &Value,
    max_level: i8,
    fluid_ids: &BTreeMap<String, i32>,
    block_ids: &BTreeMap<String, i32>,
) -> FluidBehavior {
    let ticker = ticker_json.as_object().filter(|ticker| !ticker.is_empty());
    let kind = match ticker
        .and_then(|ticker| ticker.get("Type"))
        .and_then(Value::as_str)
    {
        Some("Finite") => FluidTickerKind::Finite,
        Some("Fire") => FluidTickerKind::Fire,
        _ => FluidTickerKind::Default,
    };
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
    let spread_fluid = if matches!(kind, FluidTickerKind::Default | FluidTickerKind::Fire) {
        ticker
            .and_then(|ticker| ticker.get("SpreadFluid"))
            .and_then(Value::as_str)
            .and_then(|name| fluid_ids.get(name).copied())
    } else {
        None
    };
    let collisions = if kind == FluidTickerKind::Default {
        ticker
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
                            sound_event: entry
                                .get("SoundEvent")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let flammability = if kind == FluidTickerKind::Fire {
        ticker
            .and_then(|ticker| ticker.get("Flammability"))
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        let pattern = entry.get("TagPattern").and_then(parse_tag_pattern)?;
                        Some(Flammability {
                            pattern: Some(pattern),
                            priority: entry.get("Priority").and_then(Value::as_i64).unwrap_or(0)
                                as i32,
                            burn_level: entry.get("BurnLevel").and_then(Value::as_i64).unwrap_or(1)
                                as i8,
                            burn_chance: entry
                                .get("BurnChance")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.1) as f32,
                            resulting_block: entry
                                .get("ResultingBlock")
                                .and_then(Value::as_str)
                                .and_then(|name| block_ids.get(name).copied()),
                            resulting_state: entry
                                .get("ResultingState")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            sound_event: entry
                                .get("SoundEvent")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut tags = BTreeSet::new();
    collect_tags(json, &mut tags);
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
        kind,
        tags,
        flammability,
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

/// One pending block-health replication delta (T57 packet seam).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockHealthDelta {
    /// X.
    pub x: i32,
    /// Y.
    pub y: i32,
    /// Z.
    pub z: i32,
    /// Change in remaining health (negative = damage, positive = regeneration).
    pub delta: f32,
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
    /// Pending block-health/fragility deltas (consumed by T57 packets).
    pub health_deltas: Vec<BlockHealthDelta>,
}

#[derive(Debug, Clone)]
struct FallingBlock {
    block_id: i32,
    rotation: i32,
    x: i32,
    y: i32,
    z: i32,
}

/// One deterministic block-simulation pass.
#[derive(Debug, Clone)]
pub struct BlockSimulation {
    tables: SimulationTables,
    random: FastRandom,
    falling: Vec<FallingBlock>,
    disabled_fluids: BTreeSet<i32>,
    sunlight_factor: f32,
    suppressed_random: BTreeSet<(i32, i32, i32)>,
    suppressed_scheduled: BTreeSet<(i32, i32, i32)>,
}

impl BlockSimulation {
    /// Create a simulation over the given asset-derived tables.
    pub fn new(tables: SimulationTables, seed: i64) -> Self {
        Self {
            tables,
            random: FastRandom::new(seed),
            falling: Vec::new(),
            disabled_fluids: BTreeSet::new(),
            sunlight_factor: 1.0,
            suppressed_random: BTreeSet::new(),
            suppressed_scheduled: BTreeSet::new(),
        }
    }

    /// Update the world clock's sunlight factor for `SpreadTo` light gates.
    pub fn set_sunlight_factor(&mut self, factor: f32) {
        self.sunlight_factor = factor.clamp(0.0, 1.0);
    }

    /// Replace the disabled fluid set from a world config.
    ///
    /// Java `FluidPlugin.resolveFluidIds` resolves the config tags against the
    /// fluid asset tag map; both raw tags (`Fluid`) and composites
    /// (`Fluid=Water`) are accepted.
    pub fn set_disabled_fluid_tickers(&mut self, tags: &BTreeSet<String>) {
        self.disabled_fluids.clear();
        if tags.is_empty() {
            return;
        }
        for (id, fluid) in self.tables.fluids.iter().enumerate() {
            let Some(fluid) = fluid else {
                continue;
            };
            if tags.iter().any(|tag| fluid.tags.contains(tag)) {
                self.disabled_fluids.insert(id as i32);
            }
        }
    }

    /// Numeric ids currently excluded from fluid ticking.
    pub fn disabled_fluid_ids(&self) -> &BTreeSet<i32> {
        &self.disabled_fluids
    }

    /// Java `TickSuppressionRegistry` seam: suppress one random tick position.
    pub fn suppress_random_tick(&mut self, x: i32, y: i32, z: i32) {
        self.suppressed_random.insert((x, y, z));
    }

    /// Java `TickSuppressionRegistry` seam: suppress one scheduled tick.
    pub fn suppress_scheduled_tick(&mut self, x: i32, y: i32, z: i32) {
        self.suppressed_scheduled.insert((x, y, z));
    }

    /// Clear every suppressed tick position.
    pub fn clear_tick_suppression(&mut self) {
        self.suppressed_random.clear();
        self.suppressed_scheduled.clear();
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
        if self.suppressed_random.contains(&(x, y, z)) {
            return report;
        }
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
        for &(x, y, z) in &changed {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        self.schedule(world, x + dx, y + dy, z + dz, 0);
                    }
                }
            }
        }

        let mut report = SimulationReport::default();
        // A full solid cube cannot hold fluid (`FluidTicker.isFullySolid`).
        // Partial blocks keep their waterlogged fluid. `FireFluidTicker`
        // overrides `canOccupySolidBlocks`, so fire is not cleared here.
        for (x, y, z) in changed {
            let fluid_id = world.get_fluid_id(x, y, z);
            if fluid_id != 0
                && self
                    .tables
                    .fluid(fluid_id)
                    .is_none_or(|fluid| fluid.kind != FluidTickerKind::Fire)
                && self.is_fully_solid(world.get_block(x, y, z))
                && world.set_fluid(x, y, z, 0, 0).unwrap_or(false)
            {
                report.fluid_changes.push(FluidChange {
                    x,
                    y,
                    z,
                    fluid_id: 0,
                    level: 0,
                });
            }
        }
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
                let strategy = if self.suppressed_scheduled.contains(&(x, y, z)) {
                    BlockTickStrategy::Ignored
                } else {
                    self.tick_block(world, x, y, z, &mut report)
                };
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
        report.health_deltas = world
            .take_block_health_deltas()
            .into_iter()
            .map(|(x, y, z, delta)| BlockHealthDelta { x, y, z, delta })
            .collect();
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
        let filler = world.get_filler(x, y, z);
        if filler != 0 {
            // Java `applyBlockPhysics` skips filler cells; the base block owns
            // the multi-cell support test.
            return BlockTickStrategy::Sleep;
        }
        if !block.support.is_empty() {
            let rotation = world.get_rotation(x, y, z);
            match self.test_block_physics(world, x, y, z, block_id, rotation, filler, &block) {
                SupportResult::Waiting => return BlockTickStrategy::WaitForAdjacentChunkLoad,
                SupportResult::Ignored | SupportResult::Satisfies => {
                    if world.support_value(x, y, z) != 0 {
                        world.set_support_value(x, y, z, 0);
                    }
                }
                SupportResult::Distance(distance) => {
                    if world.support_value(x, y, z) != distance {
                        world.set_support_value(x, y, z, distance);
                    }
                }
                SupportResult::DoesntSatisfy => {
                    report.support_changes += 1;
                    match block.support_drop {
                        SupportDropType::Fall => {
                            self.remove_falling(world, x, y, z, block_id, rotation);
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
            }
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
            TickProcedure::ChanceBlockGrowth {
                chance_min,
                chance,
                targets,
                next_ticking,
            } => {
                let roll = self.random.next_int_bound(chance.max(1));
                if roll >= chance_min {
                    return BlockTickStrategy::Continue;
                }
                let Some(target) = self.pick_weighted(&targets) else {
                    return BlockTickStrategy::Sleep;
                };
                if world.set_block_full(x, y, z, target, 0, 0).unwrap_or(false) {
                    report.block_changes.push(BlockChange {
                        x,
                        y,
                        z,
                        block_id: target,
                        rotation: 0,
                        filler: 0,
                    });
                }
                // Java `executeToBlock` returns `!nextTicking`; true means the
                // block keeps ticking, false sleeps until it is woken.
                if next_ticking {
                    BlockTickStrategy::Sleep
                } else {
                    BlockTickStrategy::Continue
                }
            }
            TickProcedure::Unsupported(_) => BlockTickStrategy::Sleep,
        }
    }

    fn light_level(&self, world: &World, x: i32, y: i32, z: i32) -> i32 {
        let Some(column) = world.chunk(x >> 5, z >> 5) else {
            return 0;
        };
        let Some(section) = column.section(y) else {
            return 0;
        };
        let packed = section.block.local_light().get(index(x, y, z));
        let sky = i32::from((packed >> 12) & 0xF);
        let block = i32::from(packed & 0xF)
            .max(i32::from((packed >> 4) & 0xF))
            .max(i32::from((packed >> 8) & 0xF));
        ((sky as f32 * self.sunlight_factor) as i32).max(block)
    }

    fn pick_weighted(&mut self, targets: &[WeightedBlock]) -> Option<i32> {
        if targets.is_empty() {
            return None;
        }
        let total: f64 = targets.iter().map(|target| target.weight.max(0.0)).sum();
        if total <= 0.0 {
            return targets.first().map(|target| target.block);
        }
        let mut pointer = self.random.next_double() * total;
        for target in targets {
            pointer -= target.weight.max(0.0);
            if pointer <= 0.0 {
                return Some(target.block);
            }
        }
        targets.last().map(|target| target.block)
    }

    #[allow(clippy::too_many_arguments)]
    fn test_block_physics(
        &self,
        world: &World,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        rotation: i32,
        filler: i32,
        block: &BlockBehavior,
    ) -> SupportResult {
        let (shape, protrudes) = match self.tables.block_shape(block_id) {
            Some(shape) if self.tables.block_protrudes(block_id) => (Some(shape), true),
            shape => (shape, false),
        };
        if protrudes {
            let Some(shape) = shape else {
                return self.test_block_physics_at_position(
                    world,
                    x,
                    y,
                    z,
                    block_id,
                    rotation,
                    block,
                    filler_offset(filler),
                    None,
                );
            };
            let Some(bounding) = shape.rotated_bounding_box(rotation) else {
                return SupportResult::Ignored;
            };
            let min_x = bounding.min.x.floor() as i32;
            let min_y = bounding.min.y.floor() as i32;
            let min_z = bounding.min.z.floor() as i32;
            let max_x = bounding.max.x.ceil() as i32;
            let max_y = bounding.max.y.ceil() as i32;
            let max_z = bounding.max.z.ceil() as i32;
            let width = (max_x - min_x).max(1);
            let height = (max_y - min_y).max(1);
            let depth = (max_z - min_z).max(1);
            let mut support = SUPPORT_IGNORE;
            'top: for dx in 0..width {
                for dy in 0..height {
                    for dz in 0..depth {
                        let filler_x = x + min_x + dx;
                        let filler_y = y + min_y + dy;
                        let filler_z = z + min_z + dz;
                        if !loaded(world, filler_x, filler_y, filler_z) {
                            return SupportResult::Waiting;
                        }
                        let filler_rotation = world.get_rotation(filler_x, filler_y, filler_z);
                        let result = self.test_block_physics_at_position(
                            world,
                            filler_x,
                            filler_y,
                            filler_z,
                            block_id,
                            if filler_rotation == 0 {
                                rotation
                            } else {
                                filler_rotation
                            },
                            block,
                            (min_x + dx, min_y + dy, min_z + dz),
                            Some((shape, rotation)),
                        );
                        match block.supports_required_for {
                            SupportsRequiredFor::Any => match result {
                                SupportResult::Satisfies => {
                                    support = SUPPORT_SATISFIES;
                                    break 'top;
                                }
                                SupportResult::DoesntSatisfy => support = SUPPORT_DOESNT,
                                SupportResult::Distance(distance) if distance > support => {
                                    support = distance;
                                }
                                _ => {}
                            },
                            SupportsRequiredFor::All => match result {
                                SupportResult::DoesntSatisfy => {
                                    support = SUPPORT_DOESNT;
                                    break 'top;
                                }
                                SupportResult::Satisfies => support = SUPPORT_SATISFIES,
                                SupportResult::Distance(distance) if support == SUPPORT_IGNORE => {
                                    support = distance;
                                }
                                _ => {}
                            },
                        }
                    }
                }
            }
            return match support {
                SUPPORT_SATISFIES => SupportResult::Satisfies,
                SUPPORT_DOESNT => SupportResult::DoesntSatisfy,
                SUPPORT_IGNORE => SupportResult::Ignored,
                distance => SupportResult::Distance(distance),
            };
        }
        self.test_block_physics_at_position(
            world,
            x,
            y,
            z,
            block_id,
            rotation,
            block,
            filler_offset(filler),
            shape.map(|s| (s, rotation)),
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn test_block_physics_at_position(
        &self,
        world: &World,
        block_x: i32,
        block_y: i32,
        block_z: i32,
        block_id: i32,
        rotation: i32,
        block: &BlockBehavior,
        block_filler_offset: (i32, i32, i32),
        hitbox: Option<(&BlockShape, i32)>,
    ) -> SupportResult {
        let origin = (
            block_x - block_filler_offset.0,
            block_y - block_filler_offset.1,
            block_z - block_filler_offset.2,
        );
        let bounding = hitbox.and_then(|(shape, rotation)| shape.rotated_bounding_box(rotation));
        let mut has_tested = false;
        let required_distance = block.max_support_distance;
        let mut lowest_distance = i32::MAX;

        for rule in &block.support {
            let face = if rule.rotate {
                rule.face.rotated(rotation)
            } else {
                rule.face
            };
            for offset in connecting_offsets(face) {
                let neighbour = (
                    block_x + i32::from(offset.x),
                    block_y + i32::from(offset.y),
                    block_z + i32::from(offset.z),
                );
                if let Some(bounding) = bounding {
                    if contains_block(bounding, origin, neighbour) {
                        continue;
                    }
                }
                if !loaded(world, neighbour.0, neighbour.1, neighbour.2) {
                    return SupportResult::Waiting;
                }
                let neighbour_id = world.get_block(neighbour.0, neighbour.1, neighbour.2);
                let neighbour_rotation = world.get_rotation(neighbour.0, neighbour.1, neighbour.2);
                let neighbour_filler = world.get_filler(neighbour.0, neighbour.1, neighbour.2);
                let neighbour_fluid = world.get_fluid_id(neighbour.0, neighbour.1, neighbour.2);
                let neighbour_behavior = self.tables.block(neighbour_id);
                if neighbour_filler != 0 && neighbour_id == block_id {
                    let (ox, oy, oz) = crate::collision::filler_offset(neighbour_filler);
                    if (neighbour.0 - ox, neighbour.1 - oy, neighbour.2 - oz) == origin {
                        continue;
                    }
                }

                if !rule_applied_to_filler(rule, block_filler_offset) {
                    continue;
                }
                let satisfied = self.rule_satisfied(
                    block_id,
                    block,
                    rule,
                    face,
                    offset,
                    block_filler_offset,
                    neighbour_id,
                    neighbour_rotation,
                    neighbour_filler,
                    neighbour_fluid,
                    neighbour_behavior,
                );
                if satisfied && required_distance > 0 && rule.allow_support_propagation {
                    let support = world.support_value(neighbour.0, neighbour.1, neighbour.2);
                    if support == IS_DECO_VALUE {
                        lowest_distance = 1;
                    } else if support < lowest_distance {
                        lowest_distance = support;
                    }
                }
                match rule.support {
                    SupportMatch::Ignored => {}
                    SupportMatch::Required => {
                        has_tested = true;
                        if satisfied {
                            return SupportResult::Satisfies;
                        }
                    }
                    SupportMatch::Disallowed => {
                        has_tested = true;
                        if satisfied {
                            return SupportResult::DoesntSatisfy;
                        }
                    }
                }
            }
        }

        if !has_tested {
            return SupportResult::Ignored;
        }
        if (0..i32::MAX).contains(&lowest_distance) {
            let distance = lowest_distance + 1;
            if required_distance >= distance {
                return SupportResult::Distance(distance);
            }
        }
        SupportResult::DoesntSatisfy
    }

    #[allow(clippy::too_many_arguments)]
    fn rule_satisfied(
        &self,
        block_id: i32,
        block: &BlockBehavior,
        rule: &SupportRule,
        face: Dir,
        neighbour_face: Dir,
        block_filler_offset: (i32, i32, i32),
        neighbour_id: i32,
        neighbour_rotation: i32,
        neighbour_filler: i32,
        neighbour_fluid: i32,
        neighbour: Option<&BlockBehavior>,
    ) -> bool {
        if let Some(required) = rule.block_type {
            if neighbour_id != required {
                return false;
            }
        }
        if let Some(tag) = &rule.tag {
            let Some(behavior) = neighbour else {
                return false;
            };
            if !behavior.tags.contains(tag) {
                return false;
            }
        }
        if let Some(fluid) = rule.fluid {
            let material_empty =
                neighbour.is_none_or(|block| block.material == BlockMaterial::Empty);
            if !(material_empty && neighbour_fluid != 0 && fluid == neighbour_fluid) {
                return false;
            }
        }
        if let Some(face_type) = &rule.face_type {
            if !self.supporting_matches(
                neighbour,
                neighbour_rotation,
                neighbour_filler,
                neighbour_face,
                face_type,
            ) {
                return false;
            }
        }
        if let Some(face_type) = &rule.self_face_type {
            let packed = pack_filler(
                block_filler_offset.0,
                block_filler_offset.1,
                block_filler_offset.2,
            );
            if !self.supporting_matches(Some(block), neighbour_rotation, packed, face, face_type) {
                return false;
            }
        }
        match rule.match_self {
            MatchSelf::Ignored => {}
            MatchSelf::Required => {
                if neighbour_id != block_id {
                    return false;
                }
            }
            MatchSelf::Disallowed => {
                if neighbour_id == block_id {
                    return false;
                }
            }
        }
        let _ = block_filler_offset;
        true
    }

    fn supporting_matches(
        &self,
        neighbor: Option<&BlockBehavior>,
        rotation: i32,
        filler: i32,
        face: Dir,
        face_type: &str,
    ) -> bool {
        let Some(neighbor) = neighbor else {
            return false;
        };
        let filler_offset = filler_offset(filler);
        neighbor.supporting.iter().any(|entry| {
            let entry_face = entry.face.rotated(rotation);
            if entry_face != face || entry.face_type != face_type {
                return false;
            }
            match &entry.filler {
                None => true,
                Some(entries) => entries.iter().any(|offset| {
                    let rotated = offset.rotated(rotation);
                    (
                        i32::from(rotated.x),
                        i32::from(rotated.y),
                        i32::from(rotated.z),
                    ) == filler_offset
                }),
            }
        })
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
        if self.disabled_fluids.contains(&fluid_id) {
            return BlockTickStrategy::Ignored;
        }
        let level = world.get_fluid_level(x, y, z);
        let tick = world.tick();
        // Java `FluidTicker.tick` gates every ticker by `FlowRate * tps`; the
        // per-position hash phase is deliberately replaced by the world tick so
        // the simulation remains deterministic and reproducible in tests.
        let rate = (fluid.flow_rate * 20.0).round().max(1.0) as u64;
        if !tick.is_multiple_of(rate) {
            return BlockTickStrategy::Continue;
        }

        // Java `FluidTicker.tick`: full solid blocks destroy non-fire fluid.
        let block_id = world.get_block(x, y, z);
        if fluid.kind != FluidTickerKind::Fire && self.is_fully_solid(block_id) {
            self.clear_fluid(world, x, y, z, report);
            self.wake(world, x, y, z);
            return BlockTickStrategy::Sleep;
        }

        match fluid.kind {
            FluidTickerKind::Default => {
                match self.fluid_alive(world, x, y, z, fluid_id, &fluid, level) {
                    AliveStatus::WaitForChunk => BlockTickStrategy::WaitForAdjacentChunkLoad,
                    AliveStatus::Alive => {
                        self.spread_default(world, x, y, z, fluid_id, &fluid, level, report)
                    }
                    AliveStatus::Demote => {
                        if level <= 1 {
                            self.clear_fluid(world, x, y, z, report);
                        } else {
                            let next = if level == 0 {
                                fluid.max_level
                            } else {
                                level - 1
                            };
                            self.set_fluid(world, x, y, z, fluid_id, next, report);
                        }
                        self.wake(world, x, y, z);
                        BlockTickStrategy::Sleep
                    }
                }
            }
            FluidTickerKind::Finite => {
                self.spread_finite(world, x, y, z, fluid_id, &fluid, level, report)
            }
            FluidTickerKind::Fire => {
                self.spread_fire(world, x, y, z, fluid_id, &fluid, level, report)
            }
        }
    }

    fn clear_fluid(
        &self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        report: &mut SimulationReport,
    ) {
        if world.set_fluid(x, y, z, 0, 0).unwrap_or(false) {
            report.fluid_changes.push(FluidChange {
                x,
                y,
                z,
                fluid_id: 0,
                level: 0,
            });
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn set_fluid(
        &self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        level: i8,
        report: &mut SimulationReport,
    ) {
        if world.set_fluid(x, y, z, fluid_id, level).unwrap_or(false) {
            report.fluid_changes.push(FluidChange {
                x,
                y,
                z,
                fluid_id,
                level,
            });
        }
    }

    fn wake(&self, world: &mut World, x: i32, y: i32, z: i32) {
        // `World::set_fluid`/`set_block` already queue the 3x3x3 neighbourhood
        // for the next pass; touching the word keeps this helper explicit for
        // Java's `setTickingSurrounding` call sites.
        let _ = (world, x, y, z);
    }

    /// Java `FluidTicker.isSolid` (cube/cube-with-model draw type).
    pub fn is_solid(&self, block_id: i32) -> bool {
        self.tables.block(block_id).is_some_and(|block| {
            matches!(block.draw_type, DrawType::Cube | DrawType::CubeWithModel)
        })
    }

    /// Java `FluidTicker.isFullySolid` (solid material + full cube draw type).
    pub fn is_fully_solid(&self, block_id: i32) -> bool {
        self.tables.block(block_id).is_some_and(|block| {
            block.material == BlockMaterial::Solid
                && matches!(block.draw_type, DrawType::Cube | DrawType::CubeWithModel)
        })
    }

    /// Java `FluidTicker.blocksFluidFrom`.
    pub fn blocks_fluid_from(
        &self,
        block_id: i32,
        rotation: i32,
        offset_x: i32,
        offset_z: i32,
        filler: i32,
    ) -> bool {
        let Some(block) = self.tables.block(block_id) else {
            return false;
        };
        if block.material != BlockMaterial::Solid {
            return false;
        }
        if self.is_fully_solid(block_id) {
            return true;
        }
        let Some(shape) = self.tables.block_shape(block_id) else {
            return true;
        };

        const FULL_DIMENSION_THRESHOLD: f64 = 0.9;
        const PARTIAL_DIMENSION_THRESHOLD: f64 = 0.6;
        const FACE_BLOCK_THRESHOLD: f64 = 0.1;

        let mut boxes = Vec::new();
        shape.rotated_boxes(rotation, &mut boxes);
        if boxes.is_empty() {
            return true;
        }
        if self.tables.block_protrudes(block_id) || filler != 0 {
            let (filler_x, filler_y, filler_z) = filler_offset(filler);
            let mut max_cross_section_coverage = 0.0f64;
            for boxed in &boxes {
                let clamped_min_x = boxed.min.x.max(f64::from(filler_x));
                let clamped_max_x = boxed.max.x.min(f64::from(filler_x + 1));
                let clamped_min_y = boxed.min.y.max(f64::from(filler_y));
                let clamped_max_y = boxed.max.y.min(f64::from(filler_y + 1));
                let clamped_min_z = boxed.min.z.max(f64::from(filler_z));
                let clamped_max_z = boxed.max.z.min(f64::from(filler_z + 1));
                if clamped_max_x <= clamped_min_x
                    || clamped_max_y <= clamped_min_y
                    || clamped_max_z <= clamped_min_z
                {
                    continue;
                }
                let width = clamped_max_x - clamped_min_x;
                let height = clamped_max_y - clamped_min_y;
                let depth = clamped_max_z - clamped_min_z;
                let coverage = if offset_x != 0 {
                    height * depth
                } else if offset_z != 0 {
                    height * width
                } else {
                    0.0
                };
                max_cross_section_coverage = max_cross_section_coverage.max(coverage);
            }
            return max_cross_section_coverage > FULL_DIMENSION_THRESHOLD;
        }

        let Some(bounding_box) = shape.rotated_bounding_box(rotation) else {
            return true;
        };
        let width = bounding_box.max.x - bounding_box.min.x;
        let height = bounding_box.max.y - bounding_box.min.y;
        let depth = bounding_box.max.z - bounding_box.min.z;
        if height <= FULL_DIMENSION_THRESHOLD {
            // Flat/horizontal slabs do not block horizontal flow.
            return false;
        }

        let full_depth = depth > FULL_DIMENSION_THRESHOLD;
        let partial_width = width < PARTIAL_DIMENSION_THRESHOLD;
        if partial_width && full_depth {
            return if offset_x > 0 {
                bounding_box.min.x < FACE_BLOCK_THRESHOLD
            } else if offset_x < 0 {
                bounding_box.max.x > 1.0 - FACE_BLOCK_THRESHOLD
            } else {
                false
            };
        }

        let full_width = width > FULL_DIMENSION_THRESHOLD;
        let partial_depth = depth < PARTIAL_DIMENSION_THRESHOLD;
        if full_width && partial_depth {
            return if offset_z > 0 {
                bounding_box.min.z < FACE_BLOCK_THRESHOLD
            } else if offset_z < 0 {
                bounding_box.max.z > 1.0 - FACE_BLOCK_THRESHOLD
            } else {
                false
            };
        }

        if boxes.len() > 1 {
            let mut total_area = 0.0;
            for boxed in &boxes {
                let (height, width, depth) = (
                    boxed.max.y - boxed.min.y,
                    boxed.max.x - boxed.min.x,
                    boxed.max.z - boxed.min.z,
                );
                total_area += if (offset_x > 0 && boxed.min.x < FACE_BLOCK_THRESHOLD)
                    || (offset_x < 0 && boxed.max.x > 1.0 - FACE_BLOCK_THRESHOLD)
                {
                    height * depth
                } else if (offset_z > 0 && boxed.min.z < FACE_BLOCK_THRESHOLD)
                    || (offset_z < 0 && boxed.max.z > 1.0 - FACE_BLOCK_THRESHOLD)
                {
                    height * width
                } else {
                    0.0
                };
            }
            return total_area > FULL_DIMENSION_THRESHOLD;
        }

        let face_coverage = if (offset_x > 0 && bounding_box.min.x < FACE_BLOCK_THRESHOLD)
            || (offset_x < 0 && bounding_box.max.x > 1.0 - FACE_BLOCK_THRESHOLD)
        {
            height * depth
        } else if (offset_z > 0 && bounding_box.min.z < FACE_BLOCK_THRESHOLD)
            || (offset_z < 0 && bounding_box.max.z > 1.0 - FACE_BLOCK_THRESHOLD)
        {
            height * width
        } else {
            0.0
        };
        face_coverage > FULL_DIMENSION_THRESHOLD
    }

    #[allow(clippy::too_many_arguments)]
    fn fluid_alive(
        &self,
        world: &World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        fluid: &FluidBehavior,
        level: i8,
    ) -> AliveStatus {
        if !fluid.can_demote {
            return AliveStatus::Alive;
        }
        if !loaded(world, x, y + 1, z) {
            return AliveStatus::WaitForChunk;
        }
        let above = world.get_fluid_id(x, y + 1, z);
        if above == fluid_id || fluid.supported_by == Some(above) {
            return AliveStatus::Alive;
        }

        let this_block = world.get_block(x, y, z);
        let this_rotation = world.get_rotation(x, y, z);
        let this_filler = world.get_filler(x, y, z);
        let mut chunk_not_loaded = false;
        for direction in [Dir::WEST, Dir::EAST, Dir::NORTH, Dir::SOUTH] {
            let (dx, dz) = (i32::from(direction.x), i32::from(direction.z));
            let (block_x, block_z) = (x + dx, z + dz);
            if !loaded(world, block_x, y, block_z) {
                chunk_not_loaded = true;
                continue;
            }
            let other_fluid = world.get_fluid_id(block_x, y, block_z);
            let source_supported = fluid.supported_by == Some(other_fluid);
            let higher_same = other_fluid != 0
                && other_fluid == fluid_id
                && world.get_fluid_level(block_x, y, block_z) > level;
            if source_supported || higher_same {
                let source_block = world.get_block(block_x, y, block_z);
                let source_rotation = world.get_rotation(block_x, y, block_z);
                let source_filler = world.get_filler(block_x, y, block_z);
                if self.blocks_fluid_from(source_block, source_rotation, dx, dz, source_filler) {
                    continue;
                }
                if self.blocks_fluid_from(this_block, this_rotation, -dx, -dz, this_filler) {
                    continue;
                }
                return AliveStatus::Alive;
            }
        }
        if chunk_not_loaded {
            AliveStatus::WaitForChunk
        } else {
            AliveStatus::Demote
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spread_default(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        fluid: &FluidBehavior,
        level: i8,
        report: &mut SimulationReport,
    ) -> BlockTickStrategy {
        let spread_id = fluid.spread_fluid_id(fluid_id);
        let spread = self
            .tables
            .fluid(spread_id)
            .cloned()
            .unwrap_or_else(|| fluid.clone());

        if !loaded(world, x, y - 1, z) {
            return BlockTickStrategy::WaitForAdjacentChunkLoad;
        }
        let below_id = world.get_fluid_id(x, y - 1, z);
        let below_level = world.get_fluid_level(x, y - 1, z);
        let below_block = world.get_block(x, y - 1, z);
        if !self.is_solid(below_block)
            && (below_id == 0 || below_id == spread_id || below_id != fluid_id)
        {
            if let Some(collision) = fluid
                .collisions
                .iter()
                .find(|collision| collision.other == below_id)
            {
                self.apply_fluid_collision(world, x, y - 1, z, collision, report);
                if collision.place_fluid {
                    // Java returns CONTINUE after a collision that allows the
                    // fluid; the next pass sees the converted cell and flows.
                    return BlockTickStrategy::Continue;
                }
            } else if below_id == 0 || (below_id == spread_id && below_level < spread.max_level) {
                self.set_fluid(world, x, y - 1, z, spread_id, spread.max_level, report);
            }
            return BlockTickStrategy::Sleep;
        }

        if below_id == 0 || below_id != spread_id {
            if level == 1 && fluid.max_level != 1 {
                return BlockTickStrategy::Sleep;
            }
            let offsets = self.get_spread_offsets(
                world,
                x,
                y,
                z,
                fluid_id,
                &fluid.supported_by,
                FLUID_BLOCK_DISTANCE,
            );
            if offsets == SPREAD_NO_CHUNK_BITS {
                return BlockTickStrategy::WaitForAdjacentChunkLoad;
            }
            let child_fill_level = if spread_id != fluid_id {
                spread.max_level.saturating_sub(1)
            } else {
                level.saturating_sub(1)
            };
            let source_block = world.get_block(x, y, z);
            let source_rotation = world.get_rotation(x, y, z);
            let source_filler = world.get_filler(x, y, z);

            for (index, direction) in ORTHO_OFFSETS.iter().enumerate() {
                if offsets != 0 && (offsets & (1 << index)) == 0 {
                    continue;
                }
                let dx = i32::from(direction.x);
                let dz = i32::from(direction.z);
                let (block_x, block_z) = (x + dx, z + dz);
                if self.blocks_fluid_from(source_block, source_rotation, -dx, -dz, source_filler) {
                    continue;
                }
                if !loaded(world, block_x, y, block_z) {
                    return BlockTickStrategy::WaitForAdjacentChunkLoad;
                }
                let target_block = world.get_block(block_x, y, block_z);
                let target_rotation = world.get_rotation(block_x, y, block_z);
                let target_filler = world.get_filler(block_x, y, block_z);
                if self.blocks_fluid_from(target_block, target_rotation, dx, dz, target_filler) {
                    continue;
                }
                let target_id = world.get_fluid_id(block_x, y, block_z);
                if target_id != 0 && target_id != spread_id {
                    let Some(collision) = fluid
                        .collisions
                        .iter()
                        .find(|collision| collision.other == target_id)
                    else {
                        continue;
                    };
                    self.apply_fluid_collision(world, block_x, y, block_z, collision, report);
                    if !collision.place_fluid {
                        continue;
                    }
                }
                let target_level = world.get_fluid_level(block_x, y, block_z);
                if target_id == spread_id && target_level >= child_fill_level {
                    continue;
                }
                if child_fill_level == 0 {
                    self.clear_fluid(world, block_x, y, block_z, report);
                } else {
                    self.set_fluid(
                        world,
                        block_x,
                        y,
                        block_z,
                        spread_id,
                        child_fill_level,
                        report,
                    );
                }
            }
        }
        BlockTickStrategy::Sleep
    }

    #[allow(clippy::too_many_arguments)]
    fn spread_finite(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        fluid: &FluidBehavior,
        level: i8,
        report: &mut SimulationReport,
    ) -> BlockTickStrategy {
        if !loaded(world, x, y - 1, z) {
            return BlockTickStrategy::WaitForAdjacentChunkLoad;
        }
        let below_id = world.get_fluid_id(x, y - 1, z);
        let below_level = world.get_fluid_level(x, y - 1, z);
        let below_block = world.get_block(x, y - 1, z);
        if (fluid_id == below_id || below_id == 0) && !self.is_solid(below_block) {
            let top_y = self.finite_top_y(world, x, y, z, fluid);
            if let Some(top_y) = top_y {
                let top_level = world.get_fluid_level(x, top_y, z);
                let transfer = top_level.min(fluid.max_level.saturating_sub(below_level));
                if transfer > 0 {
                    let new_bottom = if below_id == 0 {
                        transfer
                    } else {
                        below_level.saturating_add(transfer)
                    };
                    self.set_fluid(world, x, y - 1, z, fluid_id, new_bottom, report);
                    if transfer == top_level {
                        self.clear_fluid(world, x, top_y, z, report);
                    } else {
                        self.set_fluid(world, x, top_y, z, fluid_id, top_level - transfer, report);
                    }
                    return BlockTickStrategy::Continue;
                }
            } else {
                return BlockTickStrategy::WaitForAdjacentChunkLoad;
            }
        }
        self.spread_finite_sideways(world, x, y, z, fluid_id, fluid, level, report)
    }

    #[allow(clippy::too_many_arguments)]
    fn spread_finite_sideways(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        fluid: &FluidBehavior,
        level: i8,
        report: &mut SimulationReport,
    ) -> BlockTickStrategy {
        if level == 1 {
            return BlockTickStrategy::Sleep;
        }
        let mut new_level = level;
        for offset_array in [&ORTHO_OFFSETS[..], &DIAG_OFFSETS[..]] {
            let offsets = self.get_spread_offsets(world, x, y, z, fluid_id, &fluid.supported_by, 2);
            if offsets == SPREAD_NO_CHUNK_BITS {
                return BlockTickStrategy::WaitForAdjacentChunkLoad;
            }
            let spread_downhill = offsets != 0;
            for (index, direction) in offset_array.iter().enumerate() {
                if new_level == 1 {
                    break;
                }
                if spread_downhill && (offsets & (1 << index)) == 0 {
                    continue;
                }
                if !self.finite_offset_connected(world, *direction, x, y, z) {
                    continue;
                }
                let dx = i32::from(direction.x);
                let dz = i32::from(direction.z);
                let (block_x, block_z) = (x + dx, z + dz);
                if !loaded(world, block_x, y, block_z) {
                    return BlockTickStrategy::WaitForAdjacentChunkLoad;
                }
                let target_block = world.get_block(block_x, y, block_z);
                if self.is_solid(target_block) {
                    continue;
                }
                let target_id = world.get_fluid_id(block_x, y, block_z);
                let target_level = world.get_fluid_level(block_x, y, block_z);
                let mut new_target_level = 1;
                if target_id == 0 || (target_id == fluid_id && target_level < level - 1) {
                    if target_id == fluid_id {
                        new_target_level = target_level.saturating_add(1);
                    }
                    if world
                        .set_fluid(block_x, y, block_z, fluid_id, new_target_level)
                        .unwrap_or(false)
                    {
                        report.fluid_changes.push(FluidChange {
                            x: block_x,
                            y,
                            z: block_z,
                            fluid_id,
                            level: new_target_level,
                        });
                        new_level -= 1;
                    }
                }
            }
            if spread_downhill {
                break;
            }
        }
        if new_level == level {
            return BlockTickStrategy::Sleep;
        }
        if !self.drain_from_top(world, x, y, z, fluid_id, fluid, level - new_level, report) {
            return BlockTickStrategy::WaitForAdjacentChunkLoad;
        }
        BlockTickStrategy::Continue
    }

    fn finite_offset_connected(&self, world: &World, offset: Dir, x: i32, y: i32, z: i32) -> bool {
        if offset.x == 0 || offset.z == 0 {
            return true;
        }
        let (first, second) = (
            world.get_block(x + i32::from(offset.x), y, z),
            world.get_block(x, y, z + i32::from(offset.z)),
        );
        first == 0 || second == 0 || !self.is_solid(first) || !self.is_solid(second)
    }

    fn finite_top_y(
        &self,
        world: &World,
        x: i32,
        y: i32,
        z: i32,
        fluid: &FluidBehavior,
    ) -> Option<i32> {
        let mut top = y;
        while top < 319 {
            if world.get_fluid_level(x, top, z) != fluid.max_level
                || world.get_fluid_id(x, top + 1, z) != world.get_fluid_id(x, y, z)
            {
                break;
            }
            top += 1;
        }
        if !loaded(world, x, top, z) {
            return None;
        }
        Some(top)
    }

    #[allow(clippy::too_many_arguments)]
    fn drain_from_top(
        &self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        fluid: &FluidBehavior,
        mut drain_levels: i8,
        report: &mut SimulationReport,
    ) -> bool {
        let Some(mut next_y) = self.finite_top_y(world, x, y, z, fluid) else {
            return false;
        };
        let mut updated = true;
        while drain_levels > 0 {
            if !loaded(world, x, next_y, z) {
                return false;
            }
            let next_level = world.get_fluid_level(x, next_y, z);
            let transfer = next_level.min(drain_levels);
            if transfer == next_level {
                if world.set_fluid(x, next_y, z, 0, 0).unwrap_or(false) {
                    report.fluid_changes.push(FluidChange {
                        x,
                        y: next_y,
                        z,
                        fluid_id: 0,
                        level: 0,
                    });
                } else {
                    updated = false;
                }
            } else if world
                .set_fluid(x, next_y, z, fluid_id, next_level - transfer)
                .unwrap_or(false)
            {
                report.fluid_changes.push(FluidChange {
                    x,
                    y: next_y,
                    z,
                    fluid_id,
                    level: next_level - transfer,
                });
            } else {
                updated = false;
            }
            drain_levels -= transfer;
            next_y -= 1;
            if next_y < 0 {
                break;
            }
        }
        updated
    }

    #[allow(clippy::too_many_arguments)]
    fn spread_fire(
        &mut self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        fluid: &FluidBehavior,
        level: i8,
        report: &mut SimulationReport,
    ) -> BlockTickStrategy {
        let spread_id = fluid.spread_fluid_id(fluid_id);
        let mut level = level;
        if level < fluid.max_level {
            level = level.saturating_add(1);
            self.set_fluid(world, x, y, z, fluid_id, level, report);
        }
        for direction in [
            Dir::DOWN,
            Dir::UP,
            Dir::NORTH,
            Dir::SOUTH,
            Dir::WEST,
            Dir::EAST,
        ] {
            if direction.y != 0 && !(0..320).contains(&(y + i32::from(direction.y))) {
                continue;
            }
            let (block_x, block_y, block_z) = (
                x + i32::from(direction.x),
                y + i32::from(direction.y),
                z + i32::from(direction.z),
            );
            if !loaded(world, block_x, block_y, block_z) {
                return BlockTickStrategy::WaitForAdjacentChunkLoad;
            }
            if world.get_fluid_id(block_x, block_y, block_z) != 0 {
                continue;
            }
            if self
                .flammability(world.get_block(block_x, block_y, block_z))
                .is_some()
            {
                self.set_fluid(world, block_x, block_y, block_z, spread_id, 1, report);
            }
        }

        let Some(config) = self.flammability(world.get_block(x, y, z)).cloned() else {
            self.clear_fluid(world, x, y, z, report);
            return BlockTickStrategy::Sleep;
        };
        if level >= config.burn_level && self.random.next_float() < config.burn_chance {
            self.try_burn(world, x, y, z, &config, report);
            return BlockTickStrategy::Sleep;
        }
        BlockTickStrategy::Continue
    }

    fn flammability(&self, block_id: i32) -> Option<&Flammability> {
        let block = self.tables.block(block_id)?;
        let mut configs: Vec<&Flammability> = self
            .tables
            .fluids
            .iter()
            .flatten()
            .flat_map(|fluid| fluid.flammability.iter())
            .filter(|config| config.matches(&block.tags))
            .collect();
        configs.sort_by_key(|config| std::cmp::Reverse(config.priority));
        configs.first().copied()
    }

    fn try_burn(
        &self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        config: &Flammability,
        report: &mut SimulationReport,
    ) {
        let rotation = world.get_rotation(x, y, z);
        let filler = world.get_filler(x, y, z);
        if let Some(block_id) = config.resulting_block {
            let current = world.get_block(x, y, z);
            if block_id != current
                && world
                    .set_block_full(x, y, z, block_id, rotation, filler)
                    .unwrap_or(false)
            {
                report.block_changes.push(BlockChange {
                    x,
                    y,
                    z,
                    block_id,
                    rotation,
                    filler,
                });
            }
        }
        self.clear_fluid(world, x, y, z, report);
        self.wake(world, x, y, z);
    }

    #[allow(clippy::too_many_arguments)]
    fn get_spread_offsets(
        &self,
        world: &World,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        supported_by: &Option<i32>,
        max_drop_distance: i32,
    ) -> u32 {
        let mut shortest_distance = SPREAD_NO_PATH;
        let mut offsets = 0;
        for (index, direction) in ORTHO_OFFSETS.iter().enumerate() {
            let distance = self.distance_to_drop(
                world,
                x,
                y,
                z,
                *direction,
                fluid_id,
                *supported_by,
                max_drop_distance,
            );
            if distance == SPREAD_NO_CHUNK {
                return SPREAD_NO_CHUNK_BITS;
            }
            if distance < shortest_distance {
                offsets = 0;
                shortest_distance = distance;
            }
            if distance <= shortest_distance && distance != SPREAD_NO_PATH {
                offsets |= 1 << index;
            }
        }
        offsets
    }

    #[allow(clippy::too_many_arguments)]
    fn distance_to_drop(
        &self,
        world: &World,
        x: i32,
        y: i32,
        z: i32,
        offset: Dir,
        fluid_id: i32,
        supported_by: Option<i32>,
        max_drop_distance: i32,
    ) -> i32 {
        let (ox, oz) = (i32::from(offset.x), i32::from(offset.z));
        for step in 1..max_drop_distance {
            let block_x = x + ox * step;
            let block_z = z + oz * step;
            if !loaded(world, block_x, y, block_z) || !loaded(world, block_x, y - 1, block_z) {
                return SPREAD_NO_CHUNK;
            }
            let other_fluid = world.get_fluid_id(block_x, y, block_z);
            if other_fluid != 0
                && (other_fluid != fluid_id
                    || supported_by == Some(other_fluid)
                    || supported_by.is_none())
            {
                break;
            }
            if other_fluid == 0 && self.is_solid(world.get_block(block_x, y, block_z)) {
                break;
            }
            if !self.is_solid(world.get_block(block_x, y - 1, block_z)) {
                return step;
            }
        }
        SPREAD_NO_PATH
    }

    fn apply_fluid_collision(
        &self,
        world: &mut World,
        x: i32,
        y: i32,
        z: i32,
        collision: &FluidCollision,
        report: &mut SimulationReport,
    ) {
        if let Some(block_id) = collision.block_to_place {
            let existing = world.get_block(x, y, z);
            let replaceable = existing == 0
                || self
                    .tables
                    .block(existing)
                    .and_then(|block| block.gathering.as_ref())
                    .is_some_and(BlockGathering::is_soft);
            if replaceable {
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
                self.clear_fluid(world, x, y, z, report);
                self.wake(world, x, y, z);
            }
        }
        if !collision.place_fluid {
            self.clear_fluid(world, x, y, z, report);
        }
    }

    fn random_ticks(&mut self, world: &mut World, report: &mut SimulationReport) {
        const STABLE_PER_SECTION: usize = 1;
        const UNSTABLE_PER_SECTION: usize = 3;
        const SECTION_BLOCKS: usize = 32 * 32 * 32;
        let interval = SECTION_BLOCKS / STABLE_PER_SECTION;
        let tick = world.tick();
        let chunks = world.loaded_chunk_coordinates();
        for (chunk_x, chunk_z) in chunks {
            for section_index in 0..10usize {
                let section_y = section_index as i32 * 32;
                let base_seed = hytale_math::hash_util::hash4(
                    (tick / interval as u64) as i64,
                    i64::from(chunk_x),
                    section_index as i64,
                    i64::from(chunk_z),
                ) as u64;
                let random_seed = (base_seed.rotate_left(1) | 1) & (SECTION_BLOCKS as u64 - 1);
                let random_seed2 = (base_seed >> 16) & (SECTION_BLOCKS as u64 - 1);
                let start = (tick % interval as u64) * STABLE_PER_SECTION as u64;

                // The stable walk visits every block in the section once per
                // cycle (Java `RandomTickSystem`).
                for i in 0..STABLE_PER_SECTION as u64 {
                    let index = (((start + i) * random_seed + random_seed2)
                        & (SECTION_BLOCKS as u64 - 1)) as usize;
                    self.tick_random_index(world, chunk_x, chunk_z, section_y, index, report);
                }
                // The unstable picks are genuinely random.
                for _ in 0..UNSTABLE_PER_SECTION {
                    let index = self.random.next_int_bound(SECTION_BLOCKS as i32) as usize;
                    self.tick_random_index(world, chunk_x, chunk_z, section_y, index, report);
                }
            }
        }
    }

    fn tick_random_index(
        &mut self,
        world: &mut World,
        chunk_x: i32,
        chunk_z: i32,
        section_y: i32,
        index: usize,
        report: &mut SimulationReport,
    ) {
        let x = (chunk_x << 5) + (index & 31) as i32;
        let y = section_y + ((index >> 10) & 31) as i32;
        let z = (chunk_z << 5) + ((index >> 5) & 31) as i32;
        if self.suppressed_random.contains(&(x, y, z)) {
            return;
        }
        let block_id = world.get_block(x, y, z);
        if block_id == 0 {
            return;
        }
        let Some(procedure) = self
            .tables
            .block(block_id)
            .and_then(|block| block.random_tick.clone())
        else {
            return;
        };
        report.random_ticks += 1;
        self.run_random_tick(world, (x, y, z), block_id, procedure, report);
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
                    if world.set_block_full(x, y, z, target, 0, 0).unwrap_or(false) {
                        report.block_changes.push(BlockChange {
                            x,
                            y,
                            z,
                            block_id: target,
                            rotation: 0,
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
                required_light_level,
            } => {
                if let Some(revert) = revert_block {
                    let mut should_revert = false;
                    let above = world.get_block(x, y + 1, z);
                    if self.tables.block(above).is_some_and(|block| {
                        block.material == BlockMaterial::Solid
                            && block.opacity == Opacity::Solid
                            && matches!(block.draw_type, DrawType::Cube | DrawType::CubeWithModel)
                    }) {
                        should_revert = true;
                    }
                    if let Some(allowed) = &allowed_above_fluids {
                        let above_fluid = world.get_fluid_id(x, y + 1, z);
                        if !allowed.contains(&above_fluid) {
                            should_revert = true;
                        }
                    }
                    if should_revert && world.set_block_full(x, y, z, revert, 0, 0).unwrap_or(false)
                    {
                        report.block_changes.push(BlockChange {
                            x,
                            y,
                            z,
                            block_id: revert,
                            rotation: 0,
                            filler: 0,
                        });
                        return;
                    }
                }
                if self.light_level(world, x, y + 1, z) < required_light_level {
                    return;
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
                rules,
                require_empty_target,
            } => {
                let target_x = x + i32::from(offset.x);
                let target_y = y + i32::from(offset.y);
                let target_z = z + i32::from(offset.z);
                let target_id = world.get_block(target_x, target_y, target_z);
                let Some(rule) = rules
                    .iter()
                    .find(|rule| rule.state.is_none())
                    .or_else(|| rules.first())
                else {
                    return;
                };
                if rule
                    .blocks
                    .iter()
                    .any(|candidate| candidate.block == target_id)
                {
                    return;
                }
                if require_empty_target
                    && self
                        .tables
                        .block(target_id)
                        .is_some_and(|block| block.material != BlockMaterial::Empty)
                {
                    return;
                }
                let Some(block_id) = self.pick_weighted(&rule.blocks) else {
                    return;
                };
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
            RandomTickProcedure::Unsupported(_) => {}
        }
    }
}

const SUPPORT_IGNORE: i32 = -1;
const SUPPORT_SATISFIES: i32 = -2;
const SUPPORT_DOESNT: i32 = 0;
const IS_DECO_VALUE: i32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SupportResult {
    Ignored,
    Satisfies,
    DoesntSatisfy,
    Waiting,
    Distance(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportMatch {
    Ignored,
    Required,
    Disallowed,
}

fn connecting_offsets(face: Dir) -> Vec<Dir> {
    let axis_values = [face.x, face.y, face.z];
    let used: Vec<usize> = axis_values
        .iter()
        .enumerate()
        .filter(|(_, value)| **value != 0)
        .map(|(index, _)| index)
        .collect();
    let mut out = Vec::new();
    for mask in 1u8..(1 << used.len()) {
        let mut offset = Dir::new(0, 0, 0);
        for (bit, axis) in used.iter().enumerate() {
            if mask & (1 << bit) == 0 {
                continue;
            }
            match axis {
                0 => offset.x = face.x,
                1 => offset.y = face.y,
                _ => offset.z = face.z,
            }
        }
        out.push(offset);
    }
    out
}

fn rule_applied_to_filler(rule: &SupportRule, filler_offset: (i32, i32, i32)) -> bool {
    match &rule.filler {
        None => true,
        Some(entries) => entries.iter().any(|offset| {
            (
                i32::from(offset.x),
                i32::from(offset.y),
                i32::from(offset.z),
            ) == filler_offset
        }),
    }
}

fn pack_filler(x: i32, y: i32, z: i32) -> i32 {
    const MASK: i32 = 31;
    (x & MASK) | ((z & MASK) << 5) | ((y & MASK) << 10)
}

fn contains_block(bounding: Aabb, origin: (i32, i32, i32), point: (i32, i32, i32)) -> bool {
    let min_x = bounding.min.x.floor() as i32;
    let min_y = bounding.min.y.floor() as i32;
    let min_z = bounding.min.z.floor() as i32;
    let max_x = bounding.max.x.ceil() as i32;
    let max_y = bounding.max.y.ceil() as i32;
    let max_z = bounding.max.z.ceil() as i32;
    let (x, y, z) = (point.0 - origin.0, point.1 - origin.1, point.2 - origin.2);
    x >= min_x && x < max_x && y >= min_y && y < max_y && z >= min_z && z < max_z
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
                filler: None,
            })
            .collect(),
            supports_required_for: SupportsRequiredFor::All,
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
                support: SupportMatch::Required,
                block_type: None,
                tag: None,
                fluid: None,
                face_type: Some("Full".to_string()),
                self_face_type: None,
                match_self: MatchSelf::Ignored,
                allow_support_propagation: true,
                filler: None,
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
            supports_required_for: SupportsRequiredFor::All,
        }
    }

    fn world_with_cube() -> World {
        let mut world = World::new(WorldStorage::new("/tmp/hytale-simulation-test"));
        for chunk_x in -1..=1 {
            for chunk_z in -1..=1 {
                world.insert_chunk(ChunkColumn::new(chunk_x, chunk_z));
            }
        }
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
            kind: FluidTickerKind::Default,
            tags: BTreeSet::new(),
            flammability: Vec::new(),
        };
        let fluid_flow = FluidBehavior {
            max_level: 8,
            flow_rate: 0.05,
            can_demote: true,
            supported_by: Some(1),
            spread_fluid: Some(2),
            collisions: Vec::new(),
            kind: FluidTickerKind::Default,
            tags: BTreeSet::new(),
            flammability: Vec::new(),
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

    fn support_chain_block(max_support_distance: i32) -> BlockBehavior {
        let mut block = cube();
        block.support = vec![
            SupportRule {
                face: Dir::DOWN,
                rotate: true,
                support: SupportMatch::Required,
                block_type: None,
                tag: None,
                fluid: None,
                face_type: Some("Full".to_string()),
                self_face_type: None,
                match_self: MatchSelf::Ignored,
                allow_support_propagation: true,
                filler: None,
            },
            SupportRule {
                face: Dir::DOWN,
                rotate: true,
                support: SupportMatch::Ignored,
                block_type: None,
                tag: None,
                fluid: None,
                face_type: Some("Branch".to_string()),
                self_face_type: None,
                match_self: MatchSelf::Ignored,
                allow_support_propagation: true,
                filler: None,
            },
        ];
        block.supporting = [
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
            face_type: "Branch".to_string(),
            filler: None,
        })
        .collect();
        block.max_support_distance = max_support_distance;
        block.supports_required_for = SupportsRequiredFor::All;
        block
    }

    #[test]
    fn max_support_distance_chains_and_breaks_past_the_limit() {
        let mut world = world_with_cube();
        world.set_block(0, 0, 0, 2).unwrap();
        for y in 1..=4 {
            world.set_block(0, y, 0, 1).unwrap();
        }
        let tables = SimulationTables::from_parts(
            vec![None, Some(support_chain_block(2)), Some(cube())],
            Vec::new(),
        );
        let mut simulation = BlockSimulation::new(tables, 21);
        for _ in 0..4 {
            simulation.tick(&mut world, 0.05);
        }
        assert_eq!(world.support_value(0, 1, 0), 0, "direct support");
        assert_eq!(world.support_value(0, 2, 0), 1);
        assert_eq!(world.support_value(0, 3, 0), 2);
        assert_eq!(world.get_block(0, 4, 0), 0, "distance 3 exceeds max 2");
    }

    #[test]
    fn spread_to_respects_the_light_gate_and_reverts_when_covered() {
        let grass = BlockBehavior {
            material: BlockMaterial::Empty,
            draw_type: DrawType::Model,
            opacity: Opacity::Solid,
            fluid_solid: false,
            support: Vec::new(),
            support_drop: SupportDropType::Break,
            max_support_distance: 0,
            falling: false,
            gathering: None,
            random_tick: Some(RandomTickProcedure::SpreadTo {
                directions: vec![Dir::EAST],
                min_y: 0,
                max_y: 0,
                allowed_tag: "Spreadable=Grass".to_string(),
                require_empty_above: true,
                allowed_above_fluids: None,
                revert_block: Some(3),
                required_light_level: 6,
            }),
            tick_procedure: None,
            tags: BTreeSet::new(),
            supporting: Vec::new(),
            supports_required_for: SupportsRequiredFor::All,
        };
        let mut dirt = cube();
        dirt.material = BlockMaterial::Empty;
        dirt.tags.insert("Spreadable=Grass".to_string());
        let air = BlockBehavior {
            material: BlockMaterial::Empty,
            draw_type: DrawType::Empty,
            opacity: Opacity::Transparent,
            fluid_solid: false,
            support: Vec::new(),
            support_drop: SupportDropType::Break,
            max_support_distance: 0,
            falling: false,
            gathering: None,
            random_tick: None,
            tick_procedure: None,
            tags: BTreeSet::new(),
            supporting: Vec::new(),
            supports_required_for: SupportsRequiredFor::All,
        };
        let revert = cube();
        let tables = SimulationTables::from_parts(
            vec![Some(air), Some(grass), Some(dirt), Some(revert)],
            Vec::new(),
        );
        let mut world = world_with_cube();
        world.set_block(0, 0, 0, 2).unwrap();
        world.set_block(0, 1, 0, 1).unwrap();
        world.set_block(1, 1, 0, 2).unwrap();
        let mut simulation = BlockSimulation::new(tables, 22);
        // No light: no spread.
        simulation.force_random_tick(&mut world, 0, 1, 0);
        assert_eq!(world.get_block(1, 1, 0), 2);
        // Full sky light at the block above the source: spread.
        let packed = 0xF000;
        world
            .chunk_mut(0, 0)
            .unwrap()
            .ensure_section(0)
            .unwrap()
            .block
            .set_local_light(hytale_world_light_values(packed, 0, 2, 0));
        simulation.force_random_tick(&mut world, 0, 1, 0);
        assert_eq!(world.get_block(1, 1, 0), 1);
        // Covered by an opaque cube: revert.
        world.set_block(0, 2, 0, 3).unwrap();
        simulation.force_random_tick(&mut world, 0, 1, 0);
        assert_eq!(world.get_block(0, 1, 0), 3);
    }

    #[test]
    fn weighted_place_block_chooses_a_configured_candidate() {
        let mut source = cube();
        source.random_tick = Some(RandomTickProcedure::PlaceBlock {
            offset: Dir::UP,
            rules: vec![PlacementRule {
                state: None,
                blocks: vec![
                    WeightedBlock {
                        block: 2,
                        weight: 1.0,
                    },
                    WeightedBlock {
                        block: 3,
                        weight: 3.0,
                    },
                ],
            }],
            require_empty_target: true,
        });
        let mut world = world_with_cube();
        world.set_block(0, 1, 0, 1).unwrap();
        let tables = SimulationTables::from_parts(
            vec![None, Some(source), Some(cube()), Some(cube()), Some(cube())],
            Vec::new(),
        );
        let mut simulation = BlockSimulation::new(tables, 23);
        simulation.force_random_tick(&mut world, 0, 1, 0);
        let placed = world.get_block(0, 2, 0);
        assert!(placed == 2 || placed == 3, "placed {placed}");
        assert_ne!(world.get_block(0, 2, 0), 0);
    }

    fn hytale_world_light_values(
        packed: u16,
        x: i32,
        y: i32,
        z: i32,
    ) -> crate::chunk::section::light::LightData {
        let mut values = vec![0u16; crate::chunk::section::light::LIGHT_BLOCK_COUNT];
        values[index(x, y, z)] = packed;
        crate::chunk::section::light::LightData::from_values(values, 0)
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
