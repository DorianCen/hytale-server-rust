// Ported from: `.hytale-source/.../worldgen/ZonePatternGeneratorCache.java`
//! Java `zone.ZonePatternGeneratorCache` — the per-process cache of the
//! immutable per-seed [`ZonePatternGenerator`]s.
//!
//! Java uses `Int2ObjectConcurrentHashMap.computeIfAbsent`; the Rust port uses
//! a `RwLock<HashMap>` (compute-once, shared `Rc` values). The struct is
//! intentionally `!Send`/`!Sync`: the generator (like every worldgen type
//! holding `Rc<dyn …>` trait objects) is not `Send`, so it cannot live in
//! `moka` — see `cache/mod.rs` for the discussion. Phase 6 pins the cache
//! to the owning thread.
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::RwLock;

use crate::zone::zone_pattern_generator::ZonePatternGenerator;
use crate::zone::zone_pattern_provider::ZonePatternProvider;

/// Java `zone.ZonePatternGeneratorCache`.
pub struct ZonePatternGeneratorCache {
    /// Java `Int2ObjectConcurrentHashMap.computeIfAbsent`'s function — the
    /// provider that rolls the per-seed generators.
    provider: ZonePatternProvider,
    /// Java `Int2ObjectConcurrentHashMap<ZonePatternGenerator> cache`.
    cache: RwLock<HashMap<i32, Rc<ZonePatternGenerator>>>,
}

impl ZonePatternGeneratorCache {
    /// Java `ZonePatternGeneratorCache(ZonePatternProvider provider)`.
    pub fn new(provider: ZonePatternProvider) -> Self {
        Self {
            provider,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Java `ZonePatternGenerator get(int seed)` — `computeIfAbsent` with
    /// the Java error wrapping: any compute failure is rethrown as
    /// `Error("Failed to receive UniquePrefabEntry for " + seed)`.
    pub fn get(&self, seed: i32) -> Rc<ZonePatternGenerator> {
        {
            let read = self.cache.read().unwrap_or_else(|p| p.into_inner());
            if let Some(generator) = read.get(&seed) {
                return Rc::clone(generator);
            }
        }

        // Java wraps *any* exception thrown by the compute function.
        let generator = catch_unwind(AssertUnwindSafe(|| self.provider.create_generator(seed)))
            .unwrap_or_else(|_| panic!("Failed to receive UniquePrefabEntry for {seed}"));

        let mut write = self.cache.write().unwrap_or_else(|p| p.into_inner());
        Rc::clone(write.entry(seed).or_insert(Rc::new(generator)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::point::IPointGenerator;
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
    use procedural::noise::support::Vector2i;

    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::chunk::mask_provider::MaskProvider;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::unique::{UniqueCandidate, UniqueEntry};
    use crate::zone::zone::Zone;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    fn fixed_point_generator() -> Rc<dyn IPointGenerator> {
        Rc::new(FixedPointGenerator { ix: 0, iy: 0 })
    }

    fn solid_mask(width: usize, height: usize, color: i32) -> MaskProvider {
        MaskProvider::new(FuzzyZoom::new(
            empty_randomizer(),
            PixelProvider::from_pixels(width, height, vec![color; width * height]),
            1.0,
            1.0,
            0,
            0,
        ))
    }

    /// A zone with an empty biome pattern generator (never consulted here).
    fn zone_with_stub_biomes(id: i32) -> Rc<Zone> {
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> =
            Rc::new(WeightedMap::builder().build());
        Rc::new(Zone {
            id,
            name: format!("zone-{id}"),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
                fixed_point_generator(),
                tile_biomes,
                Vec::new(),
            )),
        })
    }

    fn provider(unique_zones: Vec<UniqueCandidate>) -> ZonePatternProvider {
        ZonePatternProvider::new(
            fixed_point_generator(),
            vec![zone_with_stub_biomes(1)],
            unique_zones,
            solid_mask(4, 4, 0xFFFFFF),
            ZoneColorMapping::new(),
        )
    }

    /// Java `PointGenerator.nearest2D` with a fixed cell result.
    struct FixedPointGenerator {
        ix: i32,
        iy: i32,
    }

    impl IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            let mut buf = ResultBuffer2d::new();
            buf.ix = self.ix;
            buf.iy = self.iy;
            buf
        }

        fn nearest_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by these tests")
        }

        fn transition_2d(&self, _s: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            unimplemented!("not exercised by these tests")
        }

        fn transition_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by these tests")
        }

        fn collect(
            &self,
            _seed: i32,
            _min_x: f64,
            _min_y: f64,
            _max_x: f64,
            _max_y: f64,
            _consumer: &mut dyn FnMut(f64, f64),
        ) {
            unimplemented!("not exercised by these tests")
        }

        fn get_interval(&self) -> f64 {
            unimplemented!("not exercised by these tests")
        }
    }

    #[test]
    fn get_computes_once_and_shares_the_generator() {
        let cache = ZonePatternGeneratorCache::new(provider(Vec::new()));

        let first = cache.get(123);
        let second = cache.get(123);
        assert!(Rc::ptr_eq(&first, &second), "the cached Rc is shared");
    }

    #[test]
    fn get_caches_per_seed() {
        let cache = ZonePatternGeneratorCache::new(provider(Vec::new()));

        let a = cache.get(1);
        let b = cache.get(2);
        assert!(
            !Rc::ptr_eq(&a, &b),
            "different seeds get different generators"
        );
    }

    #[test]
    #[should_panic(expected = "Failed to receive UniquePrefabEntry for 42")]
    fn get_wraps_compute_failures_in_the_java_error() {
        // The candidate's radius-10 paint window around position (1,1) leaves
        // the 4x4 image: `setPixel` on a negative index panics (Java:
        // AIOOBE), and `get` must wrap it in the Java error.
        let zone = zone_with_stub_biomes(1);
        let candidate = UniqueCandidate::new(
            UniqueEntry::new(zone, 0x112233, vec![0xFFFFFF], 10, 0),
            vec![Vector2i::new(1, 1)],
        );
        let cache = ZonePatternGeneratorCache::new(provider(vec![candidate]));
        let _ = cache.get(42);
    }
}
