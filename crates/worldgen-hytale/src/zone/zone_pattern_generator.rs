// Ported from: `.hytale-source/.../worldgen/zone/ZonePatternGenerator.java`
//! Java `zone.ZonePatternGenerator` — the per-seed immutable zone pattern:
//! maps a world column to its [`Zone`] (plus border distance) through the
//! mask provider and the zone color mapping.
use std::rc::Rc;

use hytale_math::hash_util;
use procedural::noise::point::IPointGenerator;

use crate::chunk::mask_provider::MaskProvider;
use crate::zone::unique::Unique;
use crate::zone::zone::Zone;
use crate::zone::zone_color_mapping::ZoneColorMapping;
use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// Java `zone.ZonePatternGenerator`.
pub struct ZonePatternGenerator {
    /// Java `protected final IPointGenerator pointGenerator`.
    point_generator: Rc<dyn IPointGenerator>,
    /// Java `protected final Zone[] zones`.
    zones: Vec<Rc<Zone>>,
    /// Java `protected final Zone.Unique[] uniqueZones`.
    unique_zones: Vec<Unique>,
    /// Java `protected final MaskProvider maskProvider`.
    mask_provider: MaskProvider,
    /// Java `protected final ZoneColorMapping zoneColorMapping`.
    ///
    /// Deviation: see [`crate::zone::zone_pattern_provider::ZonePatternProvider`]
    /// — Java shares one reference; the Rust port clones per generator.
    zone_color_mapping: ZoneColorMapping,
}

impl ZonePatternGenerator {
    /// Java `ZonePatternGenerator(IPointGenerator, Zone[], Zone.Unique[],
    /// MaskProvider, ZoneColorMapping)`.
    pub fn new(
        point_generator: Rc<dyn IPointGenerator>,
        zones: Vec<Rc<Zone>>,
        unique_zones: Vec<Unique>,
        mask_provider: MaskProvider,
        zone_color_mapping: ZoneColorMapping,
    ) -> Self {
        Self {
            point_generator,
            zones,
            unique_zones,
            mask_provider,
            zone_color_mapping,
        }
    }

    /// Java `Zone[] getZones()`.
    pub fn get_zones(&self) -> &[Rc<Zone>] {
        &self.zones
    }

    /// Java `Zone.Unique[] getUniqueZones()`.
    pub fn get_unique_zones(&self) -> &[Unique] {
        &self.unique_zones
    }

    /// Java `MaskProvider getMaskProvider()`.
    pub fn get_mask_provider(&self) -> &MaskProvider {
        &self.mask_provider
    }

    /// Java `ZoneGeneratorResult generate(int seed, double x, double z)` —
    /// into a fresh result.
    pub fn generate(&self, seed: i32, x: f64, z: f64) -> ZoneGeneratorResult {
        let mut result = ZoneGeneratorResult::new();
        self.generate_into(seed, x, z, &mut result);
        result
    }

    /// Java `ZoneGeneratorResult generate(int seed, double x, double z,
    /// ZoneGeneratorResult result)` — transcribes the Java method exactly.
    pub fn generate_into<'a>(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        result: &'a mut ZoneGeneratorResult,
    ) -> &'a mut ZoneGeneratorResult {
        let rx = self.mask_provider.get_x(seed, x, z);
        let rz = self.mask_provider.get_y(seed, x, z);
        let mask = self.mask_provider.get(seed, rx, rz) & 0xFFFFFF;
        let zone_arr = self
            .zone_color_mapping
            .get(mask)
            .unwrap_or_else(|| panic!("Unknown zone colour mapping for mask: {mask}"));
        if zone_arr.len() == 1 {
            let mut distance = f64::INFINITY;
            if self.mask_provider.in_bounds(rx, rz) {
                distance = self.mask_provider.distance(rx, rz);
            }

            result.set_zone(Some(Rc::clone(&zone_arr[0])));
            result.set_border_distance(distance);
        } else {
            self.get_zone(seed, x, z, result, zone_arr);
        }

        result
    }

    /// Java `protected void getZone(int seed, double x, double z,
    /// ZoneGeneratorResult result, Zone[] zoneArr)`.
    fn get_zone(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        result: &mut ZoneGeneratorResult,
        zone_arr: &[Rc<Zone>],
    ) {
        let buf = self.point_generator.nearest_2d(seed, x, z);
        // Java: int index = ((int) HashUtil.hash(seed, buf.ix, buf.iy)
        //          & 0x7FFF_FFFF) % zoneArr.length;
        // `HashUtil.hash(long, long, long)` is `hash3`; the Java `(int)` cast
        // truncates the long to its low 32 bits (`as i32` in Rust).
        let index = ((hash_util::hash3(seed as i64, buf.ix as i64, buf.iy as i64) as i32)
            & 0x7FFF_FFFF) as usize
            % zone_arr.len();

        result.set_zone(Some(Rc::clone(&zone_arr[index])));
        result.set_border_distance(buf.distance);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    /// A fixed mask: every pixel has the same RGB color.
    fn solid_pixels(width: usize, height: usize, color: i32) -> PixelProvider {
        PixelProvider::from_pixels(width, height, vec![color; width * height])
    }

    fn mask_provider(width: usize, height: usize, color: i32) -> MaskProvider {
        MaskProvider::new(FuzzyZoom::new(
            empty_randomizer(),
            solid_pixels(width, height, color),
            1.0,
            1.0,
            0,
            0,
        ))
    }

    /// A zone whose biome pattern generator has an empty tile map (the
    /// generator never consults it).
    fn zone_with_stub_biomes(id: i32) -> Rc<Zone> {
        let point_generator: Rc<dyn IPointGenerator> =
            Rc::new(FixedPointGenerator { ix: 0, iy: 0 });
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

    /// Java `PointGenerator.nearest2D` with a fixed cell result — the buffer
    /// fields are pinned so the `hash & 0x7FFF_FFFF` modulo path can be
    /// verified against a hand-computed index.
    struct FixedPointGenerator {
        ix: i32,
        iy: i32,
    }

    impl IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            let mut buf = ResultBuffer2d::new();
            buf.ix = self.ix;
            buf.iy = self.iy;
            buf.distance = 2.5;
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

    fn generator(
        mask: MaskProvider,
        mut mapping: ZoneColorMapping,
        zones: Vec<Rc<Zone>>,
        color: i32,
    ) -> ZonePatternGenerator {
        mapping.add(color, zones[0].clone());
        ZonePatternGenerator::new(
            Rc::new(FixedPointGenerator { ix: 3, iy: 5 }),
            zones,
            Vec::new(),
            mask,
            mapping,
        )
    }

    #[test]
    fn single_zone_resolves_through_the_mask() {
        let mask = mask_provider(8, 8, 0x00FF00);
        let mapping = ZoneColorMapping::new();
        let zone = zone_with_stub_biomes(11);
        let gen = generator(mask, mapping, vec![zone], 0x00FF00);

        let result = gen.generate(42, 1.0, 1.0);
        assert_eq!(result.get_zone().map(|z| z.id), Some(11));
        // Java quirk faithfully reproduced: the mask is a single solid color,
        // so `PixelDistanceProvider.distanceSqToDifferentPixel` finds no
        // different pixel anywhere → the border distance stays
        // POSITIVE_INFINITY even for in-bounds queries.
        assert!(result.get_border_distance().is_infinite());
    }

    #[test]
    fn single_zone_out_of_bounds_gives_infinite_border_distance() {
        // A 1x1 mask: every query outside the single pixel is out of bounds,
        // so the single-zone branch keeps POSITIVE_INFINITY.
        let mask = mask_provider(1, 1, 0x0000FF);
        let mapping = ZoneColorMapping::new();
        let zone = zone_with_stub_biomes(5);
        let gen = generator(mask, mapping, vec![zone], 0x0000FF);

        let result = gen.generate(7, 100.0, 100.0);
        assert_eq!(result.get_zone().map(|z| z.id), Some(5));
        assert_eq!(
            result.get_border_distance(),
            f64::INFINITY,
            "Java keeps POSITIVE_INFINITY when the mask is out of bounds"
        );
    }

    #[test]
    #[should_panic(expected = "Unknown zone colour mapping for mask: 255")]
    fn unknown_mask_colour_panics() {
        let mask = mask_provider(4, 4, 0x0000FF);
        let mapping = ZoneColorMapping::new(); // nothing registered
        let gen = ZonePatternGenerator::new(
            Rc::new(FixedPointGenerator { ix: 0, iy: 0 }),
            vec![zone_with_stub_biomes(1)],
            Vec::new(),
            mask,
            mapping,
        );
        let _ = gen.generate(1, 0.0, 0.0);
    }

    #[test]
    fn multi_zone_index_follows_the_java_hash_modulo() {
        let mask = mask_provider(8, 8, 0xFFFF00);
        let mut mapping = ZoneColorMapping::new();
        let a = zone_with_stub_biomes(2);
        let b = zone_with_stub_biomes(3);
        mapping.add_zones(0xFFFF00, vec![a.clone(), b.clone()]);
        let gen = ZonePatternGenerator::new(
            Rc::new(FixedPointGenerator { ix: 3, iy: 5 }),
            vec![a, b],
            Vec::new(),
            mask,
            mapping,
        );

        // Java: ((int) HashUtil.hash(seed, 3, 5) & 0x7FFF_FFFF) % 2
        let expected_index = ((hash_util::hash3(42, 3, 5) as i32) & 0x7FFF_FFFF) as usize % 2;
        let expected_id = if expected_index == 0 { 2 } else { 3 };

        let result = gen.generate(42, 1.0, 1.0);
        assert_eq!(result.get_zone().map(|z| z.id), Some(expected_id));
        // Multi-zone: the border distance is the voronoi buffer distance.
        assert_eq!(result.get_border_distance(), 2.5);
    }

    #[test]
    fn generate_into_reuses_the_scratch_result() {
        let mask = mask_provider(8, 8, 0x00FF00);
        let mapping = ZoneColorMapping::new();
        let zone = zone_with_stub_biomes(11);
        let gen = generator(mask, mapping, vec![Rc::clone(&zone)], 0x00FF00);

        let mut result = ZoneGeneratorResult::new_with(Some(Rc::clone(&zone)), -1.0);
        let returned = gen.generate_into(42, 1.0, 1.0, &mut result);
        assert_eq!(returned.get_zone().map(|z| z.id), Some(11));
        // Solid single-color mask → no different pixel → infinite distance
        // (see the Java quirk note in `single_zone_resolves_through_the_mask`).
        assert!(returned.get_border_distance().is_infinite());
        // Java reuses the caller-provided scratch result instance.
        assert!(std::ptr::eq(returned, &result));
    }
}
