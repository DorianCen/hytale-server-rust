//! Deterministic asset-JSON/parent-chain fuzzing smoke test.
//!
//! The asset loader recursively resolves `Parent` documents and merges JSON
//! trees. Random graphs must produce `AssetError`s at worst, never panic or
//! recurse without bound. Budget: `HYTALE_FUZZ_ITERATIONS` (default 10,000).

use std::collections::BTreeMap;

use hytale_assets::{merge_values, ArrayMerge, ParentResolver};
use serde_json::{Map, Value};

fn iterations() -> usize {
    std::env::var("HYTALE_FUZZ_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_000)
        .clamp(1, 1_000_000)
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
}

fn value(rng: &mut Rng, depth: usize) -> Value {
    if depth == 0 || rng.next().is_multiple_of(4) {
        return match rng.next() % 5 {
            0 => Value::Null,
            1 => Value::Bool(rng.next() & 1 == 0),
            2 => Value::from(rng.next() as i64),
            3 => Value::from(rng.next() % 10_000),
            _ => Value::String(format!("s{}", rng.next() % 32)),
        };
    }
    if rng.next() & 1 == 0 {
        let len = (rng.next() % 4) as usize;
        Value::Array((0..len).map(|_| value(rng, depth - 1)).collect())
    } else {
        let len = (rng.next() % 5) as usize;
        let mut object = Map::new();
        for index in 0..len {
            object.insert(format!("k{index}"), value(rng, depth - 1));
        }
        Value::Object(object)
    }
}

#[test]
fn random_parent_graphs_and_json_merges_never_panic() {
    let mut rng = Rng(0x243f_6a88_85a3_08d3);
    let budget = iterations();
    for iteration in 0..budget {
        let count = (rng.next() % 8 + 1) as usize;
        let mut assets = BTreeMap::new();
        for index in 0..count {
            let mut object = Map::new();
            if !rng.next().is_multiple_of(5) {
                let parent = rng.next() as usize % (count + 2);
                object.insert(
                    "Parent".to_string(),
                    Value::String(format!("Asset{parent}")),
                );
            }
            for field in 0..(rng.next() % 4) {
                let depth = (rng.next() % 3) as usize;
                object.insert(format!("field{field}"), value(&mut rng, depth));
            }
            assets.insert(format!("Asset{index}"), Value::Object(object));
        }

        let resolver = ParentResolver::new(&assets);
        for key in assets.keys() {
            let _ = resolver.resolve(key);
        }
        let _ = resolver.resolve_all(&assets.keys().cloned().collect::<Vec<_>>());

        let parent_depth = (rng.next() % 3) as usize;
        let parent = value(&mut rng, parent_depth);
        let child_depth = (rng.next() % 3) as usize;
        let child = value(&mut rng, child_depth);
        let arrays = if iteration & 1 == 0 {
            ArrayMerge::Replace
        } else {
            ArrayMerge::Append
        };
        let merged = merge_values(parent, child, arrays);
        // The output must stay a valid JSON value for the next store stage.
        let _ = serde_json::to_vec(&merged);
    }
}
