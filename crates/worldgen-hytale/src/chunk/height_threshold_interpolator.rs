// Ported from: `.hytale-source/.../worldgen/chunk/HeightThresholdInterpolator.java`
//! Java `com.hypixel.hytale.server.worldgen.chunk.HeightThresholdInterpolator`
//! — the per-chunk table of core-data entries.
//!
//! The table holds one [`CoreDataCacheEntry`] per column of the chunk plus a
//! `MAX_RADIUS` (5) block border on every side, i.e. `(32 + 2 * 5)^2 = 42^2`
//! entries. The table is filled from the generator's core-data cache, and
//! every *inner* column additionally gets its interpolated biome-count list
//! and its interpolated height noise.
//!
//! # Deviations
//!
//! * **The interpolator does not hold a reference to the execution.** Java
//!   keeps a `ChunkGeneratorExecution` and asks it for the global/local
//!   coordinates; the Rust port holds the chunk coordinates and the
//!   [`ChunkGenerator`] directly, which breaks the reference cycle between
//!   the execution and the interpolator. The coordinate arithmetic is
//!   identical (see [`HeightThresholdInterpolator::global_x`] and
//!   [`HeightThresholdInterpolator::local_x`]).
//! * **The entries are `Option<Rc<CoreDataCacheEntry>>`** — the Java array
//!   holds `null` slots until `populate` fills them.
//! * **The "all biomes are the same" collapse of the biome-count list is
//!   applied while the list is built** (see
//!   [`crate::chunk::chunk_generator::ChunkGenerator::generate_interpolated_biome_count_at`]),
//!   because the ported list exposes no setter for its per-biome records.
use std::rc::Rc;

use hytale_math::chunk_util;

use crate::cache::core_data_cache_entry::CoreDataCacheEntry;
use crate::cache::interpolated_biome_count_list::InterpolatedBiomeCountList;
use crate::chunk::chunk_generator::{
    generate_interpolated_threshold, region_has_multiple_biomes, ChunkGenerator,
};
use crate::chunk::zone_biome_result::ZoneBiomeResult;

/// Java `HeightThresholdInterpolator.MAX_RADIUS` — the interpolation radius
/// (the border of the per-chunk table).
///
/// Re-exported from the package module so that the Java class constant
/// (`HeightThresholdInterpolator.MAX_RADIUS`) and the package constant stay
/// in sync.
pub use crate::chunk::{MAX_RADIUS, MAX_RADIUS2};

/// Java `chunk.HeightThresholdInterpolator`.
pub struct HeightThresholdInterpolator {
    /// Java `private final ChunkGeneratorExecution execution` — the Rust
    /// port holds the generator directly (see the module docs).
    chunk_generator: Rc<ChunkGenerator>,
    /// Java `private final CoreDataCacheEntry[] entries`.
    entries: Vec<Option<Rc<CoreDataCacheEntry>>>,
    /// Java `private final int radius`.
    radius: i32,
    /// Java `private final int size` — the chunk size (`ChunkUtil.SIZE`).
    size: i32,
    /// Java `private final int totalSize` — `size + 2 * radius`.
    total_size: i32,
    /// The chunk x coordinate (Java: `execution.getChunk().getX()`).
    chunk_x: i32,
    /// The chunk z coordinate (Java: `execution.getChunk().getZ()`).
    chunk_z: i32,
}

impl HeightThresholdInterpolator {
    /// Java `HeightThresholdInterpolator(ChunkGeneratorExecution execution)`.
    ///
    /// The Rust port takes the generator and the chunk coordinates instead
    /// of the execution (see the module docs).
    pub fn new(chunk_generator: Rc<ChunkGenerator>, chunk_x: i32, chunk_z: i32) -> Self {
        let radius = MAX_RADIUS;
        let size = chunk_util::SIZE;
        let total_size = size + 2 * radius;
        Self {
            chunk_generator,
            entries: vec![None; (total_size * total_size) as usize],
            radius,
            size,
            total_size,
            chunk_x,
            chunk_z,
        }
    }

    /// Java `@Nonnull CoreDataCacheEntry[] getEntries()`.
    pub fn get_entries(&self) -> &[Option<Rc<CoreDataCacheEntry>>] {
        &self.entries
    }

    /// The chunk x coordinate (Java: `execution.getChunk().getX()`).
    pub fn get_chunk_x(&self) -> i32 {
        self.chunk_x
    }

    /// The chunk z coordinate (Java: `execution.getChunk().getZ()`).
    pub fn get_chunk_z(&self) -> i32 {
        self.chunk_z
    }

    /// Java `@Nonnull HeightThresholdInterpolator populate(int seed)`.
    pub fn populate(&mut self, seed: i32) -> &mut Self {
        for cx in -self.radius..(self.size + self.radius) {
            for cz in -self.radius..(self.size + self.radius) {
                let entry =
                    self.chunk_generator
                        .get_core_data(seed, self.global_x(cx), self.global_z(cz));
                self.set_table_entry(cx, cz, entry);
            }
        }

        for cx in 0..self.size {
            for cz in 0..self.size {
                let entry = self.table_entry(cx, cz);
                if entry
                    .biome_count_list
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .is_none()
                {
                    let list = InterpolatedBiomeCountList::new();
                    self.generate_interpolated_biome_count_at(cx, cz, &list);
                    *entry
                        .biome_count_list
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Rc::new(list));
                }
                if *entry
                    .height_noise
                    .read()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    == CoreDataCacheEntry::NO_HEIGHT_NOISE
                {
                    let list = entry
                        .biome_count_list
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .clone()
                        .expect("the biome count list was populated above");
                    let noise = self
                        .chunk_generator
                        .generate_interpolated_height_noise(&list);
                    *entry
                        .height_noise
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = noise;
                }
            }
        }

        self
    }

    /// Java `void generateInterpolatedBiomeCountAt(int cx, int cz, @Nonnull
    /// InterpolatedBiomeCountList biomeCountList)`.
    ///
    /// Deviation: the "all biomes are the same" collapse is applied while
    /// the list is built (the ported list has no setter for its records) —
    /// the resulting list state is identical to the Java one.
    pub fn generate_interpolated_biome_count_at(
        &self,
        cx: i32,
        cz: i32,
        biome_count_list: &InterpolatedBiomeCountList,
    ) {
        let center = self.zone_biome_result(cx, cz);
        biome_count_list.set_center(&center);

        let radius = center
            .get_biome()
            .expect("generateInterpolatedBiomeCountAt: the centre has no biome (Java: NPE)")
            .get_interpolation()
            .get_radius();
        let radius2 = radius * radius;

        // TODO (Java): consider weighting heightmap/threshold contributions
        // by distance from the center point?
        let mut neighbours = Vec::new();
        for ix in -radius..=radius {
            for iz in -radius..=radius {
                if ix == 0 && iz == 0 {
                    continue;
                }

                let distance2 = ix * ix + iz * iz;
                if distance2 > radius2 {
                    continue;
                }

                let biome_result = self.zone_biome_result(cx + ix, cz + iz);
                neighbours.push((biome_result, distance2));
            }
        }

        // "Don't interpolate if all biomes in the region are the same" — the
        // collapsed list is exactly the centre-only list.
        if !region_has_multiple_biomes(&center, &neighbours) {
            return;
        }

        for (biome_result, distance2) in neighbours {
            biome_count_list.add(&biome_result, distance2);
        }
    }

    /// Java `double getHeightNoise(int cx, int cz)`.
    pub fn get_height_noise(&self, cx: i32, cz: i32) -> f64 {
        *self
            .table_entry(cx, cz)
            .height_noise
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Java `float getHeightThreshold(int seed, int x, int z, int y)`.
    pub fn get_height_threshold(&self, seed: i32, x: i32, z: i32, y: i32) -> f32 {
        self.interpolate_height_threshold(seed, x, z, y)
    }

    /// Java `private float interpolateHeightThreshold(int seed, int x, int z,
    /// int y)`.
    fn interpolate_height_threshold(&self, seed: i32, x: i32, z: i32, y: i32) -> f32 {
        let entry = self.table_entry(self.local_x(x), self.local_z(z));
        let list = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("the biome count list was populated");
        generate_interpolated_threshold(seed, x, z, y, &list)
    }

    /// Java `protected CoreDataCacheEntry tableEntry(int cx, int cz)`.
    pub fn table_entry(&self, cx: i32, cz: i32) -> Rc<CoreDataCacheEntry> {
        let index = self.index_local(cx, cz);
        self.entries
            .get(index as usize)
            .and_then(|entry| entry.clone())
            .unwrap_or_else(|| {
                panic!(
                    "tableEntry({cx}, {cz}): the entry is not populated (Java: NullPointerException)"
                )
            })
    }

    /// Java `protected void setTableEntry(int cx, int cz, CoreDataCacheEntry
    /// entry)`.
    pub fn set_table_entry(&mut self, cx: i32, cz: i32, entry: Rc<CoreDataCacheEntry>) {
        let index = self.index_local(cx, cz);
        self.entries[index as usize] = Some(entry);
    }

    /// Java `protected ZoneBiomeResult zoneBiomeResult(int cx, int cz)`.
    ///
    /// Deviation: the Java method hands out the live result object; the Rust
    /// port returns a snapshot clone.
    pub fn zone_biome_result(&self, cx: i32, cz: i32) -> ZoneBiomeResult {
        self.table_entry(cx, cz)
            .zone_biome_result
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Java `public int getLowestNonOne(int cx, int cz)`.
    pub fn get_lowest_non_one(&self, cx: i32, cz: i32) -> i32 {
        let entry = self.table_entry(cx, cz);
        let list = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("the biome count list was populated");
        self.chunk_generator
            .generate_lowest_threshold_dependent(&list)
    }

    /// Java `public int getHighestNonZero(int cx, int cz)`.
    pub fn get_highest_non_zero(&self, cx: i32, cz: i32) -> i32 {
        let entry = self.table_entry(cx, cz);
        let list = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("the biome count list was populated");
        self.chunk_generator
            .generate_highest_threshold_dependent(&list)
    }

    /// Java `protected int indexLocal(int x, int z)`.
    pub fn index_local(&self, x: i32, z: i32) -> i32 {
        (x + self.radius) * self.total_size + z + self.radius
    }

    /// Java `execution.localX(int x)` — `x - blockChunk.getX() *
    /// ChunkUtil.SIZE`.
    pub fn local_x(&self, x: i32) -> i32 {
        x - self.chunk_x * chunk_util::SIZE
    }

    /// Java `execution.localZ(int z)` — `z - blockChunk.getZ() *
    /// ChunkUtil.SIZE`.
    pub fn local_z(&self, z: i32) -> i32 {
        z - self.chunk_z * chunk_util::SIZE
    }

    /// Java `execution.globalX(int localX)` — `minBlock(blockChunk.getX()) +
    /// localX`.
    pub fn global_x(&self, local_x: i32) -> i32 {
        chunk_util::min_block(self.chunk_x) + local_x
    }

    /// Java `execution.globalZ(int localZ)` — `minBlock(blockChunk.getZ()) +
    /// localZ`.
    pub fn global_z(&self, local_z: i32) -> i32 {
        chunk_util::min_block(self.chunk_z) + local_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::chunk::chunk_generator::test_stubs::{ConstantNoise, StubInterpreter};
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone::Zone;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zone::zone_pattern_provider::ZonePatternProvider;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    struct FixedPointGenerator;

    impl procedural::noise::point::IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            ResultBuffer2d::new()
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

    fn tint_container(color: i32) -> Rc<TintContainer> {
        let mut builder = WeightedMap::<i32>::builder();
        builder.put(color, 1.0);
        let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
        Rc::new(TintContainer::new(
            DefaultTintContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
            Vec::new(),
        ))
    }

    fn environment_container(environment: i32) -> Rc<EnvironmentContainer> {
        let mut builder = WeightedMap::<i32>::builder();
        builder.put(environment, 1.0);
        let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
        Rc::new(EnvironmentContainer::new(
            DefaultEnvironmentContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
            Vec::new(),
        ))
    }

    fn fade_container() -> Rc<FadeContainer> {
        Rc::new(FadeContainer::new(
            0.0,
            1.0,
            0.0,
            1.0,
            FadeContainer::NO_FADE_HEIGHTMAP,
        ))
    }

    /// A biome with a constant threshold of 0.75 and a height noise of 0.5.
    fn tile_biome(id: i32) -> Rc<TileBiome> {
        Rc::new(TileBiome::new(
            id,
            format!("biome-{id}"),
            Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
            None,
            tint_container(0x112233),
            environment_container(5),
            Rc::new(WaterContainer::new(Vec::new())),
            fade_container(),
            Rc::new(ConstantNoise(0.5)),
            1.0,
            1.0,
            0,
        ))
    }

    fn generator() -> Rc<ChunkGenerator> {
        let biome = tile_biome(7);
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        builder.put(Rc::clone(&biome), 1.0);
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());
        let zone = Rc::new(Zone {
            id: 3,
            name: "zone-3".to_string(),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            cave_generator: None,
            unique_prefab_container: Rc::new(
                crate::container::unique_prefab_container::UniquePrefabContainer::new(
                    0,
                    Vec::new(),
                ),
            ),
            biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
                Rc::new(FixedPointGenerator),
                tile_biomes,
                Vec::new(),
            )),
        });
        let mut mapping = ZoneColorMapping::new();
        mapping.add(0x00FF00, Rc::clone(&zone));
        let provider = ZonePatternProvider::new(
            Rc::new(FixedPointGenerator),
            vec![zone],
            Vec::new(),
            crate::chunk::mask_provider::MaskProvider::new(FuzzyZoom::new(
                empty_randomizer(),
                PixelProvider::from_pixels(4, 4, vec![0x00FF00; 16]),
                1.0,
                1.0,
                0,
                0,
            )),
            mapping,
        );
        ChunkGenerator::new(provider, None)
    }

    #[test]
    fn the_table_is_a_padded_square() {
        let interpolator = HeightThresholdInterpolator::new(generator(), 0, 0);
        assert_eq!(interpolator.get_entries().len(), 42 * 42);
        // The index of the table's first entry is the corner (-5, -5).
        assert_eq!(interpolator.index_local(-5, -5), 0);
        assert_eq!(interpolator.index_local(0, 0), 5 * 42 + 5);
        assert_eq!(interpolator.index_local(26, 26), 31 * 42 + 31);
    }

    #[test]
    fn the_local_and_global_coordinates_match_the_execution() {
        let interpolator = HeightThresholdInterpolator::new(generator(), 2, -3);
        // Java: `localX(x) = x - chunkX * SIZE`.
        assert_eq!(interpolator.local_x(70), 70 - 2 * 32);
        assert_eq!(interpolator.local_z(-90), -90 - (-3) * 32);
        // Java: `globalX(localX) = minBlock(chunkX) + localX`.
        assert_eq!(interpolator.global_x(5), 2 * 32 + 5);
        assert_eq!(interpolator.global_z(5), -3 * 32 + 5);
    }

    #[test]
    fn populate_fills_the_table_and_the_biome_count_lists() {
        let generator = generator();
        let mut interpolator = HeightThresholdInterpolator::new(Rc::clone(&generator), 0, 0);
        interpolator.populate(42);

        // Every table slot is filled.
        assert!(interpolator
            .get_entries()
            .iter()
            .all(|entry| entry.is_some()));

        // The height noise of an inner column is the interpolated noise.
        assert_eq!(interpolator.get_height_noise(0, 0), 0.5);
        // The single biome is the one of the zone.
        let result = interpolator.zone_biome_result(0, 0);
        assert_eq!(result.get_biome().map(|b| b.get_id()), Some(7));
        // The threshold folds use the biome's interpreter.
        assert_eq!(interpolator.get_lowest_non_one(0, 0), 0);
        assert_eq!(interpolator.get_highest_non_zero(0, 0), 63);
    }

    #[test]
    fn the_height_threshold_uses_the_column_of_the_world_coordinate() {
        let generator = generator();
        let mut interpolator = HeightThresholdInterpolator::new(Rc::clone(&generator), 0, 0);
        interpolator.populate(42);
        // The threshold of the (single) biome is 0.75 everywhere.
        assert_eq!(interpolator.get_height_threshold(42, 10, 20, 5), 0.75);
    }

    #[test]
    fn the_table_entry_of_an_unpopulated_slot_panics() {
        let interpolator = HeightThresholdInterpolator::new(generator(), 0, 0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            interpolator.table_entry(0, 0)
        }));
        assert!(result.is_err(), "an unpopulated slot panics");
    }
}
