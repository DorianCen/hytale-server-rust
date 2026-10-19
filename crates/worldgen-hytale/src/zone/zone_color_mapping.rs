// Ported from: `.hytale-source/.../worldgen/zone/ZoneColorMapping.java`
//! Java `zone.ZoneColorMapping` — maps a mask RGB value to the candidate
//! zones of that color (one entry per color; the multi-entry case is the
//! "pick one of several zones by voronoi hash" pattern).
use std::collections::HashMap;
use std::rc::Rc;

use crate::zone::zone::Zone;

/// Java `zone.ZoneColorMapping` (`Int2ObjectMap<Zone[]>`).
#[derive(Clone)]
pub struct ZoneColorMapping {
    /// Java `Int2ObjectMap<Zone[]> mapping`.
    mapping: HashMap<i32, Vec<Rc<Zone>>>,
}

impl ZoneColorMapping {
    /// Java `ZoneColorMapping()` (`Int2ObjectOpenHashMap`).
    pub fn new() -> Self {
        Self {
            mapping: HashMap::new(),
        }
    }

    /// Java `void add(int rgb, Zone zone)` — registers a single zone under
    /// the color (delegates to the array overload with a one-element array).
    pub fn add(&mut self, rgb: i32, zone: Rc<Zone>) {
        self.add_zones(rgb, vec![zone]);
    }

    /// Java `void add(int rgb, Zone[] zones)`.
    pub fn add_zones(&mut self, rgb: i32, zones: Vec<Rc<Zone>>) {
        self.mapping.insert(rgb, zones);
    }

    /// Java `Zone[] get(int rgb)` — `null` when the color is unknown.
    pub fn get(&self, rgb: i32) -> Option<&Vec<Rc<Zone>>> {
        self.mapping.get(&rgb)
    }
}

impl Default for ZoneColorMapping {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone::Zone;
    use procedural::noise::cell::evaluator::normal_point_evaluator::NormalPointEvaluator;
    use procedural::noise::cell::grid_cell_distance_function::GridCellDistanceFunction;
    use procedural::noise::point::{IPointGenerator, PointGenerator};
    use std::rc::Rc;

    fn zone(id: i32) -> Rc<Zone> {
        let point_generator: Rc<dyn IPointGenerator> = Rc::new(PointGenerator::new(
            0,
            GridCellDistanceFunction::distance_function(),
            NormalPointEvaluator::euclidean(),
        ));
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> =
            Rc::new(WeightedMap::builder().build());
        Rc::new(Zone {
            id,
            name: format!("zone-{id}"),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
                point_generator,
                tile_biomes,
                Vec::new(),
            )),
        })
    }

    #[test]
    fn add_single_zone_then_get() {
        let mut mapping = ZoneColorMapping::new();
        let z = zone(7);
        mapping.add(0x112233, Rc::clone(&z));
        let got = mapping.get(0x112233).expect("the color is registered");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, 7);
    }

    #[test]
    fn add_multiple_zones_preserves_order() {
        let mut mapping = ZoneColorMapping::new();
        let a = zone(1);
        let b = zone(2);
        let c = zone(3);
        mapping.add_zones(0xFF0000, vec![Rc::clone(&a), Rc::clone(&b), Rc::clone(&c)]);
        let got = mapping.get(0xFF0000).expect("the color is registered");
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].id, 1);
        assert_eq!(got[1].id, 2);
        assert_eq!(got[2].id, 3);
    }

    #[test]
    fn get_unknown_rgb_is_none() {
        let mapping = ZoneColorMapping::new();
        assert!(mapping.get(0x000001).is_none());
    }
}
