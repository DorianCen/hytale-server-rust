// Ported from: `.hytale-source/.../worldgen/loader/biome/**`
//! Java `loader.biome` package — the biome chain (the weighted tile-biome
//! map, the custom biomes, the pattern generator, the interpolation rules,
//! the biome masks and the custom-biome generators).

pub mod biome_interpolation_json_loader;
pub mod biome_json_loader;
pub mod biome_mask_json_loader;
pub mod biome_pattern_generator_json_loader;
pub mod custom_biome_generator_json_loader;
pub mod custom_biome_json_loader;
pub mod tile_biome_json_loader;
