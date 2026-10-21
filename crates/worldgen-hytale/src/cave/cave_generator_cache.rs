// Ported from: `.hytale-source/.../worldgen/cache/CaveGeneratorCache.java`
//! Java `com.hypixel.hytale.server.worldgen.cache.CaveGeneratorCache` — the
//! cache that keeps the generated caves of a cave type at a chunk position.
//!
//! # Deviations
//!
//! * **The cache is a private twin of the ported
//!   [`crate::cache::extended_coordinate_cache::ExtendedCoordinateCache`]**.
//!   The ported cache is backed by `moka`, which requires its key and value
//!   types to be `Send + Sync`; the cave graph is `Rc`-based (the whole
//!   world-generator object graph is `!Send`), so the cave cannot be stored
//!   in it. The twin keeps the *same* semantics — the key is the ported
//!   [`ExtendedCoordinateKey`] (with the Java hash formula), the value is
//!   computed at most once per key, the cache is sized and the entries
//!   expire when they are idle. It mirrors the twin the
//!   [`crate::chunk::chunk_generator::ChunkGenerator`] uses for its core
//!   data.
//! * The Java `localKey()` hook (the thread-local key object) is not needed
//!   in Rust — the key is built per lookup.
//! * The Java removal listener is `null` in the cave cache; the Rust port
//!   has no listener either.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;

use crate::cache::extended_coordinate_cache::ExtendedCoordinateKey;
use crate::cave::cave::Cave;
use crate::cave::cave_type::CaveType;

/// The Java `CaveGeneratorCache.CaveFunction` —
/// `Cave compute(CaveType caveType, int seed, int x, int z)`.
///
/// The Java function may return `null` (the zone has no cave generator);
/// the Rust port returns `None` in that case.
pub type CaveFunction = Rc<dyn Fn(&CaveType, i32, i32, i32) -> Option<Cave>>;

/// One live cache record: the shared cave plus its idle deadline (millis
/// since the cache was created).
struct CaveRecord {
    /// The shared, immutable cave.
    value: Rc<Cave>,
    /// The idle deadline (millis on the cache's clock).
    expires_at: AtomicU64,
}

/// Java `class CaveGeneratorCache extends ExtendedCoordinateCache<CaveType,
/// Cave>`.
pub struct CaveGeneratorCache {
    /// The live cache entries.
    entries: RwLock<HashMap<ExtendedCoordinateKey<CaveType>, CaveRecord>>,
    /// Java `ExtendedCoordinateObjectFunction<CaveType, Cave> loader`.
    compute: CaveFunction,
    /// Java `int maxSize`.
    max_size: u64,
    /// Java `long expireAfterSeconds`.
    expire_after_seconds: u64,
    /// The origin of the cache's idle clock.
    created: Instant,
}

impl CaveGeneratorCache {
    /// Java `CaveGeneratorCache(CaveFunction caveFunction, int maxSize, long
    /// expireAfterSeconds)`.
    pub fn new(compute: CaveFunction, max_size: u64, expire_after_seconds: u64) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            compute,
            max_size,
            expire_after_seconds,
            created: Instant::now(),
        }
    }

    /// Java `@Nullable Cave get(CaveType k, int seed, int x, int y)` — the
    /// cached cave of the position (computed on a miss).
    ///
    /// Deviation: the Java cache hands out the `Cave` object; the Rust port
    /// hands out a shared handle to it (the cave is immutable once it is
    /// compiled).
    pub fn get(&self, cave_type: &CaveType, seed: i32, x: i32, z: i32) -> Option<Rc<Cave>> {
        let key = ExtendedCoordinateKey::new(Some(cave_type.clone()), seed, x, z);
        if let Some(value) = self.lookup_live(&key) {
            return Some(value);
        }

        // Java: the value is computed while the map is locked; the Rust port
        // computes it outside the lock and re-uses the entry another thread
        // may have inserted in the meantime (the computed value is the same).
        let value = Rc::new((self.compute)(cave_type, seed, x, z)?);
        Some(self.insert(key, value))
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
    fn lookup_live(&self, key: &ExtendedCoordinateKey<CaveType>) -> Option<Rc<Cave>> {
        let inner = self
            .entries
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
    fn insert(&self, key: ExtendedCoordinateKey<CaveType>, value: Rc<Cave>) -> Rc<Cave> {
        let mut inner = self
            .entries
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
            CaveRecord {
                value: Rc::clone(&value),
                expires_at: AtomicU64::new(now + self.ttl_millis()),
            },
        );
        value
    }

    /// Sized-cache eviction: expired entries first, then arbitrary ones.
    ///
    /// The key holds a `CaveType`, which holds a `CaveNodeType` — and the
    /// node type stores its children behind a `RefCell` (the children are
    /// set after the node type has been created, because the node-type
    /// graph is cyclic). The key is never mutated (the children are set
    /// once, during the load), so the lint is a false positive here.
    #[allow(clippy::mutable_key_type)]
    fn evict_if_full(
        &self,
        inner: &mut HashMap<ExtendedCoordinateKey<CaveType>, CaveRecord>,
        now: u64,
    ) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::test_stubs::cave_type;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    #[test]
    fn the_cave_is_computed_once_per_key() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let cache: CaveGeneratorCache = CaveGeneratorCache::new(
            Rc::new(move |_cave_type: &CaveType, _seed: i32, _x: i32, _z: i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                Some(Cave::new(cave_type("generated")))
            }),
            16,
            60,
        );

        let cave_type = cave_type("cave");
        assert!(cache.get(&cave_type, 1, 2, 3).is_some());
        assert!(cache.get(&cave_type, 1, 2, 3).is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the cave is cached");
        assert_eq!(cache.size(), 1);

        // A different position computes a new cave.
        assert!(cache.get(&cave_type, 1, 2, 4).is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_missing_cave_is_not_cached() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let cache: CaveGeneratorCache = CaveGeneratorCache::new(
            Rc::new(move |_cave_type: &CaveType, _seed: i32, _x: i32, _z: i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                None
            }),
            16,
            60,
        );

        let cave_type = cave_type("cave");
        assert!(cache.get(&cave_type, 1, 2, 3).is_none());
        assert!(cache.get(&cave_type, 1, 2, 3).is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 2, "a null cave is not cached");
        assert_eq!(cache.size(), 0);
    }
}
