//! `hytale-world` — universe, world, and runtime chunk state.
//!
//! This crate owns the universe/world lifecycle, the runtime `ChunkColumn` model with
//! palettes, lighting and heightmaps, the chunk store, player data, world time and
//! weather, and the `WorldGenRuntime` adapter around the existing worldgen crates. It
//! consumes the asset and storage layers and is the only place where
//! `worldgen_model::GeneratedChunk` is converted into runtime chunks.

#![deny(clippy::all)]

pub mod chunk;
pub mod registry;
pub mod world;
pub mod world_config;
