//! Port of the Java `PrefabWeightsBenchmark` (JMH):
//!
//! ```text
//! PrefabWeightsBenchmark.randomLookup       avgt   3   5.137 ± 0.787  ns/op
//! PrefabWeightsBenchmark.weightedLookup_03  avgt   3  15.552 ± 0.947  ns/op
//! PrefabWeightsBenchmark.weightedLookup_05  avgt   3  17.194 ± 1.160  ns/op
//! PrefabWeightsBenchmark.weightedLookup_10  avgt   3  21.726 ± 1.451  ns/op
//! ```

use criterion::{criterion_group, criterion_main, Criterion};
use hytale_math::fast_random::FastRandom;
use std::hint::black_box;
use worldgen_hytale::prefab::prefab_weights::PrefabWeights;

struct DummyPrefab {
    name: String,
}

impl DummyPrefab {
    fn new(id: usize) -> Self {
        Self {
            name: format!("Prefab_{id}"),
        }
    }
}

fn prefabs(count: usize) -> Vec<DummyPrefab> {
    (0..count).map(DummyPrefab::new).collect()
}

/// Deterministic weights (the Java benchmark uses `ThreadLocalRandom`; a
/// seeded LCG keeps the benchmark reproducible).
fn weights(count: usize) -> PrefabWeights {
    let mut random = FastRandom::new(0x5eed);
    let mut weights = PrefabWeights::new();
    for index in 0..count {
        weights.set_weight(&format!("Prefab_{index}"), random.next_double());
    }
    weights
}

fn prefab_weights(c: &mut Criterion) {
    let children_3 = prefabs(3);
    let children_5 = prefabs(5);
    let children_10 = prefabs(10);
    let weights_3 = weights(3);
    let weights_5 = weights(5);
    let weights_10 = weights(10);
    let name = |prefab: &DummyPrefab| prefab.name.clone();

    let mut random = FastRandom::new(0);
    c.bench_function("prefab_weights/random_lookup", |b| {
        b.iter(|| {
            let index = random.next_int_bound(children_10.len() as i32) as usize;
            black_box(&children_10[index]);
        })
    });

    for (label, children, weights) in [
        ("03", &children_3, &weights_3),
        ("05", &children_5, &weights_5),
        ("10", &children_10, &weights_10),
    ] {
        c.bench_function(&format!("prefab_weights/weighted_lookup_{label}"), |b| {
            b.iter(|| black_box(weights.get(children, &name, &mut random)));
        });
    }
}

criterion_group!(benches, prefab_weights);
criterion_main!(benches);
