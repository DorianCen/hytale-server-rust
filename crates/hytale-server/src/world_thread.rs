//! One world thread: owns the save, the generator and the demand queue.
//!
//! Chunk loads/generations run on the world thread. `SetView` replaces the
//! pending queue (cancelling loads that are no longer inside the view), unloads
//! chunks that left it, and orders the remaining work nearest-first. `Tick`
//! advances a bounded number of chunks so a live server can interleave other
//! world work between loads.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread::{self, JoinHandle};

use hytale_world::chunk::storage::ChunkCodec;
use hytale_world::player_data::PlayerDataError;
use hytale_world::view::{ChunkPlan, ChunkView};
use hytale_world::world::{WorldError, WorldSave};
use hytale_world::world_config::{WorldConfigError, WorldUuid};
use thiserror::Error;

use crate::generation::{GenerationError, GeneratorOptions, WorldGenerator};

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
    Tick {
        reply: SyncSender<Result<TickReport, String>>,
    },
    LoadChunk {
        x: i32,
        z: i32,
        reply: SyncSender<Result<LoadOutcome, String>>,
    },
    SetBlock {
        x: i32,
        y: i32,
        z: i32,
        block_id: i32,
        reply: SyncSender<Result<(i32, i32), String>>,
    },
    Save {
        reply: SyncSender<Result<usize, String>>,
    },
    Shutdown {
        reply: SyncSender<Result<usize, String>>,
    },
}

/// Handle to one world thread.
pub struct WorldThread {
    commands: Sender<Command>,
    events: Receiver<WorldEvent>,
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
                    events: event_rx,
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
        self.events.try_recv().ok()
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
        }) => {
            if ready.send(Ok(start)).is_err() {
                generator.shutdown();
                return;
            }
            let seed = save.config().seed as i32;
            let budget = options.chunks_per_tick.max(1);
            for command in commands {
                match command {
                    Command::SetView {
                        center_x,
                        center_z,
                        radius,
                        reply,
                    } => {
                        let result = apply_view(
                            &mut save,
                            &generator,
                            &events,
                            center_x,
                            center_z,
                            radius,
                            &mut pending,
                        );
                        let _ = reply.send(result);
                    }
                    Command::Tick { reply } => {
                        let result =
                            tick(&mut save, &generator, &events, seed, budget, &mut pending);
                        let _ = reply.send(result);
                    }
                    Command::LoadChunk { x, z, reply } => {
                        let result = load_chunk(&mut save, &generator, &events, seed, x, z);
                        let _ = reply.send(result);
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
                    Command::Save { reply } => {
                        let result = save
                            .save_all(&ChunkCodec::new(generator.assets()))
                            .map_err(|error| error.to_string());
                        let _ = reply.send(result);
                    }
                    Command::Shutdown { reply } => {
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
}

fn start(options: &WorldThreadOptions) -> Result<Started, WorldThreadError> {
    let generator = WorldGenerator::open(&options.generator)?;
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
    save.save_player(player_uuid, &player)?;

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
    })
}

fn apply_view(
    save: &mut WorldSave,
    generator: &WorldGenerator,
    events: &Sender<WorldEvent>,
    center_x: i32,
    center_z: i32,
    radius: u32,
    pending: &mut VecDeque<(i32, i32)>,
) -> Result<ViewReport, String> {
    let view = ChunkView::new(center_x, center_z, radius);
    let plan: ChunkPlan = view.plan(save.world().loaded_chunk_coordinates());
    let codec = ChunkCodec::new(generator.assets());
    let mut unloaded = 0;
    for (x, z) in plan.unloads {
        if save
            .world_mut()
            .unload_chunk(&codec, x, z)
            .map_err(|error| error.to_string())?
        {
            unloaded += 1;
            let _ = events.send(WorldEvent::ChunkUnloaded { x, z });
        }
    }
    // Replacing the queue cancels every pending load outside the new view.
    pending.clear();
    pending.extend(plan.loads.iter().copied());
    Ok(ViewReport {
        queued: pending.len(),
        unloaded,
    })
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

fn block_to_chunk(value: f64) -> i32 {
    (value.floor() as i32) >> 5
}
