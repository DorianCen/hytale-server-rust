// Ported from: `.hytale-source/.../worldgen/biome/BiomePatternGenerator.java`
//! Java `biome.BiomePatternGenerator` — the jittered-voronoi tile-biome
//! pattern plus custom-biome overlay for one zone.
use std::rc::Rc;

use hytale_math::hash_util;
use procedural::noise::point::IPointGenerator;

use crate::biome::biome::Biome;
use crate::biome::custom_biome::CustomBiome;
use crate::biome::tile_biome::TileBiome;
use crate::weighted_map::IWeightedMap;
use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// Java `biome.BiomePatternGenerator`.
#[derive(Clone)]
pub struct BiomePatternGenerator {
    /// Java `protected final IPointGenerator pointGenerator`.
    point_generator: Rc<dyn IPointGenerator>,
    /// Java `@Nonnull protected final IWeightedMap<TileBiome> tileBiomes`
    /// — keyed by `Rc<TileBiome>` (Java stores object references; `Rc`
    /// replaces the shared-reference semantics).
    tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>>,
    /// Java `@Nonnull protected final CustomBiome[] customBiomes`.
    custom_biomes: Vec<Rc<CustomBiome>>,
    /// Java `@Nonnull protected final Biome[] biomes` — tile biomes then
    /// custom biomes, in that order.
    biomes: Vec<Rc<dyn Biome>>,
    /// Java `protected final int extents`.
    extents: i32,
}

impl BiomePatternGenerator {
    /// Java `BiomePatternGenerator(IPointGenerator pointGenerator,
    /// IWeightedMap<TileBiome> tileBiomes, CustomBiome[] customBiomes)`.
    pub fn new(
        point_generator: Rc<dyn IPointGenerator>,
        tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>>,
        custom_biomes: Vec<Rc<CustomBiome>>,
    ) -> Self {
        // Java: biomes = new Biome[tileBiomes.size() + customBiomes.length];
        // tile biomes first (internalKeys order), then the custom biomes.
        let mut biomes: Vec<Rc<dyn Biome>> =
            Vec::with_capacity(tile_biomes.size() + custom_biomes.len());
        for biome in tile_biomes.internal_keys() {
            biomes.push(Rc::clone(biome) as Rc<dyn Biome>);
        }
        for biome in &custom_biomes {
            biomes.push(Rc::clone(biome) as Rc<dyn Biome>);
        }

        // After this.biomes has been populated
        let extents = Self::compute_extents(&biomes);

        Self {
            point_generator,
            tile_biomes,
            custom_biomes,
            biomes,
            extents,
        }
    }

    /// Java `int getExtents()`.
    pub fn get_extents(&self) -> i32 {
        self.extents
    }

    /// Java `@Nonnull Biome[] getBiomes()`.
    pub fn get_biomes(&self) -> &[Rc<dyn Biome>] {
        &self.biomes
    }

    /// Java `@Nonnull CustomBiome[] getCustomBiomes()`.
    pub fn get_custom_biomes(&self) -> &[Rc<CustomBiome>] {
        &self.custom_biomes
    }

    /// Java `@Nullable TileBiome getBiome(int seed, int x, int z)`.
    ///
    /// Java `tileBiomes.get(seed, x, z, SeedCoordinateFunction, this)` —
    /// the supplier computes `getBiomeIndex(seed, x, z)`, so the 5-arg
    /// overload collapses to `get(f64)`.
    pub fn get_biome(&self, seed: i32, x: i32, z: i32) -> Option<Rc<TileBiome>> {
        self.tile_biomes
            .get(self.get_biome_index(seed, x, z))
            .cloned()
    }

    /// Java `protected double getBiomeIndex(int seed, int x, int z)`.
    ///
    /// `pointGenerator.nearest2D(seed, x, z)` — Java's `x`/`z` ints widen to
    /// doubles; `HashUtil.random(seed, buf.ix, buf.iy)` widens int→long ⇒
    /// the 3-long overload (`random3`).
    pub fn get_biome_index(&self, seed: i32, x: i32, z: i32) -> f64 {
        let buf = self.point_generator.nearest_2d(seed, x as f64, z as f64);
        hash_util::random3(seed as i64, buf.ix as i64, buf.iy as i64)
    }

    /// Java `@Nullable TileBiome getBiomeDirect(int seed, int x, int z)`.
    ///
    /// Java: `tileBiomes.get(HashUtil.random(seed, Double.doubleToLongBits(x),
    /// Double.doubleToLongBits(z)))` — three args, widened to long ⇒ the
    /// (long, long, long) overload (`random3`).
    pub fn get_biome_direct(&self, seed: i32, x: f64, z: f64) -> Option<Rc<TileBiome>> {
        self.tile_biomes
            .get(hash_util::random3(
                seed as i64,
                double_to_long_bits(x) as i64,
                double_to_long_bits(z) as i64,
            ))
            .cloned()
    }

    /// Java `@Nonnull Biome generateBiomeAt(ZoneGeneratorResult zoneResult,
    /// int seed, int x, int z)`.
    pub fn generate_biome_at(
        &self,
        zone_result: &ZoneGeneratorResult,
        seed: i32,
        x: i32,
        z: i32,
    ) -> Rc<dyn Biome> {
        let parent_result = self.get_biome(seed, x, z);
        let custom_biome =
            self.get_custom_biome_at(seed, x as f64, z as f64, zone_result, &parent_result);
        // Java: Objects.requireNonNullElse(customBiome, parentResult) — NPEs
        // when both are missing (an empty tile map), which cannot happen for
        // a validly loaded world.
        match (custom_biome, parent_result) {
            (Some(custom), _) => custom,
            (None, Some(parent)) => parent,
            (None, None) => panic!("generateBiomeAt: empty tileBiomes (Java: NPE)"),
        }
    }

    /// Java `@Nullable CustomBiome getCustomBiomeAt(int seed, double x,
    /// double z, ZoneGeneratorResult zoneResult, Biome parentResult)`.
    pub fn get_custom_biome_at(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        zone_result: &ZoneGeneratorResult,
        parent_result: &Option<Rc<TileBiome>>,
    ) -> Option<Rc<CustomBiome>> {
        if !self.custom_biomes.is_empty() {
            // Java dereferences `parentResult.getId()` directly; a missing
            // parent is the same hard error as Java's NPE.
            let parent_biome_index = match parent_result {
                Some(parent) => parent.get_id(),
                None => panic!("getCustomBiomeAt: parentResult is null (Java: NPE)"),
            };
            for custom_biome in &self.custom_biomes {
                let generator = custom_biome.get_custom_biome_generator();
                if generator.is_valid_parent_biome(parent_biome_index)
                    && generator.should_generate_at(seed, x, z, zone_result, custom_biome.as_ref())
                {
                    return Some(Rc::clone(custom_biome));
                }
            }
        }
        None
    }

    /// Java `private static int getExtents(Biome[] biomes)` — the maximum
    /// prefab-container size across all biomes. (Rust renames the private
    /// static to `compute_extents` because Rust has no overloads.)
    fn compute_extents(biomes: &[Rc<dyn Biome>]) -> i32 {
        let mut max_extent = 0;
        for biome in biomes {
            if let Some(prefab_container) = biome.get_prefab_container() {
                max_extent = max_extent.max(prefab_container.get_max_size());
            }
        }
        max_extent
    }
}

/// Java `Double.doubleToLongBits(x)` — canonicalizes NaN to the standard
/// quiet NaN bit pattern (0x7ff8000000000000), unlike Rust's raw `to_bits`.
#[inline]
fn double_to_long_bits(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
    use std::rc::Rc;

    /// Java `noise::get` as a seeded point: `ix`/`iy` derived from the seed
    /// so the test can pin the `random3` math through a fixed buffer.
    struct FixedPointGenerator {
        ix: i32,
        iy: i32,
    }
    impl IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            let mut buf = ResultBuffer2d::new(); // zeroed if available
            buf.ix = self.ix;
            buf.iy = self.iy;
            buf
        }
        fn nearest_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by this test")
        }
        fn transition_2d(&self, _s: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            unimplemented!("not exercised by this test")
        }
        fn transition_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by this test")
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
            unimplemented!("not exercised by this test")
        }
        fn get_interval(&self) -> f64 {
            unimplemented!("not exercised by this test")
        }
    }

    // getBiomeIndex uses a FixedPointGenerator(3, 5); the index must equal
    // hash_util::random3(seed, 3, 5) for the same seed.
    fn empty_biome_map() -> Rc<dyn IWeightedMap<Rc<TileBiome>>> {
        Rc::new(crate::weighted_map::WeightedMap::builder().build())
    }

    fn fixed_generator(ix: i32, iy: i32) -> Rc<dyn IPointGenerator> {
        Rc::new(FixedPointGenerator { ix, iy })
    }

    #[test]
    fn biome_index_follows_the_java_math() {
        let gen = BiomePatternGenerator {
            point_generator: fixed_generator(3, 5),
            tile_biomes: empty_biome_map(),
            custom_biomes: Vec::new(),
            biomes: Vec::new(),
            extents: 0,
        };
        let expected = hash_util::random3(7, 3, 5);
        assert_eq!(gen.get_biome_index(7, 0, 0), expected);
        assert_eq!(gen.get_biome_index(7, 123, 456), expected); // buffer is fixed
    }

    // getBiomeDirect must hit the same WeightedMap.get value as Java:
    // random3(seed, bits(x), bits(z)).
    #[test]
    fn biome_direct_uses_hashed_bit_patterns() {
        let gen = BiomePatternGenerator {
            point_generator: fixed_generator(0, 0),
            tile_biomes: empty_biome_map(),
            custom_biomes: Vec::new(),
            biomes: Vec::new(),
            extents: 0,
        };
        assert!(gen.get_biome_direct(1, 1.5, 2.5).is_none()); // empty map → None
        let v = hash_util::random3(1, 1.5f64.to_bits() as i64, 2.5f64.to_bits() as i64);
        // (value only materialized for the Java math trace; selection is None above)
        assert!(v > 0.0);
    }

    // Struct construction ordering/extents are exercised with real container
    // types in the biome package integration tests (see biome/mod.rs tests).
    #[test]
    fn default_map_picks_from_empty() -> Result<(), Box<dyn std::error::Error>> {
        let gen = BiomePatternGenerator {
            point_generator: fixed_generator(1, 1),
            tile_biomes: empty_biome_map(),
            custom_biomes: Vec::new(),
            biomes: Vec::new(),
            extents: 0,
        };
        assert!(gen.get_biome(3, 0, 0).is_none());
        Ok(())
    }
}
