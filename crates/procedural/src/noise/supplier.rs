// Ported from: `.hytale-source/.../procedurallib/supplier/*.java`
//! Java `procedurallib.supplier` package — range value suppliers.
//!
//! Only the members of the Java files that are used by the world generator
//! are declared; the `DoubleSupplier` / `FloatSupplier` overloads stay
//! available ([`IDoubleRange::get_value`] callers pass the produced value,
//! the float range keeps its [`FloatSupplier`] overload).
//!
//! | Java class / interface                  | Rust item                            |
//! |-----------------------------------------|--------------------------------------|
//! | `IDoubleRange`                          | [`IDoubleRange`]                     |
//! | `DoubleRange` (`Constant`/`Normal`/`Multiple`) | [`DoubleRange`]               |
//! | `DoubleRangeNoiseSupplier`              | [`DoubleRangeNoiseSupplier`]         |
//! | `IFloatRange`                           | [`IFloatRange`]                      |
//! | `FloatRange` (`Constant` / `Normal`)    | [`FloatRange`]                       |
//! | `FloatSupplier`                         | [`FloatSupplier`]                    |
//! | `IDoubleCoordinateHashSupplier`         | [`IDoubleCoordinateHashSupplier`]    |
//! | `ConstantDoubleCoordinateHashSupplier`  | [`ConstantDoubleCoordinateHashSupplier`] |
//! | `DoubleRangeCoordinateHashSupplier`     | [`DoubleRangeCoordinateHashSupplier`]    |
//!
//! The float range keeps the Java precision (`f32` stays `f32`).

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;

use crate::noise::property::NoiseProperty;

// ---------------------------------------------------------------------
// supplier interfaces
// ---------------------------------------------------------------------

/// Java `supplier.IDoubleCoordinateSupplier2d` — `double get(int seed, double x, double y)`.
pub trait IDoubleCoordinateSupplier2d {
    /// Java `get(int seed, double x, double y)`.
    fn get(&self, seed: i32, x: f64, y: f64) -> f64;
}

/// Java `supplier.IDoubleCoordinateSupplier3d` — `double get(int seed, double x, double y, double z)`.
pub trait IDoubleCoordinateSupplier3d {
    /// Java `get(int seed, double x, double y, double z)`.
    fn get(&self, seed: i32, x: f64, y: f64, z: f64) -> f64;
}

/// Java `supplier.IDoubleCoordinateSupplier` — the 2d + 3d double-supplier.
pub trait IDoubleCoordinateSupplier {
    /// Java `get(int seed, double x, double y)`.
    fn get2(&self, seed: i32, x: f64, y: f64) -> f64;
    /// Java `get(int seed, double x, double y, double z)`.
    fn get3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64;
}

/// Java `supplier.IDoubleRange`.
///
/// Java's `getValue(DoubleSupplier)` overload collapses into
/// [`IDoubleRange::get_value`] (callers pass the produced value);
/// `getValue(Random)` becomes [`IDoubleRange::get_value_random`].
pub trait IDoubleRange {
    /// Java `getValue(double v)`.
    fn get_value(&self, v: f64) -> f64;
    /// Java `getValue(Random random)`.
    fn get_value_random(&self, random: &mut FastRandom) -> f64;
    /// Java `getValue(int seed, double x, double y, IDoubleCoordinateSupplier2d supplier)`.
    fn get_value_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        supplier: &dyn IDoubleCoordinateSupplier2d,
    ) -> f64;
    /// Java `getValue(int seed, double x, double y, double z, IDoubleCoordinateSupplier3d supplier)`.
    fn get_value_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        supplier: &dyn IDoubleCoordinateSupplier3d,
    ) -> f64;
}

/// Java `supplier.ISeedDoubleRange` — `double getValue(int seed, double value)`.
pub trait ISeedDoubleRange {
    /// Java `ISeedDoubleRange.getValue(int seed, double value)`.
    fn get_value(&self, seed: i32, value: f64) -> f64;
}

/// Java `ISeedDoubleRange.DIRECT` — identity range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectSeedDoubleRange;

impl ISeedDoubleRange for DirectSeedDoubleRange {
    #[inline]
    fn get_value(&self, _seed: i32, value: f64) -> f64 {
        value
    }
}

// ---------------------------------------------------------------------
// method-reference adapters (Java: `noiseProperty::get` used as a supplier)
// ---------------------------------------------------------------------

/// Adapter letting a `NoiseProperty` act as an `IDoubleCoordinateSupplier2d`
/// (Java: the `this.noiseSupplier = noise::get` method reference).
pub struct NoisePropertySupplier2d<'a>(pub &'a dyn NoiseProperty);

impl IDoubleCoordinateSupplier2d for NoisePropertySupplier2d<'_> {
    #[inline]
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.0.get(seed, x, y)
    }
}

/// Adapter letting a `NoiseProperty` act as an `IDoubleCoordinateSupplier3d`
/// (Java: the `this.noiseSupplier = noise::get` method reference).
pub struct NoisePropertySupplier3d<'a>(pub &'a dyn NoiseProperty);

impl IDoubleCoordinateSupplier3d for NoisePropertySupplier3d<'_> {
    #[inline]
    fn get(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.0.get_3d(seed, x, y, z)
    }
}

// ---------------------------------------------------------------------
// DoubleRange
// ---------------------------------------------------------------------

/// Java `DoubleRange` — the `Constant` / `Normal` / `Multiple` family unified
/// into one enum (Java uses nested classes with no other members).
#[derive(Debug, Clone, PartialEq)]
pub enum DoubleRange {
    /// Java `DoubleRange.Constant`.
    Constant {
        /// Java `protected final double result`.
        result: f64,
    },
    /// Java `DoubleRange.Normal`.
    Normal {
        /// Java `protected final double min`.
        min: f64,
        /// Java `protected final double range` (`max - min`).
        range: f64,
    },
    /// Java `DoubleRange.Multiple`.
    Multiple {
        /// Java `protected final double[] thresholds`.
        thresholds: Vec<f64>,
        /// Java `protected final double[] values`.
        values: Vec<f64>,
    },
}

impl DoubleRange {
    /// Java `DoubleRange.ZERO`.
    pub const ZERO: Self = Self::Constant { result: 0.0 };
    /// Java `DoubleRange.ONE`.
    pub const ONE: Self = Self::Constant { result: 1.0 };

    /// Java `DoubleRange.Constant(double result)`.
    pub const fn constant(result: f64) -> Self {
        Self::Constant { result }
    }

    /// Java `DoubleRange.Normal(double min, double max)`.
    pub const fn normal(min: f64, max: f64) -> Self {
        Self::Normal {
            min,
            range: max - min,
        }
    }

    /// Java `DoubleRange.Multiple(double[] thresholds, double[] values)`.
    pub fn multiple(thresholds: Vec<f64>, values: Vec<f64>) -> Self {
        Self::Multiple { thresholds, values }
    }

    /// Java `Constant.getResult()`.
    pub const fn get_result(&self) -> f64 {
        match self {
            Self::Constant { result } => *result,
            _ => panic!("getResult() on non-Constant DoubleRange"),
        }
    }

    /// Java `Normal.getMin()`.
    pub const fn get_min(&self) -> f64 {
        match self {
            Self::Normal { min, .. } => *min,
            _ => panic!("getMin() on non-Normal DoubleRange"),
        }
    }

    /// Java `Normal.getRange()`.
    pub const fn get_range(&self) -> f64 {
        match self {
            Self::Normal { range, .. } => *range,
            _ => panic!("getRange() on non-Normal DoubleRange"),
        }
    }
}

impl IDoubleRange for DoubleRange {
    fn get_value(&self, v: f64) -> f64 {
        match self {
            Self::Constant { result } => *result,
            Self::Normal { min, range } => min + range * v,
            Self::Multiple { thresholds, values } => multiple_get_value(thresholds, values, v),
        }
    }

    fn get_value_random(&self, random: &mut FastRandom) -> f64 {
        match self {
            // Java: Constant.getValue(Random) returns the result without
            // consuming the random source.
            Self::Constant { result } => *result,
            Self::Normal { min, range } => min + range * random.next_double(),
            Self::Multiple { thresholds, values } => {
                multiple_get_value(thresholds, values, random.next_double())
            }
        }
    }

    fn get_value_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        supplier: &dyn IDoubleCoordinateSupplier2d,
    ) -> f64 {
        match self {
            Self::Constant { result } => *result,
            Self::Normal { min, range } => min + range * supplier.get(seed, x, y),
            Self::Multiple { thresholds, values } => {
                multiple_get_value(thresholds, values, supplier.get(seed, x, y))
            }
        }
    }

    fn get_value_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        supplier: &dyn IDoubleCoordinateSupplier3d,
    ) -> f64 {
        match self {
            Self::Constant { result } => *result,
            Self::Normal { min, range } => min + range * supplier.get(seed, x, y, z),
            Self::Multiple { thresholds, values } => {
                multiple_get_value(thresholds, values, supplier.get(seed, x, y, z))
            }
        }
    }
}

/// Java `DoubleRange.Multiple.getValue(double v)` — exact transcription
/// (including the early-exit ordering and the `0` fall-through).
fn multiple_get_value(thresholds: &[f64], values: &[f64], v: f64) -> f64 {
    if v > thresholds[thresholds.len() - 1] {
        return values[values.len() - 1];
    }

    let mut min = 0.0;

    for i in 0..thresholds.len() {
        let max = thresholds[i];

        if v < max {
            if i == 0 {
                return values[0];
            }

            let alpha = (v - min) / (max - min);
            let value_min = values[i - 1];
            let value_max = values[i];
            let range = value_max - value_min;
            return value_min + alpha * range;
        }

        min = max;
    }

    0.0
}

// ---------------------------------------------------------------------
// DoubleRangeNoiseSupplier
// ---------------------------------------------------------------------

/// Java `supplier.DoubleRangeNoiseSupplier`.
pub struct DoubleRangeNoiseSupplier {
    /// Java `protected final IDoubleRange range`.
    pub range: Rc<dyn IDoubleRange>,
    /// Java `protected final NoiseProperty noiseProperty` (non-null).
    pub noise_property: Rc<dyn NoiseProperty>,
}

impl IDoubleCoordinateSupplier for DoubleRangeNoiseSupplier {
    fn get2(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.range.get_value_2d(
            seed,
            x,
            y,
            &NoisePropertySupplier2d(self.noise_property.as_ref()),
        )
    }

    fn get3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.range.get_value_3d(
            seed,
            x,
            y,
            z,
            &NoisePropertySupplier3d(self.noise_property.as_ref()),
        )
    }
}

// ---------------------------------------------------------------------
// float range
// ---------------------------------------------------------------------

/// Java `supplier.FloatSupplier`.
pub trait FloatSupplier {
    /// Java `float getAsFloat()`.
    fn get_as_float(&self) -> f32;
}

/// Java `supplier.IFloatRange`.
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

/// Java `supplier.FloatRange` — the `Constant` / `Normal` family unified into
/// one enum (Java uses nested classes with no other members).
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
    fn get_value(&self, v: f32) -> f32 {
        match self {
            FloatRange::Constant { result } => *result,
            FloatRange::Normal { min, range } => min + range * v,
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

// ---------------------------------------------------------------------
// coordinate-hash suppliers
// ---------------------------------------------------------------------

/// Java `supplier.IDoubleCoordinateHashSupplier`.
///
/// `double get(int seed, int x, int y, long hash)`.
pub trait IDoubleCoordinateHashSupplier {
    /// Java `double get(int seed, int x, int y, long hash)`.
    fn get(&self, seed: i32, x: i32, y: i32, hash: i64) -> f64;
}

/// Java `supplier.ConstantDoubleCoordinateHashSupplier`.
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

/// Java `supplier.DoubleRangeCoordinateHashSupplier`.
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
    use hytale_math::fast_random::FastRandom;

    #[test]
    fn constant_ignores_input() {
        let range = DoubleRange::constant(0.25);
        assert_eq!(range.get_value(0.0), 0.25);
        assert_eq!(range.get_value(0.9), 0.25);
        assert_eq!(range.get_value(f64::NAN), 0.25);
    }

    #[test]
    fn normal_interpolates() {
        let range = DoubleRange::normal(1.0, 3.0);
        assert_eq!(range.get_value(0.0), 1.0);
        assert_eq!(range.get_value(0.5), 2.0);
        assert_eq!(range.get_value(1.0), 3.0);
    }

    #[test]
    fn multiple_segment_lerp() {
        // thresholds [1, 2], values [10, 20]: below 1 → 10, above 2 → 20,
        // between → linear. Mirrors the Java `Multiple` walk.
        let range = DoubleRange::multiple(vec![1.0, 2.0], vec![10.0, 20.0]);
        assert_eq!(range.get_value(0.5), 10.0);
        assert_eq!(range.get_value(2.5), 20.0);
        assert_eq!(range.get_value(1.25), 12.5);
        assert_eq!(range.get_value(1.0), 10.0);
        // v == 2.0 is not < 2.0 → loop ends via min update, returns 0.
        assert_eq!(range.get_value(2.0), 0.0);
    }

    #[test]
    fn normal_random_uses_single_draw() {
        let range = DoubleRange::normal(0.0, 10.0);
        let mut random = FastRandom::new(42);
        let v = range.get_value_random(&mut random);
        let mut reference = FastRandom::new(42);
        let d = reference.next_double();
        // Java: min + range * random.nextDouble() — one draw.
        assert_eq!(v, 10.0 * d);
    }

    #[test]
    fn constant_random_does_not_consume() {
        let range = DoubleRange::ZERO;
        let mut random = FastRandom::new(42);
        let v = range.get_value_random(&mut random);
        assert_eq!(v, 0.0);
        let first = random.next_double();
        let mut random2 = FastRandom::new(42);
        assert_eq!(first, random2.next_double());
    }

    #[test]
    fn noise_supplier_2d() {
        use crate::noise::constant_noise::ConstantNoise;
        use crate::noise::function::NoiseFunction;
        use crate::noise::property::single_noise_property::SingleNoiseProperty;

        let function: std::rc::Rc<dyn NoiseFunction> = std::rc::Rc::new(ConstantNoise::new(0.5));
        let property: std::rc::Rc<dyn NoiseProperty> =
            std::rc::Rc::new(SingleNoiseProperty::of_function(function));
        let range = DoubleRange::normal(2.0, 4.0);
        let supplier = NoisePropertySupplier2d(property.as_ref());
        // ConstantNoise(0.5) remapped by SingleNoiseProperty: 0.5*0.5+0.5 = 0.75
        // → 2 + 2*0.75 = 3.5.
        assert_eq!(range.get_value_2d(1, 3.0, 7.0, &supplier), 3.5);
    }

    // --- float range (ported from `worldgen-hytale::supplier`) ---

    /// A `NoiseProperty` that always evaluates to a constant.
    struct ConstantProperty(f64);

    impl NoiseProperty for ConstantProperty {
        fn get(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }
        fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    #[test]
    fn the_constant_float_range_ignores_its_input() {
        let range = FloatRange::constant(2.5);
        assert_eq!(range.get_value(0.0), 2.5);
        assert_eq!(range.get_value(-7.5), 2.5);
        assert_eq!(range.get_value_random(&mut FastRandom::new(1)), 2.5);
    }

    #[test]
    fn the_normal_float_range_interpolates() {
        let range = FloatRange::normal(10.0, 20.0);
        assert_eq!(range.get_value(0.0), 10.0);
        assert_eq!(range.get_value(0.5), 15.0);
        assert_eq!(range.get_value(1.0), 20.0);
    }

    #[test]
    fn the_normal_float_range_uses_the_float_draw_of_the_random() {
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
    fn the_float_coordinate_overloads_cast_the_double_to_float() {
        let range = FloatRange::normal(0.0, 2.0);
        let property = ConstantProperty(0.25);
        let supplier = NoisePropertySupplier2d(&property);
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
        let range = Rc::new(DoubleRange::normal(0.0, 1.0));
        let supplier = DoubleRangeCoordinateHashSupplier::new(range);
        let expected = hytale_math::hash_util::random4(1, 2, 3, 4);
        assert_eq!(supplier.get(1, 2, 3, 4), expected);
    }
}
