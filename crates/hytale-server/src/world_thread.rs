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
use hytale_entities::movement::{resolve_target, PlayerShape, TargetResolution};
use hytale_interactions::{InteractionCatalog, InteractionError};
use hytale_inventory::PlayerInventory;
use hytale_protocol::generated::common::{FlyMode, MovementSettings};
use hytale_protocol::generated::world::WorldGameplayConfig;
use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::chunk::stream::ChunkPacketBatch;
use hytale_world::collision::{BlockCollisionTable, Vec3, WorldCollision};
use hytale_world::player_data::{PlayerAppearance, PlayerDataError, PlayerTransform};
use hytale_world::simulation::{BlockDrop, BlockSimulation, SimulationReport, SimulationTables};
use hytale_world::view::ChunkView;
use hytale_world::weather::{environment_forecasts, EnvironmentForecast, WeatherState};
use hytale_world::world::{WorldError, WorldSave};
use hytale_world::world_clock::{format_iso8601, parse_iso8601, WorldClock, WorldClockSettings};
use hytale_world::world_config::{WorldConfigError, WorldUuid};
use thiserror::Error;

use crate::generation::{GenerationError, GeneratorOptions, WorldGenerator};

/// The world clock snapshot sent on join (`WorldTimeState` keeps its previous
/// name for session compatibility).
pub use hytale_world::world_clock::WorldClockState as WorldTimeState;

/// Java player model `PhysicsValues.Mass` (`Server/Models/Human/Player.json`).
pub const DEFAULT_PLAYER_MASS: f64 = 68.0;
/// Java `PhysicsValues.DEFAULT_DRAG_COEFFICIENT`.
pub const DEFAULT_PLAYER_DRAG_COEFFICIENT: f64 = 0.5;

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

    /// Save dirty chunks and the config.
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
        }) => {
            if ready.send(Ok(start.clone())).is_err() {
                generator.shutdown();
                return;
            }
            let seed = save.config().seed as i32;
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
                    Command::SetBlockState {
                        x,
                        y,
                        z,
                        state,
                        reply,
                    } => {
                        let result = set_block_state(&mut save, &generator, x, y, z, &state);
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
                        let result = simulation.tick(save.world_mut(), dt);
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
                    Command::Save { reply } => {
                        advance_clock(&mut clock, &mut last_clock);
                        store_clock(&clock, &mut save);
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
}

fn start(options: &WorldThreadOptions) -> Result<Started, WorldThreadError> {
    let generator = WorldGenerator::open(&options.generator)?;
    let interaction_pack =
        AssetPack::open(&options.generator.assets_pack).map_err(InteractionError::from)?;
    let interactions = Arc::new(InteractionCatalog::load(&interaction_pack)?);
    // Encode the config burst once per world; every session shares the Arc.
    let config_assets = Arc::new(generator.assets().config_assets()?);
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
    })
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
    save.world_mut()
        .set_block(x, y, z, target_id)
        .map_err(|error| error.to_string())?;
    Ok(Some((before, save.world().get_block(x, y, z))))
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
