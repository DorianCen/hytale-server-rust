//! `hytale-server` library surface.
//!
//! The binary is a thin CLI; the reusable boot paths live here so integration
//! tests can drive them without spawning a process.

#![deny(clippy::all)]

pub mod headless;
pub mod world_tool;

pub use headless::{run as run_headless, HeadlessError, HeadlessOptions, HeadlessReport};
pub use world_tool::{run as run_chunk_tool, ChunkToolError, ChunkToolOptions, ChunkToolReport};
