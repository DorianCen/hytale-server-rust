// Ported from: `.hytale-source/.../worldgen/loader/context/**`
//! Java `loader.context` package — the file-context tree that assigns ids to
//! zones and biomes in asset-scan order and resolves file references back to
//! their owning zone.

pub mod biome_file_context;
pub mod cave_file_context;
pub mod file_context;
pub mod file_context_loader;
pub mod file_loading_context;
pub mod zone_file_context;
