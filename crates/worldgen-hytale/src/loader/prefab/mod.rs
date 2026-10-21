// Ported from: `.hytale-source/.../worldgen/loader/prefab/**`
//! Java `loader.prefab` package — the loaders the prefab *containers* and
//! the cave configurations share.
//!
//! | Java class                       | Rust module                                        |
//! |----------------------------------|----------------------------------------------------|
//! | `BlockPlacementMaskJsonLoader`   | [`block_placement_mask_json_loader`]               |
//! | `WeightedPrefabMapJsonLoader`    | [`weighted_prefab_map_json_loader`]                |
//! | `PrefabPatternGeneratorJsonLoader` | [`prefab_pattern_generator_json_loader`]          |
//! | `UniquePrefabConfigurationJsonLoader` | [`unique::unique_prefab_configuration_json_loader`] |

pub mod block_placement_mask_json_loader;
pub mod prefab_pattern_generator_json_loader;
pub mod unique;
pub mod weighted_prefab_map_json_loader;
