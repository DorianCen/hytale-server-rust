// Ported from: `.hytale-source/.../worldgen/cache/ChunkGeneratorCache.java`
//! Java `cache.ChunkGeneratorCache` — the per-chunk core-data cache:
//! zone/biome results, interpolated biome counts, heights and height
//! noises, shared by all later chunk stages.
//!
//! # In-house cache (deviation)
//!
//! Java builds this on `ConcurrentSizedTimeoutCache` (sized + timed,
//! compute-once). The Rust port keeps the semantic contract but uses an
//! in-house `RwLock<HashMap<…>>` (see `cache/mod.rs`): the values are
//! `Rc<CoreDataCacheEntry>` and the entry type is `!Send` (it contains
//! `Rc<dyn Biome>`), so `moka` cannot hold them.
//!
//! * **Sized**: inserts beyond `max_size` evict expired entries first, then
//!   arbitrary entries (the in-house cache tracks no access order).
//! * **Timed**: time-to-idle — every `get` refreshes the deadline (the
//!   house `moka` contract); expired entries are replaced on the next
//!   insert and swept by [`ChunkGeneratorCache::cleanup`].
//! * **Compute-once**: a key present and unexpired is never recomputed;
//!   a concurrent first insert may compute twice (the value is pure, so
//!   the result is identical) — the second insert loses the race.
//!
//! `get` returns `Rc<CoreDataCacheEntry>` sharing the **live** entry (Java
//! returns the live object); mutations through `put_height`/`ensure_*` are
//! visible to every holder, mirroring Java's `volatile` fields.
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, RwLock};
use std::time::Instant;

use hytale_math::chunk_util;

use crate::cache::coordinate_cache::CoordinateKey;
use crate::cache::core_data_cache_entry::CoreDataCacheEntry;
use crate::cache::interpolated_biome_count_list::InterpolatedBiomeCountList;
use crate::chunk::zone_biome_result::ZoneBiomeResult;
use crate::chunk::POOL_SIZE;

/// Java `ChunkGeneratorCache.CONCURRENCY_LEVEL`
/// = `Math.max(1, ChunkGenerator.POOL_SIZE / 2)`.
///
/// Kept for signature parity with the Java class; the in-house cache is
/// single-locked, so the level is not used to size anything.
pub static CONCURRENCY_LEVEL: LazyLock<i32> = LazyLock::new(|| 1.max(*POOL_SIZE / 2));

/// Java `ChunkGeneratorCache.ZoneBiomeResultFunction`
/// — `ZoneBiomeResult compute(int seed, int x, int z)`.
///
/// Deviation: Java returns the value by hand-off; the Rust closure
/// returns `Rc<ZoneBiomeResult>` (the fresh-compute handle, unwrapped into
/// the entry at insertion).
pub type ZoneBiomeResultFunction = Arc<dyn Fn(i32, i32, i32) -> Rc<ZoneBiomeResult> + Send + Sync>;

/// Java `ChunkGeneratorCache.BiomeCountFunction`
/// — `void compute(int seed, int x, int z, InterpolatedBiomeCountList)`.
pub type BiomeCountFunction = Arc<dyn Fn(i32, i32, i32, &InterpolatedBiomeCountList) + Send + Sync>;

/// Java `ChunkGeneratorCache.HeightFunction`
/// — `int compute(int seed, int x, int z)`.
pub type HeightFunction = Arc<dyn Fn(i32, i32, i32) -> i32 + Send + Sync>;

/// Java `ChunkGeneratorCache.HeightNoiseFunction`
/// — `double compute(InterpolatedBiomeCountList)`.
pub type HeightNoiseFunction = Arc<dyn Fn(&InterpolatedBiomeCountList) -> f64 + Send + Sync>;

/// One live cache record: the shared entry plus its idle deadline
/// (millis since the cache was created).
struct CacheRecord {
    value: Rc<CoreDataCacheEntry>,
    expires_at: AtomicU64,
}

/// Java `cache.ChunkGeneratorCache`.
pub struct ChunkGeneratorCache {
    /// The in-house sized+timed map (see the module documentation).
    inner: RwLock<HashMap<CoordinateKey, CacheRecord>>,
    /// Java `ZoneBiomeResultFunction zoneBiomeResultFunction`.
    zone_biome_result_function: ZoneBiomeResultFunction,
    /// Java `BiomeCountFunction biomeCountFunction`.
    biome_count_function: BiomeCountFunction,
    /// Java `HeightFunction heightFunction`.
    height_function: HeightFunction,
    /// Java `HeightNoiseFunction heightNoiseFunction`.
    height_noise_function: HeightNoiseFunction,
    /// Java `int maxSize`.
    max_size: u64,
    /// Java `long expireAfterSeconds` (as millis).
    ttl_millis: u64,
    /// The idle clock origin.
    created: Instant,
}

impl ChunkGeneratorCache {
    /// Java `ChunkGeneratorCache(ZoneBiomeResultFunction,
    /// BiomeCountFunction, HeightFunction, HeightNoiseFunction, int maxSize,
    /// long expireAfterSeconds)`.
    pub fn new(
        zone_biome_result_function: ZoneBiomeResultFunction,
        biome_count_function: BiomeCountFunction,
        height_function: HeightFunction,
        height_noise_function: HeightNoiseFunction,
        max_size: u64,
        expire_after_seconds: u64,
    ) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            zone_biome_result_function,
            biome_count_function,
            height_function,
            height_noise_function,
            max_size,
            ttl_millis: expire_after_seconds * 1000,
            created: Instant::now(),
        }
    }

    /// Java `CoreDataCacheEntry get(int seed, int x, int z)` — returns the
    /// live shared entry (the Java callers mutate the result).
    pub fn get(&self, seed: i32, x: i32, z: i32) -> Rc<CoreDataCacheEntry> {
        let key = CoordinateKey::new(seed, x, z);
        if let Some(value) = self.lookup_live(&key) {
            return value;
        }
        let value = self.compute_value(&key);
        self.insert(key, value.clone())
    }

    /// Java `ZoneBiomeResult getZoneBiomeResult(int seed, int x, int z)`.
    ///
    /// Deviation: returns a snapshot clone — the shared entry cannot hand
    /// out a `&mut` scratch record.
    pub fn get_zone_biome_result(&self, seed: i32, x: i32, z: i32) -> ZoneBiomeResult {
        self.get(seed, x, z)
            .zone_biome_result
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Java `@Nullable InterpolatedBiomeCountList getBiomeCountResult(int
    /// seed, int x, int z)` — ensures the list before returning it (the
    /// returned `Rc` shares the live list).
    pub fn get_biome_count_result(
        &self,
        seed: i32,
        x: i32,
        z: i32,
    ) -> Option<Rc<InterpolatedBiomeCountList>> {
        let entry = self.get(seed, x, z);
        self.ensure_biome_count_list(seed, x, z, &entry);
        let list = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        list
    }

    /// Java `void putHeight(int seed, int x, int z, int height)` — mutates
    /// the shared entry in place.
    pub fn put_height(&self, seed: i32, x: i32, z: i32, height: i32) {
        self.get(seed, x, z).height.store(height, Ordering::SeqCst);
    }

    /// Java `int getHeight(int seed, int x, int z)` — computes and caches
    /// the height on first use.
    pub fn get_height(&self, seed: i32, x: i32, z: i32) -> i32 {
        let entry = self.get(seed, x, z);
        self.ensure_height(seed, x, z, &entry);
        entry.height.load(Ordering::SeqCst)
    }

    /// Java `void ensureBiomeCountList(int seed, int x, int z,
    /// CoreDataCacheEntry entry)` — computes the list exactly once per
    /// entry (the `ensure*` family is also the cache-reuse pattern: the
    /// entry arrives half-filled from a recycled instance).
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
                .unwrap_or_else(|p| p.into_inner());
            if list.is_some() {
                return;
            }
        }
        let list = Rc::new(InterpolatedBiomeCountList::new());
        (self.biome_count_function)(seed, x, z, &list);
        *entry
            .biome_count_list
            .write()
            .unwrap_or_else(|p| p.into_inner()) = Some(list);
    }

    /// Java `void ensureHeight(int seed, int x, int z,
    /// CoreDataCacheEntry entry)` — `NO_HEIGHT` marks "not computed yet".
    pub fn ensure_height(&self, seed: i32, x: i32, z: i32, entry: &Rc<CoreDataCacheEntry>) {
        if entry.height.load(Ordering::SeqCst) != CoreDataCacheEntry::NO_HEIGHT {
            return;
        }
        let height = (self.height_function)(seed, x, z);
        entry.height.store(height, Ordering::SeqCst);
    }

    /// Java `void ensureHeightNoise(int seed, int x, int z,
    /// CoreDataCacheEntry entry)` — `NO_HEIGHT_NOISE` marks "not computed
    /// yet"; depends on the biome count list.
    pub fn ensure_height_noise(&self, seed: i32, x: i32, z: i32, entry: &Rc<CoreDataCacheEntry>) {
        {
            let noise = entry.height_noise.read().unwrap_or_else(|p| p.into_inner());
            if *noise != CoreDataCacheEntry::NO_HEIGHT_NOISE {
                return;
            }
        }
        self.ensure_biome_count_list(seed, x, z, entry);
        let list = entry
            .biome_count_list
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let list = list.expect("ensure_biome_count_list populated the list");
        let noise = (self.height_noise_function)(&list);
        *entry
            .height_noise
            .write()
            .unwrap_or_else(|p| p.into_inner()) = noise;
    }

    /// Java `CoreDataCacheEntry computeValue(CoordinateKey key)` — the
    /// cache loader.
    fn compute_value(&self, key: &CoordinateKey) -> Rc<CoreDataCacheEntry> {
        let seed = key.seed();
        let x = chunk_util::x_of_chunk_index(key.coord());
        let z = chunk_util::z_of_chunk_index(key.coord());
        let zone_biome_result = (self.zone_biome_result_function)(seed, x, z);
        // The closure hands over a freshly computed Rc; unwrap it (fall
        // back to a clone if a caller ever shares the handle).
        let zone_biome_result = match Rc::try_unwrap(zone_biome_result) {
            Ok(value) => value,
            Err(rc) => rc.as_ref().clone(),
        };
        Rc::new(CoreDataCacheEntry::new_with(zone_biome_result))
    }

    /// Millis on the cache's idle clock.
    fn now_millis(&self) -> u64 {
        self.created.elapsed().as_millis() as u64
    }

    /// Returns the live entry, refreshing its idle deadline (time-to-idle).
    fn lookup_live(&self, key: &CoordinateKey) -> Option<Rc<CoreDataCacheEntry>> {
        let inner = self.inner.read().unwrap_or_else(|p| p.into_inner());
        let now = self.now_millis();
        inner.get(key).and_then(|record| {
            if now < record.expires_at.load(Ordering::SeqCst) {
                record
                    .expires_at
                    .store(now + self.ttl_millis, Ordering::SeqCst);
                Some(Rc::clone(&record.value))
            } else {
                None
            }
        })
    }

    /// Insert-or-reuse under the write lock (compute-once for live keys).
    fn insert(&self, key: CoordinateKey, value: Rc<CoreDataCacheEntry>) -> Rc<CoreDataCacheEntry> {
        let mut inner = self.inner.write().unwrap_or_else(|p| p.into_inner());
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
                expires_at: AtomicU64::new(now + self.ttl_millis),
            },
        );
        value
    }

    /// Sized-cache eviction: expired entries first, then arbitrary ones.
    fn evict_if_full(&self, inner: &mut HashMap<CoordinateKey, CacheRecord>, now: u64) {
        if inner.len() < self.max_size as usize {
            return;
        }
        inner.retain(|_, record| now < record.expires_at.load(Ordering::SeqCst));
        while inner.len() >= self.max_size as usize {
            if let Some(first) = inner.keys().next().cloned() {
                inner.remove(&first);
            } else {
                break;
            }
        }
    }

    /// The number of entries currently held (Java has no equivalent on this
    /// class; exposed for tests/monitoring).
    pub fn size(&self) -> usize {
        self.inner.read().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// Drops every expired entry (the periodic sweep the house port's
    /// `moka`-based caches run from their cleanup task).
    pub fn cleanup(&self) {
        let mut inner = self.inner.write().unwrap_or_else(|p| p.into_inner());
        let now = self.now_millis();
        inner.retain(|_, record| now < record.expires_at.load(Ordering::SeqCst));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::biome::Biome;
    use crate::biome::biome_interpolation::BiomeInterpolation;
    use std::sync::atomic::AtomicUsize;

    fn stub_zone_result(biome_id: i32) -> Rc<ZoneBiomeResult> {
        Rc::new(ZoneBiomeResult::new_with(
            None,
            Some(Rc::new(StubBiome(biome_id))),
            1.0,
            2.0,
        ))
    }

    struct StubBiome(i32);

    impl Biome for StubBiome {
        fn get_id(&self) -> i32 {
            self.0
        }

        fn get_name(&self) -> &str {
            "stub"
        }

        fn get_interpolation(&self) -> Rc<BiomeInterpolation> {
            Rc::new(BiomeInterpolation::create(
                5,
                std::collections::HashMap::new(),
            ))
        }

        fn get_heightmap_interpreter(
            &self,
        ) -> Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter> {
            unimplemented!("not exercised by these tests")
        }

        fn get_cover_container(&self) -> Rc<crate::container::cover_container::CoverContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_layer_container(&self) -> Rc<crate::container::layer_container::LayerContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_prefab_container(
            &self,
        ) -> Option<Rc<crate::container::prefab_container::PrefabContainer>> {
            unimplemented!("not exercised by these tests")
        }

        fn get_tint_container(&self) -> Rc<crate::container::tint_container::TintContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_environment_container(
            &self,
        ) -> Rc<crate::container::environment_container::EnvironmentContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_water_container(&self) -> Rc<crate::container::water_container::WaterContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_fade_container(&self) -> Rc<crate::container::fade_container::FadeContainer> {
            unimplemented!("not exercised by these tests")
        }

        fn get_heightmap_noise(&self) -> Rc<dyn procedural::noise::support::NoiseProperty> {
            unimplemented!("not exercised by these tests")
        }

        fn get_map_color(&self) -> i32 {
            unimplemented!("not exercised by these tests")
        }
    }

    fn counting_cache(
        zone_calls: &Arc<AtomicUsize>,
        height_calls: &Arc<AtomicUsize>,
        noise_calls: &Arc<AtomicUsize>,
    ) -> ChunkGeneratorCache {
        let zone_counter = Arc::clone(zone_calls);
        let height_counter = Arc::clone(height_calls);
        let noise_counter = Arc::clone(noise_calls);
        ChunkGeneratorCache::new(
            Arc::new(move |seed: i32, _x: i32, _z: i32| {
                zone_counter.fetch_add(1, Ordering::SeqCst);
                stub_zone_result(seed)
            }),
            Arc::new(
                |_seed: i32, _x: i32, _z: i32, list: &InterpolatedBiomeCountList| {
                    list.set_center(&stub_zone_result(1));
                    list.add(&stub_zone_result(2), 0);
                },
            ),
            Arc::new(move |seed: i32, _x: i32, _z: i32| {
                height_counter.fetch_add(1, Ordering::SeqCst);
                seed + 1000
            }),
            Arc::new(move |_list: &InterpolatedBiomeCountList| {
                noise_counter.fetch_add(1, Ordering::SeqCst);
                42.0
            }),
            64,
            60,
        )
    }

    #[test]
    fn get_computes_once_and_shares_the_live_entry() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cache = counting_cache(
            &calls,
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        );

        let first = cache.get(1, 2, 3);
        let second = cache.get(1, 2, 3);
        assert!(Rc::ptr_eq(&first, &second), "the live entry is shared");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "compute-once per key");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let _ = cache.get(1, 2, 4);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "a different key recomputes"
        );
    }

    #[test]
    fn put_height_mutates_the_shared_entry() {
        let cache = counting_cache(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        );
        cache.put_height(1, 2, 3, -7);
        assert_eq!(cache.get_height(1, 2, 3), -7);
        // The height function never ran — put_height overrode it.
        assert_eq!(cache.get(1, 2, 3).height.load(Ordering::SeqCst), -7);
    }

    #[test]
    fn get_height_computes_once_through_the_height_function() {
        let heights = Arc::new(AtomicUsize::new(0));
        let cache = counting_cache(
            &Arc::new(AtomicUsize::new(0)),
            &heights,
            &Arc::new(AtomicUsize::new(0)),
        );

        assert_eq!(cache.get_height(5, 6, 7), 1005);
        assert_eq!(cache.get_height(5, 6, 7), 1005);
        assert_eq!(heights.load(Ordering::SeqCst), 1, "the height is cached");
    }

    #[test]
    fn get_zone_biome_result_returns_the_computed_result() {
        let cache = counting_cache(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        );
        let result = cache.get_zone_biome_result(12, 0, 0);
        assert_eq!(result.get_biome().map(|b| b.get_id()), Some(12));
        assert_eq!(result.get_height_threshold_context(), 1.0);
        assert_eq!(result.get_heightmap_noise(), 2.0);
    }

    #[test]
    fn get_biome_count_result_fills_and_returns_the_list() {
        let cache = counting_cache(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        );
        let list = cache
            .get_biome_count_result(1, 2, 3)
            .expect("the list is computed");
        assert_eq!(list.get_biome_ids(), vec![1, 2]);
        // Second call returns the same list without recomputing.
        let again = cache.get_biome_count_result(1, 2, 3).unwrap();
        assert!(Rc::ptr_eq(&list, &again));
    }

    #[test]
    fn ensure_height_noise_uses_the_list_and_caches() {
        let noises = Arc::new(AtomicUsize::new(0));
        let cache = counting_cache(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
            &noises,
        );

        let entry = cache.get(1, 2, 3);
        cache.ensure_height_noise(1, 2, 3, &entry);
        assert_eq!(*entry.height_noise.read().unwrap(), 42.0);
        cache.ensure_height_noise(1, 2, 3, &entry);
        assert_eq!(noises.load(Ordering::SeqCst), 1, "the noise is cached");
    }

    #[test]
    fn the_capacity_bound_evicts_entries() {
        // max_size = 2: a third insert evicts (expired first, then arbitrary).
        let small = ChunkGeneratorCache::new(
            Arc::new(|_seed: i32, _x: i32, _z: i32| stub_zone_result(0)),
            Arc::new(|_seed: i32, _x: i32, _z: i32, _list: &InterpolatedBiomeCountList| {}),
            Arc::new(|_seed: i32, _x: i32, _z: i32| 0),
            Arc::new(|_list: &InterpolatedBiomeCountList| 0.0),
            2,
            60,
        );
        let _ = small.get(1, 0, 0);
        let _ = small.get(1, 0, 1);
        assert_eq!(small.size(), 2);
        let _ = small.get(1, 0, 2);
        assert!(small.size() <= 2, "the cache stays sized");
    }
}
