// Ported from: `.hytale-source/.../worldgen/chunk/populator/PrefabPopulator.java`
//! The shared *placement pass* of the prefab stage: the collection of the
//! placement candidates and their invalidation.
//!
//! The pass is used by [`crate::chunk::populator::prefab_populator::PrefabPopulator`]
//! (Phase 5b of
//! [`crate::chunk::ChunkGeneratorExecution::execute`]) and — later — by the
//! headless feature query. Both run the *same* collection, only through a
//! different [`PlacementContext`]: the populator collects against the chunk
//! being generated, the query collects against an arbitrary rectangle. The
//! generation/paste side (the actual block writes) is **not** part of this
//! module; it stays in the populator.
//!
//! # The pass
//!
//! 1. **Collect the biomes** that can place prefabs into the window: the
//!    biomes the context already knows for the window interior (the
//!    populator: the chunk's core-data entries) plus a *sparse* sample
//!    (every [`BIOME_SAMPLE_STEP_SIZE`] blocks) of the biomes of the region
//!    around the window (the region is the window padded by the
//!    interpolation radius and by the maximum prefab extent of the world).
//! 2. **Collect the candidates**: for every biome and every entry of its
//!    prefab container, the entry's *grid generator* collects the lattice
//!    points of the window (padded by the entry's extents); every lattice
//!    point is a *candidate* that is filtered by the noise density, the
//!    unique-prefab exclusion radius, the biome match, the height condition,
//!    the parent block and the base checks.
//! 3. **Invalidate** the candidates that overlap a *higher priority*
//!    candidate (the AABB overlap test).
//!
//! # Bit-exactness
//!
//! The pass is a **long random stream**: every candidate consumes draws from
//! the *same* [`FastRandom`], and the draws happen in the Java order. The
//! order is:
//!
//! * `random.setSeed(HashUtil.hash(prefabSeed, x, z) * 0x5feb8cafL)` — the
//!   stream is **re-seeded for every lattice point**
//!   (`hash_util::hash3(prefab_seed, x, z).wrapping_mul(0x5feb_8caf)`, with
//!   `prefab_seed = HashUtil.hash(seed, biomeId, entryId)`), so the draws of
//!   one point do not leak into the next one and the per-point evaluation is
//!   **order-independent**. The transcription keeps the Java order anyway
//!   (biome order, entry order, lattice order): a future change must not
//!   start relying on the independence without proving the draw counts.
//! * the weighted prefab pick (`IWeightedMap.get(Random)` — the *singleton*
//!   map does **not** consume a draw),
//! * the rotation pick (`random.nextInt(rotations.length)`),
//! * the height condition (the deep-search-on-water variant evaluates the
//!   height condition *inside* the height search),
//! * the height condition of the candidate,
//! * the cover rolls of the parent-block check
//!   (`random.nextDouble() < coverDensity`).
//!
//! Swapping two of those draws changes every candidate that follows, so the
//! order above is transcribed literally.
//!
//! # Deviations
//!
//! * **The `atMostEvery(1, TimeUnit.SECONDS)` rate limit of the base-check
//!   log message is not ported** — the message is logged at the `FINE`
//!   level (the `log` facade filters it).
//! * **The two `ConstantIntCondition` identity checks of
//!   [`is_matching_parent_block`](PlacementCollector::is_matching_parent_block)
//!   are dead code** — the Java compares an `IBlockFluidCondition` with a
//!   `ConstantIntCondition` (which implements `IIntCondition`), so the
//!   comparisons can never be true. See the method docs.
//! * **The base-check height is read through the context**
//!   ([`PlacementContext::base_check_height`]) instead of the execution — see
//!   the trait docs for why the populator's implementation must stay
//!   execution-backed and the query's must not.

use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;
use hytale_math::math_util;

use crate::biome::biome::Biome;
use crate::chunk::chunk_generator::ChunkGenerator;
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use crate::container::cover_container::CoverContainerEntry;
use crate::container::prefab_container::PrefabContainerEntry;
use crate::container::unique_prefab_container::UniquePrefabEntry;
use crate::container::water_container::WaterContainer;
use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::prefab_category::PrefabCategory;
use crate::prefab::prefab_paste_util::PrefabSupplier;
use crate::prefab::prefab_pattern_generator::PrefabPatternGenerator;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::log_util;
use crate::zone::zone_generator_result::ZoneGeneratorResult;

/// Java `private static final int BIOME_SAMPLE_STEP_SIZE` = `8`.
pub const BIOME_SAMPLE_STEP_SIZE: i32 = 8;

/// Java `private static final int BASE_CHECK_MIN_RADIUS` = `3`.
pub const BASE_CHECK_MIN_RADIUS: i32 = 3;

/// What the placement pass needs from its surroundings.
///
/// The populator implements it over a
/// [`ChunkGeneratorExecution`](crate::chunk::chunk_generator_execution::ChunkGeneratorExecution)
/// (the window is the chunk); the feature query of a later packet implements
/// it over a plain rectangle (the window is the query rect, the core data is
/// empty and the whole window is scanned for biomes).
///
/// # The base-check height is context-specific on purpose
///
/// [`base_check_height`](Self::base_check_height) is **not** a stand-in for
/// [`ChunkGenerator::get_height`]: Java's `isBaseValid` reads the height
/// through `ChunkGeneratorExecution.getHeight`, which returns the *stored*
/// interpolation-table height — `CoreDataCacheEntry.NO_HEIGHT` (`-1`) for a
/// column whose height has not been computed yet (a transcribed Java
/// artifact). `ChunkGenerator::get_height` computes the height instead. The
/// two differ on uncomputed border columns, so the populator must keep the
/// execution-backed read to stay byte-identical to the Java pipeline; the
/// headless query, which has no execution, deliberately uses the generator's
/// idealized height. That difference is documented, not accidental.
pub(crate) trait PlacementContext {
    /// Inclusive minimum block x of the collection window.
    fn window_min_x(&self) -> i32;

    /// Inclusive minimum block z of the collection window.
    fn window_min_z(&self) -> i32;

    /// Inclusive maximum block x of the collection window.
    fn window_max_x(&self) -> i32;

    /// Inclusive maximum block z of the collection window.
    fn window_max_z(&self) -> i32;

    /// The generator the window is generated by (the zone pattern, the biome
    /// lookup and the height queries).
    fn generator(&self) -> &ChunkGenerator;

    /// The biomes known exactly for the window interior — the collection
    /// seeds its biome list with them (the populator: the chunk's core-data
    /// entries; a future query: empty, the region scan covers everything).
    fn core_biomes(&self) -> Vec<Rc<dyn Biome>>;

    /// The region-scan skip zone (`min_x, min_z, max_x, max_z`, inclusive):
    /// the samples inside it are already covered by
    /// [`core_biomes`](Self::core_biomes), so the scan skips them. `None`
    /// scans the whole region.
    fn interior_skip_bounds(&self) -> Option<(i32, i32, i32, i32)>;

    /// The height used by the base check — see the trait docs.
    fn base_check_height(&self, seed: i32, x: i32, z: i32) -> i32;
}

/// Java `private record Candidate(int x, int y, int z, int priority,
/// PrefabRotation rotation, IPrefabBuffer buffer, WorldGenPrefabSupplier
/// supplier, PrefabContainer.PrefabContainerEntry entry,
/// PrefabPatternGenerator generator)`.
pub(crate) struct Candidate {
    /// Java `int x`.
    pub(crate) x: i32,
    /// Java `int y`.
    pub(crate) y: i32,
    /// Java `int z`.
    pub(crate) z: i32,
    /// Java `int priority` — the priority of the prefab category.
    pub(crate) priority: i32,
    /// Java `PrefabRotation rotation`.
    pub(crate) rotation: PrefabRotation,
    /// Java `IPrefabBuffer buffer`.
    pub(crate) buffer: Rc<PrefabBuffer>,
    /// Java `WorldGenPrefabSupplier supplier`.
    pub(crate) supplier: Rc<WorldGenPrefabSupplier>,
    /// Java `PrefabContainer.PrefabContainerEntry entry`.
    pub(crate) entry: Rc<PrefabContainerEntry>,
    /// Java `PrefabPatternGenerator generator`.
    pub(crate) generator: Rc<PrefabPatternGenerator>,
}

/// The collection state of the placement pass (the Java `PrefabPopulator`
/// fields that are not paste-side).
///
/// The Java populator is a *stateful* object that lives in the thread-local
/// [`ChunkGeneratorResource`](crate::chunk::chunk_generator_resource::ChunkGeneratorResource);
/// the Rust port keeps the same reuse shape in this collector so that the
/// populator's reuse semantics are preserved.
pub(crate) struct PlacementCollector {
    /// Java `private int worldSeed`.
    pub(crate) world_seed: i32,
    /// Java `private long prefabSeed`.
    pub(crate) prefab_seed: i64,
    /// Java `private int minPriority`.
    pub(crate) min_priority: i32,
    /// Java `private final FastRandom random = new FastRandom(0)`.
    random: FastRandom,
    /// Java `private final ObjectArrayList<Biome> biomes`.
    pub(crate) biomes: Vec<Rc<dyn Biome>>,
    /// Java `private final ObjectArrayList<Candidate> prefabs`.
    pub(crate) prefabs: Vec<Candidate>,
    /// Java `private final BitSet invalidated`.
    invalidated: Vec<bool>,
}

impl Default for PlacementCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl PlacementCollector {
    /// Java `new PrefabPopulator()` — the implicit constructor of the Java
    /// class (the fields are the initialised ones).
    pub(crate) fn new() -> Self {
        Self {
            world_seed: 0,
            prefab_seed: 0,
            min_priority: PrefabCategory::MAX_PRIORITY,
            random: FastRandom::new(0),
            biomes: Vec::new(),
            prefabs: Vec::new(),
            invalidated: Vec::new(),
        }
    }

    /// Java: the tail of `run` clears the state; the Rust port also resets
    /// before a run so that a previous collection cannot leak in.
    pub(crate) fn reset(&mut self) {
        self.biomes.clear();
        self.prefabs.clear();
        self.invalidated.clear();
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                               BIOME COLLECTION                                               ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `private void collectBiomes(final int seed, final
    /// ChunkGeneratorExecution execution)`.
    ///
    /// Java passes `seed` explicitly; it always equals the `worldSeed` field
    /// of the run, so the pass reads the field.
    pub(crate) fn collect_biomes(&mut self, ctx: &dyn PlacementContext) {
        // The biomes the context knows exactly (the populator: the chunk's
        // own core-data entries).
        for biome in ctx.core_biomes() {
            if biome.get_prefab_container().is_some() {
                self.collect_biome(biome);
            }
        }

        // Inclusive bounds of the window + interpolation border around it.
        let chunk_min_x = ctx.window_min_x() - crate::chunk::MAX_RADIUS;
        let chunk_min_z = ctx.window_min_z() - crate::chunk::MAX_RADIUS;
        let chunk_max_x = ctx.window_max_x() + crate::chunk::MAX_RADIUS;
        let chunk_max_z = ctx.window_max_z() + crate::chunk::MAX_RADIUS;

        // Inclusive bounds of the region to sample biomes from around the
        // window.
        let extents = ctx.generator().get_zone_pattern_provider().get_max_extent();
        let region_min_x = chunk_min_x - extents;
        let region_min_z = chunk_min_z - extents;
        let region_max_x = chunk_max_x + extents;
        let region_max_z = chunk_max_z + extents;

        // Java: `ChunkGenerator.getResource().zoneBiomeResult.zoneResult` — the
        // scratch zone result of the resource. The Rust port uses a local
        // scratch (the record is tiny and the resource is borrowed by the
        // populator itself).
        let mut zone_result = ZoneGeneratorResult::new();
        let zone_generator = ctx.generator().get_zone_pattern_generator(self.world_seed);

        // The samples inside this zone are already covered by the core data
        // of the context (the populator: the chunk's interpolation region).
        let interior_skip_bounds = ctx.interior_skip_bounds();

        for z in (region_min_z..=region_max_z).step_by(BIOME_SAMPLE_STEP_SIZE as usize) {
            for x in (region_min_x..=region_max_x).step_by(BIOME_SAMPLE_STEP_SIZE as usize) {
                // Skip if the position lies inside the interpolation region of
                // the chunk.
                if let Some((skip_min_x, skip_min_z, skip_max_x, skip_max_z)) = interior_skip_bounds
                {
                    if x >= skip_min_x && z >= skip_min_z && x <= skip_max_x && z <= skip_max_z {
                        continue;
                    }
                }

                zone_generator.generate_into(self.world_seed, x as f64, z as f64, &mut zone_result);
                let zone = zone_result
                    .get_zone()
                    .expect("collectBiomes: the zone result has no zone (Java: NPE)")
                    .clone();
                let biome_generator = zone.biome_pattern_generator.clone();

                // Skip if the zone doesn't have any prefabs that can extend
                // into the chunk from this position.
                let biome_extents = biome_generator.get_extents();
                let min_x = chunk_min_x - biome_extents;
                let min_z = chunk_min_z - biome_extents;
                let max_x = chunk_max_x + biome_extents;
                let max_z = chunk_max_z + biome_extents;
                if x < min_x || z < min_z || x > max_x || z > max_z {
                    continue;
                }

                let Some(biome) = biome_generator.get_biome(self.world_seed, x, z) else {
                    continue;
                };

                if biome.get_prefab_container().is_some() {
                    self.collect_biome(Rc::clone(&biome) as Rc<dyn Biome>);
                }

                // Collect all custom biomes that might generate at or near
                // this position (we're sampling sparsely). The actual biome is
                // checked later during the prefab position generation (see
                // the uses of `is_matching_biome`).
                for custom_biome in biome_generator.get_custom_biomes() {
                    if custom_biome.get_prefab_container().is_some()
                        && custom_biome
                            .get_custom_biome_generator()
                            .is_valid_parent_biome(biome.get_id())
                    {
                        self.collect_biome(Rc::clone(custom_biome) as Rc<dyn Biome>);
                    }
                }
            }
        }
    }

    /// Java `private void collectBiome(@Nonnull Biome biome)`.
    ///
    /// The Java method inserts the biome into the list *ordered by the biome
    /// id*; whatever gets swapped out becomes the new insertion candidate.
    fn collect_biome(&mut self, biome: Rc<dyn Biome>) {
        let mut insert = biome;

        for i in 0..self.biomes.len() {
            let id = self.biomes[i].get_id();

            // Exit early if already in the list.
            if insert.get_id() == id {
                return;
            }

            // Ordered insertion based on the biome id. Whatever gets swapped
            // out here becomes the new insertion candidate.
            if insert.get_id() < id {
                std::mem::swap(&mut self.biomes[i], &mut insert);
            }
        }

        // Append the last insert candidate to have been swapped out (or the
        // original 'biome' if it has the largest id).
        self.biomes.push(insert);
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                              CANDIDATE COLLECTION                                              ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `private void collectPrefabs(final int seed, final
    /// ChunkGeneratorExecution execution)`.
    ///
    /// Java passes `seed` explicitly; it always equals the `worldSeed` field
    /// of the run, so the pass reads the field.
    pub(crate) fn collect_prefabs(
        &mut self,
        ctx: &dyn PlacementContext,
        unique_prefabs: &[UniquePrefabEntry],
    ) {
        // The biome list is cloned so that the closure below can borrow the
        // collector mutably (the Java keeps the biome in a field instead).
        let biomes = self.biomes.clone();

        for biome in biomes {
            let Some(container) = biome.get_prefab_container() else {
                continue;
            };

            let mut id = 0;
            for entry in container.get_entries() {
                let entry = Rc::clone(entry);
                id += 1;
                self.prefab_seed =
                    hash_util::hash3(self.world_seed as i64, biome.get_id() as i64, id as i64);

                let pattern_generator = entry.get_prefab_pattern_generator();
                let grid_generator = pattern_generator
                    .get_grid_generator()
                    .expect("collectPrefabs: the pattern has no grid generator (Java: NPE)");

                let extents = entry.get_extents();
                let min_x = (ctx.window_min_x() - extents) as f64;
                let min_z = (ctx.window_min_z() - extents) as f64;
                let max_x = (ctx.window_max_x() + extents) as f64;
                let max_z = (ctx.window_max_z() + extents) as f64;

                let biome = Rc::clone(&biome);
                grid_generator.collect(
                    self.world_seed,
                    min_x,
                    min_z,
                    max_x,
                    max_z,
                    &mut |px: f64, pz: f64| {
                        self.collect_prefab(px, pz, &biome, &entry, ctx, unique_prefabs);
                    },
                );
            }
        }
    }

    /// Java `private void collectPrefab(final double px, final double pz)`.
    fn collect_prefab(
        &mut self,
        px: f64,
        pz: f64,
        biome: &Rc<dyn Biome>,
        entry: &Rc<PrefabContainerEntry>,
        ctx: &dyn PlacementContext,
        unique_prefabs: &[UniquePrefabEntry],
    ) {
        let x = math_util::floor(px);
        let z = math_util::floor(pz);

        self.random.set_seed(
            hash_util::hash3(self.prefab_seed, x as i64, z as i64).wrapping_mul(0x5feb_8caf),
        );

        let pattern_generator = entry.get_prefab_pattern_generator();
        if !Self::is_matching_noise_density(self.world_seed, x, z, pattern_generator) {
            return;
        }

        // Occlude if a higher priority unique-prefab is within the exclusion
        // radius.
        if Self::is_within_unique_prefab_exclusion_range(x, z, pattern_generator, unique_prefabs) {
            return;
        }

        let result = ctx
            .generator()
            .get_zone_biome_result_at(self.world_seed, x, z);
        if !Self::is_matching_biome(biome, &result) {
            return;
        }

        let prefabs = entry.get_prefabs();
        let supplier = prefabs.get_random(&mut self.random).cloned();
        let Some(supplier) = supplier else {
            return;
        };

        // Java: `final var prefab = supplier.get(); if (prefab == null) return;`
        // — the Java supplier returns the buffer of the per-thread prefab
        // cache (never `null` in practice); the Rust port uses the trait
        // method, which keeps the nullable contract.
        let prefab = PrefabSupplier::get(&supplier);
        let Some(prefab) = prefab else {
            return;
        };

        let rotation = Self::generate_rotation(x, z, &mut self.random, pattern_generator);

        // Estimate the height at that position and check if the prefab should
        // spawn at that height.
        let biome_at_position = result
            .get_biome()
            .expect("collectPrefab: the zone biome result has no biome (Java: NPE)")
            .clone();
        let y = Self::get_height(
            self.world_seed,
            x,
            z,
            ctx,
            &biome_at_position,
            pattern_generator,
            &mut self.random,
        );

        if !Self::is_matching_height(
            self.world_seed,
            x,
            z,
            y,
            &mut self.random,
            pattern_generator,
        ) {
            return;
        }

        // Check if the prefab is able to spawn on that block.
        if !Self::is_matching_parent_block(
            self.world_seed,
            x,
            z,
            y,
            &mut self.random,
            &result,
            entry,
        ) {
            return;
        }

        if !Self::is_base_valid(
            self.world_seed,
            x,
            z,
            y,
            rotation,
            &prefab,
            pattern_generator,
            ctx,
        ) {
            log_util::debug(&format!(
                "Prefab failed base-check at: /tp {x} {y} {z} [{}]",
                supplier.get_prefab_name()
            ));
            return;
        }

        let category = pattern_generator.get_category();

        self.prefabs.push(Candidate {
            x,
            y,
            z,
            priority: category.priority,
            rotation,
            buffer: prefab,
            supplier: Rc::new(supplier),
            entry: Rc::clone(entry),
            generator: Rc::clone(pattern_generator),
        });
        self.min_priority = self.min_priority.min(category.priority);
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                                INVALIDATION                                                 ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `private void invalidatePrefabs()`.
    pub(crate) fn invalidate_prefabs(&mut self) {
        if self.invalidated.len() < self.prefabs.len() {
            self.invalidated.resize(self.prefabs.len(), false);
        }
        Self::invalidate_candidates(&self.prefabs, &mut self.invalidated, self.min_priority);
    }

    /// The reusable core of `invalidatePrefabs`: marks every candidate that
    /// overlaps a *higher priority* one.
    ///
    /// `invalidated` must hold one bit per candidate (the caller sizes it);
    /// the bits that are already set are left alone.
    pub(crate) fn invalidate_candidates(
        candidates: &[Candidate],
        invalidated: &mut [bool],
        min_priority: i32,
    ) {
        for i in 0..candidates.len() {
            let candidate = &candidates[i];
            let (min_x, min_y, min_z, max_x, max_y, max_z) = Self::candidate_bounds(candidate);

            // Skip if the candidate is already marked invalid or is too low
            // priority to invalidate any others.
            if candidate.priority <= min_priority || invalidated[i] {
                continue;
            }

            // Mark any lower-priority candidates invalid if they overlap this
            // one.
            for j in 0..candidates.len() {
                let other = &candidates[j];

                if candidate.priority <= other.priority {
                    continue;
                }

                let (other_min_x, other_min_y, other_min_z, other_max_x, other_max_y, other_max_z) =
                    Self::candidate_bounds(other);
                if Self::intersects(
                    min_x,
                    min_y,
                    min_z,
                    max_x,
                    max_y,
                    max_z,
                    other_min_x,
                    other_min_y,
                    other_min_z,
                    other_max_x,
                    other_max_y,
                    other_max_z,
                ) {
                    invalidated[j] = true;
                }
            }
        }
    }

    /// The rotated inclusive AABB of `candidate`, in world coordinates:
    /// `(min_x, min_y, min_z, max_x, max_y, max_z)`.
    pub(crate) fn candidate_bounds(candidate: &Candidate) -> (i32, i32, i32, i32, i32, i32) {
        Self::rotated_bounds(
            candidate.x,
            candidate.y,
            candidate.z,
            candidate.rotation,
            &candidate.buffer,
        )
    }

    /// The rotated inclusive AABB of a prefab placed at `(x, y, z)`.
    fn rotated_bounds(
        x: i32,
        y: i32,
        z: i32,
        rotation: PrefabRotation,
        buffer: &PrefabBuffer,
    ) -> (i32, i32, i32, i32, i32, i32) {
        let access = buffer.new_access();
        (
            x.wrapping_add(access.get_min_x(rotation)),
            y.wrapping_add(access.get_min_y()),
            z.wrapping_add(access.get_min_z(rotation)),
            x.wrapping_add(access.get_max_x(rotation)),
            y.wrapping_add(access.get_max_y()),
            z.wrapping_add(access.get_max_z(rotation)),
        )
    }

    /// Java `private static boolean intersects(…)`.
    #[allow(clippy::too_many_arguments)]
    fn intersects(
        min_x1: i32,
        min_y1: i32,
        min_z1: i32,
        max_x1: i32,
        max_y1: i32,
        max_z1: i32,
        min_x2: i32,
        min_y2: i32,
        min_z2: i32,
        max_x2: i32,
        max_y2: i32,
        max_z2: i32,
    ) -> bool {
        !(max_x1 < min_x2
            || min_x1 > max_x2
            || max_y1 < min_y2
            || min_y1 > max_y2
            || max_z1 < min_z2
            || min_z1 > max_z2)
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                               PREDICATES                                                    ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `private static boolean isBaseValid(…)`.
    #[allow(clippy::too_many_arguments)]
    fn is_base_valid(
        seed: i32,
        x: i32,
        z: i32,
        y: i32,
        rotation: PrefabRotation,
        buffer: &PrefabBuffer,
        pattern: &PrefabPatternGenerator,
        ctx: &dyn PlacementContext,
    ) -> bool {
        if pattern.get_base_checks().is_empty() {
            return true;
        }

        let (min_x, min_y, min_z, max_x, _, max_z) =
            Self::rotated_bounds(x, y, z, rotation, buffer);

        let size_x = max_x.wrapping_sub(min_x);
        let size_z = max_z.wrapping_sub(min_z);
        let size = size_x.max(size_z);

        for check in pattern.get_base_checks() {
            if size < check.get_prefab_size() {
                continue;
            }

            let mid_x = min_x.wrapping_add(max_x) >> 1;
            let mid_z = min_z.wrapping_add(max_z) >> 1;

            let threshold = min_y.wrapping_sub(check.get_height_tolerance());
            // Java: `MathUtil.fastFloor((sizeX >> 1) * check.getBaseScale())` —
            // the multiplication is a *float* multiplication (the base scale is
            // a `float`), and `fastFloor(float)` returns an `int`.
            let rad_x = math_util::floor(f64::from((size_x >> 1) as f32 * check.get_base_scale()));
            let rad_z = math_util::floor(f64::from((size_z >> 1) as f32 * check.get_base_scale()));

            // Check the height at 4 additional points in a plus (+) pattern
            // around the body center (this works better for a wider variety
            // of prefabs than using the corners [round and organic shapes!]).
            // --------X--------
            // |               |
            // X               X
            // |               |
            // --------X--------
            //
            // Skip some checks if the prefab is narrow along the x or z axis
            // (rad <= BASE_CHECK_MIN_RADIUS).
            return (rad_x <= BASE_CHECK_MIN_RADIUS
                || ctx.base_check_height(seed, mid_x - rad_x, mid_z) >= threshold)
                && (rad_x <= BASE_CHECK_MIN_RADIUS
                    || ctx.base_check_height(seed, mid_x + rad_x, mid_z) >= threshold)
                && (rad_z <= BASE_CHECK_MIN_RADIUS
                    || ctx.base_check_height(seed, mid_x, mid_z - rad_z) >= threshold)
                && (rad_z <= BASE_CHECK_MIN_RADIUS
                    || ctx.base_check_height(seed, mid_x, mid_z + rad_z) >= threshold);
        }

        true
    }

    /// Java `private static boolean isWithinUniquePrefabExclusionRange(…)`.
    fn is_within_unique_prefab_exclusion_range(
        x: i32,
        z: i32,
        generator: &PrefabPatternGenerator,
        unique_prefabs: &[UniquePrefabEntry],
    ) -> bool {
        let radius = generator.get_exclusion_radius() as i64;

        if radius <= 0 {
            return false;
        }

        let radius2 = radius * radius;
        let priority = generator.get_category().priority;

        for unique in unique_prefabs {
            if priority >= unique.get_category().priority {
                continue;
            }

            let position = unique.get_position();
            let dx = x as i64 - position.0 as i64;
            let dz = z as i64 - position.2 as i64;
            if dx * dx + dz * dz <= radius2 {
                return true;
            }
        }

        false
    }

    /// Java `private static int getHeight(int seed, int x, int z, @Nonnull
    /// ChunkGeneratorExecution execution, @Nonnull Biome biome, @Nonnull
    /// PrefabPatternGenerator prefabPatternGenerator, Random random)`.
    fn get_height(
        seed: i32,
        x: i32,
        z: i32,
        ctx: &dyn PlacementContext,
        biome: &Rc<dyn Biome>,
        prefab_pattern_generator: &PrefabPatternGenerator,
        random: &mut FastRandom,
    ) -> i32 {
        let mut height;
        if prefab_pattern_generator.is_on_water() && prefab_pattern_generator.is_deep_search() {
            height = i32::MIN;
            let water_container = biome.get_water_container();
            for water_entry in water_container.get_entries() {
                let max = water_entry.get_max(seed, x, z);
                if max != WaterContainer::NO_WATER_AT_COORDINATED
                    && prefab_pattern_generator
                        .get_height_condition()
                        .eval(seed, x, z, max, random)
                {
                    height = max;
                    break;
                }
            }
        } else if prefab_pattern_generator.is_on_water() {
            height = biome.get_water_container().get_max_height(seed, x, z);
        } else if prefab_pattern_generator.is_deep_search() {
            let interpreter = prefab_pattern_generator
                .get_height_threshold_interpreter()
                .expect(
                    "getHeight: the deep search has no height threshold interpreter (Java: NPE)",
                );
            height = ctx
                .generator()
                .generate_height_between(seed, x, z, interpreter);
        } else {
            height = ctx.generator().get_height(seed, x, z);
        }
        height = height.wrapping_add(prefab_pattern_generator.get_displacement(seed, x, z));
        height
    }

    /// Java `private static PrefabRotation generateRotation(int x, int z,
    /// @Nonnull Random random, @Nonnull PrefabPatternGenerator
    /// patternGenerator)`.
    fn generate_rotation(
        _x: i32,
        _z: i32,
        random: &mut FastRandom,
        pattern_generator: &PrefabPatternGenerator,
    ) -> PrefabRotation {
        match pattern_generator.get_rotations() {
            Some(rotations) => {
                let index = random.next_int_bound(rotations.len() as i32) as usize;
                rotations[index]
            }
            None => {
                let index = random.next_int_bound(PrefabRotation::VALUES.len() as i32) as usize;
                PrefabRotation::VALUES[index]
            }
        }
    }

    /// Java `private static boolean isMatchingBiome(Biome biome, @Nonnull
    /// ZoneBiomeResult zoneAndBiomeResult)`.
    ///
    /// The Java compares the *references* (`==`); the Rust port compares the
    /// shared handles (`Rc::ptr_eq`) — the biome instances are shared
    /// through the zone's biome pattern generator.
    fn is_matching_biome(biome: &Rc<dyn Biome>, zone_and_biome_result: &ZoneBiomeResult) -> bool {
        match zone_and_biome_result.get_biome() {
            Some(other) => Rc::ptr_eq(other, biome),
            None => false,
        }
    }

    /// Java `private static boolean isMatchingHeight(int seed, int x, int z,
    /// int y, Random random, @Nonnull PrefabPatternGenerator
    /// prefabPatternGenerator)`.
    fn is_matching_height(
        seed: i32,
        x: i32,
        z: i32,
        y: i32,
        random: &mut FastRandom,
        prefab_pattern_generator: &PrefabPatternGenerator,
    ) -> bool {
        prefab_pattern_generator
            .get_height_condition()
            .eval(seed, x, z, y, random)
    }

    /// Java `private static boolean isMatchingNoiseDensity(int seed, int x,
    /// int z, @Nonnull PrefabPatternGenerator prefabPatternGenerator)`.
    fn is_matching_noise_density(
        seed: i32,
        x: i32,
        z: i32,
        prefab_pattern_generator: &PrefabPatternGenerator,
    ) -> bool {
        prefab_pattern_generator
            .get_map_condition()
            .eval2(seed, x, z)
    }

    /// Java `private static boolean isMatchingParentBlock(int seed, int x,
    /// int z, int y, @Nonnull Random random, @Nonnull ZoneBiomeResult
    /// zoneAndBiomeResult, @Nonnull PrefabContainer.PrefabContainerEntry
    /// containerEntry)`.
    #[allow(clippy::too_many_arguments)]
    fn is_matching_parent_block(
        seed: i32,
        x: i32,
        z: i32,
        y: i32,
        random: &mut FastRandom,
        zone_and_biome_result: &ZoneBiomeResult,
        container_entry: &PrefabContainerEntry,
    ) -> bool {
        let parent_condition = container_entry
            .get_prefab_pattern_generator()
            .get_parent_condition();

        // ⚠ The Java method has two more checks:
        //
        // ```java
        // if (parentCondition == ConstantIntCondition.DEFAULT_TRUE) return true;
        // if (parentCondition == ConstantIntCondition.DEFAULT_FALSE) return false;
        // ```
        //
        // They are **dead code**: `parentCondition` is declared as an
        // `IBlockFluidCondition` and `ConstantIntCondition` implements
        // `IIntCondition`, so a `ConstantIntCondition` can never be stored in
        // that field — the `==` comparisons can never be true. The checks are
        // therefore not ported (the same reasoning as the `BlockPopulator`).

        let biome = zone_and_biome_result
            .get_biome()
            .expect("isMatchingParentBlock: the zone biome result has no biome (Java: NPE)");

        let ground_cover = Self::get_cover_in_ground_at(seed, x, z, y, random, biome);
        if ground_cover != BlockFluidEntry::EMPTY
            && !parent_condition.eval(ground_cover.block_id, ground_cover.fluid_id)
        {
            // The parent block of the prefab is not correct.
            return false;
        }

        let top_block = biome.get_layer_container().get_top_block_at(seed, x, z);
        parent_condition.eval(top_block.block_id, top_block.fluid_id)
    }

    /// Java `private static BlockFluidEntry getCoverInGroundAt(int seed, int
    /// x, int z, int y, @Nonnull Random random, @Nonnull Biome biome)`.
    fn get_cover_in_ground_at(
        seed: i32,
        x: i32,
        z: i32,
        y: i32,
        random: &mut FastRandom,
        biome: &Rc<dyn Biome>,
    ) -> BlockFluidEntry {
        for cover_container_entry in biome.get_cover_container().get_entries() {
            if y < Self::world_height()
                && Self::is_matching_cover(seed, x, z, y, random, cover_container_entry)
            {
                let cover_entry = cover_container_entry.get(random).copied();
                if let Some(cover_entry) = cover_entry {
                    if cover_entry.get_offset() == -1 {
                        return cover_entry.get_entry();
                    }
                }
            }
        }
        BlockFluidEntry::EMPTY
    }

    /// Java `private static boolean isMatchingCover(int seed, int x, int z,
    /// int y, @Nonnull Random random, @Nonnull CoverContainer
    /// .CoverContainerEntry coverContainerEntry)`.
    fn is_matching_cover(
        seed: i32,
        x: i32,
        z: i32,
        y: i32,
        random: &mut FastRandom,
        cover_container_entry: &CoverContainerEntry,
    ) -> bool {
        random.next_double() < cover_container_entry.get_cover_density()
            && cover_container_entry.get_map_condition().eval2(seed, x, z)
            && cover_container_entry
                .get_height_condition()
                .eval(seed, x, z, y, random)
    }

    /// Java `ChunkUtil.HEIGHT` — the legacy chunk height.
    #[allow(deprecated)]
    fn world_height() -> i32 {
        chunk_util::HEIGHT
    }

    /// Java `BitSet.get(int)` — the invalidation bit of a candidate.
    pub(crate) fn invalidated(&self, index: usize) -> bool {
        self.invalidated.get(index).copied().unwrap_or(false)
    }

    /// Java `BitSet.set(int)` — the invalidation bit of a candidate.
    ///
    /// Reserved for the later feature-query pass, which invalidates
    /// candidates incrementally instead of through
    /// [`invalidate_candidates`](Self::invalidate_candidates).
    #[allow(dead_code)]
    fn set_invalidated(&mut self, index: usize) {
        if self.invalidated.len() <= index {
            self.invalidated.resize(index + 1, false);
        }
        self.invalidated[index] = true;
    }

    /// The candidates the paste phase iterates (surviving ones included).
    pub(crate) fn candidates(&self) -> &[Candidate] {
        &self.prefabs
    }
}

#[cfg(test)]
mod tests {
    use crate::biome::biome::Biome;
    use crate::chunk::populator::prefab_populator::ChunkCtx;
    use crate::chunk::populator::test_stubs::{
        execution, generator_for_biome, pattern_generator, supplier_map, test_buffer,
        test_supplier, tile_biome_with_prefabs, RecordingPointGenerator,
    };
    use crate::container::prefab_container::PrefabContainerEntry;
    use crate::prefab::prefab_base_check::PrefabBaseCheck;
    use crate::prefab::prefab_rotation::PrefabRotation;
    use procedural::noise::point::IPointGenerator;

    use super::*;

    /* ============================================================================================================ */
    /*                                             the biome collection                                              */
    /* ============================================================================================================ */

    #[test]
    fn the_biome_list_is_ordered_by_the_biome_id() {
        let mut collector = PlacementCollector::new();
        collector.collect_biome(tile_biome_with_prefabs(5, None) as Rc<dyn Biome>);
        collector.collect_biome(tile_biome_with_prefabs(9, None) as Rc<dyn Biome>);
        collector.collect_biome(tile_biome_with_prefabs(7, None) as Rc<dyn Biome>);

        let ids: Vec<i32> = collector
            .biomes
            .iter()
            .map(|biome| biome.get_id())
            .collect();
        assert_eq!(ids, vec![5, 7, 9]);
    }

    #[test]
    fn a_biome_that_is_already_in_the_list_is_not_added_twice() {
        let mut collector = PlacementCollector::new();
        collector.collect_biome(tile_biome_with_prefabs(5, None) as Rc<dyn Biome>);
        collector.collect_biome(tile_biome_with_prefabs(9, None) as Rc<dyn Biome>);
        collector.collect_biome(tile_biome_with_prefabs(5, None) as Rc<dyn Biome>);

        let ids: Vec<i32> = collector
            .biomes
            .iter()
            .map(|biome| biome.get_id())
            .collect();
        assert_eq!(ids, vec![5, 9]);
    }

    /* ============================================================================================================ */
    /*                                             the intersection test                                             */
    /* ============================================================================================================ */

    #[test]
    fn the_intersection_test_reports_the_overlap() {
        // Two boxes that overlap in x, y and z.
        assert!(PlacementCollector::intersects(
            0, 0, 0, 10, 10, 10, 5, 5, 5, 15, 15, 15
        ));
        // A box that is entirely above the other one in y.
        assert!(!PlacementCollector::intersects(
            0, 0, 0, 10, 10, 10, 0, 11, 0, 10, 20, 10
        ));
        // A box that is entirely to the right in x.
        assert!(!PlacementCollector::intersects(
            0, 0, 0, 10, 10, 10, 11, 0, 0, 20, 10, 10
        ));
        // A box that is entirely behind in z.
        assert!(!PlacementCollector::intersects(
            0, 0, 0, 10, 10, 10, 0, 0, 11, 10, 10, 20
        ));
        // Touching boxes intersect (the test is inclusive).
        assert!(PlacementCollector::intersects(
            0, 0, 0, 10, 10, 10, 10, 0, 0, 20, 10, 10
        ));
    }

    /* ============================================================================================================ */
    /*                                             the rotation pick                                                 */
    /* ============================================================================================================ */

    #[test]
    fn the_rotation_is_picked_from_the_pattern_rotations() {
        let pattern = pattern_generator(
            None,
            Some(vec![
                PrefabRotation::Rotation90,
                PrefabRotation::Rotation270,
            ]),
            0,
            Vec::new(),
        );
        let mut random = FastRandom::new(1234);
        // The pick consumes exactly one `nextInt(2)` draw.
        let mut reference = FastRandom::new(1234);
        let expected_index = reference.next_int_bound(2) as usize;

        let rotation = PlacementCollector::generate_rotation(0, 0, &mut random, &pattern);
        assert_eq!(
            rotation,
            [PrefabRotation::Rotation90, PrefabRotation::Rotation270][expected_index]
        );
        assert_eq!(
            random.next_int(),
            reference.next_int(),
            "the rotation pick consumes exactly one draw"
        );
    }

    #[test]
    fn the_rotation_defaults_to_the_four_quarter_turns() {
        let pattern = pattern_generator(None, None, 0, Vec::new());
        let mut random = FastRandom::new(99);
        let mut reference = FastRandom::new(99);
        let expected_index = reference.next_int_bound(4) as usize;

        let rotation = PlacementCollector::generate_rotation(0, 0, &mut random, &pattern);
        assert_eq!(rotation, PrefabRotation::VALUES[expected_index]);
    }

    /* ============================================================================================================ */
    /*                                             the base check                                                    */
    /* ============================================================================================================ */

    /// A base check with the given parameters.
    fn base_check(prefab_size: i32, base_scale: f32, height_tolerance: i32) -> PrefabBaseCheck {
        PrefabBaseCheck {
            base_scale,
            prefab_size,
            height_tolerance,
        }
    }

    #[test]
    fn a_prefab_without_base_checks_is_always_valid() {
        let pattern = pattern_generator(None, None, 0, Vec::new());
        let buffer = test_buffer(&[(0, 0, 0)]);
        let generator = generator_for_biome(tile_biome_with_prefabs(7, None));
        let execution = execution(generator);
        let ctx = ChunkCtx::new(&execution);
        assert!(PlacementCollector::is_base_valid(
            0,
            0,
            0,
            0,
            PrefabRotation::Rotation0,
            &buffer,
            &pattern,
            &ctx
        ));
    }

    #[test]
    fn the_base_check_probes_the_four_points_of_the_plus_pattern() {
        // The prefab spans (0,0,0)..(8,0,8) → size 8, rad 4, mid (4, 4).
        let buffer = test_buffer(&[(0, 0, 0), (8, 0, 0), (0, 0, 8), (8, 0, 8)]);
        let pattern = pattern_generator(None, None, 0, vec![base_check(8, 1.0, 2)]);

        // The stub generator reports the height 63 for every column.
        let generator = generator_for_biome(tile_biome_with_prefabs(7, None));
        let execution = execution(generator);
        let ctx = ChunkCtx::new(&execution);

        // The prefab is placed at y = 60 → the threshold is 58 → the probe
        // heights (63) are above it.
        assert!(PlacementCollector::is_base_valid(
            0,
            100,
            100,
            60,
            PrefabRotation::Rotation0,
            &buffer,
            &pattern,
            &ctx
        ));

        // The prefab is placed at y = 100 → the threshold is 98 → the probe
        // heights (63) are below it.
        assert!(!PlacementCollector::is_base_valid(
            0,
            100,
            100,
            100,
            PrefabRotation::Rotation0,
            &buffer,
            &pattern,
            &ctx
        ));
    }

    #[test]
    fn the_base_check_skips_the_narrow_prefabs() {
        // The prefab spans (0,0,0)..(2,0,2) → size 2 → rad 1, which is below
        // the `BASE_CHECK_MIN_RADIUS` (3) → the probes are skipped.
        let buffer = test_buffer(&[(0, 0, 0), (2, 0, 0), (0, 0, 2), (2, 0, 2)]);
        let pattern = pattern_generator(None, None, 0, vec![base_check(2, 1.0, 2)]);

        let generator = generator_for_biome(tile_biome_with_prefabs(7, None));
        let execution = execution(generator);
        let ctx = ChunkCtx::new(&execution);

        assert!(
            PlacementCollector::is_base_valid(
                0,
                100,
                100,
                100,
                PrefabRotation::Rotation0,
                &buffer,
                &pattern,
                &ctx
            ),
            "a narrow prefab skips the height probes"
        );
    }

    #[test]
    fn the_base_check_only_uses_the_first_matching_check() {
        // The first check (prefab size 30) does not apply to a prefab of
        // size 8; the second one (prefab size 8) does — and it *fails*, so
        // the result is "invalid" even though a third check would pass.
        let buffer = test_buffer(&[(0, 0, 0), (8, 0, 0), (0, 0, 8), (8, 0, 8)]);
        let pattern = pattern_generator(
            None,
            None,
            0,
            vec![
                base_check(30, 1.0, 2),
                base_check(8, 1.0, 2),
                base_check(1, 1.0, 1000),
            ],
        );

        let generator = generator_for_biome(tile_biome_with_prefabs(7, None));
        let execution = execution(generator);
        let ctx = ChunkCtx::new(&execution);

        assert!(!PlacementCollector::is_base_valid(
            0,
            100,
            100,
            100,
            PrefabRotation::Rotation0,
            &buffer,
            &pattern,
            &ctx
        ));
    }

    /* ============================================================================================================ */
    /*                                             the draw order                                                    */
    /* ============================================================================================================ */

    #[test]
    fn the_prefab_pick_and_the_rotation_follow_the_java_draw_order() {
        // Two prefabs (the weighted map draws a `nextDouble`) and the four
        // quarter turns (the rotation draws a `nextInt(4)`).
        let first = test_supplier("PopulatorFirst", &[(0, 0, 0)]);
        let second = test_supplier("PopulatorSecond", &[(0, 0, 0)]);
        let map = supplier_map(vec![first.clone(), second.clone()]);

        let grid = Rc::new(RecordingPointGenerator::new(Vec::new()));
        let pattern = pattern_generator(Some(grid), None, 0, Vec::new());
        let entry = Rc::new(PrefabContainerEntry::new(map, pattern, 0));

        let biome_tile = tile_biome_with_prefabs(7, None);
        let biome: Rc<dyn Biome> = biome_tile.clone();
        let generator = generator_for_biome(Rc::clone(&biome_tile));
        let execution = execution(Rc::clone(&generator));
        let ctx = ChunkCtx::new(&execution);

        let mut collector = PlacementCollector::new();
        collector.world_seed = 42;
        collector.prefab_seed = hash_util::hash3(42, 7, 1);

        // The lattice point (10.5, 20.5) → the block (10, 20).
        collector.collect_prefab(10.5, 20.5, &biome, &entry, &ctx, &[]);

        assert_eq!(collector.prefabs.len(), 1, "the candidate is collected");
        let candidate = &collector.prefabs[0];
        assert_eq!(candidate.x, 10);
        assert_eq!(candidate.z, 20);

        // The Java draw order: the stream is re-seeded for the lattice
        // point, the weighted pick draws a `nextDouble` and the rotation
        // draws a `nextInt(4)`.
        let mut reference = FastRandom::new(0);
        reference
            .set_seed(hash_util::hash3(collector.prefab_seed, 10, 20).wrapping_mul(0x5feb_8caf));
        let expected_pick = reference.next_double();
        let expected_rotation = PrefabRotation::VALUES[reference.next_int_bound(4) as usize];

        let expected_name = if expected_pick < 0.5 {
            first.get_prefab_name()
        } else {
            second.get_prefab_name()
        };
        assert_eq!(
            candidate.supplier.get_prefab_name(),
            expected_name,
            "the prefab pick consumes the first draw"
        );
        assert_eq!(
            candidate.rotation, expected_rotation,
            "the rotation consumes the second draw"
        );
    }

    #[test]
    fn a_singleton_prefab_map_does_not_consume_a_draw() {
        let only = test_supplier("PopulatorOnly", &[(0, 0, 0)]);
        let map = supplier_map(vec![only.clone()]);

        let grid = Rc::new(RecordingPointGenerator::new(Vec::new()));
        let pattern = pattern_generator(Some(grid), None, 0, Vec::new());
        let entry = Rc::new(PrefabContainerEntry::new(map, pattern, 0));

        let biome_tile = tile_biome_with_prefabs(7, None);
        let biome: Rc<dyn Biome> = biome_tile.clone();
        let generator = generator_for_biome(Rc::clone(&biome_tile));
        let execution = execution(Rc::clone(&generator));
        let ctx = ChunkCtx::new(&execution);

        let mut collector = PlacementCollector::new();
        collector.world_seed = 42;
        collector.prefab_seed = hash_util::hash3(42, 7, 1);
        collector.collect_prefab(10.5, 20.5, &biome, &entry, &ctx, &[]);

        assert_eq!(collector.prefabs.len(), 1);
        let candidate = &collector.prefabs[0];

        // The singleton map does not consume a draw — the rotation is the
        // *first* draw of the stream.
        let mut reference = FastRandom::new(0);
        reference
            .set_seed(hash_util::hash3(collector.prefab_seed, 10, 20).wrapping_mul(0x5feb_8caf));
        let expected_rotation = PrefabRotation::VALUES[reference.next_int_bound(4) as usize];
        assert_eq!(candidate.rotation, expected_rotation);
        assert_eq!(candidate.supplier.get_prefab_name(), only.get_prefab_name());
    }

    /* ============================================================================================================ */
    /*                                             the lattice collection                                            */
    /* ============================================================================================================ */

    #[test]
    fn the_lattice_is_collected_over_the_chunk_padded_by_the_entry_extents() {
        let supplier = test_supplier("PopulatorLattice", &[(0, 0, 0), (2, 0, 2)]);
        let map = supplier_map(vec![supplier]);
        // The extents of the entry are the maximum length of the prefab
        // bounds: the prefab spans (0,0,0)..(2,0,2) → 2.
        assert_eq!(
            PrefabContainerEntry::new(map.clone(), pattern_generator(None, None, 0, Vec::new()), 0)
                .get_extents(),
            2
        );

        let grid = Rc::new(RecordingPointGenerator::new(Vec::new()));
        let pattern = pattern_generator(
            Some(Rc::clone(&grid) as Rc<dyn IPointGenerator>),
            None,
            0,
            Vec::new(),
        );
        let entry = Rc::new(PrefabContainerEntry::new(map, pattern, 0));

        let container = Rc::new(crate::container::prefab_container::PrefabContainer::new(
            vec![Rc::clone(&entry)],
        ));
        let biome = tile_biome_with_prefabs(7, Some(container));
        let generator = generator_for_biome(Rc::clone(&biome));
        let execution = execution(Rc::clone(&generator));
        let ctx = ChunkCtx::new(&execution);

        let mut collector = PlacementCollector::new();
        collector.world_seed = 42;
        collector.biomes.push(Rc::clone(&biome) as Rc<dyn Biome>);
        collector.collect_prefabs(&ctx, &[]);

        let bounds = grid.bounds.borrow().expect("the lattice was collected");
        assert_eq!(
            bounds,
            (-2.0, -2.0, 33.0, 33.0),
            "the chunk (0,0) padded by the extents of the entry"
        );
    }

    /* ============================================================================================================ */
    /*                                             the invalidation                                                  */
    /* ============================================================================================================ */

    /// A candidate with the given position, priority and prefab size.
    fn candidate(x: i32, y: i32, z: i32, priority: i32, size: i32) -> Candidate {
        let blocks = vec![(0, 0, 0), (size, 0, 0), (0, 0, size), (size, 0, size)];
        Candidate {
            x,
            y,
            z,
            priority,
            rotation: PrefabRotation::Rotation0,
            buffer: test_buffer(&blocks),
            supplier: Rc::new(test_supplier("PopulatorInvalidation", &blocks)),
            entry: Rc::new(PrefabContainerEntry::new(
                supplier_map(Vec::new()),
                pattern_generator(None, None, priority, Vec::new()),
                0,
            )),
            generator: pattern_generator(None, None, priority, Vec::new()),
        }
    }

    #[test]
    fn a_higher_priority_candidate_invalidates_an_overlapping_one() {
        let mut collector = PlacementCollector::new();
        // The high priority candidate covers (0,0,0)..(8,0,8); the low
        // priority one sits inside it.
        collector.prefabs.push(candidate(0, 0, 0, 10, 8));
        collector.prefabs.push(candidate(4, 0, 4, 5, 8));
        collector.min_priority = 5;

        collector.invalidate_prefabs();

        assert!(!collector.invalidated(0), "the high priority one survives");
        assert!(
            collector.invalidated(1),
            "the low priority one is invalidated"
        );
    }

    #[test]
    fn a_candidate_of_the_same_priority_is_not_invalidated() {
        let mut collector = PlacementCollector::new();
        collector.prefabs.push(candidate(0, 0, 0, 7, 8));
        collector.prefabs.push(candidate(4, 0, 4, 7, 8));
        collector.min_priority = 7;

        collector.invalidate_prefabs();

        assert!(!collector.invalidated(0));
        assert!(!collector.invalidated(1));
    }

    #[test]
    fn a_candidate_of_the_minimum_priority_never_invalidates_another_one() {
        let mut collector = PlacementCollector::new();
        // The second candidate has the *lowest* priority, so it cannot
        // invalidate the first one.
        collector.prefabs.push(candidate(0, 0, 0, 10, 8));
        collector.prefabs.push(candidate(4, 0, 4, 1, 8));
        collector.min_priority = 1;

        collector.invalidate_prefabs();

        assert!(
            !collector.invalidated(0),
            "the minimum priority candidate does not invalidate"
        );
        assert!(collector.invalidated(1));
    }

    #[test]
    fn a_candidate_that_does_not_overlap_is_not_invalidated() {
        let mut collector = PlacementCollector::new();
        collector.prefabs.push(candidate(0, 0, 0, 10, 8));
        collector.prefabs.push(candidate(100, 0, 100, 5, 8));
        collector.min_priority = 5;

        collector.invalidate_prefabs();

        assert!(!collector.invalidated(0));
        assert!(!collector.invalidated(1));
    }
}
