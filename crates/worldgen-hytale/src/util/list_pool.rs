// Ported from: `.hytale-source/.../worldgen/util/ListPool.java`
//! Java `com.hypixel.hytale.server.worldgen.util.ListPool` — a pool of
//! reusable lists.
//!
//! Java's `Resource` is an `AutoCloseable` `ObjectArrayList` that is returned
//! to the pool by `close()` (try-with-resources). The Rust port returns the
//! list to the pool in [`Resource`]'s `Drop` implementation, which is the
//! direct equivalent of the Java `close()` call.

use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;

/// Java `class ListPool<T>`.
pub struct ListPool<T> {
    /// Java `private final int capacity`.
    capacity: usize,
    /// Java `private final ConcurrentLinkedQueue<Resource<T>> pool`.
    pool: Mutex<Vec<Vec<T>>>,
}

impl<T> ListPool<T> {
    /// Java `ListPool(int capacity, T[] empty)`.
    ///
    /// Deviation: Java's second parameter is the `T[] empty` template array
    /// used by `Resource.toArray()` (`super.toArray(pool.empty)`). Rust has no
    /// array template and `Vec` is converted with `into_boxed_slice()` /
    /// `as_slice()`, so the parameter is not needed.
    pub fn new(capacity: usize) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            // Java: pool.add(new Resource<>(this));
            pool.push(Vec::new());
        }
        Self {
            capacity,
            pool: Mutex::new(pool),
        }
    }

    /// Java `int capacity` (the pool size limit).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Java `Resource<T> acquire()`.
    pub fn acquire(&self) -> Resource<'_, T> {
        let list = {
            let mut pool = self.lock();
            pool.pop().unwrap_or_default()
        };
        Resource { pool: self, list }
    }

    /// Java `Resource<T> acquire(int capacity)`.
    pub fn acquire_with_capacity(&self, capacity: usize) -> Resource<'_, T> {
        let mut resource = self.acquire();
        resource.ensure_capacity(capacity);
        resource
    }

    /// Java `void release(Resource<T> resource)`.
    fn release(&self, mut list: Vec<T>) {
        let mut pool = self.lock();
        if pool.len() < self.capacity {
            // Java: resource.clear();
            list.clear();
            pool.push(list);
        }
    }

    /// Locks the pool, ignoring lock poisoning (a panicking thread must not
    /// make the pool unusable).
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Vec<T>>> {
        self.pool
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Java `static class ListPool.Resource<T> extends ObjectArrayList<T>
/// implements AutoCloseable`.
pub struct Resource<'a, T> {
    /// Java `private final ListPool<T> pool`.
    pool: &'a ListPool<T>,
    /// Java `ObjectArrayList<T>` — the list contents.
    list: Vec<T>,
}

impl<T> Resource<'_, T> {
    /// Java `ObjectArrayList.ensureCapacity(int)`.
    fn ensure_capacity(&mut self, capacity: usize) {
        if self.list.capacity() < capacity {
            let additional = capacity.saturating_sub(self.list.len());
            self.list.reserve(additional);
        }
    }

    /// Consumes the resource and returns the underlying list **without**
    /// returning it to the pool (the Java equivalent of not calling
    /// `close()`).
    pub fn into_inner(self) -> Vec<T> {
        let mut this = ManuallyDrop::new(self);
        std::mem::take(&mut this.list)
    }
}

impl<T> Deref for Resource<'_, T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl<T> DerefMut for Resource<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

impl<T> Drop for Resource<'_, T> {
    /// Java `void close()` — returns the (cleared) list to the pool.
    fn drop(&mut self) {
        self.pool.release(std::mem::take(&mut self.list));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_returns_a_usable_list() {
        let pool: ListPool<i32> = ListPool::new(2);
        {
            let mut resource = pool.acquire();
            resource.push(1);
            resource.push(2);
            assert_eq!(resource.len(), 2);
            assert_eq!(resource[0], 1);
        }
    }

    #[test]
    fn dropped_resources_return_to_the_pool() {
        let pool: ListPool<i32> = ListPool::new(1);
        {
            let mut resource = pool.acquire();
            resource.push(42);
        }
        // The list was returned to the pool and cleared on release.
        let resource = pool.acquire();
        assert!(resource.is_empty());
    }

    #[test]
    fn pool_never_exceeds_its_capacity() {
        let pool: ListPool<i32> = ListPool::new(1);
        let first = pool.acquire();
        let second = pool.acquire();
        // Both are alive: the pool is empty, so the second acquire creates a
        // fresh list. Dropping both must not grow the pool beyond capacity.
        drop(first);
        drop(second);
        let mut pool_inner = pool.lock();
        assert_eq!(pool_inner.len(), 1);
        pool_inner.clear();
    }

    #[test]
    fn acquire_with_capacity_reserves_space() {
        let pool: ListPool<i32> = ListPool::new(1);
        let mut resource = pool.acquire_with_capacity(64);
        assert!(resource.capacity() >= 64);
        resource.push(1);
    }

    #[test]
    fn into_inner_detaches_the_list_from_the_pool() {
        let pool: ListPool<i32> = ListPool::new(1);
        let mut resource = pool.acquire();
        resource.push(7);
        let list = resource.into_inner();
        assert_eq!(list, vec![7]);
        // The pool is still empty: the list was not returned.
        let pool_inner = pool.lock();
        assert!(pool_inner.is_empty());
    }
}
