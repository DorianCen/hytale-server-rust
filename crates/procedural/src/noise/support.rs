// Support utilities used by the noise core but defined in other Java packages.
//!
//! The trait definitions that used to live here have moved to the module of
//! their Java package:
//!
//! - `condition.IIntCondition` / `IDoubleCondition` / `IDoubleThreshold` →
//!   [`crate::noise::condition`]
//! - `supplier.*` (double/float ranges, coordinate suppliers) →
//!   [`crate::noise::supplier`]
//! - `property.NoiseProperty` / `NoiseFormulaProperty.NoiseFormula.Formula` →
//!   [`crate::noise::property`]
//!
//! What remains are the small helpers the noise core needs but that have no
//! Rust port of their Java type: the generic `PointConsumer` (replaced by
//! closures), a minimal `org.joml.Vector2i`, and a `fastutil` helper.

/// Java `logic.point.PointConsumer<T>` — `void accept(double x, double y, T t)`.
///
/// The Rust call sites use `&mut dyn FnMut(f64, f64)` instead (the Java
/// generic pair `(T ctx, PointConsumer<T>)` becomes one closure).
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
