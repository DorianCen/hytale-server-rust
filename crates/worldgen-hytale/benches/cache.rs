//! Port of the Java `CacheBenchmark` (JMH) — the lookups the world generator's
//! core-data / cave caches perform. The Java benchmark compares the cache
//! implementations; the Rust port benchmarks the ported moka-backed cache
//! against a plain `HashMap` baseline over the same 1000-key working set.

use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use worldgen_hytale::util::cache::{Cache, SizedTimeoutCache};

const SIZE: i32 = 1000;

fn sized_cache(c: &mut Criterion) {
    let cache = SizedTimeoutCache::<i32, i32>::new(
        Duration::from_secs(30),
        SIZE as u64,
        Some(Arc::new(|key: &i32| *key * 2)),
    );
    for key in 0..SIZE {
        cache.put(key, key * 2);
    }

    c.bench_function("cache/sized_timeout_get_hit", |b| {
        let mut key = 0;
        b.iter(|| {
            key = (key + 1) % SIZE;
            black_box(cache.get(key));
        })
    });

    let mut map: HashMap<i32, i32> = (0..SIZE).map(|key| (key, key * 2)).collect();
    c.bench_function("cache/hash_map_get_hit", |b| {
        let mut key = 0;
        b.iter(|| {
            key = (key + 1) % SIZE;
            black_box(map.get(&key).copied());
        })
    });

    c.bench_function("cache/hash_map_get_or_insert", |b| {
        let mut key = 0;
        b.iter(|| {
            key = (key + 1) % SIZE;
            black_box(*map.entry(key).or_insert(key * 2));
        })
    });
}

criterion_group!(benches, sized_cache);
criterion_main!(benches);
