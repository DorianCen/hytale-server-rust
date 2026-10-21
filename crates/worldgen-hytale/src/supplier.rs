// Ported from: `.hytale-source/.../ProceduralLibrary/.../procedurallib/supplier/*.java`
//! Java `com.hypixel.hytale.procedurallib.supplier` — the supplier interfaces
//! the world generator needs that the `procedural` crate does not port (yet).
//!
//! | Java class / interface                  | Rust item                            |
//! |-----------------------------------------|--------------------------------------|
//! | `IFloatRange`                           | [`IFloatRange`]                      |
//! | `FloatRange` (`Constant` / `Normal`)    | [`FloatRange`]                       |
//! | `FloatSupplier`                         | [`FloatSupplier`]                    |
//! | `IDoubleCoordinateHashSupplier`         | [`IDoubleCoordinateHashSupplier`]    |
//! | `ConstantDoubleCoordinateHashSupplier`  | [`ConstantDoubleCoordinateHashSupplier`] |
//! | `DoubleRangeCoordinateHashSupplier`     | [`DoubleRangeCoordinateHashSupplier`]    |
//!
//! ⚠ **SEAM(phase-4e)**: the canonical home of these ports is the
//! `procedural` crate (`procedural::noise::supplier`, which already carries
//! `IDoubleRange` and the coordinate suppliers). The `procedural` crate is out
//! of scope for the Phase-4a slice, so the float range (used by the yaw and
//! the pitch of a cave type) and the coordinate-hash supplier (used by the
//! cave prefab displacement) live here until they are moved.
//!
//! The port is 1:1 — the float/double precision of the Java code is kept
//! (`f32` stays `f32`).

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;
use procedural::noise::supplier::{
    IDoubleCoordinateSupplier2d, IDoubleCoordinateSupplier3d, IDoubleRange,
};

/// Java `procedurallib.supplier.FloatSupplier`.
pub trait FloatSupplier {
    /// Java `float getAsFloat()`.
    fn get_as_float(&self) -> f32;
}

/// Java `procedurallib.supplier.IFloatRange`.
///
/// Java's `getValue(FloatSupplier)` overload is kept as
/// [`IFloatRange::get_value_supplier`]; the coordinate overloads take the
/// matching `IDoubleCoordinateSupplier`.
pub trait IFloatRange {
    /// Java `float getValue(float v)`.
    fn get_value(&self, v: f32) -> f32;

    /// Java `float getValue(FloatSupplier supplier)`.
    fn get_value_supplier(&self, supplier: &dyn FloatSupplier) -> f32;

    /// Java `float getValue(Random random)`.
    fn get_value_random(&self, random: &mut FastRandom) -> f32;

    /// Java `float getValue(int seed, double x, double y,
    /// IDoubleCoordinateSupplier2d supplier)`.
    fn get_value_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        supplier: &dyn IDoubleCoordinateSupplier2d,
    ) -> f32;

    /// Java `float getValue(int seed, double x, double y, double z,
    /// IDoubleCoordinateSupplier3d supplier)`.
    fn get_value_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        supplier: &dyn IDoubleCoordinateSupplier3d,
    ) -> f32;
}

/// Java `procedurallib.supplier.FloatRange` — the `Constant` / `Normal` family
/// unified into one enum (Java uses nested classes with no other members).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FloatRange {
    /// Java `FloatRange.Constant`.
    Constant {
        /// Java `protected final float result`.
        result: f32,
    },
    /// Java `FloatRange.Normal`.
    Normal {
        /// Java `protected final float min`.
        min: f32,
        /// Java `protected final float range` (`max - min`).
        range: f32,
    },
}

impl FloatRange {
    /// Java `FloatRange.ZERO`.
    pub const ZERO: Self = Self::Constant { result: 0.0 };
    /// Java `FloatRange.ONE`.
    pub const ONE: Self = Self::Constant { result: 1.0 };

    /// Java `FloatRange.Constant(float result)`.
    pub const fn constant(result: f32) -> Self {
        Self::Constant { result }
    }

    /// Java `FloatRange.Normal(float min, float max)`.
    pub const fn normal(min: f32, max: f32) -> Self {
        Self::Normal {
            min,
            range: max - min,
        }
    }
}

impl IFloatRange for FloatRange {
    fn get_value(&self, _v: f32) -> f32 {
        match self {
            FloatRange::Constant { result } => *result,
            FloatRange::Normal { min, range } => min + range * _v,
        }
    }

    fn get_value_supplier(&self, supplier: &dyn FloatSupplier) -> f32 {
        match self {
            FloatRange::Constant { result } => *result,
            FloatRange::Normal { min, range } => min + range * supplier.get_as_float(),
        }
    }

    fn get_value_random(&self, random: &mut FastRandom) -> f32 {
        match self {
            FloatRange::Constant { result } => *result,
            // Java: `getValue(random.nextFloat())`.
            FloatRange::Normal { min, range } => min + range * random.next_float(),
        }
    }

    fn get_value_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        supplier: &dyn IDoubleCoordinateSupplier2d,
    ) -> f32 {
        match self {
            FloatRange::Constant { result } => *result,
            // Java: `min + range * (float) supplier.get(seed, x, y)`.
            FloatRange::Normal { min, range } => min + range * (supplier.get(seed, x, y) as f32),
        }
    }

    fn get_value_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        supplier: &dyn IDoubleCoordinateSupplier3d,
    ) -> f32 {
        match self {
            FloatRange::Constant { result } => *result,
            // Java: `min + range * (float) supplier.get(seed, x, y, z)`.
            FloatRange::Normal { min, range } => min + range * (supplier.get(seed, x, y, z) as f32),
        }
    }
}

/// Java `procedurallib.supplier.IDoubleCoordinateHashSupplier`.
///
/// `double get(int seed, int x, int y, long hash)`.
pub trait IDoubleCoordinateHashSupplier {
    /// Java `double get(int seed, int x, int y, long hash)`.
    fn get(&self, seed: i32, x: i32, y: i32, hash: i64) -> f64;
}

/// Java `procedurallib.supplier.ConstantDoubleCoordinateHashSupplier`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstantDoubleCoordinateHashSupplier {
    /// Java `protected final double result`.
    pub result: f64,
}

impl ConstantDoubleCoordinateHashSupplier {
    /// Java `ConstantDoubleCoordinateHashSupplier.ZERO`.
    pub const ZERO: Self = Self { result: 0.0 };
    /// Java `ConstantDoubleCoordinateHashSupplier.ONE`.
    pub const ONE: Self = Self { result: 1.0 };

    /// Java `ConstantDoubleCoordinateHashSupplier(double result)`.
    pub const fn new(result: f64) -> Self {
        Self { result }
    }

    /// Java `double getResult()`.
    pub fn get_result(&self) -> f64 {
        self.result
    }
}

impl IDoubleCoordinateHashSupplier for ConstantDoubleCoordinateHashSupplier {
    #[inline]
    fn get(&self, _seed: i32, _x: i32, _y: i32, _hash: i64) -> f64 {
        self.result
    }
}

/// Java `procedurallib.supplier.DoubleRangeCoordinateHashSupplier`.
pub struct DoubleRangeCoordinateHashSupplier {
    /// Java `protected final IDoubleRange range`.
    pub range: Rc<dyn IDoubleRange>,
}

impl DoubleRangeCoordinateHashSupplier {
    /// Java `DoubleRangeCoordinateHashSupplier(IDoubleRange range)`.
    pub fn new(range: Rc<dyn IDoubleRange>) -> Self {
        Self { range }
    }
}

impl IDoubleCoordinateHashSupplier for DoubleRangeCoordinateHashSupplier {
    fn get(&self, seed: i32, x: i32, y: i32, hash: i64) -> f64 {
        // Java: `range.getValue(HashUtil.random(seed, x, y, hash))`.
        self.range
            .get_value(hash_util::random4(seed as i64, x as i64, y as i64, hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::supplier::NoisePropertySupplier2d;

    /// A `NoiseProperty` that always evaluates to a constant.
    struct ConstantNoise(f64);

    impl procedural::noise::support::NoiseProperty for ConstantNoise {
        fn get(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }
        fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    #[test]
    fn the_constant_range_ignores_its_input() {
        let range = FloatRange::constant(2.5);
        assert_eq!(range.get_value(0.0), 2.5);
        assert_eq!(range.get_value(-7.5), 2.5);
        assert_eq!(range.get_value_random(&mut FastRandom::new(1)), 2.5);
    }

    #[test]
    fn the_normal_range_interpolates() {
        let range = FloatRange::normal(10.0, 20.0);
        assert_eq!(range.get_value(0.0), 10.0);
        assert_eq!(range.get_value(0.5), 15.0);
        assert_eq!(range.get_value(1.0), 20.0);
    }

    #[test]
    fn the_normal_range_uses_the_float_draw_of_the_random() {
        let range = FloatRange::normal(0.0, 1.0);
        let mut random = FastRandom::new(42);
        let drawn = random.next_float();
        let mut reference = FastRandom::new(42);
        assert_eq!(range.get_value_random(&mut reference), drawn);
        // The random source advanced by exactly one draw.
        let mut check = FastRandom::new(42);
        let _ = check.next_float();
        assert_eq!(reference.next_float(), check.next_float());
    }

    #[test]
    fn the_coordinate_overloads_cast_the_double_to_float() {
        let range = FloatRange::normal(0.0, 2.0);
        let noise = ConstantNoise(0.25);
        let supplier = NoisePropertySupplier2d(&noise);
        assert_eq!(range.get_value_2d(0, 0.0, 0.0, &supplier), 0.5);
    }

    #[test]
    fn the_constant_hash_supplier_ignores_its_input() {
        let supplier = ConstantDoubleCoordinateHashSupplier::new(3.5);
        assert_eq!(supplier.get(1, 2, 3, 4), 3.5);
        assert_eq!(
            ConstantDoubleCoordinateHashSupplier::ZERO.get(0, 0, 0, 0),
            0.0
        );
        assert_eq!(
            ConstantDoubleCoordinateHashSupplier::ONE.get(0, 0, 0, 0),
            1.0
        );
    }

    #[test]
    fn the_double_range_hash_supplier_hashes_the_coordinates() {
        // The `Normal` range maps the hashed value to `[0, 1)`.
        let range = Rc::new(procedural::noise::supplier::DoubleRange::normal(0.0, 1.0));
        let supplier = DoubleRangeCoordinateHashSupplier::new(range);
        let expected = hash_util::random4(1, 2, 3, 4);
        assert_eq!(supplier.get(1, 2, 3, 4), expected);
    }
}
