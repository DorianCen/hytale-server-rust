// Ported from: `.hytale-source/.../worldgen/cache/UniquePrefabCache.java`
//! Java `com.hypixel.hytale.server.worldgen.cache.UniquePrefabCache` — the
//! cache that keeps the *unique* prefab entries of a seed.
//!
//! # Deviations
//!
//! * **The cache is a private twin** of the Java
//!   `SizedTimeoutCache<Integer, UniquePrefabEntry[]>`: the Java cache is
//!   `moka`-backed in the house port, which requires `Send + Sync` values;
//!   the unique prefab entries hold `Rc<dyn …>` payloads (the whole
//!   world-generator object graph is `!Send`), so the cache is built in-house
//!   with the *same* semantics — compute-once per seed, sized and
//!   time-to-idle eviction. It mirrors the twin the
//!   [`crate::chunk::chunk_generator::ChunkGenerator`] uses for its core data
//!   and the [`crate::cave::cave_generator_cache::CaveGeneratorCache`] for
//!   its caves.
//! * The Java `get` wraps a failure in an `Error("Failed to receive
//!   UniquePrefabEntry for " + seed)`; the Rust port lets the panic of the
//!   compute function propagate (the compute function does not fail today —
//!   the unique prefabs are the Phase-5c seam).

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;

use crate::container::unique_prefab_container::UniquePrefabEntry;

/// The Java `UniquePrefabCache.UniquePrefabFunction` —
/// `UniquePrefabEntry[] get(int seed)`.
pub type UniquePrefabFunction = Rc<dyn Fn(i32) -> Vec<UniquePrefabEntry>>;

/// One live cache record: the entries plus their idle deadline (millis
/// since the cache was created).
struct CacheRecord {
    /// The shared entry list.
    value: Vec<UniquePrefabEntry>,
    /// The idle deadline (millis on the cache's clock).
    expires_at: AtomicU64,
}

/// Java `class UniquePrefabCache`.
pub struct UniquePrefabCache {
    /// The live cache entries.
    entries: RwLock<HashMap<i32, CacheRecord>>,
    /// Java `UniquePrefabFunction function`.
    compute: UniquePrefabFunction,
    /// Java `int maxSize`.
    max_size: usize,
    /// Java `long expireAfterSeconds`.
    expire_after_seconds: u64,
    /// The origin of the cache's idle clock.
    created: Instant,
}

impl UniquePrefabCache {
    /// Java `public UniquePrefabCache(@Nonnull UniquePrefabFunction function,
    /// int maxSize, long expireAfterSeconds)`.
    pub fn new(compute: UniquePrefabFunction, max_size: usize, expire_after_seconds: u64) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            compute,
            max_size,
            expire_after_seconds,
            created: Instant::now(),
        }
    }

    /// Java `@Nullable UniquePrefabContainer.UniquePrefabEntry[] get(int
    /// seed)` — the cached entries of the seed (computed on a miss).
    pub fn get(&self, seed: i32) -> Vec<UniquePrefabEntry> {
        if let Some(value) = self.lookup_live(&seed) {
            return value;
        }
        let value = (self.compute)(seed);
        self.insert(seed, value.clone());
        value
    }

    /// The number of entries the cache currently holds (the Java class has no
    /// equivalent; exposed for tests/monitoring).
    pub fn size(&self) -> usize {
        self.entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    /// Drops every expired cache entry.
    pub fn cleanup(&self) {
        let now = self.now_millis();
        self.entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|_, record| now < record.expires_at.load(Ordering::SeqCst));
    }

    /// Millis on the cache's idle clock.
    fn now_millis(&self) -> u64 {
        self.created.elapsed().as_millis() as u64
    }

    /// The idle timeout of the cache in millis.
    fn ttl_millis(&self) -> u64 {
        self.expire_after_seconds * 1000
    }

    /// Returns the live entry, refreshing its idle deadline (time-to-idle).
    fn lookup_live(&self, seed: &i32) -> Option<Vec<UniquePrefabEntry>> {
        let inner = self
            .entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = self.now_millis();
        inner.get(seed).and_then(|record| {
            if now < record.expires_at.load(Ordering::SeqCst) {
                record
                    .expires_at
                    .store(now + self.ttl_millis(), Ordering::SeqCst);
                Some(record.value.clone())
            } else {
                None
            }
        })
    }

    /// Insert-or-reuse under the write lock (compute-once for live keys).
    fn insert(&self, seed: i32, value: Vec<UniquePrefabEntry>) {
        let mut inner = self
            .entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = self.now_millis();
        if let Some(record) = inner.get(&seed) {
            if now < record.expires_at.load(Ordering::SeqCst) {
                return;
            }
            inner.remove(&seed);
        }
        self.evict_if_full(&mut inner, now);
        inner.insert(
            seed,
            CacheRecord {
                value,
                expires_at: AtomicU64::new(now + self.ttl_millis()),
            },
        );
    }

    /// Sized-cache eviction: expired entries first, then arbitrary ones.
    fn evict_if_full(&self, inner: &mut HashMap<i32, CacheRecord>, now: u64) {
        if inner.len() < self.max_size {
            return;
        }
        inner.retain(|_, record| now < record.expires_at.load(Ordering::SeqCst));
        while inner.len() >= self.max_size {
            if let Some(first) = inner.keys().next().copied() {
                inner.remove(&first);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn the_entries_are_computed_once_per_seed() {
        let calls = Rc::new(AtomicUsize::new(0));
        let counter = Rc::clone(&calls);
        let cache = UniquePrefabCache::new(
            Rc::new(move |_seed: i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }),
            16,
            60,
        );

        assert!(cache.get(7).is_empty());
        assert!(cache.get(7).is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the entries are cached");
        assert_eq!(cache.size(), 1);

        // A different seed computes a new list.
        assert!(cache.get(8).is_empty());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_cache_is_sized() {
        let cache = UniquePrefabCache::new(Rc::new(|_seed: i32| Vec::new()), 2, 60);
        cache.get(1);
        cache.get(2);
        cache.get(3);
        assert!(cache.size() <= 2, "the cache is sized");
    }
}
