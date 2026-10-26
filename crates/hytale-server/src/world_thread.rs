//! One world thread: owns the save, the generator and the demand queue.
//!
//! Chunk loads/generations run on the world thread. `SetView` replaces the
//! pending queue (cancelling loads that are no longer inside the view), unloads
//! chunks that left it, and orders the remaining work nearest-first. `Tick`
//! advances a bounded number of chunks so a live server can interleave other
//! world work between loads.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use hytale_assets::{AssetPack, CommonAssetRegistry};
use hytale_entities::damage::{Damage, DamageCause, DamageCauses, DamageSource};
use hytale_entities::movement::{resolve_target, PlayerShape, TargetResolution};
use hytale_interactions::{InteractionCatalog, InteractionError};
use hytale_inventory::{ItemStack, PlayerInventory};
use hytale_npc::block_set::BlockSetCatalog;
use hytale_npc::combat::{NpcActor, NpcTarget, NpcTickReport};
use hytale_npc::flock::FlockRng;
use hytale_npc::role::RoleCatalog;
use hytale_npc::spawn::{WorldSpawnCatalog, WorldSpawnController};
use hytale_npc::spawn_position::{SpawnMovementMode, SpawnPositionSearch, SpawnSearchOptions};
use hytale_protocol::generated::common::{
    BlockDamageCmd, BlockMaterial, BlockPosition, FlyMode, InventorySection, MapImage,
    MovementSettings,
};
use hytale_protocol::generated::world::WorldGameplayConfig;
use hytale_protocol::generated::world::{UpdateBlockDamage, UpdateBlockDamages};
use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::chunk::stream::ChunkPacketBatch;
use hytale_world::collision::{BlockCollisionTable, Vec3, WorldCollision};
use hytale_world::player_data::{PlayerAppearance, PlayerDataError, PlayerTransform};
use hytale_world::prefab::{paste_prefab, PrefabPasteReport};
use hytale_world::runtime::{
    transform, BlockComponent, RuntimeEntity, RuntimeEntityId, RuntimeEntityKind,
};
use hytale_world::simulation::{BlockDrop, BlockSimulation, SimulationReport, SimulationTables};
use hytale_world::view::ChunkView;
use hytale_world::weather::{environment_forecasts, EnvironmentForecast, WeatherState};
use hytale_world::world::{WorldError, WorldSave};
use hytale_world::world_clock::{format_iso8601, parse_iso8601, WorldClock, WorldClockSettings};
use hytale_world::world_config::{WorldConfigError, WorldUuid};
use thiserror::Error;
use worldgen_hytale::prefab::prefab_rotation::PrefabRotation;

use crate::generation::{GenerationError, GeneratorOptions, WorldGenerator};
use crate::map_image;

/// The world clock snapshot sent on join (`WorldTimeState` keeps its previous
/// name for session compatibility).
pub use hytale_world::world_clock::WorldClockState as WorldTimeState;

/// Java player model `PhysicsValues.Mass` (`Server/Models/Human/Player.json`).
pub const DEFAULT_PLAYER_MASS: f64 = 68.0;
/// Java `PhysicsValues.DEFAULT_DRAG_COEFFICIENT`.
pub const DEFAULT_PLAYER_DRAG_COEFFICIENT: f64 = 0.5;

/// All block components as `(position, component)` pairs, coordinate ordered.
pub type BlockComponentList = Vec<((i32, i32, i32), BlockComponent)>;

/// Paths and settings for a world thread.
#[derive(Debug, Clone)]
pub struct WorldThreadOptions {
    /// Generator boot paths and worker count.
    pub generator: GeneratorOptions,
    /// World save root.
    pub save_dir: PathBuf,
    /// Seed used only when a new config is created.
    pub seed: i64,
    /// Display name used only when a new config is created.
    pub display_name: Option<String>,
    /// World UUID used only when a new config is created.
    pub world_uuid: Option<String>,
    /// Player to load or create.
    pub player_uuid: Option<String>,
    /// Initial view radius around the player chunk.
    pub view_radius: u32,
    /// Maximum chunks loaded/generated per `Tick`.
    pub chunks_per_tick: usize,
}

impl WorldThreadOptions {
    /// A 1-chunk-per-tick thread with a radius-0 view.
    pub fn with_defaults(generator: GeneratorOptions, save_dir: impl Into<PathBuf>) -> Self {
        Self {
            generator,
            save_dir: save_dir.into(),
            seed: 0,
            display_name: None,
            world_uuid: None,
            player_uuid: None,
            view_radius: 0,
            chunks_per_tick: 1,
        }
    }
}

/// What the thread opened at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldStartReport {
    /// Save root.
    pub save_dir: PathBuf,
    /// True when the thread created `config.json`.
    pub config_created: bool,
    /// World UUID.
    pub world_uuid: String,
    /// World seed.
    pub seed: i64,
    /// Player UUID.
    pub player_uuid: String,
    /// True when the thread created the player file.
    pub player_created: bool,
    /// Chunks stored on disk at startup.
    pub stored_chunks: usize,
    /// Player chunk used as the view center.
    pub player_chunk: (i32, i32),
    /// View radius.
    pub view_radius: u32,
    /// Chunks queued by the initial view plan.
    pub initial_loads: usize,
    /// Chunks outside the initial view (normally empty at startup).
    pub initial_unloads: usize,
}

/// A chunk finished loading or generating on the world thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldEvent {
    /// A chunk is ready in memory.
    ChunkLoaded {
        /// Chunk x.
        x: i32,
        /// Chunk z.
        z: i32,
        /// True when it came from worldgen rather than a region file.
        generated: bool,
    },
    /// A chunk left the view and was saved/unloaded.
    ChunkUnloaded {
        /// Chunk x.
        x: i32,
        /// Chunk z.
        z: i32,
    },
}

/// Result of applying a new view.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewReport {
    /// Chunks queued for load/generation.
    pub queued: usize,
    /// Chunks unloaded because they left the view.
    pub unloaded: usize,
}

/// Identity of one independently registered chunk view.
///
/// The base view ([`ViewId::BASE`]) is the one driven by [`WorldThread::set_view`]
/// for headless/CLI callers. A listener registers one view per session with
/// [`WorldThread::add_view`], updates it as the player moves and removes it
/// during teardown. Loaded chunks are the union of every live view, so one
/// session leaving cannot unload another session's terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewId(u64);

impl ViewId {
    /// The startup/CLI view.
    pub const BASE: Self = Self(0);

    /// The stable numeric id (0 is the base view).
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Per-player save state loaded for a joining session.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSnapshot {
    /// Saved transform, or the world spawn for a new player.
    pub transform: PlayerTransform,
    /// Saved appearance/equipment with unknown item ids resolved away.
    pub appearance: PlayerAppearance,
    /// Saved inventory components.
    pub inventory: PlayerInventory,
    /// Revealed world-map marker ids.
    pub revealed_markers: BTreeSet<String>,
}

/// One block-health packet, on the `Chunks` stream.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockDamagePacket {
    /// Java `UpdateBlockDamage` (one changed block).
    Single(UpdateBlockDamage),
    /// Java `UpdateBlockDamages` (one 32³ section).
    Section(UpdateBlockDamages),
}

/// One runtime-entity lifecycle event for the session presentation layer.
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEntityEvent {
    /// A new entity is available.
    Spawned(RuntimeEntityId),
    /// An existing entity changed (currently transform-only).
    Updated(RuntimeEntityId),
    /// An entity was removed; the last position filters viewers.
    Despawned {
        /// Removed entity id.
        id: RuntimeEntityId,
        /// Last world position.
        position: hytale_world::player_data::Position,
    },
}

/// One opened block container, ready for an `OpenWindow` packet.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockContainerOpen {
    /// Item key of the block (`window_data.blockItemId`).
    pub block_item_id: String,
    /// Container slot count.
    pub capacity: u16,
    /// Current `InventorySection` contents.
    pub section: InventorySection,
}

/// One environment population target for automatic spawning.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcSpawnTarget {
    /// Environment asset id used to select spawn configurations.
    pub environment: String,
    /// Search center.
    pub center: Vec3,
    /// Horizontal search radius in blocks.
    pub radius: i32,
    /// Movement mode used by the position search.
    pub mode: SpawnMovementMode,
    /// Environment spawn density.
    pub density: f64,
    /// Tracked segment/column count for the density formula. `0` discovers the
    /// count from loaded world sections for `environment`.
    pub segments: usize,
}

/// Result of one automatic population tick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NpcSpawnReport {
    /// NPCs spawned this tick.
    pub spawned: usize,
    /// Targets that could not find a role or position.
    pub failed: usize,
    /// Total tracked live NPCs across targets.
    pub population: i32,
}

/// Result of one NPC runtime tick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NpcRuntimeReport {
    /// Live actors considered.
    pub ticked: usize,
    /// Actors that moved.
    pub moved: usize,
    /// Melee attacks produced.
    pub attacks: usize,
    /// Actors that died and were despawned.
    pub deaths: usize,
    /// Live actors after the tick.
    pub alive: usize,
}

/// Result of one demand-load tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TickReport {
    /// Chunks read from region files.
    pub loaded: usize,
    /// Chunks generated.
    pub generated: usize,
    /// Chunks still pending.
    pub remaining: usize,
}

/// What an explicit chunk load did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// The chunk was already in memory.
    AlreadyLoaded,
    /// The chunk was read from a region file.
    Loaded,
    /// The chunk was generated.
    Generated,
}

/// Errors from the world thread.
#[derive(Debug, Error)]
pub enum WorldThreadError {
    /// The generator boot failed.
    #[error(transparent)]
    Generation(#[from] GenerationError),
    /// The world/config/player layer failed.
    #[error(transparent)]
    World(#[from] WorldError),
    /// The config codec failed.
    #[error(transparent)]
    Config(#[from] WorldConfigError),
    /// A player file failed.
    #[error(transparent)]
    Player(#[from] PlayerDataError),
    /// Common-asset index loading failed.
    #[error(transparent)]
    CommonAssets(#[from] hytale_assets::CommonAssetError),
    /// Config asset packet encoding failed.
    #[error(transparent)]
    AssetPackets(#[from] hytale_assets::AssetPacketError),
    /// Interaction assets failed to load.
    #[error(transparent)]
    Interactions(#[from] InteractionError),
    /// The asset pack could not be opened for NPC role loading.
    #[error(transparent)]
    Assets(#[from] hytale_assets::AssetError),
    /// NPC role assets failed to load.
    #[error(transparent)]
    Roles(#[from] hytale_npc::role::RoleError),
    /// An NPC actor could not be built.
    #[error(transparent)]
    Npc(#[from] hytale_npc::combat::CombatError),
    /// A UUID argument is malformed.
    #[error("invalid UUID `{0}`")]
    InvalidUuid(String),
    /// The command/event channel closed.
    #[error("world thread channel closed")]
    ChannelClosed,
    /// The thread reported a command failure.
    #[error("world thread: {0}")]
    Thread(String),
}

enum Command {
    SetView {
        center_x: i32,
        center_z: i32,
        radius: u32,
        reply: SyncSender<Result<ViewReport, String>>,
    },
    AddView {
        center_x: i32,
        center_z: i32,
        radius: u32,
        reply: SyncSender<Result<ViewId, String>>,
    },
    UpdateView {
        view_id: ViewId,
        center_x: i32,
        center_z: i32,
        radius: u32,
        reply: SyncSender<Result<ViewReport, String>>,
    },
    RemoveView {
        view_id: ViewId,
        reply: SyncSender<Result<ViewReport, String>>,
    },
    Tick {
        reply: SyncSender<Result<TickReport, String>>,
    },
    LoadChunk {
        x: i32,
        z: i32,
        reply: SyncSender<Result<LoadOutcome, String>>,
    },
    ChunkPackets {
        x: i32,
        z: i32,
        full: bool,
        reply: SyncSender<Result<ChunkPacketBatch, String>>,
    },
    LoadedChunks {
        reply: SyncSender<Result<Vec<(i32, i32)>, String>>,
    },
    SetBlock {
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        reply: SyncSender<Result<(i32, i32), String>>,
    },
    PlaceBlock {
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        reply: SyncSender<Result<Option<(i32, i32)>, String>>,
    },
    GetBlock {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<Option<i32>, String>>,
    },
    BlockIdForItem {
        item_id: String,
        reply: SyncSender<Result<Option<i32>, String>>,
    },
    BlockTypeJson {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<Option<(String, serde_json::Value)>, String>>,
    },
    MapChunkImage {
        x: i32,
        z: i32,
        reply: SyncSender<Result<Option<MapImage>, String>>,
    },
    OpenBlockContainer {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<Option<BlockContainerOpen>, String>>,
    },
    SetBlockState {
        x: i32,
        y: i32,
        z: i32,
        state: String,
        reply: SyncSender<Result<Option<(i32, i32)>, String>>,
    },
    InteractionCatalog {
        reply: SyncSender<Arc<InteractionCatalog>>,
    },
    MovementSettings {
        reply: SyncSender<Result<MovementSettings, String>>,
    },
    ResolveMovement {
        from: Vec3,
        to: Vec3,
        crouching: bool,
        step: bool,
        reply: SyncSender<Result<TargetResolution, String>>,
    },
    Simulate {
        dt: f32,
        reply: SyncSender<Result<SimulationReport, String>>,
    },
    SetFluid {
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        level: i8,
        reply: SyncSender<Result<bool, String>>,
    },
    SetLocalLight {
        x: i32,
        y: i32,
        z: i32,
        value: u16,
        reply: SyncSender<Result<bool, String>>,
    },
    GetFluid {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<Option<(i32, i8)>, String>>,
    },
    WeatherAt {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<Option<i32>, String>>,
    },
    BlockDrops {
        block_id: i32,
        reply: SyncSender<Result<Vec<BlockDrop>, String>>,
    },
    FluidId {
        name: String,
        reply: SyncSender<Result<i32, String>>,
    },
    ForceRandomTick {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<SimulationReport, String>>,
    },
    DamageBlock {
        x: i32,
        y: i32,
        z: i32,
        amount: f32,
        reply: SyncSender<Result<f32, String>>,
    },
    BlockHealth {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<f32, String>>,
    },
    BlockDamagePackets {
        chunk: Option<(i32, i32)>,
        reply: SyncSender<Result<Vec<BlockDamagePacket>, String>>,
    },
    BlockDamagePacketsForDeltas {
        deltas: Vec<hytale_world::simulation::BlockHealthDelta>,
        reply: SyncSender<Result<Vec<BlockDamagePacket>, String>>,
    },
    TakeBlockHealthDelta {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<Option<f32>, String>>,
    },
    Entity {
        id: RuntimeEntityId,
        reply: SyncSender<Result<Option<RuntimeEntity>, String>>,
    },
    EntityEvents {
        reply: SyncSender<Vec<RuntimeEntityEvent>>,
    },
    SpawnNpc {
        role: String,
        transform: PlayerTransform,
        reply: SyncSender<Result<RuntimeEntity, String>>,
    },
    SpawnNpcNear {
        role: String,
        center: Vec3,
        radius: i32,
        mode: SpawnMovementMode,
        reply: SyncSender<Result<Option<RuntimeEntity>, String>>,
    },
    SpawnNpcInEnvironment {
        environment: String,
        center: Vec3,
        radius: i32,
        mode: SpawnMovementMode,
        reply: SyncSender<Result<Option<RuntimeEntity>, String>>,
    },
    DespawnNpc {
        id: RuntimeEntityId,
        reply: SyncSender<Result<Option<RuntimeEntity>, String>>,
    },
    SetNpcTargets {
        targets: Vec<NpcTarget>,
        reply: SyncSender<()>,
    },
    SetNpcSpawnTargets {
        targets: Vec<NpcSpawnTarget>,
        reply: SyncSender<()>,
    },
    TickNpcSpawns {
        reply: SyncSender<Result<NpcSpawnReport, String>>,
    },
    TickNpcs {
        reply: SyncSender<Result<NpcRuntimeReport, String>>,
    },
    DamageNpc {
        id: RuntimeEntityId,
        amount: f32,
        cause: String,
        reply: SyncSender<Result<Option<hytale_entities::damage::DamageResult>, String>>,
    },
    NpcActors {
        reply: SyncSender<Vec<(RuntimeEntityId, NpcActor)>>,
    },
    UpdateEntityTransform {
        id: RuntimeEntityId,
        transform: PlayerTransform,
        reply: SyncSender<Result<bool, String>>,
    },
    PlacePrefab {
        name: String,
        x: i32,
        y: i32,
        z: i32,
        rotation: u16,
        reply: SyncSender<Result<PrefabPasteReport, String>>,
    },
    Entities {
        reply: SyncSender<Result<Vec<RuntimeEntity>, String>>,
    },
    DespawnEntity {
        id: RuntimeEntityId,
        reply: SyncSender<Result<Option<RuntimeEntity>, String>>,
    },
    SpawnItemEntity {
        item_id: String,
        quantity: i32,
        x: f64,
        y: f64,
        z: f64,
        reply: SyncSender<Result<RuntimeEntity, String>>,
    },
    SpawnItemStackEntity {
        stack: ItemStack,
        x: f64,
        y: f64,
        z: f64,
        reply: SyncSender<Result<RuntimeEntity, String>>,
    },
    BlockComponent {
        x: i32,
        y: i32,
        z: i32,
        reply: SyncSender<Result<Option<BlockComponent>, String>>,
    },
    BlockComponents {
        reply: SyncSender<Result<BlockComponentList, String>>,
    },
    Save {
        reply: SyncSender<Result<usize, String>>,
    },
    ConfigAssetPackets {
        reply: SyncSender<Result<Arc<hytale_assets::ConfigAssets>, String>>,
    },
    CommonAssets {
        reply: SyncSender<Result<CommonAssetRegistry, String>>,
    },
    PlayerTransform {
        reply: SyncSender<Result<Option<PlayerTransform>, String>>,
    },
    PlayerSnapshot {
        uuid: WorldUuid,
        reply: SyncSender<Result<PlayerSnapshot, String>>,
    },
    SavePlayerTransform {
        uuid: WorldUuid,
        transform: PlayerTransform,
        reply: SyncSender<Result<(), String>>,
    },
    SavePlayerState {
        uuid: WorldUuid,
        transform: PlayerTransform,
        inventory: Box<PlayerInventory>,
        reply: SyncSender<Result<(), String>>,
    },
    SavePlayerRevealedMarkers {
        uuid: WorldUuid,
        markers: BTreeSet<String>,
        reply: SyncSender<Result<(), String>>,
    },
    PlayerAppearance {
        reply: SyncSender<Result<PlayerAppearance, String>>,
    },
    WorldTime {
        reply: SyncSender<Result<WorldTimeState, String>>,
    },
    GameplayConfig {
        reply: SyncSender<Result<WorldGameplayConfig, String>>,
    },
    SetTimePaused {
        paused: bool,
        reply: SyncSender<Result<WorldTimeState, String>>,
    },
    AdvanceTime {
        seconds: f64,
        reply: SyncSender<Result<WorldTimeState, String>>,
    },
    Shutdown {
        reply: SyncSender<Result<usize, String>>,
    },
}

/// Handle to one world thread.
pub struct WorldThread {
    commands: Sender<Command>,
    // `std::sync::mpsc::Receiver` is `Send` but not `Sync`; the mutex keeps
    // `&WorldThread` usable from async connection tasks.
    events: std::sync::Mutex<Receiver<WorldEvent>>,
    join: Option<JoinHandle<()>>,
    start: WorldStartReport,
}

impl WorldThread {
    /// Spawn the thread, open the save/generator and apply the initial view.
    pub fn spawn(
        options: WorldThreadOptions,
    ) -> Result<(Self, WorldStartReport), WorldThreadError> {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);

        let handle = thread::Builder::new()
            .name("hytale-world".to_string())
            .spawn(move || run(options, command_rx, event_tx, ready_tx))
            .map_err(|error| WorldThreadError::Thread(error.to_string()))?;

        match ready_rx.recv() {
            Ok(Ok(start)) => Ok((
                Self {
                    commands: command_tx,
                    events: std::sync::Mutex::new(event_rx),
                    join: Some(handle),
                    start: start.clone(),
                },
                start,
            )),
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => {
                let _ = handle.join();
                Err(WorldThreadError::ChannelClosed)
            }
        }
    }

    /// The startup report.
    pub fn start_report(&self) -> &WorldStartReport {
        &self.start
    }

    /// Replace the view; queued chunks outside it are cancelled and loaded
    /// chunks outside it are unloaded.
    pub fn set_view(
        &self,
        center_x: i32,
        center_z: i32,
        radius: u32,
    ) -> Result<ViewReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetView {
                center_x,
                center_z,
                radius,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Register an independent view for one session.
    ///
    /// The first registered session view replaces the startup base view, so a
    /// served world's loaded chunks are owned entirely by live sessions.
    pub fn add_view(
        &self,
        center_x: i32,
        center_z: i32,
        radius: u32,
    ) -> Result<ViewId, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::AddView {
                center_x,
                center_z,
                radius,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Move/restretch one registered view.
    pub fn update_view(
        &self,
        view_id: ViewId,
        center_x: i32,
        center_z: i32,
        radius: u32,
    ) -> Result<ViewReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::UpdateView {
                view_id,
                center_x,
                center_z,
                radius,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Remove one session view and unload chunks no other view keeps alive.
    pub fn remove_view(&self, view_id: ViewId) -> Result<ViewReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::RemoveView { view_id, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Process up to `chunks_per_tick` queued chunks.
    pub fn tick(&self) -> Result<TickReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Tick { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Load or generate one chunk outside the current view.
    pub fn load_chunk(&self, x: i32, z: i32) -> Result<LoadOutcome, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::LoadChunk { x, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Encode the chunk packets for a loaded chunk.
    ///
    /// `full == false` is the join set (`SetColumn` + section packets);
    /// `full == true` adds the tint and fluid update packets.
    pub fn chunk_packets(
        &self,
        x: i32,
        z: i32,
        full: bool,
    ) -> Result<ChunkPacketBatch, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::ChunkPackets { x, z, full, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Coordinates of every chunk currently in memory.
    pub fn loaded_chunks(&self) -> Result<Vec<(i32, i32)>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::LoadedChunks { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Set a block in a loaded chunk; returns `(before, after)`.
    pub fn set_block(
        &self,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
    ) -> Result<(i32, i32), WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetBlock {
                x,
                y,
                z,
                block_id,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Place `block_id` only when the target block is empty.
    ///
    /// Returns `(before, after)` when applied and `None` when the target was
    /// occupied or `block_id` was not positive. A missing loaded chunk is an
    /// error.
    pub fn place_block(
        &self,
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
    ) -> Result<Option<(i32, i32)>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::PlaceBlock {
                x,
                y,
                z,
                block_id,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Read a block from a loaded chunk, or `None` when the chunk is not
    /// loaded.
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Result<Option<i32>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::GetBlock { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Resolve an item id to the block id it places, if it is a block item.
    pub fn block_id_for_item(&self, item_id: &str) -> Result<Option<i32>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockIdForItem {
                item_id: item_id.to_string(),
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// The block type key and merged JSON at a loaded block position.
    pub fn block_type_json(
        &self,
        x: i32,
        y: i32,
        z: i32,
    ) -> Result<Option<(String, serde_json::Value)>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockTypeJson { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Build (or take from the per-world cache) the world-map image of a
    /// loaded chunk. `None` when the chunk is not loaded.
    pub fn map_chunk_image(&self, x: i32, z: i32) -> Result<Option<MapImage>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::MapChunkImage { x, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Open a block container and return its current inventory section.
    ///
    /// `None` means the target is not a container block. The container is
    /// created on first open and persisted in `block-containers.json`.
    pub fn open_block_container(
        &self,
        x: i32,
        y: i32,
        z: i32,
    ) -> Result<Option<BlockContainerOpen>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::OpenBlockContainer { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Change a block to its `State.Definitions.<state>` variant, preserving
    /// rotation/filler. Returns `(before, after)`.
    pub fn set_block_state(
        &self,
        x: i32,
        y: i32,
        z: i32,
        state: &str,
    ) -> Result<Option<(i32, i32)>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetBlockState {
                x,
                y,
                z,
                state: state.to_string(),
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Loaded interaction catalog (shared, immutable).
    pub fn interaction_catalog(&self) -> Result<Arc<InteractionCatalog>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::InteractionCatalog { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response.recv().map_err(|_| WorldThreadError::ChannelClosed)
    }

    /// The selected `MovementConfig` as an `UpdateMovementSettings` payload.
    pub fn movement_settings(&self) -> Result<MovementSettings, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::MovementSettings { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Resolve a client position against the loaded block collision shapes.
    pub fn resolve_movement(
        &self,
        from: Vec3,
        to: Vec3,
        crouching: bool,
        step: bool,
    ) -> Result<TargetResolution, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::ResolveMovement {
                from,
                to,
                crouching,
                step,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Run one block-simulation pass (`dt` seconds, normally 0.05).
    pub fn simulate(&self, dt: f32) -> Result<SimulationReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Simulate { dt, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Set one fluid on a loaded chunk.
    pub fn set_fluid(
        &self,
        x: i32,
        y: i32,
        z: i32,
        fluid_id: i32,
        level: i8,
    ) -> Result<bool, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetFluid {
                x,
                y,
                z,
                fluid_id,
                level,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Set one packed local-light value (test/diagnostic seam for the
    /// `SpreadTo` light gate).
    pub fn set_local_light(
        &self,
        x: i32,
        y: i32,
        z: i32,
        value: u16,
    ) -> Result<bool, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetLocalLight {
                x,
                y,
                z,
                value,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Effective weather index at a block position (`None` when the
    /// environment has no forecast).
    pub fn weather_at(&self, x: i32, y: i32, z: i32) -> Result<Option<i32>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::WeatherAt { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Read one fluid id/level, or `None` when the chunk is not loaded.
    pub fn get_fluid(&self, x: i32, y: i32, z: i32) -> Result<Option<(i32, i8)>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::GetFluid { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Player-breaking drops declared by a block asset.
    pub fn block_drops(&self, block_id: i32) -> Result<Vec<BlockDrop>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockDrops { block_id, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Resolve a fluid asset key to its numeric id.
    pub fn fluid_id(&self, name: &str) -> Result<i32, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::FluidId {
                name: name.to_string(),
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Force one block's random-tick procedure (debug/test seam).
    pub fn force_random_tick(
        &self,
        x: i32,
        y: i32,
        z: i32,
    ) -> Result<SimulationReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::ForceRandomTick { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Apply block-health damage. Returns the remaining health.
    pub fn damage_block(
        &self,
        x: i32,
        y: i32,
        z: i32,
        amount: f32,
    ) -> Result<f32, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::DamageBlock {
                x,
                y,
                z,
                amount,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Remaining block health at one position.
    pub fn block_health(&self, x: i32, y: i32, z: i32) -> Result<f32, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockHealth { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Block-health load packets for one chunk (delta `0`).
    ///
    /// A `None` chunk requests every loaded chunk. Java writes these on the
    /// chunks stream after the chunk's block data.
    pub fn block_damage_packets(
        &self,
        chunk: Option<(i32, i32)>,
    ) -> Result<Vec<BlockDamagePacket>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockDamagePackets { chunk, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Take one block's pending replication delta.
    pub fn take_block_health_delta(
        &self,
        x: i32,
        y: i32,
        z: i32,
    ) -> Result<Option<f32>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::TakeBlockHealthDelta { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Block-health packets for simulation deltas.
    ///
    /// Reads the authoritative health after the simulation step so damage and
    /// regeneration use the same absolute value Java sends.
    pub fn block_damage_packets_for_deltas(
        &self,
        deltas: &[hytale_world::simulation::BlockHealthDelta],
    ) -> Result<Vec<BlockDamagePacket>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockDamagePacketsForDeltas {
                deltas: deltas.to_vec(),
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// One runtime entity by id.
    pub fn entity(&self, id: RuntimeEntityId) -> Result<Option<RuntimeEntity>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Entity { id, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Take the pending runtime entity lifecycle events.
    pub fn runtime_entity_events(&self) -> Result<Vec<RuntimeEntityEvent>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::EntityEvents { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response.recv().map_err(|_| WorldThreadError::ChannelClosed)
    }

    /// Spawn an NPC runtime entity from a role asset key.
    pub fn spawn_npc(
        &self,
        role: impl Into<String>,
        transform: PlayerTransform,
    ) -> Result<RuntimeEntity, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SpawnNpc {
                role: role.into(),
                transform,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Find a valid spawn position near `center` and spawn the role there.
    pub fn spawn_npc_near(
        &self,
        role: impl Into<String>,
        center: Vec3,
        radius: i32,
        mode: SpawnMovementMode,
    ) -> Result<Option<RuntimeEntity>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SpawnNpcNear {
                role: role.into(),
                center,
                radius,
                mode,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Pick an eligible role for `environment` and spawn it at a valid nearby
    /// position.
    pub fn spawn_npc_in_environment(
        &self,
        environment: impl Into<String>,
        center: Vec3,
        radius: i32,
        mode: SpawnMovementMode,
    ) -> Result<Option<RuntimeEntity>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SpawnNpcInEnvironment {
                environment: environment.into(),
                center,
                radius,
                mode,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Despawn an NPC/runtime entity.
    pub fn despawn_npc(
        &self,
        id: RuntimeEntityId,
    ) -> Result<Option<RuntimeEntity>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::DespawnNpc { id, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Replace the live NPC target list (player/NPC positions).
    pub fn set_npc_targets(&self, targets: Vec<NpcTarget>) -> Result<(), WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetNpcTargets { targets, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        Ok(())
    }

    /// Replace the automatic environment population targets.
    pub fn set_npc_spawn_targets(
        &self,
        targets: Vec<NpcSpawnTarget>,
    ) -> Result<(), WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetNpcSpawnTargets { targets, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        Ok(())
    }

    /// Spawn toward the configured environment populations.
    pub fn tick_npc_spawns(&self) -> Result<NpcSpawnReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::TickNpcSpawns { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Advance every live NPC by the elapsed wall-clock time.
    pub fn tick_npcs(&self) -> Result<NpcRuntimeReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::TickNpcs { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Apply damage to one NPC actor.
    pub fn damage_npc(
        &self,
        id: RuntimeEntityId,
        amount: f32,
        cause: impl Into<String>,
    ) -> Result<Option<hytale_entities::damage::DamageResult>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::DamageNpc {
                id,
                amount,
                cause: cause.into(),
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Live NPC actors (debug/tests).
    pub fn npc_actors(&self) -> Result<Vec<(RuntimeEntityId, NpcActor)>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::NpcActors { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response.recv().map_err(|_| WorldThreadError::ChannelClosed)
    }

    /// Move a runtime entity and queue an update event for viewers.
    pub fn update_entity_transform(
        &self,
        id: RuntimeEntityId,
        transform: PlayerTransform,
    ) -> Result<bool, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::UpdateEntityTransform {
                id,
                transform,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Paste a prefab from the asset pack at an origin.
    ///
    /// `rotation` is the quarter-turn degrees (`0`, `90`, `180`, `270`). The
    /// world thread loads/generates the chunks the prefab touches first, then
    /// writes the blocks/fluids/components and spawns its entities.
    pub fn place_prefab(
        &self,
        name: impl Into<String>,
        x: i32,
        y: i32,
        z: i32,
        rotation: u16,
    ) -> Result<PrefabPasteReport, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::PlacePrefab {
                name: name.into(),
                x,
                y,
                z,
                rotation,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Every runtime entity (prefabs, item drops, projectiles).
    pub fn entities(&self) -> Result<Vec<RuntimeEntity>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Entities { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Remove one runtime entity.
    pub fn despawn_entity(
        &self,
        id: RuntimeEntityId,
    ) -> Result<Option<RuntimeEntity>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::DespawnEntity { id, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Spawn an item entity at a world position.
    pub fn spawn_item_entity(
        &self,
        item_id: impl Into<String>,
        quantity: i32,
        x: f64,
        y: f64,
        z: f64,
    ) -> Result<RuntimeEntity, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SpawnItemEntity {
                item_id: item_id.into(),
                quantity,
                x,
                y,
                z,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Spawn an item entity carrying the dropped stack's metadata.
    pub fn spawn_item_stack_entity(
        &self,
        stack: ItemStack,
        x: f64,
        y: f64,
        z: f64,
    ) -> Result<RuntimeEntity, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SpawnItemStackEntity {
                stack,
                x,
                y,
                z,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// One block's component document.
    pub fn block_component(
        &self,
        x: i32,
        y: i32,
        z: i32,
    ) -> Result<Option<BlockComponent>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockComponent { x, y, z, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Every block component, in coordinate order.
    pub fn block_components(&self) -> Result<BlockComponentList, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::BlockComponents { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Save dirty chunks, runtime entities, block components and the config.
    pub fn save(&self) -> Result<usize, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Save { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Take one queued event, if any.
    pub fn try_event(&self) -> Option<WorldEvent> {
        self.events
            .lock()
            .expect("world thread event receiver poisoned")
            .try_recv()
            .ok()
    }

    /// The config asset packets and tag map the session sends during setup.
    ///
    /// The payload is encoded once when the world starts and shared by every
    /// connection.
    pub fn config_asset_packets(
        &self,
    ) -> Result<Arc<hytale_assets::ConfigAssets>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::ConfigAssetPackets { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Load the pack's common-asset index (metadata only; blobs stay lazy).
    pub fn common_assets(&self) -> Result<CommonAssetRegistry, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::CommonAssets { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// The loaded player's position/rotation, or `None` when the save has no
    /// transform component.
    pub fn player_transform(&self) -> Result<Option<PlayerTransform>, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::PlayerTransform { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Load or create one player's transform/appearance for a joining session.
    ///
    /// Unlike [`Self::player_transform`], this is keyed by the session's UUID, so
    /// reconnecting players resume their own saved position and equipment.
    pub fn player_snapshot(&self, uuid: WorldUuid) -> Result<PlayerSnapshot, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::PlayerSnapshot { uuid, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Persist one player's live transform, preserving every other component.
    pub fn save_player_transform(
        &self,
        uuid: WorldUuid,
        transform: PlayerTransform,
    ) -> Result<(), WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SavePlayerTransform {
                uuid,
                transform,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Persist the revealed world-map marker ids for one player.
    pub fn save_player_revealed_markers(
        &self,
        uuid: WorldUuid,
        markers: BTreeSet<String>,
    ) -> Result<(), WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SavePlayerRevealedMarkers {
                uuid,
                markers,
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Persist the live transform and inventory together, preserving every
    /// other saved component.
    pub fn save_player_state(
        &self,
        uuid: WorldUuid,
        transform: PlayerTransform,
        inventory: PlayerInventory,
    ) -> Result<(), WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SavePlayerState {
                uuid,
                transform,
                inventory: Box::new(inventory),
                reply,
            })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// The loaded player's saved model, skin, armor and active held items.
    ///
    /// Unknown item ids are already dropped against the world's asset
    /// registry; the self-spawn falls back to Java's empty values.
    pub fn player_appearance(&self) -> Result<PlayerAppearance, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::PlayerAppearance { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// The world clock snapshot sent on join.
    pub fn world_time(&self) -> Result<WorldTimeState, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::WorldTime { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// The `WorldGameplayConfig` packet built from the world's loaded
    /// `GameplayConfig` asset (or the Java defaults).
    pub fn gameplay_config(&self) -> Result<WorldGameplayConfig, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::GameplayConfig { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Pause or resume world time and persist the flag in the world config.
    pub fn set_time_paused(&self, paused: bool) -> Result<WorldTimeState, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::SetTimePaused { paused, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Advance the clock by `seconds` of real time (test/debug seam).
    pub fn advance_time(&self, seconds: f64) -> Result<WorldTimeState, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::AdvanceTime { seconds, reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)
    }

    /// Save, stop the generator and join the thread.
    pub fn shutdown(mut self) -> Result<usize, WorldThreadError> {
        let (reply, response) = mpsc::sync_channel(1);
        self.commands
            .send(Command::Shutdown { reply })
            .map_err(|_| WorldThreadError::ChannelClosed)?;
        let saved = response
            .recv()
            .map_err(|_| WorldThreadError::ChannelClosed)?
            .map_err(WorldThreadError::Thread)?;
        if let Some(handle) = self.join.take() {
            handle
                .join()
                .map_err(|error| WorldThreadError::Thread(format!("thread panicked: {error:?}")))?;
        }
        Ok(saved)
    }
}

impl Drop for WorldThread {
    fn drop(&mut self) {
        if let Some(handle) = self.join.take() {
            let (reply, _response) = mpsc::sync_channel(1);
            let _ = self.commands.send(Command::Shutdown { reply });
            let _ = handle.join();
        }
    }
}

fn run(
    options: WorldThreadOptions,
    commands: Receiver<Command>,
    events: Sender<WorldEvent>,
    ready: SyncSender<Result<WorldStartReport, WorldThreadError>>,
) {
    match start(&options) {
        Ok(Started {
            mut save,
            generator,
            mut pending,
            start,
            player_transform,
            player_appearance,
            mut clock,
            gameplay_config,
            movement_settings,
            collision,
            mut simulation,
            mut weather,
            weather_forecasts,
            config_assets,
            interactions,
            mut roles,
            mut spawns,
            mut block_sets,
            damage_causes,
            mut npcs,
            mut last_npc_tick,
        }) => {
            if ready.send(Ok(start.clone())).is_err() {
                generator.shutdown();
                return;
            }
            let seed = save.config().seed as i32;
            let mut entity_events: Vec<RuntimeEntityEvent> = Vec::new();
            let mut npc_targets: Vec<NpcTarget> = Vec::new();
            let mut npc_spawn_targets: Vec<NpcSpawnTarget> = Vec::new();
            let mut npc_spawn_controllers: BTreeMap<String, WorldSpawnController> = BTreeMap::new();
            let mut spawn_rng = SpawnRng::new(u64::from_le_bytes(save.config().seed.to_le_bytes()));
            let mut map_images: BTreeMap<(i32, i32), MapImage> = BTreeMap::new();
            let budget = options.chunks_per_tick.max(1);
            let mut last_clock = Instant::now();
            let mut views = BTreeMap::from([(
                ViewId::BASE,
                ChunkView::new(
                    start.player_chunk.0,
                    start.player_chunk.1,
                    options.view_radius,
                ),
            )]);
            let mut next_view_id = 1u64;
            for command in commands {
                match command {
                    Command::SetView {
                        center_x,
                        center_z,
                        radius,
                        reply,
                    } => {
                        views.insert(ViewId::BASE, ChunkView::new(center_x, center_z, radius));
                        let result =
                            apply_views(&mut save, &generator, &events, &views, &mut pending, true);
                        let _ = reply.send(result);
                    }
                    Command::AddView {
                        center_x,
                        center_z,
                        radius,
                        reply,
                    } => {
                        // The first session-owned view takes over from the
                        // startup base view so teardown can unload everything.
                        if views.len() == 1 && views.contains_key(&ViewId::BASE) {
                            views.remove(&ViewId::BASE);
                        }
                        let view_id = ViewId(next_view_id);
                        next_view_id = if next_view_id == u64::MAX {
                            1
                        } else {
                            next_view_id + 1
                        };
                        views.insert(view_id, ChunkView::new(center_x, center_z, radius));
                        let result = apply_views(
                            &mut save,
                            &generator,
                            &events,
                            &views,
                            &mut pending,
                            false,
                        )
                        .map(|_| view_id);
                        let _ = reply.send(result);
                    }
                    Command::UpdateView {
                        view_id,
                        center_x,
                        center_z,
                        radius,
                        reply,
                    } => {
                        let result = if let Some(view) = views.get_mut(&view_id) {
                            *view = ChunkView::new(center_x, center_z, radius);
                            apply_views(&mut save, &generator, &events, &views, &mut pending, true)
                        } else {
                            Err(format!("view {} is not registered", view_id.get()))
                        };
                        let _ = reply.send(result);
                    }
                    Command::RemoveView { view_id, reply } => {
                        let result = if views.remove(&view_id).is_some() {
                            // A leaving client does not need unload packets, and
                            // chunks outside every remaining view are invisible
                            // to the other sessions too.
                            apply_views(&mut save, &generator, &events, &views, &mut pending, false)
                        } else {
                            Ok(ViewReport::default())
                        };
                        let _ = reply.send(result);
                    }
                    Command::Tick { reply } => {
                        advance_clock(&mut clock, &mut last_clock);
                        let result =
                            tick(&mut save, &generator, &events, seed, budget, &mut pending);
                        let _ = reply.send(result);
                    }
                    Command::LoadChunk { x, z, reply } => {
                        let result = load_chunk(&mut save, &generator, &events, seed, x, z);
                        let _ = reply.send(result);
                    }
                    Command::ChunkPackets { x, z, full, reply } => {
                        let result = chunk_packets(&save, x, z, full);
                        let _ = reply.send(result);
                    }
                    Command::LoadedChunks { reply } => {
                        let _ = reply.send(Ok(save.world().loaded_chunk_coordinates()));
                    }
                    Command::SetBlock {
                        x,
                        y,
                        z,
                        block_id,
                        reply,
                    } => {
                        let result = set_block(&mut save, x, y, z, block_id);
                        map_images.remove(&(x >> 5, z >> 5));
                        let _ = reply.send(result);
                    }
                    Command::PlaceBlock {
                        x,
                        y,
                        z,
                        block_id,
                        reply,
                    } => {
                        let result = place_block(&mut save, x, y, z, block_id);
                        map_images.remove(&(x >> 5, z >> 5));
                        let _ = reply.send(result);
                    }
                    Command::GetBlock { x, y, z, reply } => {
                        let result = if save.world().chunk(x >> 5, z >> 5).is_some() {
                            Ok(Some(save.world().get_block(x, y, z)))
                        } else {
                            Ok(None)
                        };
                        let _ = reply.send(result);
                    }
                    Command::BlockIdForItem { item_id, reply } => {
                        let result = block_id_for_item(generator.assets(), &item_id);
                        let _ = reply.send(result);
                    }
                    Command::BlockTypeJson { x, y, z, reply } => {
                        let result = block_type_json(&save, &generator, x, y, z);
                        let _ = reply.send(result);
                    }
                    Command::MapChunkImage { x, z, reply } => {
                        let result = if let Some(image) = map_images.get(&(x, z)) {
                            Ok(Some(image.clone()))
                        } else if save.world().chunk(x, z).is_some() {
                            let image =
                                map_image::render_chunk(save.world(), generator.assets(), x, z);
                            if let Some(image) = &image {
                                map_images.insert((x, z), image.clone());
                            }
                            Ok(image)
                        } else {
                            Ok(None)
                        };
                        let _ = reply.send(result);
                    }
                    Command::OpenBlockContainer { x, y, z, reply } => {
                        let result = open_block_container(&mut save, &generator, x, y, z);
                        let _ = reply.send(result);
                    }
                    Command::SetBlockState {
                        x,
                        y,
                        z,
                        state,
                        reply,
                    } => {
                        let result = set_block_state(&mut save, &generator, x, y, z, &state);
                        map_images.remove(&(x >> 5, z >> 5));
                        let _ = reply.send(result);
                    }
                    Command::InteractionCatalog { reply } => {
                        let _ = reply.send(Arc::clone(&interactions));
                    }
                    Command::MovementSettings { reply } => {
                        let _ = reply.send(Ok(movement_settings.clone()));
                    }
                    Command::ResolveMovement {
                        from,
                        to,
                        crouching,
                        step,
                        reply,
                    } => {
                        let world = WorldCollision::new(save.world(), &collision);
                        let result = resolve_target(
                            &world,
                            from,
                            to,
                            PlayerShape::default(),
                            crouching,
                            if step { 1.0 } else { 0.0 },
                        );
                        let _ = reply.send(Ok(result));
                    }
                    Command::Simulate { dt, reply } => {
                        simulation.set_sunlight_factor(clock.sunlight_factor());
                        let mut result = simulation.tick(save.world_mut(), dt);
                        result.spawned_entities = spawn_simulation_drops(save.world_mut(), &result);
                        if !result.block_changes.is_empty() || !result.fluid_changes.is_empty() {
                            map_images.clear();
                        }
                        let _ = reply.send(Ok(result));
                    }
                    Command::SetFluid {
                        x,
                        y,
                        z,
                        fluid_id,
                        level,
                        reply,
                    } => {
                        let result = save
                            .world_mut()
                            .set_fluid(x, y, z, fluid_id, level)
                            .map_err(|error| error.to_string());
                        map_images.remove(&(x >> 5, z >> 5));
                        let _ = reply.send(result);
                    }
                    Command::WeatherAt { x, y, z, reply } => {
                        let hour = clock.current_hour();
                        weather.update(
                            hour,
                            clock.game_time().seconds,
                            save.config().seed,
                            &weather_forecasts,
                        );
                        let environment = save.world().get_environment(x, y, z);
                        let _ = reply.send(Ok(weather.effective_weather(environment)));
                    }
                    Command::SetLocalLight {
                        x,
                        y,
                        z,
                        value,
                        reply,
                    } => {
                        let result = save
                            .world_mut()
                            .chunk_mut(x >> 5, z >> 5)
                            .and_then(|column| column.ensure_section(y))
                            .map(|section| {
                                let index =
                                    (((y & 31) << 10) | ((z & 31) << 5) | (x & 31)) as usize;
                                let mut light = section.block.local_light().clone();
                                light.set(index, value);
                                section.block.set_local_light(light);
                                true
                            })
                            .unwrap_or(false);
                        let _ = reply.send(Ok(result));
                    }
                    Command::GetFluid { x, y, z, reply } => {
                        let result = if save.world().chunk(x >> 5, z >> 5).is_some() {
                            Ok(Some((
                                save.world().get_fluid_id(x, y, z),
                                save.world().get_fluid_level(x, y, z),
                            )))
                        } else {
                            Ok(None)
                        };
                        let _ = reply.send(result);
                    }
                    Command::BlockDrops { block_id, reply } => {
                        let _ = reply.send(Ok(simulation.tables().breaking_drops(block_id)));
                    }
                    Command::FluidId { name, reply } => {
                        let result = generator
                            .assets()
                            .fluids
                            .id_of(&name)
                            .ok_or_else(|| format!("unknown fluid `{name}`"));
                        let _ = reply.send(result);
                    }
                    Command::ForceRandomTick { x, y, z, reply } => {
                        let result = simulation.force_random_tick(save.world_mut(), x, y, z);
                        let _ = reply.send(Ok(result));
                    }
                    Command::DamageBlock {
                        x,
                        y,
                        z,
                        amount,
                        reply,
                    } => {
                        let _ = reply.send(Ok(save.world_mut().damage_block(x, y, z, amount)));
                    }
                    Command::BlockHealth { x, y, z, reply } => {
                        let _ = reply.send(Ok(save.world().block_health(x, y, z)));
                    }
                    Command::BlockDamagePackets { chunk, reply } => {
                        let entries = match chunk {
                            Some((x, z)) => save.world().block_health_entries_in_chunk(x, z),
                            None => save.world().block_health_entries(),
                        };
                        let _ = reply.send(Ok(group_block_damage(
                            entries
                                .into_iter()
                                .map(|(x, y, z, health)| (x, y, z, health, 0.0)),
                        )));
                    }
                    Command::TakeBlockHealthDelta { x, y, z, reply } => {
                        let _ = reply.send(Ok(save.world_mut().take_block_health_delta(x, y, z)));
                    }
                    Command::BlockDamagePacketsForDeltas { deltas, reply } => {
                        let entries = deltas.iter().map(|delta| {
                            (
                                delta.x,
                                delta.y,
                                delta.z,
                                save.world().block_health(delta.x, delta.y, delta.z),
                                delta.delta,
                            )
                        });
                        let _ = reply.send(Ok(group_block_damage(entries)));
                    }
                    Command::Entity { id, reply } => {
                        let _ = reply.send(Ok(save.world().entity(id).cloned()));
                    }
                    Command::PlacePrefab {
                        name,
                        x,
                        y,
                        z,
                        rotation,
                        reply,
                    } => {
                        let result = place_prefab(
                            &mut save,
                            &generator,
                            &events,
                            seed,
                            &name,
                            (x, y, z),
                            rotation,
                        );
                        if let Ok(report) = &result {
                            entity_events.extend(
                                report
                                    .entities
                                    .iter()
                                    .map(|id| RuntimeEntityEvent::Spawned(*id)),
                            );
                        }
                        if result.is_ok() {
                            map_images.clear();
                        }
                        let _ = reply.send(result);
                    }
                    Command::Entities { reply } => {
                        let entities = save.world().entities().iter().cloned().collect();
                        let _ = reply.send(Ok(entities));
                    }
                    Command::DespawnEntity { id, reply } => {
                        let removed = save.world_mut().despawn_entity(id);
                        if let Some(entity) = &removed {
                            entity_events.push(RuntimeEntityEvent::Despawned {
                                id,
                                position: entity.transform.position,
                            });
                        }
                        let _ = reply.send(Ok(removed));
                    }
                    Command::EntityEvents { reply } => {
                        let _ = reply.send(std::mem::take(&mut entity_events));
                    }
                    Command::SpawnNpc {
                        role,
                        transform,
                        reply,
                    } => {
                        let result = (|| {
                            let role_catalog =
                                load_npc_roles(&mut roles, &options.generator.assets_pack)?;
                            let entity = spawn_npc_entity(
                                &mut save,
                                generator.assets(),
                                role_catalog,
                                &role,
                                transform,
                                &mut npcs,
                            )?;
                            entity_events.push(RuntimeEntityEvent::Spawned(entity.id));
                            Ok(entity)
                        })();
                        let _ = reply.send(result);
                    }
                    Command::SpawnNpcNear {
                        role,
                        center,
                        radius,
                        mode,
                        reply,
                    } => {
                        let result = (|| {
                            let role_catalog =
                                load_npc_roles(&mut roles, &options.generator.assets_pack)?;
                            let is_solid = npc_solid_rule(generator.assets());
                            let position = SpawnPositionSearch::new(save.world(), is_solid).find(
                                center,
                                &SpawnSearchOptions {
                                    radius,
                                    vertical_range: 16,
                                    mode,
                                    ..SpawnSearchOptions::default()
                                },
                            );
                            let Some(position) = position else {
                                return Ok(None);
                            };
                            let entity = spawn_npc_entity(
                                &mut save,
                                generator.assets(),
                                role_catalog,
                                &role,
                                npc_transform(position.position),
                                &mut npcs,
                            )?;
                            entity_events.push(RuntimeEntityEvent::Spawned(entity.id));
                            Ok(Some(entity))
                        })();
                        let _ = reply.send(result);
                    }
                    Command::SpawnNpcInEnvironment {
                        environment,
                        center,
                        radius,
                        mode,
                        reply,
                    } => {
                        let result = (|| {
                            let role_catalog =
                                load_npc_roles(&mut roles, &options.generator.assets_pack)?;
                            let catalog =
                                load_spawn_catalog(&mut spawns, &options.generator.assets_root)?;
                            let mut rng = SpawnRng::new(
                                save.world().tick()
                                    ^ save.world().entities().next_id().rotate_left(17),
                            );
                            let Some((_, role)) = catalog.pick_spawn(
                                &environment,
                                &clock,
                                i32::from(clock.moon_phase()),
                                &mut rng,
                            ) else {
                                return Ok(None);
                            };
                            let role_key = role.id.clone();
                            let spawn_block_set = role.spawn_block_set.clone();
                            let require_fluid = role.spawn_fluid_tag.is_some();
                            let allowed_floor = match &spawn_block_set {
                                Some(name) => load_block_sets(
                                    &mut block_sets,
                                    &options.generator.assets_pack,
                                )?
                                .resolve(generator.assets(), name),
                                None => None,
                            };
                            let is_solid = npc_solid_rule(generator.assets());
                            let position = SpawnPositionSearch::new(save.world(), is_solid).find(
                                center,
                                &SpawnSearchOptions {
                                    radius,
                                    vertical_range: 16,
                                    mode,
                                    allowed_floor,
                                    require_fluid,
                                },
                            );
                            let Some(position) = position else {
                                return Ok(None);
                            };
                            let entity = spawn_npc_entity(
                                &mut save,
                                generator.assets(),
                                role_catalog,
                                &role_key,
                                npc_transform(position.position),
                                &mut npcs,
                            )?;
                            entity_events.push(RuntimeEntityEvent::Spawned(entity.id));
                            Ok(Some(entity))
                        })();
                        let _ = reply.send(result);
                    }
                    Command::DespawnNpc { id, reply } => {
                        npcs.remove(&id);
                        let removed = save.world_mut().despawn_entity(id);
                        if let Some(entity) = &removed {
                            entity_events.push(RuntimeEntityEvent::Despawned {
                                id,
                                position: entity.transform.position,
                            });
                        }
                        let _ = reply.send(Ok(removed));
                    }
                    Command::SetNpcTargets { targets, reply } => {
                        npc_targets = targets;
                        let _ = reply.send(());
                    }
                    Command::SetNpcSpawnTargets { targets, reply } => {
                        npc_spawn_targets = targets;
                        npc_spawn_controllers.clear();
                        let _ = reply.send(());
                    }
                    Command::TickNpcSpawns { reply } => {
                        let result = (|| {
                            let role_catalog =
                                load_npc_roles(&mut roles, &options.generator.assets_pack)?;
                            let catalog =
                                load_spawn_catalog(&mut spawns, &options.generator.assets_root)?;
                            let is_solid = npc_solid_rule(generator.assets());
                            let mut report = NpcSpawnReport::default();
                            for target in &npc_spawn_targets {
                                let controller = npc_spawn_controllers
                                    .entry(target.environment.clone())
                                    .or_insert_with(|| WorldSpawnController::new(target.density));
                                controller.set_density(target.density);
                                let segments = if target.segments == 0 {
                                    generator
                                        .assets()
                                        .environments
                                        .id_of(&target.environment)
                                        .map(|id| save.world().environment_segment_count(id))
                                        .unwrap_or(0)
                                } else {
                                    target.segments
                                };
                                controller.set_segment_count(segments);
                                let mut budget = 8usize;
                                while controller.missing().ceil() >= 1.0 && budget > 0 {
                                    budget -= 1;
                                    let Some((_, role)) = catalog.pick_spawn(
                                        &target.environment,
                                        &clock,
                                        i32::from(clock.moon_phase()),
                                        &mut spawn_rng,
                                    ) else {
                                        report.failed += 1;
                                        break;
                                    };
                                    let role_key = role.id.clone();
                                    let spawn_block_set = role.spawn_block_set.clone();
                                    let require_fluid = role.spawn_fluid_tag.is_some();
                                    let allowed_floor = match &spawn_block_set {
                                        Some(name) => load_block_sets(
                                            &mut block_sets,
                                            &options.generator.assets_pack,
                                        )?
                                        .resolve(generator.assets(), name),
                                        None => None,
                                    };
                                    let Some(position) =
                                        SpawnPositionSearch::new(save.world(), is_solid).find(
                                            target.center,
                                            &SpawnSearchOptions {
                                                radius: target.radius,
                                                vertical_range: 16,
                                                mode: target.mode,
                                                allowed_floor,
                                                require_fluid,
                                            },
                                        )
                                    else {
                                        report.failed += 1;
                                        break;
                                    };
                                    match spawn_npc_entity(
                                        &mut save,
                                        generator.assets(),
                                        role_catalog,
                                        &role_key,
                                        npc_transform(position.position),
                                        &mut npcs,
                                    ) {
                                        Ok(entity) => {
                                            entity_events
                                                .push(RuntimeEntityEvent::Spawned(entity.id));
                                            controller.record_spawn();
                                            report.spawned += 1;
                                        }
                                        Err(_) => {
                                            report.failed += 1;
                                            break;
                                        }
                                    }
                                }
                                report.population += controller.actual;
                            }
                            Ok(report)
                        })();
                        let _ = reply.send(result);
                    }
                    Command::TickNpcs { reply } => {
                        let now = Instant::now();
                        let dt = now
                            .saturating_duration_since(last_npc_tick)
                            .as_secs_f64()
                            .min(0.25);
                        last_npc_tick = now;
                        let is_solid =
                            |block: i32| {
                                generator.assets().block_types.get_by_id(block).is_some_and(
                                    |record| record.value.material == BlockMaterial::Solid,
                                )
                            };
                        let mut result = NpcRuntimeReport::default();
                        let mut attacks = Vec::new();
                        if dt > 0.0 {
                            for (&id, actor) in npcs.iter_mut() {
                                result.ticked += 1;
                                let report: NpcTickReport =
                                    actor.tick(dt, save.world(), is_solid, &npc_targets, 0.5);
                                if report.moved {
                                    result.moved += 1;
                                    if let Some(entity) =
                                        save.world_mut().entities_mut().get_mut(id)
                                    {
                                        entity.transform.position.x = actor.position.x;
                                        entity.transform.position.y = actor.position.y;
                                        entity.transform.position.z = actor.position.z;
                                        entity.transform.rotation.yaw = actor.yaw;
                                    }
                                    entity_events.push(RuntimeEntityEvent::Updated(id));
                                }
                                if let Some(attack) = report.attack {
                                    result.attacks += 1;
                                    attacks.push((id, attack));
                                }
                            }
                        }
                        let mut dead = BTreeSet::new();
                        for (source, attack) in attacks {
                            let Some(target) = npcs.get_mut(&attack.target) else {
                                continue;
                            };
                            for (cause, amount) in attack.damages {
                                target.take_damage(
                                    &Damage::new(amount, cause, DamageSource::Entity(source)),
                                    &damage_causes,
                                );
                            }
                            if target.is_dead() {
                                dead.insert(attack.target);
                            }
                        }
                        for id in dead {
                            if let Some(entity) = save.world_mut().despawn_entity(id) {
                                entity_events.push(RuntimeEntityEvent::Despawned {
                                    id,
                                    position: entity.transform.position,
                                });
                                result.deaths += 1;
                            }
                            npcs.remove(&id);
                        }
                        result.alive = npcs.len();
                        let _ = reply.send(Ok(result));
                    }
                    Command::DamageNpc {
                        id,
                        amount,
                        cause,
                        reply,
                    } => {
                        let result = npcs.get_mut(&id).map(|actor| {
                            let result = actor.take_damage(
                                &Damage::new(amount, cause.clone(), DamageSource::Command),
                                &damage_causes,
                            );
                            if !result.died {
                                if let Some(entity) = save.world_mut().entities_mut().get_mut(id) {
                                    actor.sync_runtime_entity(entity);
                                }
                            }
                            result
                        });
                        if result.as_ref().is_some_and(|result| result.died) {
                            if let Some(entity) = save.world_mut().despawn_entity(id) {
                                entity_events.push(RuntimeEntityEvent::Despawned {
                                    id,
                                    position: entity.transform.position,
                                });
                            }
                            npcs.remove(&id);
                        }
                        let _ = reply.send(Ok(result));
                    }
                    Command::NpcActors { reply } => {
                        let actors = npcs
                            .iter()
                            .map(|(&id, actor)| (id, actor.clone()))
                            .collect();
                        let _ = reply.send(actors);
                    }
                    Command::UpdateEntityTransform {
                        id,
                        transform,
                        reply,
                    } => {
                        let changed = save
                            .world_mut()
                            .entities_mut()
                            .get_mut(id)
                            .map(|entity| {
                                entity.transform = transform;
                                true
                            })
                            .unwrap_or(false);
                        if changed {
                            entity_events.push(RuntimeEntityEvent::Updated(id));
                        }
                        let _ = reply.send(Ok(changed));
                    }
                    Command::SpawnItemEntity {
                        item_id,
                        quantity,
                        x,
                        y,
                        z,
                        reply,
                    } => {
                        let result =
                            spawn_item_entity(&mut save, &generator, &item_id, quantity, x, y, z);
                        if let Ok(entity) = &result {
                            entity_events.push(RuntimeEntityEvent::Spawned(entity.id));
                        }
                        let _ = reply.send(result);
                    }
                    Command::SpawnItemStackEntity {
                        stack,
                        x,
                        y,
                        z,
                        reply,
                    } => {
                        let result = spawn_item_stack_entity(&mut save, &generator, stack, x, y, z);
                        if let Ok(entity) = &result {
                            entity_events.push(RuntimeEntityEvent::Spawned(entity.id));
                        }
                        let _ = reply.send(result);
                    }
                    Command::BlockComponent { x, y, z, reply } => {
                        let component = save.world().block_component(x, y, z).cloned();
                        let _ = reply.send(Ok(component));
                    }
                    Command::BlockComponents { reply } => {
                        let blocks = save
                            .world()
                            .block_components()
                            .iter()
                            .map(|(position, component)| (position, component.clone()))
                            .collect();
                        let _ = reply.send(Ok(blocks));
                    }
                    Command::Save { reply } => {
                        advance_clock(&mut clock, &mut last_clock);
                        store_clock(&clock, &mut save);
                        sync_npc_entities(&mut save, &npcs);
                        let result = save
                            .save_all(&ChunkCodec::new(generator.assets()))
                            .map_err(|error| error.to_string());
                        let _ = reply.send(result);
                    }
                    Command::ConfigAssetPackets { reply } => {
                        let _ = reply.send(Ok(Arc::clone(&config_assets)));
                    }
                    Command::CommonAssets { reply } => {
                        let result = AssetPack::open(&options.generator.assets_pack)
                            .map_err(|error| error.to_string())
                            .and_then(|pack| {
                                CommonAssetRegistry::load(&pack).map_err(|error| error.to_string())
                            });
                        let _ = reply.send(result);
                    }
                    Command::PlayerTransform { reply } => {
                        let _ = reply.send(Ok(player_transform));
                    }
                    Command::PlayerSnapshot { uuid, reply } => {
                        let result = player_snapshot(&mut save, &generator, uuid);
                        let _ = reply.send(result);
                    }
                    Command::SavePlayerTransform {
                        uuid,
                        transform,
                        reply,
                    } => {
                        let result = save_player_transform(&mut save, uuid, transform);
                        let _ = reply.send(result);
                    }
                    Command::SavePlayerState {
                        uuid,
                        transform,
                        inventory,
                        reply,
                    } => {
                        let result = save_player_state(&mut save, uuid, transform, &inventory);
                        let _ = reply.send(result);
                    }
                    Command::SavePlayerRevealedMarkers {
                        uuid,
                        markers,
                        reply,
                    } => {
                        let result = save_player_revealed_markers(&mut save, uuid, &markers);
                        let _ = reply.send(result);
                    }
                    Command::PlayerAppearance { reply } => {
                        let _ = reply.send(Ok(player_appearance.clone()));
                    }
                    Command::WorldTime { reply } => {
                        advance_clock(&mut clock, &mut last_clock);
                        let _ = reply.send(Ok(clock.state()));
                    }
                    Command::GameplayConfig { reply } => {
                        let _ = reply.send(Ok(gameplay_config.clone()));
                    }
                    Command::SetTimePaused { paused, reply } => {
                        advance_clock(&mut clock, &mut last_clock);
                        clock.set_paused(paused);
                        store_clock(&clock, &mut save);
                        let _ = reply.send(Ok(clock.state()));
                    }
                    Command::AdvanceTime { seconds, reply } => {
                        advance_clock(&mut clock, &mut last_clock);
                        clock.tick(seconds);
                        let _ = reply.send(Ok(clock.state()));
                    }
                    Command::Shutdown { reply } => {
                        advance_clock(&mut clock, &mut last_clock);
                        store_clock(&clock, &mut save);
                        sync_npc_entities(&mut save, &npcs);
                        let result = save
                            .save_all(&ChunkCodec::new(generator.assets()))
                            .map_err(|error| error.to_string());
                        generator.shutdown();
                        let _ = reply.send(result);
                        break;
                    }
                }
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
}

struct Started {
    save: WorldSave,
    generator: WorldGenerator,
    pending: VecDeque<(i32, i32)>,
    start: WorldStartReport,
    player_transform: Option<PlayerTransform>,
    player_appearance: PlayerAppearance,
    clock: WorldClock,
    gameplay_config: WorldGameplayConfig,
    movement_settings: MovementSettings,
    collision: BlockCollisionTable,
    simulation: BlockSimulation,
    weather: WeatherState,
    weather_forecasts: BTreeMap<i32, EnvironmentForecast>,
    config_assets: Arc<hytale_assets::ConfigAssets>,
    interactions: Arc<InteractionCatalog>,
    roles: Option<RoleCatalog>,
    spawns: Option<WorldSpawnCatalog>,
    block_sets: Option<BlockSetCatalog>,
    damage_causes: DamageCauses,
    npcs: BTreeMap<RuntimeEntityId, NpcActor>,
    last_npc_tick: Instant,
}

fn start(options: &WorldThreadOptions) -> Result<Started, WorldThreadError> {
    let generator = WorldGenerator::open(&options.generator)?;
    // In shared mode the interaction catalog and encoded config burst are
    // process-wide; otherwise load/encode them once for this world.
    let (interactions, config_assets) = match generator.shared_assets() {
        Some(shared) => (shared.interactions(), shared.config_assets()),
        None => {
            let interaction_pack =
                AssetPack::open(&options.generator.assets_pack).map_err(InteractionError::from)?;
            let interactions = Arc::new(InteractionCatalog::load(&interaction_pack)?);
            let config_assets = Arc::new(generator.assets().config_assets()?);
            (interactions, config_assets)
        }
    };
    let world_uuid = match &options.world_uuid {
        Some(value) => value
            .parse()
            .map_err(|_| WorldThreadError::InvalidUuid(value.clone()))?,
        None => WorldUuid::derive(
            options.seed,
            options.display_name.as_deref().unwrap_or("Default"),
        ),
    };
    let (mut save, config_created) = WorldSave::open(
        &options.save_dir,
        options.seed,
        options.display_name.clone(),
        world_uuid,
    )?;
    let player_uuid = match &options.player_uuid {
        Some(value) => value
            .parse()
            .map_err(|_| WorldThreadError::InvalidUuid(value.clone()))?,
        None => {
            let stored = save.players().players()?;
            stored.first().copied().unwrap_or_else(|| {
                WorldUuid::derive(
                    save.config().seed,
                    &format!(
                        "{}:player",
                        save.config().display_name.as_deref().unwrap_or("Default")
                    ),
                )
            })
        }
    };
    let (player, player_created) = save.load_or_create_player(player_uuid)?;
    let player_transform = player.transform();
    let mut player_appearance = player.appearance();
    player_appearance.resolve_items(|id| generator.assets().items.get(id).is_some());
    save.save_player(player_uuid, &player)?;

    // The gameplay config id comes from `config.json`; a missing asset (or a
    // pack that fails to load it) falls back to Java `GameplayConfig.DEFAULT`.
    let gameplay_asset = generator
        .assets()
        .gameplay_configs
        .get(save.config().gameplay_config_id())
        .map(|record| record.value.clone())
        .unwrap_or_default();
    let gameplay_config = gameplay_asset.to_packet(&config_assets.tags);
    // The selected config is the gameplay config's `Player.MovementConfig`;
    // a missing asset falls back to Java `MovementConfig.DEFAULT_MOVEMENT`.
    let movement_asset = generator
        .assets()
        .movement_configs
        .get(&gameplay_asset.player.movement_config)
        .map(|record| record.value.clone())
        .unwrap_or_default();
    let mut movement_settings = movement_asset.settings;
    movement_settings.mass = DEFAULT_PLAYER_MASS as f32;
    movement_settings.drag_coefficient = DEFAULT_PLAYER_DRAG_COEFFICIENT as f32;
    movement_settings.inverted_gravity = false;
    // The server joins players in Adventure. A future per-player game mode
    // re-derives this in `MovementManager.resetFly`.
    movement_settings.fly = movement_asset.fly.unwrap_or(FlyMode::Disabled);
    let collision = BlockCollisionTable::from_assets(generator.assets());
    let mut simulation = BlockSimulation::new(
        SimulationTables::from_assets(generator.assets()),
        save.config().seed,
    );
    simulation.set_disabled_fluid_tickers(&save.config().disabled_fluid_tickers);
    let weather_forecasts = environment_forecasts(generator.assets());
    let forced_weather = save
        .config()
        .forced_weather
        .as_deref()
        .and_then(|id| generator.assets().weather_id(id));
    let weather = WeatherState::new(forced_weather);
    let clock_settings = WorldClockSettings {
        daytime_duration_seconds: save
            .config()
            .daytime_duration_seconds
            .unwrap_or(gameplay_asset.world.daytime_duration_seconds),
        nighttime_duration_seconds: save
            .config()
            .nighttime_duration_seconds
            .unwrap_or(gameplay_asset.world.nighttime_duration_seconds),
        total_moon_phases: i8::try_from(gameplay_asset.world.total_moon_phases)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(hytale_world::world_clock::DEFAULT_TOTAL_MOON_PHASES),
    };
    let clock = WorldClock::from_game_time(
        parse_iso8601(&save.config().game_time),
        clock_settings,
        save.config().is_game_time_paused,
    );

    let stored_chunks = save.world().storage().stored_chunk_coordinates()?.len();
    let player_chunk = player
        .transform()
        .map(|transform| {
            (
                block_to_chunk(transform.position.x),
                block_to_chunk(transform.position.z),
            )
        })
        .unwrap_or((0, 0));
    let npc_entities: Vec<_> = save
        .world()
        .entities()
        .iter()
        .filter(|entity| entity.kind == RuntimeEntityKind::Npc)
        .cloned()
        .collect();
    // Role documents are only needed for worlds that already contain NPCs (or
    // once a spawn command arrives); ordinary worlds avoid the extra load.
    let roles = if npc_entities.is_empty() {
        None
    } else {
        let role_pack = AssetPack::open(&options.generator.assets_pack)?;
        Some(RoleCatalog::load(&role_pack)?)
    };
    let damage_causes = DamageCauses::new(generator.assets().damage_causes.iter().filter_map(
        |record| {
            record
                .id
                .map(|_| DamageCause::from_value(&record.key, &record.value))
        },
    ));
    let mut npcs = BTreeMap::new();
    if let Some(roles) = &roles {
        for entity in &npc_entities {
            match NpcActor::from_runtime_entity(generator.assets(), roles, entity) {
                Ok(Some(actor)) => {
                    npcs.insert(entity.id, actor);
                }
                Ok(None) => {}
                Err(error) => eprintln!(
                    "skipping NPC runtime entity {} ({:?}): {error}",
                    entity.id, entity.kind
                ),
            }
        }
    }
    let view = ChunkView::new(player_chunk.0, player_chunk.1, options.view_radius);
    let plan = view.plan(save.world().loaded_chunk_coordinates());
    let pending = VecDeque::from(plan.loads.clone());
    let start = WorldStartReport {
        save_dir: options.save_dir.clone(),
        config_created,
        world_uuid: save.config().uuid.to_string(),
        seed: save.config().seed,
        player_uuid: player_uuid.to_string(),
        player_created,
        stored_chunks,
        player_chunk,
        view_radius: options.view_radius,
        initial_loads: plan.loads.len(),
        initial_unloads: plan.unloads.len(),
    };
    Ok(Started {
        save,
        generator,
        pending,
        start,
        player_transform,
        player_appearance,
        clock,
        gameplay_config,
        movement_settings,
        collision,
        simulation,
        weather,
        weather_forecasts,
        config_assets,
        interactions,
        roles,
        spawns: None,
        block_sets: None,
        damage_causes,
        npcs,
        last_npc_tick: Instant::now(),
    })
}

fn advance_clock(clock: &mut WorldClock, last: &mut Instant) {
    let now = Instant::now();
    let seconds = now.saturating_duration_since(*last).as_secs_f64();
    *last = now;
    clock.tick(seconds);
}

fn store_clock(clock: &WorldClock, save: &mut WorldSave) {
    save.config_mut().game_time = format_iso8601(clock.game_time());
    save.config_mut().is_game_time_paused = clock.is_paused();
}

fn apply_views(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    events: &Sender<WorldEvent>,
    views: &BTreeMap<ViewId, ChunkView>,
    pending: &mut VecDeque<(i32, i32)>,
    emit_unloads: bool,
) -> Result<ViewReport, String> {
    let desired: BTreeSet<(i32, i32)> = views.values().flat_map(|view| view.desired()).collect();
    let codec = ChunkCodec::new(generator.assets());
    let loaded: Vec<(i32, i32)> = save.world().loaded_chunk_coordinates();
    let mut unloaded = 0;
    for (x, z) in loaded {
        if desired.contains(&(x, z)) {
            continue;
        }
        if save
            .world_mut()
            .unload_chunk(&codec, x, z)
            .map_err(|error| error.to_string())?
        {
            unloaded += 1;
            if emit_unloads {
                let _ = events.send(WorldEvent::ChunkUnloaded { x, z });
            }
        }
    }

    let loaded: BTreeSet<(i32, i32)> = save
        .world()
        .loaded_chunk_coordinates()
        .into_iter()
        .collect();
    let mut loads: Vec<(i32, i32)> = desired.difference(&loaded).copied().collect();
    loads.sort_by_key(|&(x, z)| (distance_to_nearest_view(views, x, z), x, z));
    // Replacing the queue cancels every pending load outside the new union.
    pending.clear();
    pending.extend(loads);
    Ok(ViewReport {
        queued: pending.len(),
        unloaded,
    })
}

fn distance_to_nearest_view(views: &BTreeMap<ViewId, ChunkView>, x: i32, z: i32) -> i64 {
    views
        .values()
        .map(|view| view.distance_squared(x, z))
        .min()
        .unwrap_or(i64::MAX)
}

fn tick(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    events: &Sender<WorldEvent>,
    seed: i32,
    budget: usize,
    pending: &mut VecDeque<(i32, i32)>,
) -> Result<TickReport, String> {
    let codec = ChunkCodec::new(generator.assets());
    let mut report = TickReport::default();
    for _ in 0..budget {
        let Some((x, z)) = pending.pop_front() else {
            break;
        };
        if save.world().chunk(x, z).is_some() {
            continue;
        }
        let generated = if save
            .world_mut()
            .load_chunk(&codec, x, z)
            .map_err(|error| error.to_string())?
        {
            false
        } else {
            let column = generator
                .generate(seed, x, z)
                .map_err(|error| error.to_string())?;
            save.world_mut().insert_chunk(column);
            true
        };
        if generated {
            report.generated += 1;
        } else {
            report.loaded += 1;
        }
        let _ = events.send(WorldEvent::ChunkLoaded { x, z, generated });
    }
    report.remaining = pending.len();
    Ok(report)
}

fn chunk_packets(save: &WorldSave, x: i32, z: i32, full: bool) -> Result<ChunkPacketBatch, String> {
    let column = save
        .world()
        .chunk(x, z)
        .ok_or_else(|| format!("chunk ({x}, {z}) is not loaded"))?;
    if full {
        ChunkPacketBatch::full(column)
    } else {
        ChunkPacketBatch::initial(column)
    }
    .map_err(|error| error.to_string())
}

fn load_chunk(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    events: &Sender<WorldEvent>,
    seed: i32,
    x: i32,
    z: i32,
) -> Result<LoadOutcome, String> {
    if save.world().chunk(x, z).is_some() {
        return Ok(LoadOutcome::AlreadyLoaded);
    }
    let codec = ChunkCodec::new(generator.assets());
    if save
        .world_mut()
        .load_chunk(&codec, x, z)
        .map_err(|error| error.to_string())?
    {
        let _ = events.send(WorldEvent::ChunkLoaded {
            x,
            z,
            generated: false,
        });
        return Ok(LoadOutcome::Loaded);
    }
    let column = generator
        .generate(seed, x, z)
        .map_err(|error| error.to_string())?;
    save.world_mut().insert_chunk(column);
    let _ = events.send(WorldEvent::ChunkLoaded {
        x,
        z,
        generated: true,
    });
    Ok(LoadOutcome::Generated)
}

/// One section's `(x, y, z, command)` entries.
type DamageSection = BTreeMap<(i32, i32, i32), Vec<(i32, i32, i32, BlockDamageCmd)>>;

/// Group health entries into Java's one-block or one-section packet form.
fn group_block_damage(
    entries: impl Iterator<Item = (i32, i32, i32, f32, f32)>,
) -> Vec<BlockDamagePacket> {
    let mut sections: DamageSection = BTreeMap::new();
    for (x, y, z, health, delta) in entries {
        let section = (x >> 5, y >> 5, z >> 5);
        sections.entry(section).or_default().push((
            x,
            y,
            z,
            BlockDamageCmd {
                // Java `ChunkUtil.indexBlock(x, y, z)`.
                index: (((y & 31) << 10) | ((z & 31) << 5) | (x & 31)) as i16,
                damage: health,
                delta,
            },
        ));
    }
    sections
        .into_iter()
        .map(|((section_x, section_y, section_z), entries)| {
            if entries.len() == 1 {
                let (x, y, z, command) = entries.into_iter().next().expect("one entry");
                BlockDamagePacket::Single(UpdateBlockDamage {
                    block_position: Some(BlockPosition { x, y, z }),
                    damage: command.damage,
                    delta: command.delta,
                })
            } else {
                BlockDamagePacket::Section(UpdateBlockDamages {
                    x: section_x,
                    y: section_y,
                    z: section_z,
                    cmds: entries.into_iter().map(|(_, _, _, cmd)| cmd).collect(),
                })
            }
        })
        .collect()
}

fn prefab_rotation(degrees: u16) -> Result<PrefabRotation, String> {
    match degrees {
        0 => Ok(PrefabRotation::Rotation0),
        90 => Ok(PrefabRotation::Rotation90),
        180 => Ok(PrefabRotation::Rotation180),
        270 => Ok(PrefabRotation::Rotation270),
        other => Err(format!(
            "invalid prefab rotation {other}; expected 0, 90, 180 or 270"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn place_prefab(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    events: &Sender<WorldEvent>,
    seed: i32,
    name: &str,
    origin: (i32, i32, i32),
    rotation_degrees: u16,
) -> Result<PrefabPasteReport, String> {
    let prefab = generator
        .prefab(name)
        .ok_or_else(|| format!("unknown prefab `{name}`"))?;
    let rotation = prefab_rotation(rotation_degrees)?;
    let access = prefab.new_access();
    let min_x = origin.0 + access.get_min_x(rotation);
    let max_x = origin.0 + access.get_max_x(rotation);
    let min_z = origin.2 + access.get_min_z(rotation);
    let max_z = origin.2 + access.get_max_z(rotation);
    for chunk_x in (min_x >> 5)..=(max_x >> 5) {
        for chunk_z in (min_z >> 5)..=(max_z >> 5) {
            load_chunk(save, generator, events, seed, chunk_x, chunk_z)?;
        }
    }
    paste_prefab(
        save.world_mut(),
        name,
        &prefab,
        origin,
        rotation,
        i64::from(seed),
    )
    .map_err(|error| error.to_string())
}

fn npc_solid_rule(assets: &hytale_assets::AssetRegistry) -> impl Fn(i32) -> bool + Copy + '_ {
    move |block: i32| {
        assets
            .block_types
            .get_by_id(block)
            .is_some_and(|record| record.value.material == BlockMaterial::Solid)
    }
}

fn npc_transform(position: Vec3) -> PlayerTransform {
    PlayerTransform::new(
        hytale_world::player_data::Position {
            x: position.x,
            y: position.y,
            z: position.z,
        },
        hytale_world::player_data::Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    )
}

fn spawn_npc_entity(
    save: &mut WorldSave,
    assets: &hytale_assets::AssetRegistry,
    roles: &RoleCatalog,
    role: &str,
    transform: PlayerTransform,
    npcs: &mut BTreeMap<RuntimeEntityId, NpcActor>,
) -> Result<RuntimeEntity, String> {
    let mut rng = |low: f64, high: f64| (low + high) * 0.5;
    let resolved = roles
        .resolve_with_rng(role, &mut rng)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("unknown NPC role `{role}`"))?;
    let actor = NpcActor::from_assets(
        assets,
        &resolved,
        Vec3::new(
            transform.position.x,
            transform.position.y,
            transform.position.z,
        ),
        transform.rotation.yaw,
    )
    .map_err(|error| error.to_string())?;
    let components = actor.runtime_components();
    let id = save
        .world_mut()
        .spawn_entity(RuntimeEntityKind::Npc, None, transform, components);
    npcs.insert(id, actor);
    save.world()
        .entity(id)
        .cloned()
        .ok_or_else(|| "the NPC entity disappeared during spawn".to_string())
}

fn load_block_sets<'a>(
    block_sets: &'a mut Option<BlockSetCatalog>,
    pack_path: &std::path::Path,
) -> Result<&'a BlockSetCatalog, String> {
    if block_sets.is_none() {
        let pack = AssetPack::open(pack_path).map_err(|error| error.to_string())?;
        *block_sets = Some(BlockSetCatalog::load(&pack).map_err(|error| error.to_string())?);
    }
    Ok(block_sets
        .as_ref()
        .expect("block-set catalog was just loaded"))
}

fn load_spawn_catalog<'a>(
    spawns: &'a mut Option<WorldSpawnCatalog>,
    root: &std::path::Path,
) -> Result<&'a WorldSpawnCatalog, String> {
    if spawns.is_none() {
        *spawns = Some(WorldSpawnCatalog::load_root(root).map_err(|error| error.to_string())?);
    }
    Ok(spawns.as_ref().expect("spawn catalog was just loaded"))
}

struct SpawnRng(u64);

impl SpawnRng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
}

impl FlockRng for SpawnRng {
    fn next_f64(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

fn load_npc_roles<'a>(
    roles: &'a mut Option<RoleCatalog>,
    pack_path: &std::path::Path,
) -> Result<&'a RoleCatalog, String> {
    if roles.is_none() {
        let pack = AssetPack::open(pack_path).map_err(|error| error.to_string())?;
        *roles = Some(RoleCatalog::load(&pack).map_err(|error| error.to_string())?);
    }
    Ok(roles.as_ref().expect("role catalog was just loaded"))
}

fn sync_npc_entities(save: &mut WorldSave, npcs: &BTreeMap<RuntimeEntityId, NpcActor>) {
    for (&id, actor) in npcs {
        if let Some(entity) = save.world_mut().entities_mut().get_mut(id) {
            actor.sync_runtime_entity(entity);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_item_entity(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    item_id: &str,
    quantity: i32,
    x: f64,
    y: f64,
    z: f64,
) -> Result<RuntimeEntity, String> {
    if generator.assets().items.get(item_id).is_none() {
        return Err(format!("unknown item `{item_id}`"));
    }
    let id = save.world_mut().spawn_entity(
        RuntimeEntityKind::Item,
        None,
        transform([x, y, z], [0.0, 0.0, 0.0]),
        serde_json::json!({
            "Item": {
                "Id": item_id,
                "Quantity": quantity.max(1),
            }
        }),
    );
    save.world()
        .entity(id)
        .cloned()
        .ok_or_else(|| "the item entity disappeared during spawn".to_string())
}

fn spawn_item_stack_entity(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    stack: ItemStack,
    x: f64,
    y: f64,
    z: f64,
) -> Result<RuntimeEntity, String> {
    if generator.assets().items.get(&stack.item_id).is_none() {
        return Err(format!("unknown item `{}`", stack.item_id));
    }
    let metadata = stack.metadata.as_ref().map(|metadata| metadata.to_string());
    let id = save.world_mut().spawn_entity(
        RuntimeEntityKind::Item,
        None,
        transform([x, y, z], [0.0, 0.0, 0.0]),
        serde_json::json!({
            "Item": {
                "Id": stack.item_id,
                "Quantity": stack.quantity.max(1),
                "Durability": stack.durability,
                "MaxDurability": stack.max_durability,
                "Quality": stack.quality_override.unwrap_or(0),
                "OverrideDroppedItemAnimation": stack.override_dropped_item_animation,
                "Metadata": metadata,
            }
        }),
    );
    save.world()
        .entity(id)
        .cloned()
        .ok_or_else(|| "the item entity disappeared during spawn".to_string())
}

/// Turn the simulation's fixed-item drops into item entities. Weighted drop
/// lists stay unresolved until the drop-list producer lands (T57/T60).
fn spawn_simulation_drops(
    world: &mut hytale_world::world::World,
    report: &SimulationReport,
) -> Vec<RuntimeEntityId> {
    let mut spawned = Vec::new();
    for (x, y, z, drop) in &report.drops {
        let Some(item_id) = drop.item_id.as_deref() else {
            continue;
        };
        spawned.push(world.spawn_entity(
            RuntimeEntityKind::Item,
            None,
            transform(
                [
                    f64::from(*x) + 0.5,
                    f64::from(*y) + 0.5,
                    f64::from(*z) + 0.5,
                ],
                [0.0, 0.0, 0.0],
            ),
            serde_json::json!({
                "Item": {
                    "Id": item_id,
                    "Quantity": drop.quantity.max(1),
                }
            }),
        ));
    }
    spawned
}

fn player_snapshot(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    uuid: WorldUuid,
) -> Result<PlayerSnapshot, String> {
    let (player, _created) = save
        .load_or_create_player(uuid)
        .map_err(|error| error.to_string())?;
    let transform = player.transform().unwrap_or_else(|| {
        PlayerTransform::new(
            hytale_world::player_data::Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            hytale_world::player_data::Rotation {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
            },
        )
    });
    let mut appearance = player.appearance();
    appearance.resolve_items(|id| generator.assets().items.get(id).is_some());
    let inventory = player.inventory().map_err(|error| error.to_string())?;
    Ok(PlayerSnapshot {
        transform,
        appearance,
        inventory,
        revealed_markers: player.revealed_markers(),
    })
}

fn save_player_revealed_markers(
    save: &mut WorldSave,
    uuid: WorldUuid,
    markers: &BTreeSet<String>,
) -> Result<(), String> {
    let (mut player, _created) = save
        .load_or_create_player(uuid)
        .map_err(|error| error.to_string())?;
    player.set_revealed_markers(markers.iter().cloned());
    save.save_player(uuid, &player)
        .map_err(|error| error.to_string())
}

fn save_player_transform(
    save: &mut WorldSave,
    uuid: WorldUuid,
    transform: PlayerTransform,
) -> Result<(), String> {
    let (mut player, _created) = save
        .load_or_create_player(uuid)
        .map_err(|error| error.to_string())?;
    player.set_transform(transform);
    save.save_player(uuid, &player)
        .map_err(|error| error.to_string())
}

fn save_player_state(
    save: &mut WorldSave,
    uuid: WorldUuid,
    transform: PlayerTransform,
    inventory: &PlayerInventory,
) -> Result<(), String> {
    let (mut player, _created) = save
        .load_or_create_player(uuid)
        .map_err(|error| error.to_string())?;
    player.set_transform(transform);
    player.set_inventory(inventory);
    save.save_player(uuid, &player)
        .map_err(|error| error.to_string())
}

fn set_block(
    save: &mut WorldSave,
    x: i32,
    y: i32,
    z: i32,
    block_id: i32,
) -> Result<(i32, i32), String> {
    let (chunk_x, chunk_z) = (x >> 5, z >> 5);
    if save.world().chunk(chunk_x, chunk_z).is_none() {
        return Err(format!(
            "chunk ({chunk_x}, {chunk_z}) is not loaded for block ({x}, {y}, {z})"
        ));
    }
    let before = save.world().get_block(x, y, z);
    save.world_mut()
        .set_block(x, y, z, block_id)
        .map_err(|error| error.to_string())?;
    Ok((before, save.world().get_block(x, y, z)))
}

fn place_block(
    save: &mut WorldSave,
    x: i32,
    y: i32,
    z: i32,
    block_id: i32,
) -> Result<Option<(i32, i32)>, String> {
    if block_id <= 0 {
        return Ok(None);
    }
    let (chunk_x, chunk_z) = (x >> 5, z >> 5);
    if save.world().chunk(chunk_x, chunk_z).is_none() {
        return Err(format!(
            "chunk ({chunk_x}, {chunk_z}) is not loaded for block ({x}, {y}, {z})"
        ));
    }
    let before = save.world().get_block(x, y, z);
    if before != 0 {
        return Ok(None);
    }
    save.world_mut()
        .set_block(x, y, z, block_id)
        .map_err(|error| error.to_string())?;
    Ok(Some((before, save.world().get_block(x, y, z))))
}

fn block_type_json(
    save: &WorldSave,
    generator: &WorldGenerator,
    x: i32,
    y: i32,
    z: i32,
) -> Result<Option<(String, serde_json::Value)>, String> {
    if save.world().chunk(x >> 5, z >> 5).is_none() {
        return Ok(None);
    }
    let block_id = save.world().get_block(x, y, z);
    let Some(record) = generator.assets().block_types.get_by_id(block_id) else {
        return Ok(None);
    };
    Ok(Some((record.key.clone(), record.value.json.clone())))
}

fn set_block_state(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    x: i32,
    y: i32,
    z: i32,
    state: &str,
) -> Result<Option<(i32, i32)>, String> {
    let (chunk_x, chunk_z) = (x >> 5, z >> 5);
    if save.world().chunk(chunk_x, chunk_z).is_none() {
        return Ok(None);
    }
    let before = save.world().get_block(x, y, z);
    let Some(record) = generator.assets().block_types.get_by_id(before) else {
        return Ok(None);
    };
    let container = record
        .value
        .container_item
        .clone()
        .unwrap_or_else(|| record.key.clone());
    let key = format!("*{container}_State_Definitions_{state}");
    let Some(target) = generator.assets().block_types.get(&key) else {
        return Ok(None);
    };
    let Some(target_id) = target.id else {
        return Ok(None);
    };
    // State variants keep the block's components and container inventory.
    let component = save.world().block_component(x, y, z).cloned();
    let inventory = save.world().container(x, y, z).cloned();
    save.world_mut()
        .set_block(x, y, z, target_id)
        .map_err(|error| error.to_string())?;
    if let Some(component) = component {
        save.world_mut()
            .set_block_component(x, y, z, component.block_id, component.components);
    }
    if let Some(inventory) = inventory {
        save.world_mut().set_container(x, y, z, inventory);
    }
    Ok(Some((before, save.world().get_block(x, y, z))))
}

fn open_block_container(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    x: i32,
    y: i32,
    z: i32,
) -> Result<Option<BlockContainerOpen>, String> {
    if save.world().chunk(x >> 5, z >> 5).is_none() {
        return Ok(None);
    }
    let block_id = save.world().get_block(x, y, z);
    let Some(record) = generator.assets().block_types.get_by_id(block_id) else {
        return Ok(None);
    };
    let Some(capacity) = block_container_capacity(&record.value.json) else {
        return Ok(None);
    };
    let block_item_id = record
        .value
        .container_item
        .clone()
        .unwrap_or_else(|| record.key.clone());
    let container = save.world_mut().container_or_create(x, y, z, capacity);
    Ok(Some(BlockContainerOpen {
        block_item_id,
        capacity,
        section: container.to_packet(),
    }))
}

fn block_container_capacity(json: &serde_json::Value) -> Option<u16> {
    let capacity = json
        .get("BlockEntity")?
        .get("Components")?
        .get("ItemContainerBlock")?
        .get("Capacity")?
        .as_u64()?;
    u16::try_from(capacity).ok()
}

fn block_id_for_item(
    assets: &hytale_assets::AssetRegistry,
    item_id: &str,
) -> Result<Option<i32>, String> {
    let Some(item) = assets.items.get(item_id) else {
        return Ok(None);
    };
    if item.value.get("BlockType").is_none() {
        return Ok(None);
    }
    Ok(assets.block_types.id_of(&item.key))
}

fn block_to_chunk(value: f64) -> i32 {
    (value.floor() as i32) >> 5
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_world::world::WorldStorage;

    #[test]
    fn block_damage_entries_group_by_section_and_keep_the_java_index() {
        let packets = group_block_damage(
            [
                (33, 65, 2, 0.75, -0.25),
                (34, 65, 2, 0.5, -0.5),
                (35, 65, 2, 0.25, -0.75),
            ]
            .into_iter(),
        );
        assert_eq!(packets.len(), 1);
        let BlockDamagePacket::Section(packet) = &packets[0] else {
            panic!("three entries must use UpdateBlockDamages");
        };
        assert_eq!((packet.x, packet.y, packet.z), (1, 2, 0));
        assert_eq!(packet.cmds.len(), 3);
        assert_eq!(packet.cmds[0].index, (1 << 10) | (2 << 5) | 1);
        assert_eq!(packet.cmds[0].damage, 0.75);
        assert_eq!(packet.cmds[0].delta, -0.25);

        let packets = group_block_damage([(3, 64, 1, 0.5, 0.5)].into_iter());
        let BlockDamagePacket::Single(packet) = &packets[0] else {
            panic!("one entry must use UpdateBlockDamage");
        };
        assert_eq!(
            packet
                .block_position
                .as_ref()
                .map(|position| (position.x, position.y, position.z)),
            Some((3, 64, 1))
        );
        assert_eq!(packet.damage, 0.5);
        assert_eq!(packet.delta, 0.5);
    }

    #[test]
    fn fixed_simulation_drops_become_item_entities() {
        let dir = std::env::temp_dir().join(format!(
            "hytale-simulation-drop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut world = hytale_world::world::World::new(WorldStorage::new(dir.clone()));
        let mut report = SimulationReport::default();
        report.drops.push((
            4,
            201,
            4,
            BlockDrop {
                item_id: Some("Rock_Stone_Cobble".to_string()),
                drop_list_id: None,
                quantity: 2,
            },
        ));
        report.drops.push((
            5,
            201,
            5,
            BlockDrop {
                item_id: None,
                drop_list_id: Some("Drop_Test".to_string()),
                quantity: 1,
            },
        ));
        spawn_simulation_drops(&mut world, &report);
        let entities: Vec<_> = world.entities().iter().collect();
        assert_eq!(entities.len(), 1, "weighted drop lists stay unresolved");
        assert_eq!(entities[0].kind, RuntimeEntityKind::Item);
        assert_eq!(entities[0].item_id(), Some("Rock_Stone_Cobble"));
        assert_eq!(entities[0].item_quantity(), 2);
        assert_eq!(entities[0].transform.position.x, 4.5);
        assert_eq!(entities[0].transform.position.z, 4.5);
        std::fs::remove_dir_all(dir).ok();
    }
}
