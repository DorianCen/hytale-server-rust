// Ported from: `.hytale-source/.../worldgen/zone/Zone.java` (RECORD — partial slice)
//! Java `zone.Zone` record.
//!
//! ⚠ Partial slice: `Zone` is a Java record with six components. The ones
//! whose packages have not ported yet are appended when they land:
//!
//! - `caveGenerator` (Phase 4, cave package)
//! - `uniquePrefabContainer` (Phase 5, prefab package)
//!
//! `id`, `name`, `discoveryConfig` and `biomePatternGenerator` are carried
//! as-is.
use std::rc::Rc;

use crate::biome::biome_pattern_generator::BiomePatternGenerator;
use crate::zone::discovery_config::ZoneDiscoveryConfig;

/// Java `zone.Zone` — a large area in the world with specific
/// characteristics (partial: see module docs).
#[derive(Clone)]
pub struct Zone {
    /// Java `int id` — the unique identifier of the zone.
    pub id: i32,
    /// Java `String name`.
    pub name: String,
    /// Java `@Nonnull ZoneDiscoveryConfig discoveryConfig`.
    pub discovery_config: Rc<ZoneDiscoveryConfig>,
    /// Java `@Nonnull BiomePatternGenerator biomePatternGenerator`.
    pub biome_pattern_generator: Rc<BiomePatternGenerator>,
}

impl std::fmt::Debug for Zone {
    /// `ZoneGeneratorResult` (and other shared types) derive `Debug`, so
    /// `Zone` must implement it; `BiomePatternGenerator` (biome package)
    /// does not derive `Debug`, so the generator is represented by its
    /// extents here.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Zone")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("discovery_config", &self.discovery_config)
            .field(
                "biome_pattern_generator",
                &self.biome_pattern_generator.get_extents(),
            )
            .finish()
    }
}
