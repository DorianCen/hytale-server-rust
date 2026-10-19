// Ported from: `.hytale-source/.../worldgen/chunk/ChunkGenerator.java`
//! Java `com.hypixel.hytale.server.worldgen.chunk.ChunkGenerator` — the
//! per-world world-generation facade: it owns the per-seed zone pattern
//! generators and the per-column core-data cache, and it answers the
//! "which biome is here / how high is the ground here" questions.
//!
//! # What is *not* ported (see `NEXT_STEPS.md` / `plan.md`)
//!
//! | Java member                          | Why it is missing                                  |
//! |--------------------------------------|----------------------------------------------------|
//! | `generate(…)` / `run(…)` / `execute(…)` | the `CompletableFuture` + thread-pool API is Phase 6 (`trait WorldGen`) |
//! | `executor`, `ChunkThreadPoolExecutor`  | the Rust worker pool is Phase 6                     |
//! | `timings`, `WorldGenTimingsCollector`   | the per-stage metrics are Phase 6 — the Java timing calls are **no-ops** here |
//! | `benchmark`, `ChunkWorldgenBenchmark`   | the JMH benchmark ports are Phase 6                |
//! | `caveGeneratorCache`, `getCave`, `generateCave` | the cave package is Phase 4                |
//! | `prefabLoadingCache`, `uniquePrefabCache`, `getUniquePrefabs`, `generateUniquePrefabs` | the prefab package is Phase 5 |
//! | `getSpawnPoints`                       | needs the unique prefabs (Phase 5)                |
//! | `getGenerator` (world map)             | the world map is Phase 6                          |
//! | `validate`, `ValidationUtil`           | the validation needs the caves/prefabs (Phase 4/5) |
//! | `toMetricResults`                      | the metric registry is Phase 6                    |
//!
//! # Deviations
//!
//! * **The core-data cache is a private twin of the ported
//!   [`crate::cache::chunk_generator_cache::ChunkGeneratorCache`]**. The
//!   ported cache takes its four compute functions as
//!   `Arc<dyn Fn(..) + Send + Sync>` (a house artifact of the Java
//!   `ConcurrentSizedTimeoutCache`), but the whole worldgen object graph is
//!   `Rc`-based and therefore `!Send`/`!Sync` (see `cache/mod.rs`), so a
//!   closure capturing the generator cannot satisfy that bound. The private
//!   twin below keeps the *same* semantics (compute-once, sized, time-to-idle
//!   eviction, live shared entries) and reuses the ported
//!   [`CoordinateKey`] / [`CoreDataCacheEntry`] types.
//! * **The zone pattern provider is owned by the per-seed cache.** The
//!   ported [`ZonePatternGeneratorCache`] takes the (move-only)
//!   [`ZonePatternProvider`] by value, so the Java `getZonePatternProvider()`
//!   getter is deferred (its only consumers — `ValidationUtil` and the world
//!   map — are deferred as well).
//! * **`getCoreData` is not nullable here.** The Java method is annotated
//!   `@Nullable`, but the cache always computes a value on a miss, so the
//!   Rust port returns an `Rc<CoreDataCacheEntry>` unconditionally.
//! * **The Java "all biomes are the same" collapse of the interpolated
//!   biome-count list is applied while the list is built** (see
//!   [`ChunkGenerator::generate_interpolated_biome_count_at`]) because the
//!   ported list exposes no setter for its per-biome records. The resulting
//!   list state is identical to the Java one.
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;

use hytale_math::chunk_util;
use hytale_math::math_util;
use procedural::noise::condition::IHeightThresholdInterpreter;

use crate::cache::coordinate_cache::CoordinateKey;
use crate::cache::core_data_cache_entry::CoreDataCacheEntry;
use crate::cache::interpolated_biome_count_list::InterpolatedBiomeCountList;
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use crate::zone::zone_generator_result::ZoneGeneratorResult;
use crate::zone::zone_pattern_generator::ZonePatternGenerator;
use crate::zone::zone_pattern_generator_cache::ZonePatternGeneratorCache;
use crate::zone::zone_pattern_provider::ZonePatternProvider;

/// Java `ChunkGenerator.TINT_INTERPOLATION_RADIUS`.
///
/// TODO (Java): "Replace by biome specific values".
pub const TINT_INTERPOLATION_RADIUS: i32 = 4;

/// Java `ChunkGenerator.CHUNK_BOUNDS_PADDING` — the number of chunks the
/// supported generation region is padded by (high and low bounds).
const CHUNK_BOUNDS_PADDING: i32 = 16;

/// Java `new ChunkGeneratorCache(..., 50_000, 20)` — the maximum number of
/// core-data entries the cache holds.
const GENERATOR_CACHE_MAX_SIZE: usize = 50_000;

/// Java `new ChunkGeneratorCache(..., 50_000, 20)` — the idle timeout of the
/// core-data cache in seconds.
const GENERATOR_CACHE_EXPIRE_AFTER_SECONDS: u64 = 20;

/// One live cache record: the shared entry plus its idle deadline (millis
/// since the cache was created).
struct CacheRecord {
    /// The shared, mutable core-data entry.
    value: Rc<CoreDataCacheEntry>,
    /// The idle deadline (millis on the cache's clock).
    expires_at: AtomicU64,
}

/// The private twin of the ported `ChunkGeneratorCache` map.
type CacheMap = HashMap<CoordinateKey, CacheRecord>;

/// Java `chunk.ChunkGenerator`.
pub struct ChunkGenerator {
    /// Java `private final ZonePatternGeneratorCache
    /// zonePatternGeneratorCache` — it owns the (move-only) zone pattern
    /// provider.
    zone_pattern_generator_cache: ZonePatternGeneratorCache,
    /// Java `private final ChunkGeneratorCache generatorCache` — the
    /// per-column core-data cache (see the module docs for the deviation).
    generator_cache: RwLock<CacheMap>,
    /// Java `private final Path dataFolder`.
    data_folder: Option<PathBuf>,
    /// Java `private final int minChunkCoord`.
    min_chunk_coord: i32,
    /// Java `private final int maxChunkCoord`.
    max_chunk_coord: i32,
    /// The origin of the cache's idle clock.
    created: Instant,
}

impl ChunkGenerator {
    /// Java `ChunkGenerator(ZonePatternProvider zonePatternProvider, Path
    /// dataFolder)`.
    pub fn new(
        zone_pattern_provider: ZonePatternProvider,
        data_folder: Option<PathBuf>,
    ) -> Rc<Self> {
        // Determine the number of chunks that large-scale features such as
        // caves may extend out from a given generating chunk and restrict the
        // supported generation area by that plus some padding.
        // This prevents integer overflow issues at extreme coordinates.
        let extents =
            get_largest_feature_chunk_extents(&zone_pattern_provider, CHUNK_BOUNDS_PADDING);
        let min_chunk_coord = chunk_util::MIN_CHUNK_COORD + extents;
        let max_chunk_coord = chunk_util::MAX_CHUNK_COORD - extents;

        Rc::new(Self {
            zone_pattern_generator_cache: ZonePatternGeneratorCache::new(zone_pattern_provider),
            generator_cache: RwLock::new(HashMap::new()),
            data_folder,
            min_chunk_coord,
            max_chunk_coord,
            created: Instant::now(),
        })
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                               GETTERS / SETTERS                                               ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `Path getDataFolder()`.
    pub fn get_data_folder(&self) -> Option<&PathBuf> {
        self.data_folder.as_ref()
    }

    /// Java `int minChunkCoord` (the lower bound of the generatable area).
    pub fn get_min_chunk_coord(&self) -> i32 {
        self.min_chunk_coord
    }

    /// Java `int maxChunkCoord` (the upper bound of the generatable area).
    pub fn get_max_chunk_coord(&self) -> i32 {
        self.max_chunk_coord
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                               CACHE LOOKUPS                                                   ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `@Nullable CoreDataCacheEntry getCoreData(int seed, int x, int
    /// z)`.
    ///
    /// Deviation: the Java method is `@Nullable`, but the cache always
    /// computes the value on a miss — the Rust port returns the shared entry
    /// unconditionally.
    pub fn get_core_data(&self, seed: i32, x: i32, z: i32) -> Rc<CoreDataCacheEntry> {
        let key = CoordinateKey::new(seed, x, z);
        if let Some(value) = self.lookup_live(&key) {
            return value;
        }
        // The compute function hands over a freshly computed record; unwrap
        // it (fall back to a clone if a caller ever shares the handle).
        let zone_biome_result = self.generate_zone_biome_result_at(seed, x, z);
        let zone_biome_result = match Rc::try_unwrap(zone_biome_result) {
            Ok(value) => value,
            Err(shared) => (*shared).clone(),
        };
        let value = Rc::new(CoreDataCacheEntry::new_with(zone_biome_result));
        self.insert(key, value.clone())
    }

    /// Java `@Nonnull ZonePatternGenerator getZonePatternGenerator(int seed)`.
    pub fn get_zone_pattern_generator(&self, seed: i32) -> Rc<ZonePatternGenerator> {
        self.zone_pattern_generator_cache.get(seed)
    }

    /// Java `ZoneBiomeResult getZoneBiomeResultAt(int seed, int x, int z)`.
    ///
    /// Deviation: the Java method hands out the *live* result object of the
    /// cache entry; the Rust port returns a snapshot clone (the shared entry
    /// cannot hand out a `&mut` record). The values are identical.
    pub fn get_zone_biome_result_at(&self, seed: i32, x: i32, z: i32) -> ZoneBiomeResult {
        self.get_core_data(seed, x, z)
            .zone_biome_result
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Java `int getHeight(int seed, int x, int z)`.
    pub fn get_height(&self, seed: i32, x: i32, z: i32) -> i32 {
        let entry = self.get_core_data(seed, x, z);
        self.ensure_height(seed, x, z, &entry);
        entry.height.load(Ordering::SeqCst)
    }

    /// Java `void putHeight(int seed, int x, int z, int y)`.
    pub fn put_height(&self, seed: i32, x: i32, z: i32, y: i32) {
        self.get_core_data(seed, x, z)
            .height
            .store(y, Ordering::SeqCst);
    }

    /// Java `@Nullable InterpolatedBiomeCountList getInterpolatedBiomeCountAt(
    /// int seed, int x, int z)`.
    ///
    /// The Java method ensures the list before returning it; the Rust port
    /// does the same and hands out the shared list.
    pub fn get_interpolated_biome_count_at(
        &self,
        seed: i32,
        x: i32,
        z: i32,
    ) -> Option<Rc<InterpolatedBiomeCountList>> {
        let entry = self.get_core_data(seed, x, z);
        self.ensure_biome_count_list(seed, x, z, &entry);
        let list = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        list
    }

    /// Java `void ensureBiomeCountList(int seed, int x, int z,
    /// CoreDataCacheEntry entry)`.
    pub fn ensure_biome_count_list(
        &self,
        seed: i32,
        x: i32,
        z: i32,
        entry: &Rc<CoreDataCacheEntry>,
    ) {
        {
            let list = entry
                .biome_count_list
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if list.is_some() {
                return;
            }
        }
        let list = Rc::new(InterpolatedBiomeCountList::new());
        self.generate_interpolated_biome_count_at(seed, x, z, &list);
        *entry
            .biome_count_list
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(list);
    }

    /// Java `void ensureHeight(int seed, int x, int z, CoreDataCacheEntry
    /// entry)`.
    pub fn ensure_height(&self, seed: i32, x: i32, z: i32, entry: &Rc<CoreDataCacheEntry>) {
        if entry.height.load(Ordering::SeqCst) != CoreDataCacheEntry::NO_HEIGHT {
            return;
        }
        let height = self.generate_height(seed, x, z);
        entry.height.store(height, Ordering::SeqCst);
    }

    /// Java `void ensureHeightNoise(int seed, int x, int z,
    /// CoreDataCacheEntry entry)`.
    pub fn ensure_height_noise(&self, seed: i32, x: i32, z: i32, entry: &Rc<CoreDataCacheEntry>) {
        {
            let noise = entry
                .height_noise
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if *noise != CoreDataCacheEntry::NO_HEIGHT_NOISE {
                return;
            }
        }
        self.ensure_biome_count_list(seed, x, z, entry);
        let list = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("ensure_biome_count_list populated the list");
        let noise = self.generate_interpolated_height_noise(&list);
        *entry
            .height_noise
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = noise;
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                               GENERATION LOGIC                                                 ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `@Nonnull ZoneBiomeResult generateZoneBiomeResultAt(int seed, int
    /// x, int z)`.
    ///
    /// Deviation: the Java method returns the record by value; the Rust port
    /// hands out a shared handle (`Rc<ZoneBiomeResult>`) so that the value
    /// can be moved into the cache entry without a second copy.
    pub fn generate_zone_biome_result_at(&self, seed: i32, x: i32, z: i32) -> Rc<ZoneBiomeResult> {
        let mut result = ZoneBiomeResult::new();
        self.generate_zone_biome_result_at_into(seed, x, z, &mut result);
        Rc::new(result)
    }

    /// Java `@Nonnull ZoneBiomeResult generateZoneBiomeResultAt(int seed, int
    /// x, int z, @Nonnull ZoneBiomeResult result)`.
    ///
    /// The Java overload fills the caller-provided scratch record; the Rust
    /// port does the same (the scratch record is reused, so no allocation
    /// happens per column).
    pub fn generate_zone_biome_result_at_into<'a>(
        &self,
        seed: i32,
        x: i32,
        z: i32,
        result: &'a mut ZoneBiomeResult,
    ) -> &'a mut ZoneBiomeResult {
        let zone_pattern_generator = self.get_zone_pattern_generator(seed);

        // Java reuses the `ZoneGeneratorResult` stored in the scratch record
        // (`result.getZoneResult()`); the Rust port takes it out of the
        // record, mutates it in place and puts it back. The record itself is
        // tiny (an `Option<Rc<Zone>>` plus a `f64`), so the clone is cheap.
        let mut zone_result = match result.zone_result.take() {
            Some(stored) => match Rc::try_unwrap(stored) {
                Ok(value) => value,
                Err(shared) => (*shared).clone(),
            },
            None => ZoneGeneratorResult::new(),
        };

        let zone_result =
            zone_pattern_generator.generate_into(seed, x as f64, z as f64, &mut zone_result);

        // Java: `zoneResult.getZone().biomePatternGenerator()` — the zone is
        // never null here (the zone pattern generator always resolves one).
        let zone = zone_result
            .get_zone()
            .expect("generateZoneBiomeResultAt: the zone result has no zone (Java: NPE)")
            .clone();
        let biome = zone
            .biome_pattern_generator
            .generate_biome_at(zone_result, seed, x, z);

        let height_threshold_context = biome
            .get_heightmap_interpreter()
            .get_context(seed, x as f64, z as f64);

        let mut heightmap_noise = biome.get_heightmap_noise().get(seed, x as f64, z as f64);
        let fade_container = biome.get_fade_container();
        if fade_container.should_fade() {
            let factor = fade_container.get_terrain_factor(zone_result);
            heightmap_noise =
                heightmap_noise * (1.0 - factor) + fade_container.get_fade_heightmap() * factor;
        }

        result.set_zone_result(Some(Rc::new(zone_result.clone())));
        result.set_biome(Some(biome));
        result.set_height_threshold_context(height_threshold_context);
        result.set_heightmap_noise(heightmap_noise);

        result
    }

    /// Java `void generateInterpolatedBiomeCountAt(int seed, int x, int z,
    /// @Nonnull InterpolatedBiomeCountList biomeCountList)`.
    ///
    /// The Java method adds every neighbour biome to the list and then
    /// collapses the list when the whole region holds a single biome
    /// (`getBiomeIds().size() == 1` → the centre record keeps the centre's
    /// height noise and a count of 1).
    ///
    /// Deviation: the ported [`InterpolatedBiomeCountList`] exposes no
    /// setter for the per-biome records, so the collapse is applied *while*
    /// the list is built: when the region holds a single biome, the
    /// neighbours are not added at all. The resulting list state is
    /// identical to the Java one (the id list holds exactly the centre's
    /// id and the centre record holds `heightNoise = center.heightmapNoise`
    /// and `count = 1`).
    pub fn generate_interpolated_biome_count_at(
        &self,
        seed: i32,
        x: i32,
        z: i32,
        biome_count_list: &InterpolatedBiomeCountList,
    ) {
        let center = self.get_zone_biome_result_at(seed, x, z);
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

                let biome_result = self.get_zone_biome_result_at(seed, x + ix, z + iz);
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

    /// Java `int generateLowestThresholdDependent(@Nonnull
    /// InterpolatedBiomeCountList biomeCounts)`.
    pub fn generate_lowest_threshold_dependent(
        &self,
        biome_counts: &InterpolatedBiomeCountList,
    ) -> i32 {
        let mut lowest_non_one = HEIGHT;
        for id in biome_counts.get_biome_ids() {
            let r = biome_counts
                .get_by_id(id)
                .expect("generateLowestThresholdDependent: unknown biome id (Java: NPE)");
            let v = r.biome.get_heightmap_interpreter().get_lowest_non_one();
            if v < lowest_non_one {
                lowest_non_one = v;
            }
        }
        lowest_non_one
    }

    /// Java `int generateHighestThresholdDependent(@Nonnull
    /// InterpolatedBiomeCountList biomeCounts)`.
    pub fn generate_highest_threshold_dependent(
        &self,
        biome_counts: &InterpolatedBiomeCountList,
    ) -> i32 {
        let mut highest_non_zero = -1;
        for id in biome_counts.get_biome_ids() {
            let r = biome_counts
                .get_by_id(id)
                .expect("generateHighestThresholdDependent: unknown biome id (Java: NPE)");
            let v = r.biome.get_heightmap_interpreter().get_highest_non_zero();
            if v > highest_non_zero {
                highest_non_zero = v;
            }
        }
        highest_non_zero
    }

    /// Java `double generateInterpolatedHeightNoise(@Nonnull
    /// InterpolatedBiomeCountList biomeCounts)`.
    pub fn generate_interpolated_height_noise(
        &self,
        biome_counts: &InterpolatedBiomeCountList,
    ) -> f64 {
        let mut n = 0.0f64;
        let mut counter = 0i32;
        for id in biome_counts.get_biome_ids() {
            let r = biome_counts
                .get_by_id(id)
                .expect("generateInterpolatedHeightNoise: unknown biome id (Java: NPE)");
            n += r.height_noise * r.count as f64;
            counter += r.count;
        }
        n /= counter as f64;
        n
    }

    /// Java `int generateHeight(int seed, int x, int z)`.
    pub fn generate_height(&self, seed: i32, x: i32, z: i32) -> i32 {
        let entry = self.get_core_data(seed, x, z);
        self.ensure_height_noise(seed, x, z, &entry);
        let biome_counts = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("ensure_height_noise populated the biome count list");
        let height_noise = *entry
            .height_noise
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut y = self.generate_highest_threshold_dependent(&biome_counts);
        while y > 0 {
            let threshold = generate_interpolated_threshold(seed, x, z, y, &biome_counts);
            // Java: `threshold > heightNoise` widens the float to the double.
            if (threshold as f64) > height_noise || threshold == 1.0 {
                return y;
            }
            y -= 1;
        }
        0
    }

    /// Java `int generateHeightBetween(int seed, int x, int z, @Nonnull
    /// IHeightThresholdInterpreter interpreter)`.
    pub fn generate_height_between(
        &self,
        seed: i32,
        x: i32,
        z: i32,
        interpreter: &Rc<dyn IHeightThresholdInterpreter>,
    ) -> i32 {
        let entry = self.get_core_data(seed, x, z);
        self.ensure_height_noise(seed, x, z, &entry);
        let biome_counts = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .expect("ensure_height_noise populated the biome count list");
        let height_noise = *entry
            .height_noise
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut y = self.generate_highest_threshold_dependent(&biome_counts);
        while y > 0 {
            if !interpreter.is_spawnable(y) {
                y -= 1;
                continue;
            }
            let threshold = generate_interpolated_threshold(seed, x, z, y, &biome_counts);
            if (threshold as f64) > height_noise || threshold == 1.0 {
                return y;
            }
            y -= 1;
        }
        0
    }

    /* ============================================================================================================ \\
    ||                                                                                                               ||
    ||                                                     MISC                                                      ||
    ||                                                                                                               ||
    \\ ============================================================================================================ */

    /// Java `boolean isChunkOutsideGeneratableArea(int x, int z)`.
    pub fn is_chunk_outside_generatable_area(&self, x: i32, z: i32) -> bool {
        x < self.min_chunk_coord
            || x > self.max_chunk_coord
            || z < self.min_chunk_coord
            || z > self.max_chunk_coord
    }

    /// The number of entries the core-data cache currently holds (the Java
    /// class has no equivalent; exposed for tests/monitoring).
    pub fn get_cache_size(&self) -> usize {
        self.generator_cache
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    /// Drops every expired cache entry (the periodic sweep the house port's
    /// `moka`-based caches run from their cleanup task).
    pub fn cleanup_cache(&self) {
        let now = self.now_millis();
        self.generator_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, record| now < record.expires_at.load(Ordering::SeqCst));
    }

    /// Millis on the cache's idle clock.
    fn now_millis(&self) -> u64 {
        self.created.elapsed().as_millis() as u64
    }

    /// The idle timeout of the core-data cache in millis.
    fn ttl_millis(&self) -> u64 {
        GENERATOR_CACHE_EXPIRE_AFTER_SECONDS * 1000
    }

    /// Returns the live entry, refreshing its idle deadline (time-to-idle).
    fn lookup_live(&self, key: &CoordinateKey) -> Option<Rc<CoreDataCacheEntry>> {
        let inner = self
            .generator_cache
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = self.now_millis();
        inner.get(key).and_then(|record| {
            if now < record.expires_at.load(Ordering::SeqCst) {
                record
                    .expires_at
                    .store(now + self.ttl_millis(), Ordering::SeqCst);
                Some(Rc::clone(&record.value))
            } else {
                None
            }
        })
    }

    /// Insert-or-reuse under the write lock (compute-once for live keys).
    fn insert(&self, key: CoordinateKey, value: Rc<CoreDataCacheEntry>) -> Rc<CoreDataCacheEntry> {
        let mut inner = self
            .generator_cache
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = self.now_millis();
        if let Some(record) = inner.get(&key) {
            if now < record.expires_at.load(Ordering::SeqCst) {
                return Rc::clone(&record.value);
            }
            inner.remove(&key);
        }
        self.evict_if_full(&mut inner, now);
        inner.insert(
            key,
            CacheRecord {
                value: Rc::clone(&value),
                expires_at: AtomicU64::new(now + self.ttl_millis()),
            },
        );
        value
    }

    /// Sized-cache eviction: expired entries first, then arbitrary ones.
    fn evict_if_full(&self, inner: &mut CacheMap, now: u64) {
        if inner.len() < GENERATOR_CACHE_MAX_SIZE {
            return;
        }
        inner.retain(|_, record| now < record.expires_at.load(Ordering::SeqCst));
        while inner.len() >= GENERATOR_CACHE_MAX_SIZE {
            if let Some(first) = inner.keys().next().cloned() {
                inner.remove(&first);
            } else {
                break;
            }
        }
    }
}

/// Java `static float generateInterpolatedThreshold(int seed, int x, int z,
/// int y, @Nonnull InterpolatedBiomeCountList biomeCounts)`.
///
/// Deviation: Java declares the method `static`; the Rust port exposes it as
/// a free function of this module (there is no static state to capture).
pub fn generate_interpolated_threshold(
    seed: i32,
    x: i32,
    z: i32,
    y: i32,
    biome_counts: &InterpolatedBiomeCountList,
) -> f32 {
    let mut threshold = 0.0f32;
    let mut counter = 0i32;
    for id in biome_counts.get_biome_ids() {
        let r = biome_counts
            .get_by_id(id)
            .expect("generateInterpolatedThreshold: unknown biome id (Java: NPE)");
        threshold += r
            .biome
            .get_heightmap_interpreter()
            .get_threshold_with_context(seed, x as f64, z as f64, y, r.height_threshold_context)
            * r.count as f32;
        counter += r.count;
    }
    threshold / counter as f32
}

/// Java `private static int getLargestFeatureChunkExtents(@Nonnull final
/// ZonePatternProvider zonePatternProvider, final int padding)`.
///
/// The Java method folds over the caves of every zone and over the prefab
/// containers of every biome of every zone:
///
/// ```java
/// for (var zone : zonePatternProvider.getZones()) {
///     if (zone.caveGenerator() != null) {
///         for (var cave : zone.caveGenerator().getCaveTypes()) {
///             max = Math.max(max, cave.getMaximumSize());
///         }
///     }
///     for (var biome : zone.biomePatternGenerator().getBiomes()) {
///         if (biome.getPrefabContainer() == null) continue;
///         for (var entry : biome.getPrefabContainer().getEntries()) {
///             max = Math.max(max, entry.getPrefabPatternGenerator().getMaxSize());
///         }
///     }
/// }
/// return ChunkUtil.chunkCoordinate(MathUtil.ceil(max)) + padding;
/// ```
///
/// Deviation: the cave generator is not ported yet (Phase 4), so the cave
/// part of the fold is missing — the extents are computed from the prefab
/// containers only. The formula itself is unchanged.
fn get_largest_feature_chunk_extents(
    zone_pattern_provider: &ZonePatternProvider,
    padding: i32,
) -> i32 {
    let mut max = 0.0f64;

    for zone in zone_pattern_provider.get_zones() {
        // Caves are likely to be the largest feature — deferred with the
        // cave package (Phase 4).

        // Prefabs may be larger in custom worlds that have limited-to-no caves
        for biome in zone.biome_pattern_generator.get_biomes() {
            let Some(prefab_container) = biome.get_prefab_container() else {
                continue;
            };

            for entry in prefab_container.get_entries() {
                max = max.max(entry.get_pattern_generator_max_size() as f64);
            }
        }
    }

    chunk_util::chunk_coordinate_i32(math_util::ceil(max)) + padding
}

/// Whether the interpolated region holds more than one biome — the Java
/// "don't interpolate if all biomes in the region are the same" test.
///
/// The neighbours are filtered exactly like
/// [`InterpolatedBiomeCountList::add`] does: a neighbour is skipped when the
/// centre biome's interpolation radius for that biome is smaller than the
/// squared distance.
pub(crate) fn region_has_multiple_biomes(
    center: &ZoneBiomeResult,
    neighbours: &[(ZoneBiomeResult, i32)],
) -> bool {
    let center_biome = center
        .get_biome()
        .expect("the centre result has no biome (Java: NPE)");
    let center_id = center_biome.get_id();

    let mut ids = vec![center_id];
    for (neighbour, distance2) in neighbours {
        let biome = neighbour
            .get_biome()
            .expect("the neighbour result has no biome (Java: NPE)");
        let id = biome.get_id();
        if id == center_id {
            continue;
        }
        if center_biome.get_interpolation().get_biome_radius2(id) < *distance2 {
            continue;
        }
        if !ids.contains(&id) {
            ids.push(id);
        }
    }

    ids.len() > 1
}

/// The legacy chunk height (`ChunkUtil.HEIGHT`) — the start value of the
/// "lowest threshold dependent" fold.
///
/// The constant is deprecated in `hytale-math` (the Java deprecation is
/// about the legacy *storage* limits, not about the world generator), so it
/// is re-exported here the same way `worldgen-model` does it.
#[allow(deprecated)]
const HEIGHT: i32 = hytale_math::chunk_util::HEIGHT;

#[cfg(test)]
pub(crate) mod test_stubs {
    //! Shared test doubles for the chunk-core tests.

    use std::rc::Rc;

    use procedural::noise::condition::IHeightThresholdInterpreter;
    use procedural::noise::support::NoiseProperty;

    use crate::biome::biome::Biome;
    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::EnvironmentContainer;
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::prefab_container::PrefabContainer;
    use crate::container::tint_container::TintContainer;
    use crate::container::water_container::WaterContainer;

    /// A constant height-threshold interpreter: every height has the same
    /// threshold and the context is a constant.
    pub(crate) struct StubInterpreter {
        /// Java `getLowestNonOne()`.
        pub lowest_non_one: i32,
        /// Java `getHighestNonZero()`.
        pub highest_non_zero: i32,
        /// Java `getThreshold(…)`.
        pub threshold: f32,
        /// Java `getContext(…)`.
        pub context: f64,
    }

    impl StubInterpreter {
        /// A stub interpreter with the given threshold.
        pub fn new(
            threshold: f32,
            context: f64,
            lowest_non_one: i32,
            highest_non_zero: i32,
        ) -> Self {
            Self {
                lowest_non_one,
                highest_non_zero,
                threshold,
                context,
            }
        }
    }

    impl IHeightThresholdInterpreter for StubInterpreter {
        fn get_lowest_non_one(&self) -> i32 {
            self.lowest_non_one
        }

        fn get_highest_non_zero(&self) -> i32 {
            self.highest_non_zero
        }

        fn get_threshold(&self, _seed: i32, _x: f64, _z: f64, _height: i32) -> f32 {
            self.threshold
        }

        fn get_threshold_with_context(
            &self,
            _seed: i32,
            _x: f64,
            _z: f64,
            _height: i32,
            _context: f64,
        ) -> f32 {
            self.threshold
        }

        fn get_context(&self, _seed: i32, _x: f64, _z: f64) -> f64 {
            self.context
        }

        fn get_length(&self) -> i32 {
            self.highest_non_zero + 1
        }
    }

    /// A `NoiseProperty` that always evaluates to a constant.
    pub(crate) struct ConstantNoise(pub f64);

    impl NoiseProperty for ConstantNoise {
        fn get(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }

        fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    /// A `Biome` whose containers are supplied by the test.
    pub(crate) struct StubBiome {
        /// Java `int id`.
        pub id: i32,
        /// Java `BiomeInterpolation interpolation`.
        pub interpolation: Rc<BiomeInterpolation>,
        /// Java `IHeightThresholdInterpreter heightmapInterpreter`.
        pub interpreter: Rc<dyn IHeightThresholdInterpreter>,
        /// Java `TintContainer tintContainer`.
        pub tint_container: Rc<TintContainer>,
        /// Java `EnvironmentContainer environmentContainer`.
        pub environment_container: Rc<EnvironmentContainer>,
        /// Java `FadeContainer fadeContainer`.
        pub fade_container: Rc<FadeContainer>,
        /// Java `NoiseProperty heightmapNoise`.
        pub heightmap_noise: Rc<dyn NoiseProperty>,
    }

    impl Biome for StubBiome {
        fn get_id(&self) -> i32 {
            self.id
        }

        fn get_name(&self) -> &str {
            "stub"
        }

        fn get_interpolation(&self) -> Rc<BiomeInterpolation> {
            Rc::clone(&self.interpolation)
        }

        fn get_heightmap_interpreter(&self) -> Rc<dyn IHeightThresholdInterpreter> {
            Rc::clone(&self.interpreter)
        }

        fn get_cover_container(&self) -> Rc<CoverContainer> {
            Rc::new(CoverContainer::new(Vec::new()))
        }

        fn get_layer_container(&self) -> Rc<LayerContainer> {
            Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new()))
        }

        fn get_prefab_container(&self) -> Option<Rc<PrefabContainer>> {
            None
        }

        fn get_tint_container(&self) -> Rc<TintContainer> {
            Rc::clone(&self.tint_container)
        }

        fn get_environment_container(&self) -> Rc<EnvironmentContainer> {
            Rc::clone(&self.environment_container)
        }

        fn get_water_container(&self) -> Rc<WaterContainer> {
            Rc::new(WaterContainer::new(Vec::new()))
        }

        fn get_fade_container(&self) -> Rc<FadeContainer> {
            Rc::clone(&self.fade_container)
        }

        fn get_heightmap_noise(&self) -> Rc<dyn NoiseProperty> {
            Rc::clone(&self.heightmap_noise)
        }

        fn get_map_color(&self) -> i32 {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone::Zone;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    use super::test_stubs::{ConstantNoise, StubBiome, StubInterpreter};

    /// Java `PointGenerator.nearest2D` with a fixed cell result.
    struct FixedPointGenerator {
        ix: i32,
        iy: i32,
        distance: f64,
    }

    impl procedural::noise::point::IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            let mut buf = ResultBuffer2d::new();
            buf.ix = self.ix;
            buf.iy = self.iy;
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

    fn fixed_point_generator() -> Rc<dyn procedural::noise::point::IPointGenerator> {
        Rc::new(FixedPointGenerator {
            ix: 0,
            iy: 0,
            distance: 0.0,
        })
    }

    fn solid_mask(
        width: usize,
        height: usize,
        color: i32,
    ) -> crate::chunk::mask_provider::MaskProvider {
        crate::chunk::mask_provider::MaskProvider::new(FuzzyZoom::new(
            empty_randomizer(),
            PixelProvider::from_pixels(width, height, vec![color; width * height]),
            1.0,
            1.0,
            0,
            0,
        ))
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

    /// A fade container that never fades.
    fn fade_container() -> Rc<FadeContainer> {
        Rc::new(FadeContainer::new(
            0.0,
            1.0,
            0.0,
            1.0,
            FadeContainer::NO_FADE_HEIGHTMAP,
        ))
    }

    /// A zone with a single tile biome.
    fn zone_with_biome(zone_id: i32, biome: Rc<TileBiome>) -> Rc<Zone> {
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        builder.put(biome, 1.0);
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());
        Rc::new(Zone {
            id: zone_id,
            name: format!("zone-{zone_id}"),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
                fixed_point_generator(),
                tile_biomes,
                Vec::new(),
            )),
        })
    }

    /// A tile biome with a constant threshold and height noise.
    fn tile_biome(
        id: i32,
        threshold: f32,
        height_noise: f64,
        tint: i32,
        environment: i32,
    ) -> Rc<TileBiome> {
        let interpolation = Rc::new(BiomeInterpolation::create(
            crate::chunk::MAX_RADIUS,
            std::collections::HashMap::new(),
        ));
        let interpreter: Rc<dyn IHeightThresholdInterpreter> =
            Rc::new(StubInterpreter::new(threshold, 0.0, 0, 63));
        Rc::new(TileBiome::new(
            id,
            format!("biome-{id}"),
            interpolation,
            interpreter,
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
            None,
            tint_container(tint),
            environment_container(environment),
            Rc::new(WaterContainer::new(Vec::new())),
            fade_container(),
            Rc::new(ConstantNoise(height_noise)),
            1.0,
            1.0,
            0,
        ))
    }

    /// A generator over a single zone whose biome has a constant threshold.
    fn generator(threshold: f32, height_noise: f64) -> Rc<ChunkGenerator> {
        let biome = tile_biome(7, threshold, height_noise, 0x112233, 5);
        let zone = zone_with_biome(3, biome);
        let mut mapping = ZoneColorMapping::new();
        mapping.add(0x00FF00, Rc::clone(&zone));
        let provider = ZonePatternProvider::new(
            fixed_point_generator(),
            vec![zone],
            Vec::new(),
            solid_mask(4, 4, 0x00FF00),
            mapping,
        );
        ChunkGenerator::new(provider, None)
    }

    #[test]
    fn the_core_data_cache_computes_once_and_shares_the_entry() {
        let generator = generator(0.75, 0.5);
        let first = generator.get_core_data(1, 2, 3);
        let second = generator.get_core_data(1, 2, 3);
        assert!(Rc::ptr_eq(&first, &second), "the live entry is shared");
        assert_eq!(generator.get_cache_size(), 1);
    }

    #[test]
    fn generate_zone_biome_result_at_resolves_the_stub_zone_and_biome() {
        let generator = generator(0.75, 0.5);
        let result = generator.generate_zone_biome_result_at(42, 10, -20);
        let zone = result
            .get_zone_result()
            .expect("the zone result is set")
            .get_zone()
            .expect("the zone is set")
            .clone();
        assert_eq!(zone.id, 3);
        assert_eq!(result.get_biome().map(|b| b.get_id()), Some(7));
        assert_eq!(result.get_heightmap_noise(), 0.5);
    }

    #[test]
    fn the_fade_blend_uses_the_java_expression_order() {
        // A fading biome: `noise * (1 - factor) + fadeHeightmap * factor`.
        // Two zones share the mask colour, so the border distance is the
        // voronoi distance of the point generator (2.5) and the terrain fade
        // factor is `1 - limit((2.5 - 0) / 10) = 0.75`.
        let fading = Rc::new(TileBiome::new(
            9,
            "biome-9".to_string(),
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
            // A fading container: the terrain fade runs from 0 to 1 over 10
            // blocks, the fade heightmap is 64.
            Rc::new(FadeContainer::new(0.0, 10.0, 0.0, 10.0, 64.0)),
            Rc::new(ConstantNoise(0.5)),
            1.0,
            1.0,
            0,
        ));
        let zone_a = zone_with_biome(3, Rc::clone(&fading));
        let zone_b = zone_with_biome(4, Rc::clone(&fading));
        let mut mapping = ZoneColorMapping::new();
        mapping.add_zones(0x00FF00, vec![Rc::clone(&zone_a), Rc::clone(&zone_b)]);
        let provider = ZonePatternProvider::new(
            Rc::new(FixedPointGenerator {
                ix: 0,
                iy: 0,
                distance: 2.5,
            }),
            vec![zone_a, zone_b],
            Vec::new(),
            solid_mask(4, 4, 0x00FF00),
            mapping,
        );
        let generator = ChunkGenerator::new(provider, None);

        let result = generator.generate_zone_biome_result_at(42, 0, 0);
        // 0.5 * (1 - 0.75) + 64.0 * 0.75 = 0.125 + 48.0
        assert_eq!(result.get_heightmap_noise(), 48.125);
    }

    #[test]
    fn the_interpolated_biome_count_list_collapses_to_the_centre() {
        let generator = generator(0.75, 0.5);
        let list = generator
            .get_interpolated_biome_count_at(42, 0, 0)
            .expect("the list is computed");
        // Every column holds the same biome → the collapsed list holds the
        // centre only, with the centre's noise and a count of 1.
        assert_eq!(list.get_biome_ids(), vec![7]);
        let r = list.get_by_id(7).expect("the biome is recorded");
        assert_eq!(r.count, 1);
        assert_eq!(r.height_noise, 0.5);
    }

    #[test]
    fn generate_height_returns_the_first_height_above_the_noise() {
        // A threshold of 0.75 is above the height noise of 0.5 → the highest
        // threshold dependent height wins.
        let generator = generator(0.75, 0.5);
        assert_eq!(generator.generate_height(42, 0, 0), 63);
    }

    #[test]
    fn generate_height_falls_back_to_zero_when_no_threshold_matches() {
        // A threshold of 0.25 is below the height noise of 0.5 → no height
        // matches and the method returns 0.
        let generator = generator(0.25, 0.5);
        assert_eq!(generator.generate_height(42, 0, 0), 0);
    }

    #[test]
    fn generate_height_between_skips_the_unspawnable_heights() {
        let generator = generator(0.75, 0.5);
        // Only the height 60 is spawnable → the loop skips 63, 62 and 61
        // (their threshold is 0.75 as well, but they are not spawnable).
        let interpreter: Rc<dyn IHeightThresholdInterpreter> =
            Rc::new(StubInterpreter::new(0.75, 0.0, 60, 60));
        assert_eq!(
            generator.generate_height_between(42, 0, 0, &interpreter),
            60
        );
    }

    #[test]
    fn the_interpolated_threshold_is_the_count_weighted_average() {
        // Two biomes: A (threshold 0.25, count 1) and B (threshold 0.75,
        // count 3) → (0.25 * 1 + 0.75 * 3) / 4 = 0.625.
        let list = InterpolatedBiomeCountList::new();
        let biome_a = Rc::new(StubBiome {
            id: 1,
            interpolation: Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            interpreter: Rc::new(StubInterpreter::new(0.25, 0.0, 0, 63)),
            tint_container: tint_container(0),
            environment_container: environment_container(0),
            fade_container: fade_container(),
            heightmap_noise: Rc::new(ConstantNoise(1.0)),
        });
        let biome_b = Rc::new(StubBiome {
            id: 2,
            interpolation: Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            interpreter: Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
            tint_container: tint_container(0),
            environment_container: environment_container(0),
            fade_container: fade_container(),
            heightmap_noise: Rc::new(ConstantNoise(0.0)),
        });

        let center = ZoneBiomeResult::new_with(None, Some(biome_a.clone()), 0.0, 1.0);
        list.set_center(&center);
        let neighbour = ZoneBiomeResult::new_with(None, Some(biome_b.clone()), 0.0, 0.0);
        list.add(&neighbour, 0);
        list.add(&neighbour, 0);
        list.add(&neighbour, 0);

        assert_eq!(list.get_biome_ids(), vec![1, 2]);
        assert_eq!(
            generate_interpolated_threshold(1, 0, 0, 10, &list),
            0.625,
            "the threshold is the count-weighted average"
        );
    }

    #[test]
    fn the_interpolated_height_noise_is_the_count_weighted_average() {
        let generator = generator(0.75, 0.5);
        let list = InterpolatedBiomeCountList::new();
        let biome_a = Rc::new(StubBiome {
            id: 1,
            interpolation: Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            interpreter: Rc::new(StubInterpreter::new(0.25, 0.0, 0, 63)),
            tint_container: tint_container(0),
            environment_container: environment_container(0),
            fade_container: fade_container(),
            heightmap_noise: Rc::new(ConstantNoise(1.0)),
        });
        let biome_b = Rc::new(StubBiome {
            id: 2,
            interpolation: Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            interpreter: Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
            tint_container: tint_container(0),
            environment_container: environment_container(0),
            fade_container: fade_container(),
            heightmap_noise: Rc::new(ConstantNoise(0.0)),
        });
        let center = ZoneBiomeResult::new_with(None, Some(biome_a), 0.0, 1.0);
        list.set_center(&center);
        let neighbour = ZoneBiomeResult::new_with(None, Some(biome_b), 0.0, 0.0);
        list.add(&neighbour, 0);
        list.add(&neighbour, 0);
        list.add(&neighbour, 0);

        // (1.0 * 1 + 0.0 * 3) / 4 = 0.25
        assert_eq!(generator.generate_interpolated_height_noise(&list), 0.25);
    }

    #[test]
    fn the_threshold_dependent_folds_use_the_biome_interpreters() {
        let generator = generator(0.75, 0.5);
        let list = InterpolatedBiomeCountList::new();
        let biome = Rc::new(StubBiome {
            id: 1,
            interpolation: Rc::new(BiomeInterpolation::create(
                crate::chunk::MAX_RADIUS,
                std::collections::HashMap::new(),
            )),
            interpreter: Rc::new(StubInterpreter::new(0.75, 0.0, 12, 42)),
            tint_container: tint_container(0),
            environment_container: environment_container(0),
            fade_container: fade_container(),
            heightmap_noise: Rc::new(ConstantNoise(1.0)),
        });
        let center = ZoneBiomeResult::new_with(None, Some(biome), 0.0, 1.0);
        list.set_center(&center);

        assert_eq!(generator.generate_lowest_threshold_dependent(&list), 12);
        assert_eq!(generator.generate_highest_threshold_dependent(&list), 42);
    }

    #[test]
    fn the_generatable_area_is_clamped_by_the_feature_extents() {
        let generator = generator(0.75, 0.5);
        // No prefab containers → the extents are the padding only (16).
        assert_eq!(
            generator.get_min_chunk_coord(),
            chunk_util::MIN_CHUNK_COORD + 16
        );
        assert_eq!(
            generator.get_max_chunk_coord(),
            chunk_util::MAX_CHUNK_COORD - 16
        );
        assert!(generator.is_chunk_outside_generatable_area(chunk_util::MIN_CHUNK_COORD, 0));
        assert!(!generator.is_chunk_outside_generatable_area(0, 0));
    }
}
