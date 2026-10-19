// Ported from: `.hytale-source/.../worldgen/util/ObjectPool.java`
//! Java `com.hypixel.hytale.server.worldgen.util.ObjectPool` — a pool of
//! reusable objects created by a supplier.
//!
//! ⚠ **Not ported**: Java's `ObjectPool` implements `Function<T, T>`, i.e.
//! `apply(T cachedKey)` returns `acquire().apply(cachedKey)`. The Java type
//! bound `T extends Function<T, T>` forces every pooled object to be a
//! function of itself; the Rust port keeps the pool generic over `T` and the
//! callers of the later phases use [`ObjectPool::acquire`] directly.

use std::sync::{Arc, Mutex};

/// Java `class ObjectPool<T extends Function<T, T>> implements Function<T, T>`.
pub struct ObjectPool<T> {
    /// Java `private final BlockingQueue<T> items` — an `ArrayBlockingQueue`
    /// with the pool's capacity.
    items: Mutex<Vec<T>>,
    /// Java `private final Supplier<T> supplier`.
    supplier: Arc<dyn Fn() -> T + Send + Sync>,
    /// Java `ArrayBlockingQueue<>(size)` — the queue capacity.
    capacity: usize,
}

impl<T> ObjectPool<T> {
    /// Java `ObjectPool(int size, Supplier<T> supplier)`.
    pub fn new(capacity: usize, supplier: Arc<dyn Fn() -> T + Send + Sync>) -> Self {
        Self {
            items: Mutex::new(Vec::with_capacity(capacity)),
            supplier,
            capacity,
        }
    }

    /// Java `T acquire()`.
    pub fn acquire(&self) -> T {
        let pooled = {
            let mut items = self.lock();
            items.pop()
        };
        match pooled {
            Some(value) => value,
            // Java: return supplier.get();
            None => (self.supplier)(),
        }
    }

    /// Java `<K extends T> void recycle(@Nonnull K v)`.
    ///
    /// Java's `ArrayBlockingQueue.offer` fails when the queue is full, so the
    /// object is dropped instead of being pooled.
    pub fn recycle(&self, value: T) {
        let mut items = self.lock();
        if items.len() < self.capacity {
            items.push(value);
        }
    }

    /// Java `int size()` — the number of currently pooled objects.
    pub fn size(&self) -> usize {
        self.lock().len()
    }

    /// The pool capacity (Java's `ArrayBlockingQueue` capacity).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Locks the pool, ignoring lock poisoning (a panicking thread must not
    /// make the pool unusable).
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<T>> {
        self.items
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn acquire_uses_the_supplier_when_the_pool_is_empty() {
        let created = Arc::new(AtomicUsize::new(0));
        let counter = created.clone();
        let pool: ObjectPool<i32> = ObjectPool::new(
            2,
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                7
            }),
        );
        assert_eq!(pool.acquire(), 7);
        assert_eq!(pool.acquire(), 7);
        assert_eq!(created.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn recycled_objects_are_reused() {
        let created = Arc::new(AtomicUsize::new(0));
        let counter = created.clone();
        let pool: ObjectPool<i32> = ObjectPool::new(
            2,
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                1
            }),
        );
        pool.recycle(5);
        assert_eq!(pool.size(), 1);
        assert_eq!(pool.acquire(), 5);
        assert_eq!(created.load(Ordering::SeqCst), 0, "no new object needed");
    }

    #[test]
    fn recycle_respects_the_capacity() {
        let pool: ObjectPool<i32> = ObjectPool::new(1, Arc::new(|| 0));
        pool.recycle(1);
        pool.recycle(2);
        assert_eq!(pool.size(), 1, "the queue is bounded");
    }
}
