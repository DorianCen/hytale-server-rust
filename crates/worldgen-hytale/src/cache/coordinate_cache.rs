// Ported from: `.hytale-source/.../worldgen/cache/CoordinateCache.java`
//! Java `cache.CoordinateCache` — the abstract generic base of the keyed
//! chunk caches: a sized+timed cache over [`CoordinateKey`], computing the
//! value on a miss.
//!
//! Deviations:
//! * Java's abstract `compute`/`localKey`/`onRemoval` triple becomes a
//!   single struct taking the `compute` closure (`Arc<dyn Fn>`); the
//!   `onRemoval` hook is omitted with the house port's `destroyer`
//!   callbacks (`util/cache.rs`).
//! * Java's `ObjectPool<CoordinateKey>` thread-local key reuse is omitted;
//!   `get` constructs a fresh key (the scratch-buffer reuse arrives with
//!   `GenContext` in the ChunkGenerator slice).
//! * Java's unchecked cast in `equals` (no `instanceof` check — a
//!   non-`CoordinateKey` argument throws `ClassCastException`) is
//!   unnecessary in Rust; the types are exact.
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use hytale_math::chunk_util;
use hytale_math::hash_util;

use crate::util::cache::{Cache, OptionalCacheLoader, SizedTimeoutCache};

/// Java `CoordinateCache.CoordinateKey` — the chunk coordinate + seed key.
#[derive(Clone, Debug)]
pub struct CoordinateKey {
    /// Java `int seed`.
    seed: i32,
    /// Java `long coord` — `ChunkUtil.indexChunk(x, y)`.
    coord: i64,
    /// Java `int hash` — precomputed in `setLocation` (Java `(int)
    /// HashUtil.hash(seed, coord)` = `hash2`).
    hash: i32,
}

impl CoordinateKey {
    /// Java `CoordinateKey(int seed, int x, int y)` — via `setLocation`.
    pub fn new(seed: i32, x: i32, y: i32) -> Self {
        let mut key = Self {
            seed: 0,
            coord: 0,
            hash: 0,
        };
        key.set_location(seed, x, y);
        key
    }

    /// Java `int seed()`.
    pub fn seed(&self) -> i32 {
        self.seed
    }

    /// Java `long coord()`.
    pub fn coord(&self) -> i64 {
        self.coord
    }

    /// The precomputed hash (Java `int hash` — kept for parity; `Hash` and
    /// `PartialEq` below mirror the Java `hashCode`/`equals` contract).
    pub fn cached_hash(&self) -> i32 {
        self.hash
    }

    /// Java `CoordinateKey setLocation(int seed, int x, int y)`.
    pub fn set_location(&mut self, seed: i32, x: i32, y: i32) -> &mut Self {
        self.seed = seed;
        self.coord = chunk_util::index_chunk(x, y);
        self.hash = hash_util::hash2(seed as i64, self.coord) as i32;
        self
    }

    /// Java `CoordinateKey apply(CoordinateKey cachedKey)` — copies the
    /// location fields from the cached (pooled) key.
    pub fn copy_from(&mut self, cached: &CoordinateKey) -> &mut Self {
        self.seed = cached.seed;
        self.coord = cached.coord;
        self.hash = cached.hash;
        self
    }
}

impl PartialEq for CoordinateKey {
    /// Java `equals`: seed + coord only (the precomputed `hash` field is
    /// not consulted — and the Java unchecked cast needs no equivalent).
    fn eq(&self, other: &Self) -> bool {
        self.seed == other.seed && self.coord == other.coord
    }
}

impl Eq for CoordinateKey {}

impl Hash for CoordinateKey {
    /// Java `hashCode`: returns the precomputed hash.
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.hash);
    }
}

/// Java `abstract class CoordinateCache<T>`.
pub struct CoordinateCache<T> {
    /// Java `SizedTimeoutCache<CoordinateKey, T> cache`.
    cache: SizedTimeoutCache<CoordinateKey, T>,
    /// Java `abstract T compute(int seed, int x, int z)`.
    compute: Arc<dyn Fn(i32, i32, i32) -> T + Send + Sync>,
}

impl<T> CoordinateCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Java `CoordinateCache(int maxSize, long expireAfterSeconds)` (the
    /// abstract `compute` becomes a constructor parameter).
    pub fn new(
        max_size: u64,
        expire_after_seconds: u64,
        compute: Arc<dyn Fn(i32, i32, i32) -> T + Send + Sync>,
    ) -> Self {
        let loader_compute = Arc::clone(&compute);
        let func: OptionalCacheLoader<CoordinateKey, T> =
            Some(Arc::new(move |key: &CoordinateKey| {
                // Java: the cache loader unwraps the key into chunk coords.
                let x = chunk_util::x_of_chunk_index(key.coord);
                let z = chunk_util::z_of_chunk_index(key.coord);
                loader_compute(key.seed, x, z)
            }));
        let cache =
            SizedTimeoutCache::new(Duration::from_secs(expire_after_seconds), max_size, func);
        Self { cache, compute }
    }

    /// Java `@Nullable T get(int seed, int x, int y)` — computes the value
    /// on a miss.
    pub fn get(&self, seed: i32, x: i32, y: i32) -> Option<T> {
        let key = CoordinateKey::new(seed, x, y);
        self.cache.get(key)
    }

    /// The compute function (Java's protected abstract method).
    pub fn get_compute(&self) -> &Arc<dyn Fn(i32, i32, i32) -> T + Send + Sync> {
        &self.compute
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn set_location_computes_coord_and_hash() {
        let mut key = CoordinateKey::new(0, 0, 0);
        key.set_location(7, 3, -2);
        assert_eq!(key.seed(), 7);
        assert_eq!(key.coord(), chunk_util::index_chunk(3, -2));
        assert_eq!(
            key.cached_hash(),
            hash_util::hash2(7, chunk_util::index_chunk(3, -2)) as i32,
            "Java: (int) HashUtil.hash(seed, coord)"
        );
    }

    #[test]
    fn equality_uses_seed_and_coord_only() {
        // The derived PartialEq compares seed + coord only — the Java
        // equals ignores the precomputed `hash` field entirely.
        let a = CoordinateKey::new(1, 2, 3);
        let b = CoordinateKey::new(1, 2, 3);
        assert_eq!(a, b);

        let c = CoordinateKey::new(1, 2, 4);
        assert_ne!(a, c, "different coord");
        let d = CoordinateKey::new(9, 2, 3);
        assert_ne!(a, d, "different seed");
    }

    #[test]
    fn equal_keys_hash_equally() {
        let mut hasher_a = std::collections::hash_map::DefaultHasher::new();
        let mut hasher_b = std::collections::hash_map::DefaultHasher::new();
        CoordinateKey::new(5, 6, 7).hash(&mut hasher_a);
        CoordinateKey::new(5, 6, 7).hash(&mut hasher_b);
        assert_eq!(hasher_a.finish(), hasher_b.finish());
    }

    #[test]
    fn keys_work_in_a_hash_map() {
        let mut map = HashMap::new();
        map.insert(CoordinateKey::new(1, 2, 3), "a");
        assert_eq!(map.get(&CoordinateKey::new(1, 2, 3)), Some(&"a"));
        assert_eq!(map.get(&CoordinateKey::new(1, 2, 4)), None);
    }

    #[test]
    fn copy_from_mirrors_the_java_apply() {
        let cached = CoordinateKey::new(4, 8, 16);
        let mut key = CoordinateKey::new(0, 0, 0);
        key.copy_from(&cached);
        assert_eq!(key.seed(), 4);
        assert_eq!(key.coord(), cached.coord());
        assert_eq!(key.cached_hash(), cached.cached_hash());
    }

    #[test]
    fn the_cache_computes_once_per_key() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let cache: CoordinateCache<i32> = CoordinateCache::new(
            16,
            60,
            Arc::new(move |seed: i32, x: i32, y: i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                seed + x * 100 + y
            }),
        );

        assert_eq!(cache.get(1, 2, 3), Some(1 + 200 + 3));
        assert_eq!(cache.get(1, 2, 3), Some(204));
        assert_eq!(cache.get(1, 2, 3), Some(204));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the value is cached");

        assert_eq!(cache.get(1, 2, 4), Some(205));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "a different key recomputes"
        );
    }
}
