// Ported from: `.hytale-source/.../worldgen/loader/container/*.java`
//! Java `loader.container` package — the JSON loaders for the biome/zone
//! containers.
//!
//! The loaders resolve block, fluid and environment names through the
//! [`worldgen_model::registry::Registries`] snapshot threaded down from
//! `ChunkGeneratorJsonLoader`.

pub mod cover_container_json_loader;
pub mod environment_container_json_loader;
pub mod fade_container_json_loader;
pub mod layer_container_json_loader;
pub mod prefab_container_json_loader;
pub mod tint_container_json_loader;
pub mod unique_prefab_container_json_loader;
pub mod water_container_json_loader;
