//! `worldgen-hytale` — Rust port of the Hytale `WorldGenerator`
//! (`com.hypixel.hytale.server.worldgen.**`).
//!
//! Bit-exactness contract: same seed → identical chunks, verified against the
//! Java reference via `testdata/golden/` (see PORTING.md).

pub mod biome;
pub mod cache;
pub mod cave;
pub mod chunk;
pub mod climate;
pub mod container;
pub mod loader;
pub mod prefab;
pub mod supplier;
pub mod unique_zone_placement;
pub mod util;
pub mod weighted_map;
pub mod world_gen_config;
pub mod zone;
pub mod zoom;
