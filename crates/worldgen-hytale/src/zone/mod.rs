// Ported from: `.hytale-source/.../worldgen/zone/`
//! Java `com.hypixel.hytale.server.worldgen.zone` package.
//!
//! | Java class                   | Rust module                      |
//! |------------------------------|----------------------------------|
//! | `Zone` (nested records)      | [`zone`], [`unique`]             |
//! | `ZoneColorMapping`           | [`zone_color_mapping`]           |
//! | `ZonePatternGenerator`       | [`zone_pattern_generator`]       |
//! | `ZonePatternProvider`        | [`zone_pattern_provider`]        |
//! | `ZonePatternGeneratorCache`  | [`zone_pattern_generator_cache`] |
//!
//! The partial `Zone` record (id/name/discoveryConfig/caveGenerator/
//! biomePatternGenerator) is completed by Phase 5
//! (`uniquePrefabContainer`).

pub mod discovery_config;
pub mod unique;
#[allow(clippy::module_inception)] // Java class Zone in Java package zone
pub mod zone;
pub mod zone_color_mapping;
pub mod zone_generator_result;
pub mod zone_pattern_generator;
pub mod zone_pattern_generator_cache;
pub mod zone_pattern_provider;

pub use discovery_config::ZoneDiscoveryConfig;
pub use unique::{Unique, UniqueCandidate, UniqueEntry};
pub use zone::Zone;
pub use zone_color_mapping::ZoneColorMapping;
pub use zone_generator_result::ZoneGeneratorResult;
pub use zone_pattern_generator::ZonePatternGenerator;
pub use zone_pattern_generator_cache::ZonePatternGeneratorCache;
pub use zone_pattern_provider::ZonePatternProvider;
