// Ported from: `.hytale-source/.../worldgen/util/cache/*.java`
//! Java `com.hypixel.hytale.server.worldgen.util.cache` package — sized and
//! timed caches used by the world generator.
//!
//! | Java class                   | Rust item                          |
//! |------------------------------|------------------------------------|
//! | `Cache`                      | [`Cache`]                          |
//! | `TimeoutCache`               | [`TimeoutCache`]                   |
//! | `SizedTimeoutCache`          | [`SizedTimeoutCache`]              |
//! | `ConcurrentSizedTimeoutCache`| [`ConcurrentSizedTimeoutCache`]    |
//!
//! # Java → Rust mapping
//!
//! The Java implementation is built on `ConcurrentHashMap` /
//! `Object2ObjectLinkedOpenHashMap` plus a scheduled cleanup task. The Rust
//! port keeps the *semantic contract* (sized + timed, an access refreshes the
//! idle timer, values are computed at most once per key) and delegates the
//! data structure to [`moka`]:
//!
//! - **Scheduled background cleanup** → moka evicts expired entries lazily;
//!   [`Cache::cleanup`] calls moka's `run_pending_tasks()` to apply the
//!   pending expirations immediately. The periodic scheduling itself is done
//!   by the worker pool in Phase 6.
//! - **`destroyer` callbacks** (invoked when an entry is evicted) → omitted.
//!   Rust frees the evicted values through `Drop`; the Java callback exists
//!   only to release resources that the GC would otherwise keep alive.
//! - **`computeKey`** (the `ConcurrentSizedTimeoutCache` key copier) →
//!   omitted, moka clones the keys internally.
//! - **Use after shutdown** → Java throws `IllegalStateException("Cache has
//!   been shutdown!")`; the Rust port panics with the same message.

use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The Java `Function<K, V>` loader of the caches.
pub type CacheLoader<K, V> = Arc<dyn Fn(&K) -> V + Send + Sync>;

/// The Java `@Nullable Function<K, V>` loader of the caches.
pub type OptionalCacheLoader<K, V> = Option<CacheLoader<K, V>>;

/// Java `interface Cache<K, V>`.
pub trait Cache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Java `void shutdown()`.
    fn shutdown(&self);

    /// Java `void cleanup()`.
    fn cleanup(&self);

    /// Java `@Nullable V get(K key)`.
    fn get(&self, key: K) -> Option<V>;
}

/// Shared shutdown flag handling (Java's `future.isCancelled()` check).
#[derive(Debug, Default)]
struct ShutdownFlag {
    shutdown: AtomicBool,
}

impl ShutdownFlag {
    fn new() -> Self {
        Self {
            shutdown: AtomicBool::new(false),
        }
    }

    /// Java: `if (future.isCancelled()) throw new IllegalStateException("Cache
    /// has been shutdown!");`
    fn check_alive(&self) {
        if self.shutdown.load(Ordering::SeqCst) {
            panic!("Cache has been shutdown!");
        }
    }

    fn mark_shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

/// Java `class TimeoutCache<K, V> implements Cache<K, V>` — an unbounded cache
/// whose entries expire when they have not been accessed for a while.
pub struct TimeoutCache<K, V> {
    /// The moka cache backing the Java `ConcurrentHashMap`.
    cache: moka::sync::Cache<K, V>,
    /// Java `@Nonnull private final Function<K, V> func`.
    func: CacheLoader<K, V>,
    /// Java's `future.isCancelled()` check.
    flag: ShutdownFlag,
}

impl<K, V> TimeoutCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Java `TimeoutCache(long expire, TimeUnit unit, Function<K, V> func,
    /// BiConsumer<K, V> destroyer)`.
    ///
    /// Deviation: the `destroyer` callback is omitted (see the module
    /// documentation) and the timeout is given as a [`Duration`] instead of a
    /// value + time unit.
    pub fn new(expire: Duration, func: CacheLoader<K, V>) -> Self {
        let cache = moka::sync::Cache::<K, V>::builder()
            .time_to_idle(expire)
            .build();
        Self {
            cache,
            func,
            flag: ShutdownFlag::new(),
        }
    }

    /// The number of entries currently held by the cache.
    pub fn size(&self) -> u64 {
        self.cache.entry_count()
    }
}

impl<K, V> Cache<K, V> for TimeoutCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Java `V get(K key)` — computes the value on a miss.
    fn get(&self, key: K) -> Option<V> {
        self.flag.check_alive();
        // Java: map.compute(key, (k, v) -> v != null ? v : new CacheEntry<>(func.apply(k)))
        Some(self.cache.get_with_by_ref(&key, || (self.func)(&key)))
    }

    /// Java `void cleanup()` — drops the expired entries.
    fn cleanup(&self) {
        self.cache.run_pending_tasks();
    }

    /// Java `void shutdown()`.
    fn shutdown(&self) {
        self.flag.mark_shutdown();
        self.cache.invalidate_all();
        self.cache.run_pending_tasks();
    }
}

/// Java `class SizedTimeoutCache<K, V> implements Cache<K, V>` — a cache with
/// a maximum size that evicts the least recently used entries.
pub struct SizedTimeoutCache<K, V> {
    /// The moka cache backing the Java `Object2ObjectLinkedOpenHashMap`.
    cache: moka::sync::Cache<K, V>,
    /// Java `@Nullable private final Function<K, V> func`.
    func: OptionalCacheLoader<K, V>,
    /// Java's `future.isCancelled()` check.
    flag: ShutdownFlag,
}

impl<K, V> SizedTimeoutCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Java `SizedTimeoutCache(long expire, TimeUnit unit, int maxSize,
    /// Function<K, V> func, BiConsumer<K, V> destroyer)`.
    ///
    /// Deviation: the `destroyer` callback is omitted and the timeout is given
    /// as a [`Duration`]. The `func` is optional in Java as well: a cache
    /// without a loader never computes a value and `get` returns `None` for
    /// unknown keys.
    pub fn new(expire: Duration, max_size: u64, func: OptionalCacheLoader<K, V>) -> Self {
        let cache = moka::sync::Cache::<K, V>::builder()
            .max_capacity(max_size)
            .time_to_idle(expire)
            .build();
        Self {
            cache,
            func,
            flag: ShutdownFlag::new(),
        }
    }

    /// Java `void put(K key, V value)` — inserts (or replaces) a value.
    ///
    /// Java's `putAndMoveToFirst` also marks the entry as the most recently
    /// used one; moka does the same.
    pub fn put(&self, key: K, value: V) {
        self.flag.check_alive();
        self.cache.insert(key, value);
    }

    /// Java `@Nullable V getWithReusedKey(K reusedKey, Function<K, K> keyPool)`.
    ///
    /// Deviation: Java's variant exists to avoid allocating a key object for
    /// the lookup; the Rust port hands the key to moka, which owns (and
    /// clones) the stored keys, so the key pool is not needed and the method
    /// behaves exactly like [`Cache::get`].
    pub fn get_with_reused_key(&self, key: K, key_pool: impl Fn(&K) -> K) -> Option<V> {
        let _ = key_pool;
        self.get(key)
    }

    /// The number of entries currently held by the cache.
    pub fn size(&self) -> u64 {
        self.cache.entry_count()
    }
}

impl<K, V> Cache<K, V> for SizedTimeoutCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Java `@Nullable V get(K key)`.
    fn get(&self, key: K) -> Option<V> {
        self.flag.check_alive();
        match &self.func {
            // Java: the value is computed and inserted while the map is locked.
            Some(func) => Some(self.cache.get_with_by_ref(&key, || func(&key))),
            // Java: if (func == null) return null;
            None => self.cache.get(&key),
        }
    }

    /// Java `void cleanup()`.
    fn cleanup(&self) {
        self.cache.run_pending_tasks();
    }

    /// Java `void shutdown()`.
    fn shutdown(&self) {
        self.flag.mark_shutdown();
        self.cache.invalidate_all();
        self.cache.run_pending_tasks();
    }
}

/// Java `class ConcurrentSizedTimeoutCache<K, V> implements Cache<K, V>` — a
/// cache sharded into buckets, each protected by its own lock.
///
/// Deviation: the hand-rolled bucket array is replaced by moka, which is
/// already sharded and concurrent. The constructor keeps Java's signature
/// (including the `concurrencyLevel`) so the call sites stay identical, but
/// the concurrency level is not used to size the cache any more.
pub struct ConcurrentSizedTimeoutCache<K, V> {
    /// The moka cache backing the Java bucket array.
    cache: moka::sync::Cache<K, V>,
    /// Java `@Nonnull private final Function<K, V> computeValue`.
    compute_value: CacheLoader<K, V>,
    /// Java's `future.isCancelled()` check.
    flag: ShutdownFlag,
}

impl<K, V> ConcurrentSizedTimeoutCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Java `ConcurrentSizedTimeoutCache(int capacity, int concurrencyLevel,
    /// long timeout, TimeUnit timeoutUnit, Function<K, K> computeKey,
    /// Function<K, V> computeValue, BiConsumer<K, V> destroyer)`.
    ///
    /// Deviations:
    /// - the `computeKey` function is omitted (moka clones the keys),
    /// - the `destroyer` callback is omitted,
    /// - the timeout is given as a [`Duration`],
    /// - `concurrency_level` is accepted for signature parity but not used.
    pub fn new(
        capacity: u64,
        concurrency_level: usize,
        timeout: Duration,
        compute_value: CacheLoader<K, V>,
    ) -> Self {
        let _ = concurrency_level;
        let cache = moka::sync::Cache::<K, V>::builder()
            .max_capacity(capacity)
            .time_to_idle(timeout)
            .build();
        Self {
            cache,
            compute_value,
            flag: ShutdownFlag::new(),
        }
    }

    /// The number of entries currently held by the cache.
    pub fn size(&self) -> u64 {
        self.cache.entry_count()
    }
}

impl<K, V> Cache<K, V> for ConcurrentSizedTimeoutCache<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Java `V get(K key)`.
    fn get(&self, key: K) -> Option<V> {
        self.flag.check_alive();
        Some(
            self.cache
                .get_with_by_ref(&key, || (self.compute_value)(&key)),
        )
    }

    /// Java `void cleanup()`.
    fn cleanup(&self) {
        self.cache.run_pending_tasks();
    }

    /// Java `void shutdown()`.
    fn shutdown(&self) {
        self.flag.mark_shutdown();
        self.cache.invalidate_all();
        self.cache.run_pending_tasks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn timeout_cache_computes_the_value_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let cache: TimeoutCache<i32, i32> = TimeoutCache::new(
            Duration::from_secs(60),
            Arc::new(move |k: &i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                k * 2
            }),
        );

        assert_eq!(cache.get(21), Some(42));
        assert_eq!(cache.get(21), Some(42));
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the value is cached");
    }

    #[test]
    fn timeout_cache_cleanup_keeps_the_entries() {
        let cache: TimeoutCache<i32, i32> =
            TimeoutCache::new(Duration::from_secs(60), Arc::new(|k: &i32| *k));
        assert_eq!(cache.get(3), Some(3));
        cache.cleanup();
        assert_eq!(cache.get(3), Some(3));
    }

    #[test]
    fn sized_timeout_cache_supports_put() {
        let cache: SizedTimeoutCache<i32, i32> =
            SizedTimeoutCache::new(Duration::from_secs(60), 16, None);
        // Without a loader function unknown keys are not computed.
        assert_eq!(cache.get(1), None);
        cache.put(1, 100);
        assert_eq!(cache.get(1), Some(100));
    }

    #[test]
    fn sized_timeout_cache_computes_on_miss() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let cache: SizedTimeoutCache<i32, i32> = SizedTimeoutCache::new(
            Duration::from_secs(60),
            16,
            Some(Arc::new(move |k: &i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                k + 1
            })),
        );
        assert_eq!(cache.get(1), Some(2));
        assert_eq!(cache.get(1), Some(2));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn get_with_reused_key_behaves_like_get() {
        let cache: SizedTimeoutCache<i32, i32> =
            SizedTimeoutCache::new(Duration::from_secs(60), 16, Some(Arc::new(|k: &i32| k * 3)));
        assert_eq!(cache.get_with_reused_key(4, |k| k + 1), Some(12));
        assert_eq!(cache.get_with_reused_key(4, |k| k + 1), Some(12));
    }

    #[test]
    fn concurrent_sized_timeout_cache_computes_the_value_once() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let cache: ConcurrentSizedTimeoutCache<i32, i32> = ConcurrentSizedTimeoutCache::new(
            64,
            4,
            Duration::from_secs(60),
            Arc::new(move |k: &i32| {
                counter.fetch_add(1, Ordering::SeqCst);
                k * 5
            }),
        );
        assert_eq!(cache.get(2), Some(10));
        assert_eq!(cache.get(2), Some(10));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    #[should_panic(expected = "Cache has been shutdown!")]
    fn timeout_cache_panics_after_shutdown() {
        let cache: TimeoutCache<i32, i32> =
            TimeoutCache::new(Duration::from_secs(60), Arc::new(|k: &i32| *k));
        cache.shutdown();
        let _ = cache.get(1);
    }

    #[test]
    #[should_panic(expected = "Cache has been shutdown!")]
    fn sized_timeout_cache_panics_after_shutdown() {
        let cache: SizedTimeoutCache<i32, i32> =
            SizedTimeoutCache::new(Duration::from_secs(60), 16, None);
        cache.shutdown();
        let _ = cache.get(1);
    }

    #[test]
    #[should_panic(expected = "Cache has been shutdown!")]
    fn concurrent_sized_timeout_cache_panics_after_shutdown() {
        let cache: ConcurrentSizedTimeoutCache<i32, i32> =
            ConcurrentSizedTimeoutCache::new(8, 2, Duration::from_secs(60), Arc::new(|k: &i32| *k));
        cache.shutdown();
        let _ = cache.get(1);
    }

    #[test]
    fn shutdown_clears_the_entries() {
        let cache: SizedTimeoutCache<i32, i32> =
            SizedTimeoutCache::new(Duration::from_secs(60), 16, Some(Arc::new(|k: &i32| *k)));
        assert_eq!(cache.get(1), Some(1));
        cache.shutdown();
        // The value is gone ...
        assert_eq!(cache.size(), 0);
        // ... but reading it now panics (checked in the panic tests above).
    }
}
