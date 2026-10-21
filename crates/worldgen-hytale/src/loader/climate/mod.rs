// Ported from: `.hytale-source/.../worldgen/loader/climate/**`
//! Java `worldgen.loader.climate` package — the loaders of the procedural
//! climate mask (`Mask.json`).
//!
//! | Java class                       | Rust module                                  |
//! |----------------------------------|---------------------------------------------|
//! | `ClimateMaskJsonLoader`          | [`climate_mask_json_loader`]                |
//! | `ClimateNoiseJsonLoader`         | [`climate_noise_json_loader`]               |
//! | `ClimateGridJsonLoader`          | [`climate_grid_json_loader`]                |
//! | `ClimateGraphJsonLoader`         | [`climate_graph_json_loader`]               |
//! | `ClimateTypeJsonLoader`          | [`climate_type_json_loader`]                |
//! | `ClimateColorJsonLoader`         | [`climate_color_json_loader`]               |
//! | `ClimatePointJsonLoader`         | [`climate_point_json_loader`]               |
//! | `ClimateRuleJsonLoader`          | [`climate_rule_json_loader`]                |
//! | `ContinentThresholdsJsonLoader`  | [`continent_thresholds_json_loader`]        |
//! | `UniqueClimateGeneratorJsonLoader`| [`unique_climate_generator_json_loader`]   |
//! | `UniqueClimateJsonLoader`        | [`unique_climate_json_loader`]              |
//!
//! None of the climate loaders appends a suffix to the seed string (the
//! Java constructors pass the seed through unchanged) — the noise property
//! loaders below them do (`".NoiseProperty"`).

pub mod climate_color_json_loader;
pub mod climate_graph_json_loader;
pub mod climate_grid_json_loader;
pub mod climate_mask_json_loader;
pub mod climate_noise_json_loader;
pub mod climate_point_json_loader;
pub mod climate_rule_json_loader;
pub mod climate_type_json_loader;
pub mod continent_thresholds_json_loader;
pub mod unique_climate_generator_json_loader;
pub mod unique_climate_json_loader;
