//! `hytale-server` library surface.
//!
//! The binary is a thin CLI; the reusable boot paths live here so integration
//! tests can drive them without spawning a process.

#![deny(clippy::all)]

pub mod chunk_serve;
pub mod chunk_streamer;
pub mod generation;
pub mod headless;
pub mod player_data_tool;
pub mod world_boot;
pub mod world_config_tool;
pub mod world_thread;
pub mod world_tool;

pub use chunk_serve::{
    run as run_chunk_serve, serve_connection as serve_chunk_connection, ChunkServeError,
    ChunkServeOptions, ChunkServeReport,
};
pub use chunk_streamer::{
    default_compression_level, send_chunk, send_view, ChunkSessionError, ChunkStreamReport,
};
pub use generation::{GenerationError, GeneratorOptions, WorldGenerator};
pub use headless::{run as run_headless, HeadlessError, HeadlessOptions, HeadlessReport};
pub use player_data_tool::{
    run as run_player_data_tool, PlayerDataToolError, PlayerDataToolOptions, PlayerDataToolReport,
};
pub use world_boot::{run as run_world_boot, WorldBootError, WorldBootOptions, WorldBootReport};
pub use world_config_tool::{
    run as run_world_config_tool, WorldConfigToolError, WorldConfigToolOptions,
    WorldConfigToolReport,
};
pub use world_thread::{
    LoadOutcome, TickReport, ViewReport, WorldEvent, WorldStartReport, WorldThread,
    WorldThreadError, WorldThreadOptions,
};
pub use world_tool::{run as run_chunk_tool, ChunkToolError, ChunkToolOptions, ChunkToolReport};
