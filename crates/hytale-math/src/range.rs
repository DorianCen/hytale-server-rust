//! Range value types: `Range` (min/max floats) and the range package types
//! `FloatRange`, `IntRange`.
//!
//! Ported from:
//! - `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/Range.java`
//! - `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/range/FloatRange.java`
//! - `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/range/IntRange.java`
//!
//! The Java `CODEC` static fields are deliberately not ported yet — the
//! codecs (`FloatRangeArrayCodec`, `IntRangeArrayCodec`) belong to a later
//! serialization phase.

use crate::math_util;

/// Java `com.hypixel.hytale.math.Range` — a range with minimum and maximum
/// `float` values.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Range {
    min: f32,
    max: f32,
}

impl Range {
    /// Java constructor `Range(min, max)`.
    pub fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    /// Java `getMin()`.
    pub fn min(&self) -> f32 {
        self.min
    }

    /// Java `getMax()`.
    pub fn max(&self) -> f32 {
        self.max
    }

    /// Java `toString()`.
    pub fn to_java_string(&self) -> String {
        format!("Range{{min={}, max={}}}", self.min, self.max)
    }
}

/// Java `com.hypixel.hytale.math.range.FloatRange`.
///
/// Equality/hashing mirror Java's `Float.compare`-based implementation:
/// `-0.0` and `0.0` are distinct, NaNs compare consistent with their bit
/// patterns (via `total_cmp`).
#[derive(Debug, Clone, Copy)]
pub struct FloatRange {
    inclusive_min: f32,
    inclusive_max: f32,
    /// Java caches `range = inclusive_max - inclusive_min` and keeps it fresh
    /// on every setter; the getters expose it only through the same derived
    /// value, so it is materialized identically here.
    range: f32,
}

impl FloatRange {
    /// Java `new FloatRange()`.
    pub fn empty() -> Self {
        Self::new(0.0, 0.0)
    }

    /// Java `new FloatRange(inclusiveMin, inclusiveMax)`.
    pub fn new(inclusive_min: f32, inclusive_max: f32) -> Self {
        Self {
            inclusive_min,
            inclusive_max,
            range: inclusive_max - inclusive_min,
        }
    }

    /// Java `getInclusiveMin()`.
    pub fn get_inclusive_min(&self) -> f32 {
        self.inclusive_min
    }

    /// Java `getInclusiveMax()`.
    pub fn get_inclusive_max(&self) -> f32 {
        self.inclusive_max
    }

    /// Java `setInclusiveMin(float)`.
    pub fn set_inclusive_min(&mut self, inclusive_min: f32) {
        self.inclusive_min = inclusive_min;
        self.range = self.inclusive_max - inclusive_min;
    }

    /// Java `setInclusiveMax(float)`.
    pub fn set_inclusive_max(&mut self, inclusive_max: f32) {
        self.inclusive_max = inclusive_max;
        self.range = inclusive_max - self.inclusive_min;
    }

    /// Java `getFloat(float factor)`.
    pub fn get_float(&self, factor: f32) -> f32 {
        // Java: value = inclusiveMin + (range * factor); return Float.min(inclusiveMax, value)
        let value = self.inclusive_min + self.range * factor;
        self.inclusive_max.min(value)
    }

    /// Java `getFloat(double factor)`.
    pub fn get_float_f64(&self, factor: f64) -> f32 {
        // Java: value = inclusiveMin + (float)(range * factor) — the product
        // is computed in f64, then narrowed to f32, then added to the f32 min.
        let value = self.inclusive_min + (self.range as f64 * factor) as f32;
        self.inclusive_max.min(value)
    }

    /// Java `includes(float)`.
    pub fn includes(&self, value: f32) -> bool {
        value >= self.inclusive_min && value <= self.inclusive_max
    }

    /// Java `toString()`.
    pub fn to_java_string(&self) -> String {
        format!(
            "FloatRange{{inclusiveMin={}, inclusiveMax={}}}",
            self.inclusive_min, self.inclusive_max
        )
    }
}

impl Default for FloatRange {
    fn default() -> Self {
        Self::empty()
    }
}

impl PartialEq for FloatRange {
    /// Java `equals`: `Float.compare` on `inclusiveMin`, then `inclusiveMax`,
    /// then `range`. `f32::total_cmp` matches `Float.compare` ordering
    /// semantics for -0.0/0.0 and NaNs.
    fn eq(&self, other: &Self) -> bool {
        use std::cmp::Ordering;
        self.inclusive_min.total_cmp(&other.inclusive_min) == Ordering::Equal
            && self.inclusive_max.total_cmp(&other.inclusive_max) == Ordering::Equal
            && self.range.total_cmp(&other.range) == Ordering::Equal
    }
}

impl std::hash::Hash for FloatRange {
    /// Java `hashCode`: mixes `Float.floatToIntBits` (with ±0.0f collapsed
    /// to 0) for `inclusiveMin`, `inclusiveMax` and `range` with factors of 31,
    /// using Java's wrapping `int` arithmetic.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        fn bits(f: f32) -> i32 {
            if f == 0.0 {
                0 // Java: (x != +0.0f ? floatToIntBits(x) : 0) — treats -0.0 as 0
            } else {
                f.to_bits() as i32
            }
        }
        let mut result = bits(self.inclusive_min);
        result = 31_i32
            .wrapping_mul(result)
            .wrapping_add(bits(self.inclusive_max));
        result = 31_i32.wrapping_mul(result).wrapping_add(bits(self.range));
        state.write_i32(result);
    }
}

impl Eq for FloatRange {}

/// Java `com.hypixel.hytale.math.range.IntRange`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct IntRange {
    inclusive_min: i32,
    inclusive_max: i32,
    /// Java caches `range = inclusive_max - inclusive_min + 1`.
    range: i32,
}

impl IntRange {
    /// Java `new IntRange()`.
    pub fn empty() -> Self {
        Self::new(0, 0)
    }

    /// Java `new IntRange(inclusiveMin, inclusiveMax)`.
    pub fn new(inclusive_min: i32, inclusive_max: i32) -> Self {
        Self {
            inclusive_min,
            inclusive_max,
            // Java subtracts silently with wrap-around.
            range: inclusive_max.wrapping_sub(inclusive_min).wrapping_add(1),
        }
    }

    /// Java `getInclusiveMin()`.
    pub fn get_inclusive_min(&self) -> i32 {
        self.inclusive_min
    }

    /// Java `getInclusiveMax()`.
    pub fn get_inclusive_max(&self) -> i32 {
        self.inclusive_max
    }

    /// Java `setInclusiveMin(int)`.
    pub fn set_inclusive_min(&mut self, inclusive_min: i32) {
        self.inclusive_min = inclusive_min;
        // Java: this.range = this.inclusiveMax - inclusiveMin + 1 (wraps silently)
        self.range = self
            .inclusive_max
            .wrapping_sub(inclusive_min)
            .wrapping_add(1);
    }

    /// Java `setInclusiveMax(int)`.
    pub fn set_inclusive_max(&mut self, inclusive_max: i32) {
        self.inclusive_max = inclusive_max;
        self.range = inclusive_max
            .wrapping_sub(self.inclusive_min)
            .wrapping_add(1);
    }

    /// Java `getInt(float factor)`.
    pub fn get_int_f32(&self, factor: f32) -> i32 {
        // Java: value = inclusiveMin + MathUtil.fastFloor(range * factor),
        // with `int range * float factor` promoted to f32.
        let value = self
            .inclusive_min
            .wrapping_add(math_util::fast_floor_f32(self.range as f32 * factor));
        self.inclusive_max.min(value)
    }

    /// Java `getInt(double factor)`.
    pub fn get_int_f64(&self, factor: f64) -> i32 {
        // Java: value = inclusiveMin + MathUtil.floor(range * factor),
        // with `int range * double factor` promoted to f64.
        let value = self.inclusive_min + math_util::floor(self.range as f64 * factor);
        self.inclusive_max.min(value)
    }

    /// Java `includes(int)`.
    pub fn includes(&self, value: i32) -> bool {
        value >= self.inclusive_min && value <= self.inclusive_max
    }

    /// Java `hashCode()`: `31 * inclusive_min + inclusive_max`.
    pub fn java_hash(&self) -> i32 {
        31i32
            .wrapping_mul(self.inclusive_min)
            .wrapping_add(self.inclusive_max)
    }

    /// Java `toString()`.
    pub fn to_java_string(&self) -> String {
        format!(
            "IntRange{{inclusiveMin={}, inclusiveMax={}}}",
            self.inclusive_min, self.inclusive_max
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_accessors() {
        let r = Range::new(1.5, 2.5);
        assert_eq!(r.min(), 1.5);
        assert_eq!(r.max(), 2.5);
    }

    #[test]
    fn float_range_maps_factor() {
        // Java semantics: value = min + range * factor, clamped by max.
        let r = FloatRange::new(1.0, 3.0);
        assert_eq!(r.get_float(0.0), 1.0);
        assert_eq!(r.get_float(1.0), 3.0);
        assert_eq!(r.get_float(0.5), 2.0);
        // factor > 1 clamps to inclusive_max (Float.min)
        assert_eq!(r.get_float(2.0), 3.0);
        assert_eq!(r.get_float_f64(0.5), 2.0);
        assert!(r.includes(2.0));
        assert!(!r.includes(3.5));
    }

    #[test]
    fn float_range_setters_refresh_cache() {
        let mut r = FloatRange::new(0.0, 4.0);
        r.set_inclusive_min(2.0);
        assert_eq!(r.get_float(1.0), 4.0); // 2 + (4-2)*1
        r.set_inclusive_max(10.0);
        assert_eq!(r.get_float(0.5), 6.0); // 2 + (10-2)*0.5
    }

    #[test]
    fn float_range_java_equality_and_hash() {
        let a = FloatRange::new(1.0, 2.0);
        let mut b = FloatRange::new(1.0, 2.0);
        assert_eq!(a, b);
        b.set_inclusive_min(1.05);
        assert_ne!(a, b);
        // -0.0 vs +0.0 are distinct under Float.compare
        let neg = FloatRange::new(-0.0, 0.0);
        let pos = FloatRange::new(0.0, 0.0);
        assert_ne!(neg, pos);
    }

    #[test]
    fn int_range_maps_factor() {
        let mut r = IntRange::new(0, 2); // range = 3
                                         // getInt(float): 0 + fastFloor(3 * factor), clamped by inclusive_max
        assert_eq!(r.get_int_f32(0.0), 0);
        assert_eq!(r.get_int_f32(1.0), 2);
        assert_eq!(r.get_int_f64(1.0), 2);
        r.set_inclusive_min(3);
        r.set_inclusive_max(9); // range = 7
        assert_eq!(r.get_int_f32(1.0), 9); // 3 + fastFloor(7)
        assert_eq!(r.get_int_f32(0.3), 5); // 3 + fastFloor(f32 7 * 0.3 = 2.0999999)
        assert_eq!(r.get_int_f64(0.4), 5); // 3 + floor(f64 7 * 0.4 = 2.8000000000000003)
        assert!(r.includes(4));
        assert!(!r.includes(10));
    }
}
