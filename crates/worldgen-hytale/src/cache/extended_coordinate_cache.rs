// Ported from: `.hytale-source/.../worldgen/cache/ExtendedCoordinateCache.java`
//! Java `cache.ExtendedCoordinateCache` — like [`CoordinateCache`], but the
//! key carries an additional generic discriminator `K` (the Java subclass
//! hook `ExtendedCoordinateObjectFunction<K, T>` becomes the `compute`
//! closure).
//!
//! Same deviations as [`coordinate_cache`]: no `ObjectPool` key reuse, no
//! removal listener, and the Java unchecked cast in `equals` needs no
//! equivalent. Java's `equals` NPEs when `k` is `null`; the Rust port
//! compares the `Option<K>` (null-safe).
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use hytale_math::chunk_util;
use hytale_math::hash_util;

use crate::util::cache::{Cache, OptionalCacheLoader, SizedTimeoutCache};

/// Java `ExtendedCoordinateCache.ExtendedCoordinateKey<K>` — chunk
/// coordinate + seed + discriminator key.
#[derive(Clone, Debug)]
pub struct ExtendedCoordinateKey<K> {
    /// Java `@Nullable K k`.
    k: Option<K>,
    /// Java `int seed`.
    seed: i32,
    /// Java `long coord` — `ChunkUtil.indexChunk(x, y)`.
    coord: i64,
    /// Java `int hash` — `31 * k.hashCode() + (int) HashUtil.hash(seed,
    /// coord)`.
    hash: i32,
}

impl<K: Hash + Clone> ExtendedCoordinateKey<K> {
    /// Java `ExtendedCoordinateKey(K k, int seed, int x, int y)` (Java
    /// stores the nullable `k` as-is).
    pub fn new(k: Option<K>, seed: i32, x: i32, y: i32) -> Self {
        let coord = chunk_util::index_chunk(x, y);
        let hash = Self::compute_hash(k.as_ref(), seed, coord);
        Self {
            k,
            seed,
            coord,
            hash,
        }
    }

    /// Java `K getK()` (the raw nullable field).
    pub fn get_k(&self) -> Option<&K> {
        self.k.as_ref()
    }

    /// Java `int seed()`.
    pub fn seed(&self) -> i32 {
        self.seed
    }

    /// Java `long coord()`.
    pub fn coord(&self) -> i64 {
        self.coord
    }

    /// The precomputed hash (kept for parity with the Java `hash` field).
    pub fn cached_hash(&self) -> i32 {
        self.hash
    }

    /// Java `ExtendedCoordinateKey<K> setLocation(K k, int seed, int x,
    /// int y)` — recomputes `coord` and `hash`.
    pub fn set_location(&mut self, k: K, seed: i32, x: i32, y: i32) -> &mut Self {
        self.k = Some(k);
        self.seed = seed;
        self.coord = chunk_util::index_chunk(x, y);
        self.hash = Self::compute_hash(self.k.as_ref(), seed, self.coord);
        self
    }

    /// Java `apply(ExtendedCoordinateKey<K> cachedKey)` — copies all
    /// fields from the cached (pooled) key.
    pub fn copy_from(&mut self, cached: &Self) -> &mut Self {
        self.k = cached.k.clone();
        self.seed = cached.seed;
        self.coord = cached.coord;
        self.hash = cached.hash;
        self
    }

    /// Java: `hash = 31 * (k != null ? k.hashCode() : 0) + (int)
    /// HashUtil.hash(seed, coord)` — the int multiply/add wrap (Java
    /// `int` arithmetic).
    fn compute_hash(k: Option<&K>, seed: i32, coord: i64) -> i32 {
        let k_hash = k.map(java_hash_code).unwrap_or(0);
        31i32
            .wrapping_mul(k_hash)
            .wrapping_add(hash_util::hash2(seed as i64, coord) as i32)
    }
}

impl<K: PartialEq> PartialEq for ExtendedCoordinateKey<K> {
    /// Java `equals`: seed + coord + `k.equals(that.k)` (NPE on a null `k`
    /// in Java; the Rust `Option` comparison is null-safe).
    fn eq(&self, other: &Self) -> bool {
        self.seed == other.seed && self.coord == other.coord && self.k == other.k
    }
}

impl<K: Eq> Eq for ExtendedCoordinateKey<K> {}

impl<K: Hash> Hash for ExtendedCoordinateKey<K> {
    /// Java `hashCode`: returns the precomputed hash.
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_i32(self.hash);
    }
}

/// Folds any `Hash` value into an `i32` (the `k.hashCode()` term of the
/// Java hash formula). Only internal consistency matters — the hash feeds
/// bucket distribution, never equality.
struct I32Hasher(i32);

impl Hasher for I32Hasher {
    fn finish(&self) -> u64 {
        self.0 as u64
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.wrapping_mul(31).wrapping_add(b as i32);
        }
    }
}

/// The Java `K.hashCode()` term: a Rust-side fold of `K`'s `Hash` impl.
fn java_hash_code<K: Hash>(k: &K) -> i32 {
    let mut hasher = I32Hasher(0);
    k.hash(&mut hasher);
    hasher.0
}

/// Java `ExtendedCoordinateCache.ExtendedCoordinateObjectFunction<K, T>` —
/// `T compute(K k, int seed, int x, int z)`.
pub type ExtendedCoordinateObjectFunction<K, T> = Arc<dyn Fn(&K, i32, i32, i32) -> T + Send + Sync>;

/// Java `abstract class ExtendedCoordinateCache<K, T>`.
pub struct ExtendedCoordinateCache<K, T> {
    /// Java `SizedTimeoutCache<ExtendedCoordinateKey<K>, T> cache`.
    cache: SizedTimeoutCache<ExtendedCoordinateKey<K>, T>,
    /// Java `ExtendedCoordinateObjectFunction<K, T> loader` —
    /// `T compute(K k, int seed, int x, int z)`.
    compute: ExtendedCoordinateObjectFunction<K, T>,
}

impl<K, T> ExtendedCoordinateCache<K, T>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    T: Clone + Send + Sync + 'static,
{
    /// Java `ExtendedCoordinateCache(ExtendedCoordinateObjectFunction<K, T>
    /// loader, ExtendedCoordinateRemovalListener<T> removalListener, int
    /// maxSize, long expireAfterSeconds)`.
    ///
    /// Deviations: the removal listener is omitted (house `destroyer`
    /// omission) and the loader becomes the `compute` parameter.
    pub fn new(
        compute: ExtendedCoordinateObjectFunction<K, T>,
        max_size: u64,
        expire_after_seconds: u64,
    ) -> Self {
        let loader_compute = Arc::clone(&compute);
        let func: OptionalCacheLoader<ExtendedCoordinateKey<K>, T> =
            Some(Arc::new(move |key: &ExtendedCoordinateKey<K>| {
                let x = chunk_util::x_of_chunk_index(key.coord);
                let z = chunk_util::z_of_chunk_index(key.coord);
                let k = key
                    .k
                    .as_ref()
                    .expect("ExtendedCoordinateKey without k (Java: NPE)");
                loader_compute(k, key.seed, x, z)
            }));
        let cache =
            SizedTimeoutCache::new(Duration::from_secs(expire_after_seconds), max_size, func);
        Self { cache, compute }
    }

    /// Java `@Nullable T get(K k, int seed, int x, int y)` — computes the
    /// value on a miss.
    pub fn get(&self, k: K, seed: i32, x: i32, y: i32) -> Option<T> {
        let key = ExtendedCoordinateKey::new(Some(k), seed, x, y);
        self.cache.get(key)
    }

    /// The compute function (Java's `loader`).
    pub fn get_compute(&self) -> &ExtendedCoordinateObjectFunction<K, T> {
        &self.compute
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn set_location_follows_the_java_hash_formula() {
        let mut key = ExtendedCoordinateKey::new(Some("biome".to_string()), 0, 0, 0);
        key.set_location("biome".to_string(), 3, 4, 5);
        let coord = chunk_util::index_chunk(4, 5);
        let expected = 31i32
            .wrapping_mul(java_hash_code(&"biome".to_string()))
            .wrapping_add(hash_util::hash2(3, coord) as i32);
        assert_eq!(key.cached_hash(), expected);
        assert_eq!(key.seed(), 3);
        assert_eq!(key.coord(), coord);
    }

    #[test]
    fn equality_uses_seed_coord_and_k() {
        let a = ExtendedCoordinateKey::new(Some(1i32), 2, 3, 4);
        let b = ExtendedCoordinateKey::new(Some(1i32), 2, 3, 4);
        assert_eq!(a, b);

        let c = ExtendedCoordinateKey::new(Some(1i32), 2, 3, 5);
        assert_ne!(a, c, "different coord");
        let d = ExtendedCoordinateKey::new(Some(1i32), 9, 3, 4);
        assert_ne!(a, d, "different seed");
        let e = ExtendedCoordinateKey::new(Some(7i32), 2, 3, 4);
        assert_ne!(a, e, "different k");
    }

    #[test]
    fn keys_work_in_a_hash_map() {
        let mut map = HashMap::new();
        map.insert(ExtendedCoordinateKey::new(Some("a"), 1, 2, 3), 10);
        assert_eq!(
            map.get(&ExtendedCoordinateKey::new(Some("a"), 1, 2, 3)),
            Some(&10)
        );
        assert_eq!(
            map.get(&ExtendedCoordinateKey::new(Some("b"), 1, 2, 3)),
            None,
            "the discriminator k is part of the key"
        );
    }

    #[test]
    fn the_cache_computes_once_per_key() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let cache: ExtendedCoordinateCache<i32, i32> = ExtendedCoordinateCache::new(
            Arc::new(move |k: &i32, seed: i32, x: i32, y: i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                k * 1000 + seed + x * 10 + y
            }),
            16,
            60,
        );

        assert_eq!(cache.get(2, 1, 3, 4), Some(2000 + 1 + 30 + 4));
        assert_eq!(cache.get(2, 1, 3, 4), Some(2035));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the value is cached");

        assert_eq!(cache.get(3, 1, 3, 4), Some(3035));
        assert_eq!(calls.load(Ordering::SeqCst), 2, "a different k recomputes");
    }
}
