//! `hytale-server` library surface.
//!
//! The binary is a thin CLI; the reusable boot paths live here so integration
//! tests can drive them without spawning a process.

#![deny(clippy::all)]

pub mod asset_validation;
pub mod chunk_serve;
pub mod chunk_streamer;
mod drop_lists;
pub mod generation;
pub mod headless;
pub mod inventory_rules;
pub mod map_image;
pub mod memory;
pub mod player_data_tool;
pub mod player_gameplay;
pub mod players;
pub mod security;
pub mod serve;
pub mod session;
pub mod universe_runtime;
pub mod world_boot;
pub mod world_config_tool;
pub mod world_thread;
pub mod world_tool;

pub use asset_validation::{
    validate_assets, AssetValidationError, AssetValidationReport, PacketReport, StoreReport,
};
pub use chunk_serve::{
    run as run_chunk_serve, serve_connection as serve_chunk_connection, ChunkServeError,
    ChunkServeOptions, ChunkServeReport,
};
pub use chunk_streamer::{
    default_compression_level, send_chunk, send_updates, send_view, send_view_registered,
    ChunkSessionError, ChunkStreamReport,
};
pub use generation::{GenerationError, GeneratorOptions, WorldGenerator};
pub use headless::{run as run_headless, HeadlessError, HeadlessOptions, HeadlessReport};
pub use hytale_builtin::{BuiltinHost, BuiltinRuntime, HostError, StartOptions};
pub use hytale_commands::{CommandSender, PermissionRegistry, PermissionSet};
pub use hytale_world::simulation::{
    BlockChange, BlockDrop, FluidChange, SimulationReport, SimulationStages, SimulationTables,
};
pub use inventory_rules::AssetItemRules;
pub use memory::{peak_resident_bytes, resident_bytes};
pub use player_data_tool::{
    run as run_player_data_tool, PlayerDataToolError, PlayerDataToolOptions, PlayerDataToolReport,
};
pub use player_gameplay::{stat_update_packet, PlayerGameplay};
pub use players::{
    channel as player_channel, channel_with_capacity as player_channel_with_capacity, PlayerJoin,
    PlayerJoinError, PlayerJoinRequest, PlayerMessage, PlayerRegistry, PlayerSession,
    PLAYER_NPC_TARGET_OFFSET,
};
pub use security::SessionLimits;
pub use serve::{
    run as run_serve, serve as serve_listener, PortalRouter, ServeError, ServeOptions, ServeReport,
    SessionSummary,
};
pub use session::{
    serve_connection as serve_session_connection,
    serve_connection_with_registry as serve_session_connection_with_registry, AssetStreamReport,
    AuthMode, GameLoopReport, GameReport, SessionError, SessionOptions, SessionReport,
};
pub use universe_runtime::{
    PortalArrival, PortalTransferReport, UniverseRuntime, UniverseRuntimeError,
};
pub use world_boot::{run as run_world_boot, WorldBootError, WorldBootOptions, WorldBootReport};
pub use world_config_tool::{
    run as run_world_config_tool, WorldConfigToolError, WorldConfigToolOptions,
    WorldConfigToolReport,
};
pub use world_thread::{
    BlockContainerOpen, BlockDamagePacket, LoadOutcome, NpcRuntimeReport, NpcSpawnReport,
    NpcSpawnTarget, PlayerSnapshot, RuntimeEntityEvent, SpawnSpecialReport, TickReport, ViewId,
    ViewReport, WorldEvent, WorldStartReport, WorldThread, WorldThreadError, WorldThreadOptions,
    WorldTimeState,
};
pub use world_tool::{run as run_chunk_tool, ChunkToolError, ChunkToolOptions, ChunkToolReport};
