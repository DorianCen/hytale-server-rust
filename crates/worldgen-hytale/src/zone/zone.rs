// Ported from: `.hytale-source/.../worldgen/zone/Zone.java` (RECORD — partial slice)
//! Java `zone.Zone` record.
//!
//! ⚠ Partial slice: `Zone` is a Java record with six components. The ones
//! whose packages have not ported yet are appended when they land:
//!
//! - `caveGenerator` (Phase 4, cave package)
//! - `biomePatternGenerator` (Subtask 4, biome package back-reference)
//! - `uniquePrefabContainer` (Phase 5, prefab package)
//!
//! `id`, `name` and `discoveryConfig` are carried as-is; nothing in the
//! current phases reads the deferred components.
use std::rc::Rc;

use crate::zone::discovery_config::ZoneDiscoveryConfig;

/// Java `zone.Zone` — a large area in the world with specific
/// characteristics (partial: see module docs).
#[derive(Debug, Clone)]
pub struct Zone {
    /// Java `int id` — the unique identifier of the zone.
    pub id: i32,
    /// Java `String name`.
    pub name: String,
    /// Java `@Nonnull ZoneDiscoveryConfig discoveryConfig`.
    pub discovery_config: Rc<ZoneDiscoveryConfig>,
}
