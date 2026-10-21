// Ported from: `.hytale-source/.../worldgen/prefab/unique/UniquePrefabGenerator.java`
//! Java `com.hypixel.hytale.server.worldgen.prefab.unique.UniquePrefabGenerator`
//! — the *per-seed* placement of one unique prefab.
//!
//! # The placement
//!
//! A generator either
//!
//! * places its prefab at the **center of a unique zone** (the Java
//!   `forceUniqueZonePlacement` — the *inverse domain warp* heuristic that
//!   walks the coordinates back through the mask provider), or
//! * **searches** for a valid position around its anchor (the Java
//!   `tryPlacement` — a random point in the anchor's disc that passes the
//!   exclusion, the biome, the zone-border, the height and the parent-block
//!   checks), falling back to the anchor itself when every attempt fails
//!   (the Java `forceGeneration`).
//!
//! # Bit-exactness
//!
//! The generator draws from the **shared** random stream of its container
//! (`new FastRandom(seed + seedOffset)`); the draws of one generator shift
//! the draws of the next one, so the order is transcribed literally:
//!
//! * the placement search: `nextDouble` (the distance) and `nextFloat` (the
//!   angle) per attempt, plus the cover rolls of the parent-block check,
//! * then the prefab pick (`IWeightedMap.get(Random)`), and
//! * then the rotation pick (`nextInt(rotations.length)`).
//!
//! The prefab pick and the rotation pick happen in
//! [`crate::container::unique_prefab_container::UniquePrefabContainer::generate`]
//! — *after* the placement — and they share the same stream.

use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use hytale_math::math_util;
use hytale_math::trig_math_util;

use procedural::noise::support::Vector2i;

use crate::biome::biome::Biome;
use crate::chunk::chunk_generator::ChunkGenerator;
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use crate::container::unique_prefab_container::UniquePrefabEntry;
use crate::prefab::prefab_category::PrefabCategory;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::prefab::unique::unique_prefab_configuration::UniquePrefabConfiguration;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::weighted_map::IWeightedMap;

/// Java `private static final int UNIQUE_ZONE_PLACEMENT_HEURISTIC_ITERATIONS`
/// = `8`.
pub const UNIQUE_ZONE_PLACEMENT_HEURISTIC_ITERATIONS: i32 = 8;

/// Java `class UniquePrefabGenerator`.
pub struct UniquePrefabGenerator {
    /// Java `protected final String name`.
    name: String,
    /// Java `protected final PrefabCategory category`.
    category: PrefabCategory,
    /// Java `protected final IWeightedMap<WorldGenPrefabSupplier> prefabs`.
    prefabs: Rc<dyn IWeightedMap<WorldGenPrefabSupplier>>,
    /// Java `protected final UniquePrefabConfiguration configuration`.
    configuration: UniquePrefabConfiguration,
    /// Java `protected final int zoneIndex`.
    zone_index: i32,
}

impl UniquePrefabGenerator {
    /// Java `public UniquePrefabGenerator(String name, PrefabCategory
    /// category, IWeightedMap<WorldGenPrefabSupplier> prefabs,
    /// UniquePrefabConfiguration configuration, int zoneIndex)`.
    pub fn new(
        name: String,
        category: PrefabCategory,
        prefabs: Rc<dyn IWeightedMap<WorldGenPrefabSupplier>>,
        configuration: UniquePrefabConfiguration,
        zone_index: i32,
    ) -> Self {
        Self {
            name,
            category,
            prefabs,
            configuration,
            zone_index,
        }
    }

    /// Java `String getName()`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Java `PrefabCategory getCategory()`.
    pub fn get_category(&self) -> &PrefabCategory {
        &self.category
    }

    /// Java `IWeightedMap<WorldGenPrefabSupplier> getPrefabs()`.
    pub fn get_prefabs(&self) -> &Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
        &self.prefabs
    }

    /// Java `UniquePrefabConfiguration getConfiguration()`.
    pub fn get_configuration(&self) -> &UniquePrefabConfiguration {
        &self.configuration
    }

    /// The zone index of the generator (Java `zoneIndex`).
    pub fn get_zone_index(&self) -> i32 {
        self.zone_index
    }

    /// Java `@Nullable WorldGenPrefabSupplier generatePrefab(Random random)` —
    /// `prefabs.get(random)`.
    ///
    /// Deviation: the Java weighted map holds the supplier by reference; the
    /// Rust map holds it by value, so the port hands out a fresh shared handle
    /// (the supplier is a value object — its `equals` is the path).
    pub fn generate_prefab(&self, random: &mut FastRandom) -> Option<Rc<WorldGenPrefabSupplier>> {
        self.prefabs.get_random(random).cloned().map(Rc::new)
    }

    /// Java `@Nonnull Vector3i generate(int seed, @Nullable Vector2i position,
    /// @Nonnull ChunkGenerator chunkGenerator, @Nonnull Random random, int
    /// maxFailed, @Nonnull UniquePrefabContainer.UniquePrefabEntry[] entries)`.
    ///
    /// The Rust port returns the `(x, y, z)` triple of the Java `Vector3i`.
    ///
    /// * `position` — the position of the *unique zone* the generator belongs
    ///   to (`null` for a generator of a normal zone),
    /// * `entries` — the entries that were already generated (the Java array
    ///   holds `null` for the not-yet-generated ones; the Rust port passes
    ///   only the generated prefix).
    #[allow(clippy::too_many_arguments)]
    pub fn generate(
        &self,
        seed: i32,
        position: Option<Vector2i>,
        chunk_generator: &ChunkGenerator,
        random: &mut FastRandom,
        max_failed: i32,
        entries: &[UniquePrefabEntry],
    ) -> (i32, i32, i32) {
        if let Some(position) = position {
            return self.force_unique_zone_placement(seed, position, chunk_generator);
        }

        let mut vec: Option<(i32, i32, i32)>;
        let mut failed = 0;
        loop {
            vec = self.try_placement(seed, chunk_generator, random, entries);
            if vec.is_some() {
                break;
            }
            failed += 1;
            if failed > max_failed {
                break;
            }
        }

        match vec {
            Some(vec) => vec,
            None => {
                // Java: `LogUtil.getLogger().at(Level.SEVERE).log("Failed to
                // generate Unique-Prefab '%s' with anchor '%s', maxDistance:
                // %s", …)`.
                crate::util::log_util::error(&format!(
                    "Failed to generate Unique-Prefab '{}' with anchor '{}, {}', maxDistance: {}",
                    self.name,
                    self.configuration.get_anchor().x,
                    self.configuration.get_anchor().y,
                    self.configuration.get_max_distance()
                ));
                let vec = self.force_generation(seed, chunk_generator);
                crate::util::log_util::warn(&format!(
                    "FORCED Unique-Prefab '{}' at ({}, {}, {}) after {failed} attempts!",
                    self.name, vec.0, vec.1, vec.2
                ));
                vec
            }
        }
    }

    /// Java `@Nullable Vector3i tryPlacement(int seed, @Nonnull
    /// ChunkGenerator chunkGenerator, @Nonnull Random random, @Nonnull
    /// UniquePrefabContainer.UniquePrefabEntry[] entries)`.
    ///
    /// ⚠ Deviation: the Java dereferences the zone and the biome of the
    /// result (`result.getZoneResult().getZone().id()`) — a missing zone or
    /// biome would be an NPE. The Rust port rejects the candidate instead
    /// (the generator then falls back to its anchor).
    fn try_placement(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        random: &mut FastRandom,
        entries: &[UniquePrefabEntry],
    ) -> Option<(i32, i32, i32)> {
        let mut x = self.configuration.get_anchor().x;
        let mut z = self.configuration.get_anchor().y;

        {
            // Java:
            // final double distance = random.nextDouble() * configuration.getMaxDistance();
            // final float angle = random.nextFloat() * TrigMathUtil.PI2;
            // x += cos(angle) * distance;
            // z += sin(angle) * distance;
            //
            // `TrigMathUtil.cos` / `sin` are the *float* table lookups; the
            // results are widened to the double of the distance.
            let distance = random.next_double() * self.configuration.get_max_distance();
            let angle = random.next_float() * trig_math_util::PI2;
            x += f64::from(trig_math_util::cos(angle)) * distance;
            z += f64::from(trig_math_util::sin(angle)) * distance;
        }

        let lx = math_util::floor(x);
        let lz = math_util::floor(z);

        for entry in entries {
            let position = entry.get_position();
            let dx = f64::from(position.0) - x;
            let dz = f64::from(position.2) - z;
            let distance = dx * dx + dz * dz;
            // ⚠ Java quirk: the distance is measured to the *previous entry*
            // (not to the anchor), and it is compared against **both** the
            // previous entry's exclusion radius **and** the configuration's
            // own exclusion radius — so the configuration's radius acts as a
            // *minimum* exclusion radius against every previous entry.
            if distance <= entry.get_exclusion_radius_squared()
                || distance <= self.configuration.get_exclusion_radius_squared()
            {
                return None;
            }
        }

        if !self.is_matching_noise_density(seed, lx, lz) {
            return None;
        }

        let result = chunk_generator.get_zone_biome_result_at(seed, lx, lz);
        let zone_id = result
            .get_zone_result()
            .and_then(|zone_result| zone_result.get_zone())
            .map(|zone| zone.id);
        if zone_id != Some(self.zone_index) {
            return None;
        }

        let biome = result.get_biome().map(|biome| biome.as_ref())?;
        if !self.configuration.is_valid_parent_biome(biome) {
            return None;
        }

        let border_distance = result
            .get_zone_result()
            .map(|zone_result| zone_result.get_border_distance())
            // The Java field default (`0.0`) — the zone result is always
            // present in practice.
            .unwrap_or(0.0);
        if border_distance < self.configuration.get_zone_border_exclusion() {
            return None;
        }

        // Estimate the height at that position and check if the prefab should
        // spawn at that height.
        let height = self.get_height(seed, chunk_generator, biome, lx, lz);
        if !self.is_matching_height(seed, lx, lz, random, height) {
            return None;
        }

        // Check if the prefab is able to spawn on that block.
        if !self.is_matching_parent_block(seed, lx, height, lz, random, &result) {
            return None;
        }

        Some((lx, height, lz))
    }

    /// Java `@Nonnull Vector3i forceGeneration(int seed, @Nonnull
    /// ChunkGenerator chunkGenerator)`.
    fn force_generation(&self, seed: i32, chunk_generator: &ChunkGenerator) -> (i32, i32, i32) {
        let x = self.configuration.get_anchor().x;
        let z = self.configuration.get_anchor().y;

        let lx = math_util::floor(x);
        let lz = math_util::floor(z);

        let result = chunk_generator.get_zone_biome_result_at(seed, lx, lz);
        let biome = result
            .get_biome()
            .map(|biome| biome.as_ref())
            .expect("forceGeneration: the zone biome result has no biome (Java: NPE)");
        let height = self.get_height(seed, chunk_generator, biome, lx, lz);
        (lx, height, lz)
    }

    /// Java `@Nonnull Vector3i forceUniqueZonePlacement(int seed, @Nonnull
    /// Vector2i position, @Nonnull ChunkGenerator chunkGenerator)`.
    ///
    /// The *inverse domain warp*: the coordinates are walked back through the
    /// mask provider until the warped position matches the target position
    /// of the unique zone.
    fn force_unique_zone_placement(
        &self,
        seed: i32,
        position: Vector2i,
        chunk_generator: &ChunkGenerator,
    ) -> (i32, i32, i32) {
        let mask_provider = chunk_generator
            .get_zone_pattern_provider()
            .get_mask_provider();

        let mut x = position.x;
        let mut z = position.y;

        // Iteratively try to find a coordinate pair that when domain-warped by
        // the mask-provider gives us the 'position' coordinates. This is not
        // accurate but should help place the prefab closer to the center of
        // the unique zone.
        for _ in 0..UNIQUE_ZONE_PLACEMENT_HEURISTIC_ITERATIONS {
            // 1. See what the warped position is
            let px = math_util::floor(mask_provider.get_x(seed, f64::from(x), f64::from(z)));
            let pz = math_util::floor(mask_provider.get_y(seed, f64::from(x), f64::from(z)));

            // 2. Calculate the delta to the target position
            let dx = px - position.x;
            let dz = pz - position.y;

            // 3. Move the coords back by half the delta
            x -= dx / 2;
            z -= dz / 2;
        }

        let result = chunk_generator.get_zone_biome_result_at(seed, x, z);
        let biome = result
            .get_biome()
            .map(|biome| biome.as_ref())
            .expect("forceUniqueZonePlacement: the zone biome result has no biome (Java: NPE)");
        let height = self.get_height(seed, chunk_generator, biome, x, z);
        (x, height, z)
    }

    /// Java `protected int getHeight(int seed, @Nonnull ChunkGenerator
    /// chunkGenerator, @Nonnull Biome biome, int x, int z)`.
    fn get_height(
        &self,
        seed: i32,
        chunk_generator: &ChunkGenerator,
        biome: &dyn Biome,
        x: i32,
        z: i32,
    ) -> i32 {
        let water_container = biome.get_water_container();
        if water_container.has_entries() && self.configuration.is_on_water() {
            water_container.get_max_height(seed, x, z)
        } else {
            chunk_generator.get_height(seed, x, z)
        }
    }

    /// Java `protected boolean isMatchingHeight(int seed, int x, int z, Random
    /// random, int y)`.
    fn is_matching_height(
        &self,
        seed: i32,
        x: i32,
        z: i32,
        random: &mut FastRandom,
        y: i32,
    ) -> bool {
        match self.configuration.get_height_condition() {
            None => true,
            Some(condition) => condition.eval(seed, x, z, y, random),
        }
    }

    /// Java `protected boolean isMatchingNoiseDensity(int seed, int x, int
    /// z)`.
    fn is_matching_noise_density(&self, seed: i32, x: i32, z: i32) -> bool {
        self.configuration.get_map_condition().eval2(seed, x, z)
    }

    /// Java `protected boolean isMatchingParentBlock(int seed, int x, int y,
    /// int z, @Nonnull Random random, @Nonnull ZoneBiomeResult
    /// zoneAndBiomeResult)`.
    fn is_matching_parent_block(
        &self,
        seed: i32,
        x: i32,
        y: i32,
        z: i32,
        random: &mut FastRandom,
        zone_and_biome_result: &ZoneBiomeResult,
    ) -> bool {
        let biome = zone_and_biome_result
            .get_biome()
            .map(|biome| biome.as_ref())
            .expect("isMatchingParentBlock: the zone biome result has no biome (Java: NPE)");

        let ground_cover = self.get_cover_in_ground_at(seed, x, y, z, random, biome);
        if ground_cover != BlockFluidEntry::EMPTY
            && !self
                .configuration
                .is_valid_parent_block(ground_cover.block_id, ground_cover.fluid_id)
        {
            // The parent block of the prefab is not correct.
            return false;
        }

        let block = biome.get_layer_container().get_top_block_at(seed, x, z);
        self.configuration
            .is_valid_parent_block(block.block_id, block.fluid_id)
    }

    /// Java `protected BlockFluidEntry getCoverInGroundAt(int seed, int x, int
    /// y, int z, @Nonnull Random random, @Nonnull Biome biome)`.
    fn get_cover_in_ground_at(
        &self,
        seed: i32,
        x: i32,
        y: i32,
        z: i32,
        random: &mut FastRandom,
        biome: &dyn Biome,
    ) -> BlockFluidEntry {
        for cover_container_entry in biome.get_cover_container().get_entries() {
            #[allow(deprecated)]
            if y < chunk_util::HEIGHT
                && self.is_matching_cover(seed, cover_container_entry, random, x, y, z)
            {
                let part = cover_container_entry.get(random).copied();
                if let Some(part) = part {
                    if part.get_offset() == -1 {
                        return part.get_entry();
                    }
                }
            }
        }
        BlockFluidEntry::EMPTY
    }

    /// Java `protected boolean isMatchingCover(int seed, @Nonnull
    /// CoverContainer.CoverContainerEntry coverContainerEntry, @Nonnull Random
    /// random, int x, int y, int z)`.
    fn is_matching_cover(
        &self,
        seed: i32,
        cover_container_entry: &crate::container::cover_container::CoverContainerEntry,
        random: &mut FastRandom,
        x: i32,
        y: i32,
        z: i32,
    ) -> bool {
        random.next_double() < cover_container_entry.get_cover_density()
            && cover_container_entry.get_map_condition().eval2(seed, x, z)
            && cover_container_entry
                .get_height_condition()
                .eval(seed, x, z, y, random)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_math::trig_math_util;
    use procedural::noise::condition::{AlwaysTrueIntCondition, IIntCondition};
    use procedural::noise::condition::{ConstantBlockFluidCondition, DefaultCoordinateCondition};
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::chunk::chunk_generator::test_stubs::{ConstantNoise, StubInterpreter};
    use crate::chunk::mask_provider::MaskProvider;
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::loader::test_prefab_store::test_prefab_loader;
    use crate::loader::util::{Vector2d, Vector3d};
    use crate::prefab::prefab_rotation::PrefabRotation;
    use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
    use crate::util::condition::BlockMaskCondition;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone::Zone;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zone::zone_pattern_provider::ZonePatternProvider;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    /// A point generator with a fixed cell result (the border distance of the
    /// multi-zone case).
    struct FixedPointGenerator {
        distance: f64,
    }

    impl procedural::noise::point::IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            let mut buf = ResultBuffer2d::new();
            buf.distance = self.distance;
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

    /// A tint container with a single (constant) colour.
    fn tint_container(color: i32) -> Rc<TintContainer> {
        let mut builder = WeightedMap::<i32>::builder();
        builder.put(color, 1.0);
        let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
        Rc::new(TintContainer::new(
            DefaultTintContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
            Vec::new(),
        ))
    }

    /// An environment container with a single (constant) environment id.
    fn environment_container(environment: i32) -> Rc<EnvironmentContainer> {
        let mut builder = WeightedMap::<i32>::builder();
        builder.put(environment, 1.0);
        let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
        Rc::new(EnvironmentContainer::new(
            DefaultEnvironmentContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
            Vec::new(),
        ))
    }

    /// A tile biome with a constant threshold.
    fn tile_biome(id: i32) -> Rc<TileBiome> {
        let interpolation = Rc::new(BiomeInterpolation::create(
            crate::chunk::MAX_RADIUS,
            std::collections::HashMap::new(),
        ));
        let interpreter: Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter> =
            Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63));
        Rc::new(TileBiome::new(
            id,
            format!("biome-{id}"),
            interpolation,
            interpreter,
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
            None,
            tint_container(0x112233),
            environment_container(5),
            Rc::new(WaterContainer::new(Vec::new())),
            Rc::new(FadeContainer::new(
                0.0,
                1.0,
                0.0,
                1.0,
                FadeContainer::NO_FADE_HEIGHTMAP,
            )),
            Rc::new(ConstantNoise(0.5)),
            1.0,
            1.0,
            0,
        ))
    }

    /// A zone with a single tile biome.
    fn zone_with_biome(zone_id: i32, biome_id: i32) -> Rc<Zone> {
        let biome = tile_biome(biome_id);
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        builder.put(biome, 1.0);
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());
        Rc::new(Zone {
            id: zone_id,
            name: format!("zone-{zone_id}"),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            cave_generator: None,
            unique_prefab_container: Rc::new(
                crate::container::unique_prefab_container::UniquePrefabContainer::new(
                    0,
                    Vec::new(),
                ),
            ),
            biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
                Rc::new(FixedPointGenerator { distance: 2.5 }),
                tile_biomes,
                Vec::new(),
            )),
        })
    }

    /// A solid mask (every pixel has the same colour).
    fn solid_mask(color: i32) -> MaskProvider {
        MaskProvider::new(FuzzyZoom::new(
            empty_randomizer(),
            PixelProvider::from_pixels(4, 4, vec![color; 16]),
            1.0,
            1.0,
            0,
            0,
        ))
    }

    /// A chunk generator over one zone (id 3) with a tile biome (id 7).
    fn generator() -> Rc<ChunkGenerator> {
        let zone = zone_with_biome(3, 7);
        let mut mapping = ZoneColorMapping::new();
        mapping.add(0x00FF00, Rc::clone(&zone));
        let provider = ZonePatternProvider::new(
            Rc::new(FixedPointGenerator {
                distance: f64::INFINITY,
            }),
            vec![zone],
            Vec::new(),
            solid_mask(0x00FF00),
            mapping,
        );
        ChunkGenerator::new(provider, None)
    }

    /// A chunk generator over *two* zones that share the mask colour — the
    /// border distance of such a zone is the distance of the point generator
    /// (the Java `getZone` branch).
    ///
    /// The returned id is the zone the `hash & 0x7FFF_FFFF % 2` pick selects
    /// for the given seed.
    fn generator_with_border_distance(
        border_distance: f64,
        seed: i32,
    ) -> (Rc<ChunkGenerator>, i32) {
        let zone_a = zone_with_biome(3, 7);
        let zone_b = zone_with_biome(4, 7);
        let mut mapping = ZoneColorMapping::new();
        mapping.add_zones(0x00FF00, vec![Rc::clone(&zone_a), Rc::clone(&zone_b)]);
        let provider = ZonePatternProvider::new(
            Rc::new(FixedPointGenerator {
                distance: border_distance,
            }),
            vec![zone_a, zone_b],
            Vec::new(),
            solid_mask(0x00FF00),
            mapping,
        );
        // Java: `((int) HashUtil.hash(seed, buf.ix, buf.iy) & 0x7FFF_FFFF) %
        // zoneArr.length` — the point generator always reports the cell
        // (0, 0).
        let index =
            ((hytale_math::hash_util::hash3(seed as i64, 0, 0) as i32) & 0x7FFF_FFFF) as usize % 2;
        let zone_id = if index == 0 { 3 } else { 4 };
        (ChunkGenerator::new(provider, None), zone_id)
    }

    /// A configuration with the given anchor, max distance and exclusions.
    fn configuration(
        anchor: Vector2d,
        max_distance: f64,
        exclusion_radius: f64,
        zone_border_exclusion: f64,
    ) -> UniquePrefabConfiguration {
        configuration_with_biome_mask(
            anchor,
            max_distance,
            exclusion_radius,
            zone_border_exclusion,
            Rc::new(AlwaysTrueIntCondition),
        )
    }

    /// The same configuration with a custom biome mask.
    fn configuration_with_biome_mask(
        anchor: Vector2d,
        max_distance: f64,
        exclusion_radius: f64,
        zone_border_exclusion: f64,
        biome_mask: Rc<dyn IIntCondition>,
    ) -> UniquePrefabConfiguration {
        UniquePrefabConfiguration::new(
            None,
            BlockMaskCondition::new(),
            None,
            biome_mask,
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(ConstantBlockFluidCondition::True),
            anchor,
            Vector3d::new(0.0, 0.0, 0.0),
            max_distance,
            false,
            false,
            false,
            -1,
            10,
            exclusion_radius,
            false,
            zone_border_exclusion,
            false,
        )
    }

    /// A weighted map with a single prefab of the test store.
    fn prefab_map() -> Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
        let loader = test_prefab_loader();
        let mut suppliers = loader.get("A");
        assert_eq!(suppliers.len(), 1);
        let mut builder = WeightedMap::<WorldGenPrefabSupplier>::builder();
        builder.put(suppliers.remove(0), 1.0);
        Rc::new(builder.build())
    }

    /// The candidate the placement search picks for the given seed — the
    /// draws are transcribed from the Java `tryPlacement`.
    fn expected_candidate(seed: i64, anchor: Vector2d, max_distance: f64) -> (i32, i32) {
        let mut random = FastRandom::new(seed);
        let distance = random.next_double() * max_distance;
        let angle = random.next_float() * trig_math_util::PI2;
        let x = anchor.x + f64::from(trig_math_util::cos(angle)) * distance;
        let z = anchor.y + f64::from(trig_math_util::sin(angle)) * distance;
        (math_util::floor(x), math_util::floor(z))
    }

    /// An entry of the test prefab store at the given position.
    fn entry_at(position: (i32, i32, i32), exclusion_radius_squared: f64) -> UniquePrefabEntry {
        let loader = test_prefab_loader();
        let mut suppliers = loader.get("A");
        assert_eq!(suppliers.len(), 1);
        UniquePrefabEntry::new(
            "previous".to_string(),
            PrefabCategory::unique(),
            position,
            Rc::new(suppliers.remove(0)),
            BlockMaskCondition::new(),
            PrefabRotation::Rotation0,
            Vector3d::new(0.0, 0.0, 0.0),
            -1,
            false,
            false,
            exclusion_radius_squared,
            false,
            false,
        )
    }

    #[test]
    fn the_placement_search_uses_the_documented_draw_order() {
        let generator = generator();
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 0.0, 0.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            3,
        );

        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 10, &[]);

        let (lx, lz) = expected_candidate(1234, Vector2d::new(0.0, 0.0), 100.0);
        assert_eq!(position.0, lx, "the x of the placement");
        assert_eq!(position.2, lz, "the z of the placement");
        assert_eq!(
            position.1,
            generator.get_height(7, lx, lz),
            "the height of the placement"
        );
    }

    #[test]
    fn the_exclusion_radius_of_a_previous_entry_rejects_the_candidate() {
        let generator = generator();
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 0.0, 0.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            3,
        );

        let (lx, lz) = expected_candidate(1234, Vector2d::new(0.0, 0.0), 100.0);
        // The previous entry sits on the candidate with an exclusion radius
        // of two blocks → the candidate is rejected and the generator falls
        // back to its anchor.
        let previous = entry_at((lx, 0, lz), 4.0);

        // Only one attempt — the exclusion rejects the *first* candidate, and
        // a second attempt would draw a different point.
        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 0, &[previous]);
        assert_eq!(
            position,
            (0, generator.get_height(7, 0, 0), 0),
            "the placement falls back to the anchor"
        );
    }

    #[test]
    fn the_configuration_exclusion_radius_rejects_a_distant_entry() {
        let generator = generator();
        // The configuration's own exclusion radius (100 blocks) is compared
        // against the distance to the *previous entries* — so a previous
        // entry that is 50 blocks away is rejected even though its own
        // exclusion radius is tiny.
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 100.0, 0.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            3,
        );

        let (lx, lz) = expected_candidate(1234, Vector2d::new(0.0, 0.0), 100.0);
        let previous = entry_at((lx + 50, 0, lz), 1.0);

        // Only one attempt — the exclusion rejects the *first* candidate, and
        // a second attempt would draw a different point.
        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 0, &[previous]);
        assert_eq!(
            position,
            (0, generator.get_height(7, 0, 0), 0),
            "the placement falls back to the anchor"
        );
    }

    #[test]
    fn a_candidate_outside_the_exclusion_radius_is_accepted() {
        let generator = generator();
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 0.0, 0.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            3,
        );

        let (lx, lz) = expected_candidate(1234, Vector2d::new(0.0, 0.0), 100.0);
        // The previous entry is 50 blocks away — outside both exclusion
        // radii → the candidate is accepted.
        let previous = entry_at((lx + 50, 0, lz), 1.0);

        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 10, &[previous]);
        assert_eq!(position.0, lx);
        assert_eq!(position.2, lz);
    }

    #[test]
    fn a_zone_that_does_not_match_falls_back_to_the_anchor() {
        let generator = generator();
        // The generator belongs to the zone 42, but the world only has the
        // zone 3 → every candidate is rejected.
        let configuration = configuration(Vector2d::new(11.5, -7.5), 100.0, 0.0, 0.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            42,
        );

        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 10, &[]);
        assert_eq!(
            position,
            (11, generator.get_height(7, 11, -8), -8),
            "the anchor is floored"
        );
    }

    #[test]
    fn the_zone_border_exclusion_rejects_a_candidate_close_to_the_border() {
        // The border distance of the zone is 2.5 blocks.
        let (generator, zone_id) = generator_with_border_distance(2.5, 7);
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 0.0, 30.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            zone_id,
        );

        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 10, &[]);
        assert_eq!(
            position,
            (0, generator.get_height(7, 0, 0), 0),
            "the border exclusion rejects the candidate"
        );
    }

    #[test]
    fn a_candidate_far_from_the_border_is_accepted() {
        let (generator, zone_id) = generator_with_border_distance(2.5, 7);
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 0.0, 1.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            zone_id,
        );

        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 10, &[]);
        let (lx, lz) = expected_candidate(1234, Vector2d::new(0.0, 0.0), 100.0);
        assert_eq!(position.0, lx);
        assert_eq!(position.2, lz);
    }

    #[test]
    fn the_unique_zone_placement_lands_on_the_target_position() {
        let generator = generator();
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 0.0, 0.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            3,
        );

        let mut random = FastRandom::new(1234);
        // The mask provider is the identity, so the inverse domain warp
        // converges immediately.
        let position = prefab_generator.generate(
            7,
            Some(Vector2i::new(123, -45)),
            &generator,
            &mut random,
            10,
            &[],
        );
        assert_eq!(position.0, 123);
        assert_eq!(position.2, -45);
        assert_eq!(position.1, generator.get_height(7, 123, -45));
    }

    #[test]
    fn the_placement_is_deterministic_for_the_same_seed() {
        let generator = generator();
        let configuration = configuration(Vector2d::new(0.0, 0.0), 100.0, 0.0, 0.0);
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            3,
        );

        let run = || {
            let mut random = FastRandom::new(99);
            prefab_generator.generate(7, None, &generator, &mut random, 10, &[])
        };
        assert_eq!(run(), run(), "the same seed gives the same placement");
    }

    #[test]
    fn the_biome_mask_rejects_a_candidate_of_another_biome() {
        let generator = generator();
        // A biome mask that only accepts the biome 42 (the world has the
        // biome 7) → every candidate is rejected.
        struct OnlyBiome42;
        impl IIntCondition for OnlyBiome42 {
            fn eval(&self, value: i32) -> bool {
                value == 42
            }
        }
        let configuration = configuration_with_biome_mask(
            Vector2d::new(0.0, 0.0),
            100.0,
            0.0,
            0.0,
            Rc::new(OnlyBiome42),
        );
        let prefab_generator = UniquePrefabGenerator::new(
            "test".to_string(),
            PrefabCategory::unique(),
            prefab_map(),
            configuration,
            3,
        );

        let mut random = FastRandom::new(1234);
        let position = prefab_generator.generate(7, None, &generator, &mut random, 10, &[]);
        assert_eq!(
            position,
            (0, generator.get_height(7, 0, 0), 0),
            "the biome mask rejects the candidate"
        );
    }
}
