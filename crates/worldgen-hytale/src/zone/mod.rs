// Ported from: `.hytale-source/.../worldgen/zone/`
//! Java `com.hypixel.hytale.server.worldgen.zone` package.
//!
//! Currently contains the unique-zone value types needed by the `zoom`
//! package ([][`unique`]) plus [`discovery_config::ZoneDiscoveryConfig`] and
//! [`zone_generator_result::ZoneGeneratorResult`]. The full `Zone` record and
//! the `ZonePatternGenerator`/`Provider` family arrive with the zone-package
//! porting subtasks.

pub mod discovery_config;
pub mod unique;
#[allow(clippy::module_inception)] // Java class Zone in Java package zone
pub mod zone;
pub mod zone_generator_result;

pub use discovery_config::ZoneDiscoveryConfig;
pub use unique::{Unique, UniqueCandidate, UniqueEntry};
pub use zone::Zone;
pub use zone_generator_result::ZoneGeneratorResult;
