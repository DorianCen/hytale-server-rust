// Ported from: `.hytale-source/.../worldgen/zone/Zone.java` (RECORD — full slice)
//! Java `zone.Zone` record.
//!
//! The record has six components; all of them are carried by the Rust port:
//! `id`, `name`, `discoveryConfig`, `caveGenerator` (Phase 4),
//! `biomePatternGenerator` (Phase 2) and `uniquePrefabContainer` (Phase 5b
//! — the container; Phase 5c — its *generators*, which are loaded from the
//! zone's `"UniquePrefabs"` key and placed per seed).
use std::rc::Rc;

use crate::biome::biome_pattern_generator::BiomePatternGenerator;
use crate::cave::cave_generator::CaveGenerator;
use crate::container::unique_prefab_container::UniquePrefabContainer;
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
    /// Java `@Nullable CaveGenerator caveGenerator` — the caves of the zone
    /// (`null` when the zone has no caves).
    pub cave_generator: Option<Rc<dyn CaveGenerator>>,
    /// Java `@Nonnull BiomePatternGenerator biomePatternGenerator`.
    pub biome_pattern_generator: Rc<BiomePatternGenerator>,
    /// Java `@Nonnull UniquePrefabContainer uniquePrefabContainer` — the
    /// *unique* prefabs of the zone (the spawn points, the landmarks, …).
    pub unique_prefab_container: Rc<UniquePrefabContainer>,
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
                "cave_generator",
                &self.cave_generator.as_ref().map(|_| "<cave generator>"),
            )
            .field(
                "biome_pattern_generator",
                &self.biome_pattern_generator.get_extents(),
            )
            .field(
                "unique_prefab_container",
                &self.unique_prefab_container.get_seed_offset(),
            )
            .finish()
    }
}
