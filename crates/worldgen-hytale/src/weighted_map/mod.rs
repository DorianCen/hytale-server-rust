// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/Common/src/main/java/com/hypixel/hytale/common/map/`
//! Java `com.hypixel.hytale.common.map` package — weighted maps.
//!
//! Contains `IWeightedElement`, the `IWeightedMap` trait, `WeightedMap`
//! (including the Java nested `SingletonWeightedMap` fast path and the
//! `Builder`), the free `resolve_keys` helper, and `DefaultMap`
//! (see [`default_map`]).
//!
//! ## Notes on the Java → Rust mapping
//!
//! * Java `WeightedMap` and its nested `SingletonWeightedMap` are unified into
//!   a single struct [`WeightedMap`]. A map with ≤1 keys is a *singleton*
//!   fast path: `get` ignores `value` entirely and returns the sole key, and
//!   `for_each_entry` yields weight `1.0`.
//! * The 8 coordinate-supplier `get` overloads on the Java interface
//!   (`get(DoubleSupplier)`, `get(Random)`, `get(int, int, …)`, …) are **not**
//!   ported. Rust callers that have an `f64` call [`IWeightedMap::get`]
//!   directly; callers that have a `&mut FastRandom` call
//!   [`IWeightedMap::get_random`].

pub mod default_map;

use std::collections::HashSet;
use std::hash::Hash;

use hytale_math::fast_random::FastRandom;

/// Java `WeightedMap.EPSILON` — upper clamp for the incoming value.
pub const EPSILON: f64 = 0.99999;

/// Java `WeightedMap.ONE_MINUS_EPSILON` — `1 - EPSILON`.
pub const ONE_MINUS_EPSILON: f64 = 1.0 - EPSILON;

/// Java `IWeightedElement` — an element that knows its own weight.
pub trait IWeightedElement {
    /// Java `IWeightedElement.getWeight()`.
    fn get_weight(&self) -> f64;
}

/// Java `IWeightedMap<T>` — a map from `T` key → `f64` weight, selected by a
/// `f64` in `[0.0, 1.0]`.
///
/// This trait is object-safe so it can be used as `&dyn IWeightedMap<T>`.
pub trait IWeightedMap<T> {
    /// Java `IWeightedMap.get(double)`.
    ///
    /// ## Bit-exactness
    ///
    /// The f64 accumulation order is transcribed exactly from the Java loop:
    /// ```java
    /// double weightPercentSum = Math.min(value, EPSILON) * sum;
    /// for (int i = 0; i < keys.length; i++) {
    ///     if ((weightPercentSum -= values[i]) <= ONE_MINUS_EPSILON) return keys[i];
    /// }
    /// return null;
    /// ```
    /// Note that Java `Math.min` propagates NaN (returns NaN if either
    /// argument is NaN), while Rust `f64::min` returns the non-NaN operand;
    /// [`java_math_min`] bridges that difference.
    fn get(&self, value: f64) -> Option<&T>;

    /// Java `IWeightedMap.get(Random)`: `get(random.next_double())`.
    /// Java `IWeightedMap.get(Random)`.
    ///
    /// ⚠ Bit-exactness: Java's `WeightedMap.get(Random)` calls
    /// `random.nextDouble()`, but the nested `SingletonWeightedMap.get(Random)`
    /// returns the key **without consuming the random source**. The override
    /// below reproduces that split, because consuming a draw on the singleton
    /// path would shift every subsequent `random` draw in the same column.
    /// Java `IWeightedMap.get(Random)`.
    ///
    /// ⚠ Bit-exactness: Java's plain `WeightedMap.get(Random)` draws
    /// `random.nextDouble()`, but the nested `SingletonWeightedMap.get(Random)`
    /// returns the key **without consuming the random source**. The default
    /// below reproduces that split (singleton ⇔ at most one key), because
    /// consuming a draw on the singleton path would shift every subsequent
    /// random draw for the same column.
    fn get_random(&self, random: &mut FastRandom) -> Option<&T> {
        if self.size() <= 1 {
            return self.internal_keys().first();
        }
        self.get(random.next_double())
    }

    /// Java `IWeightedMap.size()`.
    fn size(&self) -> usize;

    /// Java `IWeightedMap.contains(T)`.
    fn contains(&self, obj: &T) -> bool;

    /// Java `IWeightedMap.forEach(Consumer<T>)`.
    fn for_each(&self, consumer: &mut dyn FnMut(&T));

    /// Java `IWeightedMap.forEachEntry(ObjDoubleConsumer<T>)`.
    ///
    /// For singleton maps (≤1 key) the yielded weight is `1.0`; for
    /// multi-entry maps it is the stored weight.
    fn for_each_entry(&self, consumer: &mut dyn FnMut(&T, f64));

    /// Java `IWeightedMap.internalKeys()`.
    ///
    /// Returns the keys slice in insertion order (no allocation).
    fn internal_keys(&self) -> &[T];
}

/// Java `WeightedMap<T>` — a weighted key→weight map selected by `f64`.
///
/// When `keys.len() <= 1` the map is a *singleton* (Java's nested
/// `SingletonWeightedMap`): `get` returns the sole key without consulting
/// `value`, and `for_each_entry` yields weight `1.0`.
#[derive(Clone, Debug)]
pub struct WeightedMap<T> {
    keys: Vec<T>,
    values: Vec<f64>,
    sum: f64,
    /// `true` when `keys.len() <= 1` — the singleton fast path.
    singleton: bool,
    /// Java keySet semantics for `contains`.
    key_set: HashSet<T>,
}

impl<T: Clone + Eq + Hash> WeightedMap<T> {
    /// Construct a `WeightedMap` from already-built parts.
    ///
    /// Java `WeightedMap.WeightedMap(T[] keys, double[] values, double sum)` +
    /// the `SingletonWeightedMap` fast path in `Builder.build()` /
    /// `resolveKeys`.
    pub fn from_parts(keys: Vec<T>, values: Vec<f64>, sum: f64) -> Self {
        let singleton = keys.len() <= 1;
        let mut key_set = HashSet::with_capacity(keys.len());
        for k in &keys {
            key_set.insert(k.clone());
        }
        Self {
            keys,
            values,
            sum,
            singleton,
            key_set,
        }
    }

    /// Java `WeightedMap.builder(T[])` → a fresh [`Builder`].
    pub fn builder() -> Builder<T> {
        Builder::new()
    }
}

impl<T: Clone + Eq + Hash> PartialEq for WeightedMap<T> {
    /// Java: the weighted map compares its keys and its weights.
    fn eq(&self, other: &Self) -> bool {
        self.keys == other.keys && self.values == other.values
    }
}

impl<T: Clone + Eq + Hash> IWeightedMap<T> for WeightedMap<T> {
    fn get(&self, value: f64) -> Option<&T> {
        if self.singleton {
            return self.keys.first();
        }
        let mut weight_percent_sum = java_math_min(value, EPSILON) * self.sum;
        for (i, key) in self.keys.iter().enumerate() {
            weight_percent_sum -= self.values[i];
            if weight_percent_sum <= ONE_MINUS_EPSILON {
                return Some(key);
            }
        }
        None
    }

    fn size(&self) -> usize {
        self.keys.len()
    }

    fn contains(&self, obj: &T) -> bool {
        self.key_set.contains(obj)
    }

    fn for_each(&self, consumer: &mut dyn FnMut(&T)) {
        for key in &self.keys {
            consumer(key);
        }
    }

    fn for_each_entry(&self, consumer: &mut dyn FnMut(&T, f64)) {
        if self.singleton {
            if let Some(key) = self.keys.first() {
                consumer(key, 1.0);
            }
            return;
        }
        for (i, key) in self.keys.iter().enumerate() {
            consumer(key, self.values[i]);
        }
    }

    fn internal_keys(&self) -> &[T] {
        &self.keys
    }
}

/// Java `Math.min(a, b)` — propagates NaN (returns NaN if either argument is
/// NaN), unlike Rust `f64::min` which returns the non-NaN operand.
fn java_math_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// Java `IWeightedMap.resolveKeys(Function<T, K>, IntFunction<K[]>)`.
///
/// Collects every key through `for_each_entry` into a [`Builder`] and builds.
/// For singleton maps weights are ignored; for multi-entry maps the sum is
/// re-accumulated in the same insertion order → bit-identical to Java.
pub fn resolve_keys<T, K, F>(map: &dyn IWeightedMap<T>, mapper: F) -> WeightedMap<K>
where
    T: Clone + Eq + Hash,
    K: Clone + Eq + Hash,
    F: Fn(&T) -> K,
{
    let mut builder = WeightedMap::builder();
    map.for_each_entry(&mut |key: &T, weight: f64| {
        builder.put(mapper(key), weight);
    });
    builder.build()
}

/// Java `WeightedMap.Builder<T>`.
pub struct Builder<T: Clone + Eq + Hash> {
    keys: Vec<T>,
    values: Vec<f64>,
}

impl<T: Clone + Eq + Hash> Builder<T> {
    /// Java `WeightedMap.Builder.Builder(T[] emptyKeys)`.
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Java `Builder.put(T, double)`.
    pub fn put(&mut self, key: T, weight: f64) -> &mut Self {
        self.keys.push(key);
        self.values.push(weight);
        self
    }

    /// Java `Builder.putAll(IWeightedMap<T>)`.
    pub fn put_all(&mut self, map: &dyn IWeightedMap<T>) -> &mut Self {
        map.for_each_entry(&mut |key: &T, weight: f64| {
            self.put(key.clone(), weight);
        });
        self
    }

    /// Java `Builder.putAll(T[], ToDoubleFunction<T>)`.
    pub fn put_all_weighted<F>(&mut self, keys: &[T], weight: F) -> &mut Self
    where
        F: Fn(&T) -> f64,
    {
        for key in keys {
            self.put(key.clone(), weight(key));
        }
        self
    }

    /// Java `Builder.size()`.
    pub fn size(&self) -> usize {
        self.keys.len()
    }

    /// Java `Builder.clear()`.
    pub fn clear(&mut self) {
        self.keys.clear();
        self.values.clear();
    }

    /// Java `Builder.build()`.
    ///
    /// If `keys.len() <= 1` the result is a singleton (weights are ignored
    /// entirely); otherwise `sum` is the f64 fold of `values` in insertion
    /// order — identical accumulation to the Java loop. An empty builder
    /// yields an empty (singleton) map.
    pub fn build(&mut self) -> WeightedMap<T> {
        let sum: f64 = self.values.iter().fold(0.0, |acc, v| acc + v);
        WeightedMap::from_parts(self.keys.split_off(0), self.values.split_off(0), sum)
    }
}

impl<T: Clone + Eq + Hash> Default for Builder<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_math_min_nan_semantics() {
        // Java Math.min(NaN, x) == NaN; Rust f64::min(NaN, x) == x.
        assert!(f64::min(f64::NAN, 1.0) == 1.0);
        assert!(java_math_min(f64::NAN, 1.0).is_nan());
        assert!(java_math_min(1.0, f64::NAN).is_nan());
        assert_eq!(java_math_min(0.5, 0.99999), 0.5);
        assert_eq!(java_math_min(1.5, 0.99999), 0.99999);
    }

    #[test]
    fn two_key_boundaries_trace_java_arithmetic() {
        // Java arithmetic, values [1.0, 3.0], sum = 4.0, EPSILON = 0.99999,
        // ONE_MINUS_EPSILON = 0.00001.
        //
        //   get(0.0): wps = min(0.0, EPSILON)*4.0 = 0.0
        //              0.0 - 1.0 = -1.0  <= 0.00001  → return keys[0]
        //   get(0.5): wps = 0.5*4.0 = 2.0
        //              2.0 - 1.0 = 1.0   >  0.00001  → continue
        //              1.0 - 3.0 = -2.0  <= 0.00001  → return keys[1]
        //   get(1.0): wps = min(1.0, EPSILON)*4.0 = 0.99999*4.0 = 3.99996
        //              3.99996 - 1.0 = 2.99996 > 0.00001 → continue
        //              2.99996 - 3.0 = -0.00004 <= 0.00001 → return keys[1]
        let mut b = WeightedMap::<&str>::builder();
        b.put("A", 1.0).put("B", 3.0);
        let m = b.build();
        assert_eq!(m.get(0.0), Some(&"A"));
        assert_eq!(m.get(0.5), Some(&"B"));
        assert_eq!(m.get(1.0), Some(&"B"));
        assert_eq!(m.size(), 2);
        assert!(m.contains(&"A"));
        assert!(m.contains(&"B"));
        assert!(!m.contains(&"C"));
        let keys: Vec<&&str> = m.internal_keys().iter().collect();
        assert_eq!(keys, vec![&"A", &"B"]);
    }

    #[test]
    fn singleton_single_key_ignores_value() {
        let mut b = WeightedMap::<u32>::builder();
        b.put(7, 42.0);
        let m = b.build();
        // SingletonWeightedMap.get returns the sole key regardless of value.
        assert_eq!(m.get(0.0), Some(&7));
        assert_eq!(m.get(0.99999), Some(&7));
        assert_eq!(m.get(1.0), Some(&7));
        assert_eq!(m.size(), 1);
        assert!(m.contains(&7));
        assert!(!m.contains(&8));
        let mut calls = Vec::new();
        m.for_each_entry(&mut |k: &u32, w: f64| calls.push((*k, w)));
        assert_eq!(calls, vec![(7, 1.0)]);
    }

    #[test]
    fn singleton_empty_map_returns_none() {
        let mut b = WeightedMap::<u32>::builder();
        // build() on empty yields an empty singleton map: get() → None, size 0.
        let m = b.build();
        assert_eq!(m.size(), 0);
        assert!(m.internal_keys().is_empty());
        assert_eq!(m.get(0.0), None);
        assert_eq!(m.get(1.0), None);
        let mut calls = Vec::new();
        m.for_each(&mut |_: &u32| calls.push(()));
        assert!(calls.is_empty());
        let mut entries = Vec::new();
        m.for_each_entry(&mut |_: &u32, _: f64| entries.push(()));
        assert!(entries.is_empty());
    }

    #[test]
    fn for_each_iteration_order() {
        let mut b = WeightedMap::<&str>::builder();
        b.put("x", 1.0).put("y", 2.0).put("z", 3.0);
        let m = b.build();
        let mut seen = Vec::new();
        m.for_each(&mut |k: &&str| seen.push(*k));
        assert_eq!(seen, vec!["x", "y", "z"]);
        let mut entries = Vec::new();
        m.for_each_entry(&mut |k: &&str, w: f64| entries.push((*k, w)));
        assert_eq!(entries, vec![("x", 1.0), ("y", 2.0), ("z", 3.0)]);
    }

    #[test]
    fn builder_put_all_from_map() {
        let mut b1 = WeightedMap::<&str>::builder();
        b1.put("a", 1.0).put("b", 2.0);
        let m1 = b1.build();

        let mut b2 = WeightedMap::<&str>::builder();
        b2.put("c", 4.0).put("d", 8.0);
        let m2 = b2.build();

        let mut combined = WeightedMap::<&str>::builder();
        combined.put_all(&m1).put_all(&m2);
        let m = combined.build();

        let mut entries = Vec::new();
        m.for_each_entry(&mut |k: &&str, w: f64| entries.push((*k, w)));
        // Insertion order preserved: a, b from m1 then c, d from m2.
        assert_eq!(
            entries,
            vec![("a", 1.0), ("b", 2.0), ("c", 4.0), ("d", 8.0)]
        );
        assert_eq!(m.size(), 4);
    }

    #[test]
    fn builder_put_all_from_singleton_yields_weight_one() {
        // Java SingletonWeightedMap.forEachEntry always yields weight 1,
        // so putAll from a singleton map inserts weight 1.0 (stored weight
        // is ignored entirely).
        let mut b1 = WeightedMap::<&str>::builder();
        b1.put("a", 1.0).put("b", 2.0);
        let m1 = b1.build();

        let mut b2 = WeightedMap::<&str>::builder();
        b2.put("solo", 4.0);
        let m2 = b2.build();

        let mut combined = WeightedMap::<&str>::builder();
        combined.put_all(&m1).put_all(&m2);
        let m = combined.build();

        let mut entries = Vec::new();
        m.for_each_entry(&mut |k: &&str, w: f64| entries.push((*k, w)));
        assert_eq!(entries, vec![("a", 1.0), ("b", 2.0), ("solo", 1.0)]);
        assert_eq!(m.size(), 3);
    }

    #[test]
    fn builder_put_all_weighted() {
        let mut b = WeightedMap::<u32>::builder();
        b.put_all_weighted(&[10, 20, 30], |k: &u32| (*k as f64) * 2.0);
        let m = b.build();
        let mut entries = Vec::new();
        m.for_each_entry(&mut |k: &u32, w: f64| entries.push((*k, w)));
        assert_eq!(entries, vec![(10, 20.0), (20, 40.0), (30, 60.0)]);
    }

    #[test]
    fn builder_clear_resets() {
        let mut b = WeightedMap::<u32>::builder();
        b.put(1, 1.0).put(2, 2.0);
        assert_eq!(b.size(), 2);
        b.clear();
        assert_eq!(b.size(), 0);
        assert_eq!(b.build().size(), 0);
    }

    #[test]
    fn resolve_keys_preserves_pick_behavior() {
        // Build a 2-key map with known weights.
        let mut b = WeightedMap::<&str>::builder();
        b.put("left", 1.0).put("right", 3.0);
        let m = b.build();

        // Map &str -> String via resolve_keys; weights carry over unchanged.
        let mapped: WeightedMap<String> = resolve_keys(&m, |s: &&str| s.to_string());
        assert_eq!(mapped.size(), 2);
        assert_eq!(mapped.get(0.0), Some(&"left".to_string()));
        assert_eq!(mapped.get(0.5), Some(&"right".to_string()));
        assert_eq!(mapped.get(1.0), Some(&"right".to_string()));

        // Also verify singleton resolveKeys: weights ignored, key preserved.
        let mut b1 = WeightedMap::<u32>::builder();
        b1.put(9, 100.0);
        let single = b1.build();
        let mapped_single: WeightedMap<u32> = resolve_keys(&single, |x: &u32| x + 1);
        assert_eq!(mapped_single.get(0.0), Some(&10));
        assert_eq!(mapped_single.get(1.0), Some(&10));
        let mut entries = Vec::new();
        mapped_single.for_each_entry(&mut |k: &u32, w: f64| entries.push((*k, w)));
        assert_eq!(entries, vec![(10, 1.0)]);
    }

    #[test]
    fn get_random_uses_next_double() {
        // get_random calls get(random.next_double()) — just verify it returns
        // the right key given a controlled FastRandom value.
        let mut b = WeightedMap::<&str>::builder();
        b.put("a", 1.0).put("b", 3.0);
        let m = b.build();
        // FastRandom::new(0); next_double() is deterministic.
        let mut rng = FastRandom::new(0);
        let pick = m.get_random(&mut rng);
        // Whatever next_double() produced, the pick matches get() at that value.
        let mut rng2 = FastRandom::new(0);
        let expected = m.get(rng2.next_double());
        assert_eq!(pick, expected);
    }

    #[test]
    fn singleton_get_random_does_not_consume_a_draw() {
        // Java `SingletonWeightedMap.get(Random)` returns the key without
        // touching the random source.
        let mut b = WeightedMap::<i32>::builder();
        b.put(7, 5.0);
        let m = b.build();
        let mut rng = FastRandom::new(42);
        assert_eq!(m.get_random(&mut rng), Some(&7));
        // The random source is untouched: the first draw is still first.
        let mut reference = FastRandom::new(42);
        assert_eq!(rng.next_double(), reference.next_double());
    }
}
