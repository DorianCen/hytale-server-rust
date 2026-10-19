// Support interfaces used by the noise core but defined in other Java packages
// that are being ported separately.
//!
//! ⚠ NOTE(subagent): these mirror the following Java interfaces:
//! - `procedurallib.condition.IIntCondition` / `IDoubleCondition`
//! - `procedurallib.supplier.IDoubleRange` / `ISeedDoubleRange`
//! - `procedurallib.property.NoiseProperty` (+ `NoiseFormulaProperty.NoiseFormula.Formula`)
//! - `procedurallib.logic.point.PointConsumer`
//! - `it.unimi.dsi.fastutil.HashCommon.nextPowerOfTwo(int)` helper
//! - a minimal `org.joml.Vector2i`
//!
//! They are kept here under the `noise` module namespace (no conflicting
//! symbols with a future `procedural::condition` / `procedural::supplier` /
//! `procedural::property` port). Integration task: wire the real modules into
//! these seams. Only the members used by the noise code are declared (Java's
//! other default methods of these interfaces are not needed by the noise core).

/// Java `condition.IIntCondition` — `boolean eval(int value)`.
pub trait IIntCondition {
    /// Java `IIntCondition.eval(int value)`.
    fn eval(&self, value: i32) -> bool;
}

/// Java `condition.IDoubleCondition` — `boolean eval(double value)`.
pub trait IDoubleCondition {
    /// Java `IDoubleCondition.eval(double value)`.
    fn eval(&self, value: f64) -> bool;
}

/// Java `supplier.IDoubleRange` — only the `double getValue(double)` overload
/// is used by the noise core.
pub trait IDoubleRange {
    /// Java `IDoubleRange.getValue(double v)`.
    fn get_value(&self, v: f64) -> f64;
}

/// Java `supplier.ISeedDoubleRange` — `double getValue(int seed, double value)`.
pub trait ISeedDoubleRange {
    /// Java `ISeedDoubleRange.getValue(int seed, double value)`.
    fn get_value(&self, seed: i32, value: f64) -> f64;
}

/// Java `supplier.ISeedDoubleRange.DIRECT` — identity range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectSeedDoubleRange;

impl ISeedDoubleRange for DirectSeedDoubleRange {
    #[inline]
    fn get_value(&self, _seed: i32, value: f64) -> f64 {
        value
    }
}

/// Java int-condition accepting any value (used where Java writes `seed -> true`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AlwaysTrueIntCondition;

impl IIntCondition for AlwaysTrueIntCondition {
    #[inline]
    fn eval(&self, _value: i32) -> bool {
        true
    }
}

/// Java `property.NoiseProperty` — noise sampled under an owned seed.
/// Java defines `get(int seed, double x, double y)` and a 3d overload.
pub trait NoiseProperty {
    /// Java `NoiseProperty.get(int seed, double x, double y)`.
    fn get(&self, seed: i32, x: f64, y: f64) -> f64;
    /// Java `NoiseProperty.get(int seed, double x, double y, double z)`.
    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64;
}

/// Java `NoiseFormulaProperty.NoiseFormula.Formula` — `double eval(double noise)`.
pub trait NoiseFormula {
    /// Java `NoiseFormula.Formula.eval(double noise)`.
    fn eval(&self, noise: f64) -> f64;
}

/// Java `logic.point.PointConsumer<T>` — `void accept(double x, double y, T t)`.
pub trait PointConsumer<T> {
    /// Java `PointConsumer.accept(double x, double y, T t)`.
    fn accept(&self, x: f64, y: f64, ctx: &mut T);
}

/// Minimal `org.joml.Vector2i` replacement used by `MeshNoise` and
/// `BranchEvaluator` direction tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Vector2i {
    pub x: i32,
    pub y: i32,
}

impl Vector2i {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// `it.unimi.dsi.fastutil.HashCommon.nextPowerOfTwo(int)` — least power of two
/// greater than or equal to (i.e. at least as large as) `x`; ported bit-exact.
#[inline]
pub(crate) fn fastutil_next_power_of_two(x: i32) -> i32 {
    if x == 0 {
        return 1;
    }
    let mut x = x.wrapping_sub(1);
    x |= x >> 1;
    x |= x >> 2;
    x |= x >> 4;
    x |= x >> 8;
    x |= x >> 16;
    x.wrapping_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_power_of_two_matches_fastutil() {
        assert_eq!(fastutil_next_power_of_two(0), 1);
        assert_eq!(fastutil_next_power_of_two(1), 1);
        assert_eq!(fastutil_next_power_of_two(2), 2);
        assert_eq!(fastutil_next_power_of_two(3), 4);
        assert_eq!(fastutil_next_power_of_two(4), 4);
        assert_eq!(fastutil_next_power_of_two(17), 32);
        assert_eq!(fastutil_next_power_of_two(32), 32);
    }
}
