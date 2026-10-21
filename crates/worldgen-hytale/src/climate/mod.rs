// Ported from: `.hytale-source/.../worldgen/climate/*.java`
//! Java `worldgen.climate` package.
//!
//! | Java class          | Rust module               |
//! |---------------------|---------------------------|
//! | `ClimatePoint`      | [`climate_point`]         |
//! | `ClimateColor`      | [`climate_color`]         |
//! | `ClimateType`       | [`climate_type`]          |
//! | `util/{IntMap, DoubleMap}` | [`maps`]           |
//! | `util/DistanceTransform`   | [`distance_transform`] |
//! | `ClimateGraph`      | [`climate_graph`]         |
//! | `DirectGrid`        | [`direct_grid`]           |
//! | `ClimateNoise`      | [`climate_noise`]         |
//! | `ClimateSearch`     | [`climate_search`]        |
//!
//! `ClimateMaskProvider` / `UniqueClimateGenerator` ported with the loader
//! slice (Subtask 5).

pub mod climate_color;
pub mod climate_graph;
pub mod climate_mask_provider;
pub mod climate_noise;
pub mod climate_point;
pub mod climate_search;
pub mod climate_type;
pub mod direct_grid;
pub mod distance_transform;
pub mod maps;
pub mod unique_climate_generator;

pub use climate_color::ClimateColor;
pub use climate_graph::{ClimateGraph, FadeMode};
pub use climate_noise::{Buffer, ClimateNoise, Grid, Thresholds};
pub use climate_point::ClimatePoint;
pub use climate_search::{Range, Result, Rule};
pub use climate_type::{
    ClimateType, IS_ISLAND, IS_ISLAND_SHALLOW_OCEAN, IS_ISLAND_SHORE, IS_MAINLAND,
    IS_MAINLAND_SHALLOW_OCEAN, IS_MAINLAND_SHORE, IS_OCEAN, IS_SHORE, MASK,
};
pub use direct_grid::DirectGrid;
pub use maps::{DoubleMap, IntMap};

/// Java `ClimateSearch` — namespace type placeholder (the search is a set of
/// statics; re-exported constants keep the Java name discoverable).
pub struct ClimateSearch;

impl ClimateSearch {
    /// Java `ClimateSearch.STEP_SIZE`.
    pub const STEP_SIZE: i32 = climate_search::STEP_SIZE;
    /// Java `ClimateSearch.DEFAULT_RADIUS`.
    pub const DEFAULT_RADIUS: i32 = climate_search::DEFAULT_RADIUS;
    /// Java `ClimateSearch.MAX_RADIUS`.
    pub const MAX_RADIUS: i32 = climate_search::MAX_RADIUS;
    /// Java `ClimateSearch.TARGET_SCORE`.
    pub const TARGET_SCORE: f64 = climate_search::TARGET_SCORE;
}
