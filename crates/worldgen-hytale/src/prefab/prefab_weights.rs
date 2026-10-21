// Ported from: `.hytale-source/.../CoreServer/.../server/core/prefab/PrefabWeights.java`
//! Java `com.hypixel.hytale.server.core.prefab.PrefabWeights` — the mapping
//! of prefab names to the weights used when a prefab is picked from a
//! collection.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use hytale_math::fast_random::FastRandom;

use crate::weighted_map::EPSILON;

/// Java `public class PrefabWeights`.
#[derive(Clone, Debug, PartialEq)]
pub struct PrefabWeights {
    /// Java `private double defaultWeight`.
    default_weight: f64,
    /// Java `private Object2DoubleMap<String> weightsLookup`.
    weights_lookup: HashMap<String, f64>,
    /// Java `protected double sum` — the total weight (initialised on the
    /// first `get` call).
    sum: Cell<f64>,
    /// Java `protected double[] weights` — the cumulative weights (initialised
    /// on the first `get` call).
    weights: RefCell<Vec<f64>>,
    /// Java `protected volatile boolean initialized`.
    initialized: Cell<bool>,
    /// Java `public static final PrefabWeights NONE` — the immutable "no
    /// weights" instance.
    is_none: bool,
}

impl Default for PrefabWeights {
    fn default() -> Self {
        Self::new()
    }
}

impl PrefabWeights {
    /// Java `public static final double DEFAULT_WEIGHT` = `1.0`.
    pub const DEFAULT_WEIGHT: f64 = 1.0;

    /// Java `public static final char DELIMITER_CHAR` = `','`.
    pub const DELIMITER_CHAR: char = ',';

    /// Java `public static final char ASSIGNMENT_CHAR` = `'='`.
    pub const ASSIGNMENT_CHAR: char = '=';

    /// Java `public PrefabWeights()`.
    pub fn new() -> Self {
        Self {
            default_weight: Self::DEFAULT_WEIGHT,
            weights_lookup: HashMap::new(),
            sum: Cell::new(0.0),
            weights: RefCell::new(Vec::new()),
            initialized: Cell::new(false),
            is_none: false,
        }
    }

    /// Java `public static final PrefabWeights NONE`.
    pub fn none() -> Self {
        Self {
            default_weight: Self::DEFAULT_WEIGHT,
            weights_lookup: HashMap::new(),
            sum: Cell::new(0.0),
            weights: RefCell::new(Vec::new()),
            initialized: Cell::new(true),
            is_none: true,
        }
    }

    /// Java `public int size()` — the number of name → weight mappings.
    pub fn size(&self) -> usize {
        self.weights_lookup.len()
    }

    /// Java `public double getWeight(String prefab)`.
    pub fn get_weight(&self, prefab: &str) -> f64 {
        self.weights_lookup
            .get(prefab)
            .copied()
            .unwrap_or(self.default_weight)
    }

    /// Java `public void setWeight(String prefab, double weight)`.
    pub fn set_weight(&mut self, prefab: &str, weight: f64) {
        if self.is_none {
            return;
        }
        check_weight(prefab, weight);
        self.weights_lookup.insert(prefab.to_string(), weight);
    }

    /// Java `public void removeWeight(String prefab)`.
    pub fn remove_weight(&mut self, prefab: &str) {
        if self.is_none {
            return;
        }
        self.weights_lookup.remove(prefab);
    }

    /// Java `public double getDefaultWeight()`.
    pub fn get_default_weight(&self) -> f64 {
        self.default_weight
    }

    /// Java `public void setDefaultWeight(double defaultWeight)`.
    pub fn set_default_weight(&mut self, default_weight: f64) {
        if self.is_none {
            return;
        }
        self.default_weight = default_weight.max(0.0);
    }

    /// Java `public Set<Object2DoubleMap.Entry<String>> entrySet()`.
    pub fn entry_set(&self) -> impl Iterator<Item = (&String, &f64)> {
        self.weights_lookup.iter()
    }

    /// Java `public <T> T get(@Nonnull T[] elements, @Nonnull Function<T,
    /// String> nameFunc, @Nonnull Random random)`.
    pub fn get<'a, T>(
        &'a self,
        elements: &'a [T],
        name_func: &dyn Fn(&T) -> String,
        random: &mut FastRandom,
    ) -> Option<&'a T> {
        self.get_with_value(elements, name_func, random.next_double())
    }

    /// Java `public <T> T get(@Nonnull T[] elements, @Nonnull Function<T,
    /// String> nameFunc, double value)`.
    ///
    /// Returns `None` when the value is negative, when the elements array
    /// does not match the length of the array the weights were initialised
    /// with, or when no element is selected.
    pub fn get_with_value<'a, T>(
        &'a self,
        elements: &'a [T],
        name_func: &dyn Fn(&T) -> String,
        value: f64,
    ) -> Option<&'a T> {
        if value < 0.0 {
            return None;
        }
        self.initialize(elements, name_func);

        let weights = self.weights.borrow();
        if weights.len() != elements.len() {
            return None;
        }

        let weighted_value = java_math_min(value, EPSILON) * self.sum.get();
        for (index, weight) in weights.iter().enumerate() {
            if weighted_value <= *weight {
                return elements.get(index);
            }
        }
        None
    }

    /// Java `private <T> void initialize(@Nonnull T[] elements, @Nonnull
    /// Function<T, String> nameFunc)`.
    fn initialize<T>(&self, elements: &[T], name_func: &dyn Fn(&T) -> String) {
        if self.initialized.get() {
            return;
        }
        let mut sum = 0.0;
        let mut weights = Vec::with_capacity(elements.len());
        for element in elements {
            sum += self.get_weight(&name_func(element));
            weights.push(sum);
        }
        self.sum.set(sum);
        *self.weights.borrow_mut() = weights;
        self.initialized.set(true);
    }

    /// Java `public static PrefabWeights parse(@Nonnull String
    /// mappingString)`.
    ///
    /// The mapping string is a comma-separated list of `name=weight` pairs;
    /// the whitespace around the name and the weight is trimmed. An empty
    /// string yields [`PrefabWeights::none`].
    pub fn parse(mapping_string: &str) -> PrefabWeights {
        let mut map: Option<HashMap<String, f64>> = None;
        let len = mapping_string.len();
        let mut start = 0usize;
        while start < len {
            // The end of the next name=weight pair.
            let end = match mapping_string[start..].find(Self::DELIMITER_CHAR) {
                Some(offset) => start + offset,
                None => len,
            };

            // The split point between the key and the value.
            let equals = match mapping_string[start..].find(Self::ASSIGNMENT_CHAR) {
                Some(offset) => start + offset,
                None => break,
            };
            if equals <= start {
                break;
            }

            // Split the key and the value, ignoring the surrounding
            // whitespace.
            let name = mapping_string[start..equals].trim().to_string();
            let value = mapping_string[equals + 1..end.max(equals + 1)].trim();
            let weight = parse_double(value);

            let map = map.get_or_insert_with(HashMap::new);
            map.insert(name, weight);

            // Jump to the end of the current mapping (the Java loop
            // increments `startPoint` once more, so the delimiter itself is
            // skipped).
            start = end + 1;
        }

        match map {
            None => PrefabWeights::none(),
            Some(map) => {
                let mut weights = PrefabWeights::new();
                weights.weights_lookup = map;
                weights
            }
        }
    }

    /// Java `public String getMappingString()`.
    ///
    /// Deviation: the Java map is a fastutil `Object2DoubleOpenHashMap`, so
    /// its iteration order is the hash order; the Rust port uses the
    /// `HashMap` order.
    pub fn get_mapping_string(&self) -> String {
        if self.weights_lookup.is_empty() {
            return String::new();
        }
        let mut builder = String::new();
        for (key, value) in &self.weights_lookup {
            if !builder.is_empty() {
                builder.push(Self::DELIMITER_CHAR);
                builder.push(' ');
            }
            builder.push_str(key);
            builder.push(Self::ASSIGNMENT_CHAR);
            builder.push_str(&format!("{value}"));
        }
        builder
    }
}

/// Java `Math.min(double, double)` — propagates NaN.
fn java_math_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// Java `Double.parseDouble(String)`.
fn parse_double(value: &str) -> f64 {
    value
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("For input string: \"{value}\""))
}

/// Java `private static void checkWeight(String prefab, double weight)`.
fn check_weight(prefab: &str, weight: f64) {
    if weight < 0.0 {
        panic!("Negative weight {weight:.5} assigned to prefab {prefab}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_weight_is_one() {
        let weights = PrefabWeights::new();
        assert_eq!(weights.get_default_weight(), 1.0);
        assert_eq!(weights.get_weight("missing"), 1.0);
        assert_eq!(weights.size(), 0);
    }

    #[test]
    fn a_weight_can_be_set_and_removed() {
        let mut weights = PrefabWeights::new();
        weights.set_weight("Tree", 2.5);
        assert_eq!(weights.get_weight("Tree"), 2.5);
        assert_eq!(weights.size(), 1);
        weights.remove_weight("Tree");
        assert_eq!(weights.get_weight("Tree"), 1.0);
        assert_eq!(weights.size(), 0);
    }

    #[test]
    fn a_negative_weight_is_rejected() {
        let mut weights = PrefabWeights::new();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            weights.set_weight("Tree", -1.0);
        }));
        let payload = result.unwrap_err();
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_else(|| "<non-string panic>".to_string());
        assert_eq!(message, "Negative weight -1.00000 assigned to prefab Tree");
    }

    #[test]
    fn the_none_instance_is_immutable() {
        let mut weights = PrefabWeights::none();
        weights.set_weight("Tree", 2.0);
        weights.set_default_weight(3.0);
        assert_eq!(weights.get_weight("Tree"), 1.0);
        assert_eq!(weights.get_default_weight(), 1.0);
    }

    #[test]
    fn the_pick_follows_the_cumulative_weights() {
        let mut weights = PrefabWeights::new();
        weights.set_weight("a", 1.0);
        weights.set_weight("b", 3.0);
        let elements = ["a".to_string(), "b".to_string()];
        let name = |element: &String| element.clone();
        // The sum is 4.0; the value 0.0 selects the first element, 0.5 the
        // second one.
        assert_eq!(
            weights.get_with_value(&elements, &name, 0.0),
            Some(&elements[0])
        );
        assert_eq!(
            weights.get_with_value(&elements, &name, 0.5),
            Some(&elements[1])
        );
        // A negative value selects nothing.
        assert_eq!(weights.get_with_value(&elements, &name, -0.5), None);
    }

    #[test]
    fn the_mapping_string_is_parsed() {
        let weights = PrefabWeights::parse("Prefab_001=1.0, Prefab_002=2.0");
        assert_eq!(weights.size(), 2);
        assert_eq!(weights.get_weight("Prefab_001"), 1.0);
        assert_eq!(weights.get_weight("Prefab_002"), 2.0);
        // An empty string yields the NONE instance.
        let none = PrefabWeights::parse("");
        assert_eq!(none.size(), 0);
        assert!(none.is_none);
    }
}
