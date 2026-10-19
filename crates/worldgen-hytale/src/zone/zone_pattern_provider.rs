// Ported from: `.hytale-source/.../worldgen/zone/ZonePatternProvider.java`
//! Java `zone.ZonePatternProvider` — the per-world (load-time) holder of the
//! zone pattern: owns the unique-zone candidates and produces one immutable
//! [`ZonePatternGenerator`] per seed.
use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::noise::point::IPointGenerator;

use crate::chunk::mask_provider::MaskProvider;
use crate::zone::unique::UniqueCandidate;
use crate::zone::zone::Zone;
use crate::zone::zone_color_mapping::ZoneColorMapping;
use crate::zone::zone_pattern_generator::ZonePatternGenerator;

/// Java `zone.ZonePatternProvider`.
pub struct ZonePatternProvider {
    /// Java `protected final IPointGenerator pointGenerator`.
    point_generator: Rc<dyn IPointGenerator>,
    /// Java `protected final Zone[] zones`.
    zones: Vec<Rc<Zone>>,
    /// Java `protected final Zone.UniqueCandidate[] uniqueZones`.
    unique_zones: Vec<UniqueCandidate>,
    /// Java `protected final MaskProvider maskProvider`.
    mask_provider: MaskProvider,
    /// Java `protected final ZoneColorMapping zoneColorMapping`.
    ///
    /// Deviation: Java shares the mapping by reference between the provider
    /// and every per-seed generator; the Rust port clones it into each
    /// generator (a shallow `Rc` clone per entry). The mapping is only
    /// mutated in the constructor, so the semantics are identical.
    zone_color_mapping: ZoneColorMapping,
    /// Java `protected final int maxExtent`.
    max_extent: i32,
}

impl ZonePatternProvider {
    /// Java `ZonePatternProvider(IPointGenerator, Zone[],
    /// Zone.UniqueCandidate[], MaskProvider, ZoneColorMapping)`.
    pub fn new(
        point_generator: Rc<dyn IPointGenerator>,
        zones: Vec<Rc<Zone>>,
        unique_zones: Vec<UniqueCandidate>,
        mask_provider: MaskProvider,
        mut zone_color_mapping: ZoneColorMapping,
    ) -> Self {
        let max_extent = Self::compute_max_extent(&zones);

        for unique_zone in &unique_zones {
            zone_color_mapping.add(unique_zone.zone.color, Rc::clone(&unique_zone.zone.zone));
        }

        Self {
            point_generator,
            zones,
            unique_zones,
            mask_provider,
            zone_color_mapping,
            max_extent,
        }
    }

    /// Java `int getMaxExtent()`.
    pub fn get_max_extent(&self) -> i32 {
        self.max_extent
    }

    /// Java `Zone[] getZones()`.
    pub fn get_zones(&self) -> &[Rc<Zone>] {
        &self.zones
    }

    /// Java `MaskProvider getMaskProvider()`.
    pub fn get_mask_provider(&self) -> &MaskProvider {
        &self.mask_provider
    }

    /// Java `ZonePatternGenerator createGenerator(int seed)` — rolls the
    /// unique zones into the mask and returns the per-seed generator.
    pub fn create_generator(&self, seed: i32) -> ZonePatternGenerator {
        let mut random = FastRandom::new(seed as i64);
        let mut unique_zones = Vec::with_capacity(self.unique_zones.len());
        let mask_provider = self.mask_provider.generate_unique_zones(
            seed,
            &self.unique_zones,
            &mut random,
            &mut unique_zones,
        );

        ZonePatternGenerator::new(
            Rc::clone(&self.point_generator),
            self.zones.clone(),
            unique_zones,
            mask_provider,
            self.zone_color_mapping.clone(),
        )
    }

    /// Java `private static int getMaxExtent(Zone[] zones)` — the maximum
    /// prefab-container size across all zone biomes (0 when no biome has a
    /// prefab container). (Rust renames the private static to
    /// `compute_max_extent` because Rust has no overloads.)
    fn compute_max_extent(zones: &[Rc<Zone>]) -> i32 {
        let mut max = 0;
        for zone in zones {
            for biome in zone.biome_pattern_generator.get_biomes() {
                if let Some(prefab_container) = biome.get_prefab_container() {
                    max = max.max(prefab_container.get_max_size());
                }
            }
        }
        max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
    use procedural::noise::support::Vector2i;

    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::prefab_container::{
        PrefabContainer, PrefabContainerEntry, PrefabSupplier, PrefabSupplierKey,
    };
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::util::constant_noise_property::default_zero;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::unique::{UniqueCandidate, UniqueEntry};
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    /// A zone biome with a prefab container of the given max size.
    fn biome_with_prefab_size(id: i32, max_size: i32) -> Rc<TileBiome> {
        let supplier: Rc<dyn PrefabSupplier> = Rc::new(StubSupplier);
        let prefabs: Rc<dyn IWeightedMap<PrefabSupplierKey>> = {
            let mut builder = WeightedMap::<PrefabSupplierKey>::builder();
            builder.put(PrefabSupplierKey::new(supplier), 1.0);
            Rc::new(builder.build())
        };
        let container = PrefabContainer::new(vec![PrefabContainerEntry::new(prefabs, max_size, 0)]);
        let interpreter: Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter> =
            Rc::new(
                procedural::noise::condition::BasicHeightThresholdInterpreter::new(&[0], &[0.5], 1),
            );
        let noise: Rc<dyn procedural::noise::support::NoiseProperty> = default_zero();
        let cover: Rc<CoverContainer> = Rc::new(CoverContainer::new(Vec::new()));
        let layer: Rc<LayerContainer> = Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new()));
        let tint: Rc<TintContainer> = Rc::new(TintContainer::new(
            DefaultTintContainerEntry::new(empty_map(), noise.clone()),
            Vec::new(),
        ));
        let environment: Rc<EnvironmentContainer> = Rc::new(EnvironmentContainer::new(
            DefaultEnvironmentContainerEntry::new(empty_map(), noise.clone()),
            Vec::new(),
        ));
        let water: Rc<WaterContainer> = Rc::new(WaterContainer::new(Vec::new()));
        let fade: Rc<FadeContainer> =
            Rc::new(FadeContainer::new(0.0, 1.0, 0.0, 1.0, f64::NEG_INFINITY));
        Rc::new(TileBiome::new(
            id,
            format!("biome-{id}"),
            Rc::new((*crate::biome::biome_interpolation::DEFAULT).clone()),
            interpreter,
            cover,
            layer,
            Some(Rc::new(container)),
            tint,
            environment,
            water,
            fade,
            noise,
            1.0,
            1.0,
            0,
        ))
    }

    fn empty_map() -> Rc<dyn IWeightedMap<i32>> {
        Rc::new(WeightedMap::<i32>::builder().build())
    }

    struct StubSupplier;
    impl PrefabSupplier for StubSupplier {}

    fn fixed_point_generator() -> Rc<dyn IPointGenerator> {
        Rc::new(FixedPointGenerator { ix: 0, iy: 0 })
    }

    /// A zone with one biome per given prefab max size.
    fn zone_with_biomes(id: i32, biome_sizes: &[i32]) -> Rc<Zone> {
        let tile_biomes: Vec<Rc<TileBiome>> = biome_sizes
            .iter()
            .enumerate()
            .map(|(i, &size)| biome_with_prefab_size(id * 100 + i as i32, size))
            .collect();
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        for biome in &tile_biomes {
            builder.put(Rc::clone(biome), 1.0);
        }
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());
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

    fn unique_candidate(
        zone: Rc<Zone>,
        color: i32,
        parent: i32,
        positions: Vec<Vector2i>,
    ) -> UniqueCandidate {
        UniqueCandidate::new(UniqueEntry::new(zone, color, vec![parent], 0, 0), positions)
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
    fn max_extent_is_the_largest_prefab_size() {
        let zones = vec![
            zone_with_biomes(1, &[3, 11]),
            zone_with_biomes(2, &[0, 7]),
            zone_with_biomes(3, &[]),
        ];
        let provider = ZonePatternProvider::new(
            fixed_point_generator(),
            zones,
            Vec::new(),
            solid_mask(4, 4, 0xFFFFFF),
            ZoneColorMapping::new(),
        );
        assert_eq!(provider.get_max_extent(), 11);
    }

    #[test]
    fn max_extent_is_zero_without_prefab_containers() {
        let zones = vec![zone_with_biomes(1, &[])];
        let provider = ZonePatternProvider::new(
            fixed_point_generator(),
            zones,
            Vec::new(),
            solid_mask(4, 4, 0xFFFFFF),
            ZoneColorMapping::new(),
        );
        assert_eq!(provider.get_max_extent(), 0);
    }

    #[test]
    fn create_generator_places_unique_zones_and_resolves_them() {
        // 4x4 image of color 0xFFFFFF; the unique candidate (parent color
        // 0xFFFFFF present in the image, one viable position) paints pixel
        // (1,1) with 0x112233 when the per-seed generator is created.
        let zone = zone_with_biomes(1, &[]);
        let candidate = unique_candidate(
            Rc::clone(&zone),
            0x112233,
            0xFFFFFF,
            vec![Vector2i::new(1, 1)],
        );
        let provider = ZonePatternProvider::new(
            fixed_point_generator(),
            vec![Rc::clone(&zone)],
            vec![candidate],
            solid_mask(4, 4, 0xFFFFFF),
            ZoneColorMapping::new(),
        );

        let generator = provider.create_generator(99);
        assert_eq!(generator.get_unique_zones().len(), 1);

        // The constructor registered the unique color in the shared mapping,
        // and the painted pixel now resolves to the zone.
        let result = generator.generate(99, 1.0, 1.0);
        assert_eq!(result.get_zone().map(|z| z.id), Some(1));
    }
}
