// Ported from: `.hytale-source/.../worldgen/util/function/*.java`
//! Java `com.hypixel.hytale.server.worldgen.util.function` package —
//! coordinate-hash based double suppliers.
//!
//! | Java class                        | Rust item                          |
//! |-----------------------------------|------------------------------------|
//! | `ICoordinateDoubleSupplier`       | [`ICoordinateDoubleSupplier`]      |
//! | `ConstantCoordinateDoubleSupplier`| [`ConstantCoordinateDoubleSupplier`]|
//! | `RandomCoordinateDoubleSupplier`  | [`RandomCoordinateDoubleSupplier`] |

use std::rc::Rc;

use hytale_math::hash_util;
use procedural::noise::supplier::IDoubleRange;

/// Java `interface ICoordinateDoubleSupplier`.
pub trait ICoordinateDoubleSupplier {
    /// Java `double apply(int seed, int x, int y)`.
    fn apply2(&self, seed: i32, x: i32, y: i32) -> f64;

    /// Java `double apply(int seed, int x, int y, int z)`.
    fn apply3(&self, seed: i32, x: i32, y: i32, z: i32) -> f64;
}

/// Java `class ConstantCoordinateDoubleSupplier implements ICoordinateDoubleSupplier`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstantCoordinateDoubleSupplier {
    /// Java `protected final double value`.
    pub value: f64,
}

impl ConstantCoordinateDoubleSupplier {
    /// Java `ConstantCoordinateDoubleSupplier.DEFAULT_ZERO`.
    pub const DEFAULT_ZERO: Self = Self { value: 0.0 };
    /// Java `ConstantCoordinateDoubleSupplier.DEFAULT_ONE`.
    pub const DEFAULT_ONE: Self = Self { value: 1.0 };

    /// Java `ConstantCoordinateDoubleSupplier(double value)`.
    pub const fn new(value: f64) -> Self {
        Self { value }
    }

    /// Java `double getValue()`.
    pub const fn get_value(&self) -> f64 {
        self.value
    }
}

impl ICoordinateDoubleSupplier for ConstantCoordinateDoubleSupplier {
    #[inline]
    fn apply2(&self, _seed: i32, _x: i32, _y: i32) -> f64 {
        self.value
    }

    #[inline]
    fn apply3(&self, _seed: i32, _x: i32, _y: i32, _z: i32) -> f64 {
        self.value
    }
}

/// Java `class RandomCoordinateDoubleSupplier implements ICoordinateDoubleSupplier`.
pub struct RandomCoordinateDoubleSupplier {
    /// Java `protected final IDoubleRange range`.
    pub range: Rc<dyn IDoubleRange>,
}

impl RandomCoordinateDoubleSupplier {
    /// Java `RandomCoordinateDoubleSupplier(IDoubleRange range)`.
    pub fn new(range: Rc<dyn IDoubleRange>) -> Self {
        Self { range }
    }

    /// Java `IDoubleRange getRange()`.
    pub fn get_range(&self) -> &Rc<dyn IDoubleRange> {
        &self.range
    }
}

impl ICoordinateDoubleSupplier for RandomCoordinateDoubleSupplier {
    /// Java `range.getValue(HashUtil.random(seed, x, y))`.
    ///
    /// Java's `HashUtil.random(long, long, long)` is selected because the
    /// `int` arguments widen to `long`.
    fn apply2(&self, seed: i32, x: i32, y: i32) -> f64 {
        self.range
            .get_value(hash_util::random3(seed as i64, x as i64, y as i64))
    }

    /// Java `range.getValue(HashUtil.random(seed, x, y, z))`.
    fn apply3(&self, seed: i32, x: i32, y: i32, z: i32) -> f64 {
        self.range.get_value(hash_util::random4(
            seed as i64,
            x as i64,
            y as i64,
            z as i64,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::supplier::DoubleRange;

    #[test]
    fn constant_supplier_ignores_the_coordinates() {
        let supplier = ConstantCoordinateDoubleSupplier::new(0.25);
        assert_eq!(supplier.get_value(), 0.25);
        assert_eq!(supplier.apply2(1, 2, 3), 0.25);
        assert_eq!(supplier.apply3(1, 2, 3, 4), 0.25);
        assert_eq!(
            ConstantCoordinateDoubleSupplier::DEFAULT_ZERO.apply2(0, 0, 0),
            0.0
        );
        assert_eq!(
            ConstantCoordinateDoubleSupplier::DEFAULT_ONE.apply3(0, 0, 0, 0),
            1.0
        );
    }

    #[test]
    fn random_supplier_is_deterministic_for_the_same_coordinates() {
        let supplier = RandomCoordinateDoubleSupplier::new(Rc::new(DoubleRange::normal(0.0, 1.0)));
        let first = supplier.apply2(7, 11, 13);
        let second = supplier.apply2(7, 11, 13);
        assert_eq!(first, second);
        assert!((0.0..=1.0).contains(&first));
    }

    #[test]
    fn random_supplier_3d_uses_a_different_hash_than_2d() {
        let supplier =
            RandomCoordinateDoubleSupplier::new(Rc::new(DoubleRange::normal(0.0, 1000.0)));
        let a = supplier.apply2(7, 11, 13);
        let b = supplier.apply3(7, 11, 13, 17);
        assert_ne!(a, b, "the 3d overload hashes four coordinates");
    }
}
