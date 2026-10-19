// Ported from: `.hytale-source/.../procedurallib/condition/*.java` and
// `.../procedurallib/util/IntToIntFunction.java`
//! Java `procedurallib.condition` package — value/coordinate/height-threshold
//! conditions.
//!
//! The `IIntCondition` / `IDoubleCondition` traits live in
//! [`crate::noise::support`]; this package provides the concrete classes.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;

use crate::noise::general_noise::fast_floor;
use crate::noise::support::{
    IDoubleCondition, IDoubleThreshold, IIntCondition, NoiseProperty,
};

// ---------------------------------------------------------------------
// int conditions
// ---------------------------------------------------------------------

/// Java `condition.ConstantIntCondition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConstantIntCondition {
    /// Java `protected final boolean result`.
    pub result: bool,
}

impl ConstantIntCondition {
    /// Java `ConstantIntCondition.DEFAULT_TRUE`.
    pub const DEFAULT_TRUE: Self = Self { result: true };
    /// Java `ConstantIntCondition.DEFAULT_FALSE`.
    pub const DEFAULT_FALSE: Self = Self { result: false };

    /// Java `ConstantIntCondition(boolean)`.
    pub fn new(result: bool) -> Self {
        Self { result }
    }

    /// Java `getResult()`.
    pub fn get_result(&self) -> bool {
        self.result
    }
}

impl IIntCondition for ConstantIntCondition {
    #[inline]
    fn eval(&self, _value: i32) -> bool {
        self.result
    }
}

// ---------------------------------------------------------------------
// double conditions
// ---------------------------------------------------------------------

/// Java `condition.DefaultDoubleCondition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultDoubleCondition {
    /// Java `protected final boolean result`.
    pub result: bool,
}

impl DefaultDoubleCondition {
    /// Java `DefaultDoubleCondition.DEFAULT_TRUE`.
    pub const DEFAULT_TRUE: Self = Self { result: true };
    /// Java `DefaultDoubleCondition.DEFAULT_FALSE`.
    pub const DEFAULT_FALSE: Self = Self { result: false };

    /// Java `DefaultDoubleCondition(boolean)`.
    pub fn new(result: bool) -> Self {
        Self { result }
    }

    /// Java `getResult()`.
    pub fn get_result(&self) -> bool {
        self.result
    }
}

impl IDoubleCondition for DefaultDoubleCondition {
    #[inline]
    fn eval(&self, _value: f64) -> bool {
        self.result
    }
}

/// Java `condition.SingleDoubleCondition` — `value < this.value`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SingleDoubleCondition {
    /// Java `protected final double value`.
    pub value: f64,
}

impl SingleDoubleCondition {
    /// Java `SingleDoubleCondition(double)`.
    pub fn new(value: f64) -> Self {
        Self { value }
    }
}

impl IDoubleCondition for SingleDoubleCondition {
    #[inline]
    fn eval(&self, value: f64) -> bool {
        value < self.value
    }
}

/// Java `condition.DoubleThresholdCondition` — delegates to a threshold.
pub struct DoubleThresholdCondition {
    /// Java `protected final IDoubleThreshold threshold`.
    pub threshold: Rc<dyn IDoubleThreshold>,
}

impl DoubleThresholdCondition {
    /// Java `DoubleThresholdCondition(IDoubleThreshold)`.
    pub fn new(threshold: Rc<dyn IDoubleThreshold>) -> Self {
        Self { threshold }
    }
}

impl IDoubleCondition for DoubleThresholdCondition {
    #[inline]
    fn eval(&self, value: f64) -> bool {
        self.threshold.eval(value)
    }
}

// ---------------------------------------------------------------------
// thresholds
// ---------------------------------------------------------------------

/// Java `condition.DefaultDoubleThresholdCondition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultDoubleThresholdCondition {
    /// Java `protected final boolean result`.
    pub result: bool,
}

impl DefaultDoubleThresholdCondition {
    /// Java `DEFAULT_TRUE`.
    pub const DEFAULT_TRUE: Self = Self { result: true };
    /// Java `DEFAULT_FALSE`.
    pub const DEFAULT_FALSE: Self = Self { result: false };

    /// Java `DefaultDoubleThresholdCondition(boolean)`.
    pub fn new(result: bool) -> Self {
        Self { result }
    }

    /// Java `isResult()`.
    pub fn is_result(&self) -> bool {
        self.result
    }
}

impl IDoubleThreshold for DefaultDoubleThresholdCondition {
    #[inline]
    fn eval(&self, _d: f64) -> bool {
        self.result
    }
    #[inline]
    fn eval_factor(&self, _d: f64, _factor: f64) -> bool {
        self.result
    }
}

/// Java `condition.DoubleThreshold.Single`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoubleThresholdSingle {
    /// Java `protected final double min`.
    pub min: f64,
    /// Java `protected final double max`.
    pub max: f64,
    /// Java `protected final double halfRange` — `(max - min) * 0.5`.
    pub half_range: f64,
}

impl DoubleThresholdSingle {
    /// Java `Single(double min, double max)`.
    pub fn new(min: f64, max: f64) -> Self {
        Self {
            min,
            max,
            half_range: (max - min) * 0.5,
        }
    }
}

impl IDoubleThreshold for DoubleThresholdSingle {
    #[inline]
    fn eval(&self, d: f64) -> bool {
        self.min <= d && d <= self.max
    }

    fn eval_factor(&self, d: f64, factor: f64) -> bool {
        let t0 = self.min + self.half_range * factor;
        let t1 = self.max - self.half_range * factor;
        t0 <= d && d <= t1
    }
}

/// Java `condition.DoubleThreshold.Multiple`.
pub struct DoubleThresholdMultiple {
    /// Java `protected final Single[] singles`.
    pub singles: Vec<DoubleThresholdSingle>,
}

impl DoubleThresholdMultiple {
    /// Java `Multiple(Single[])`.
    pub fn new(singles: Vec<DoubleThresholdSingle>) -> Self {
        Self { singles }
    }
}

impl IDoubleThreshold for DoubleThresholdMultiple {
    fn eval(&self, d: f64) -> bool {
        self.singles.iter().any(|s| s.eval(d))
    }

    fn eval_factor(&self, d: f64, factor: f64) -> bool {
        self.singles.iter().any(|s| s.eval_factor(d, factor))
    }
}

// ---------------------------------------------------------------------
// coordinate conditions
// ---------------------------------------------------------------------

/// Java `condition.ICoordinateCondition`.
pub trait ICoordinateCondition {
    /// Java `eval(int seed, int x, int y)`.
    fn eval2(&self, seed: i32, x: i32, y: i32) -> bool;
    /// Java `eval(int seed, int x, int y, int z)`.
    fn eval3(&self, seed: i32, x: i32, y: i32, z: i32) -> bool;
}

/// Java `condition.DefaultCoordinateCondition` (private ctor + statics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultCoordinateCondition {
    /// Java `DEFAULT_TRUE`.
    True,
    /// Java `DEFAULT_FALSE`.
    False,
}

impl ICoordinateCondition for DefaultCoordinateCondition {
    #[inline]
    fn eval2(&self, _seed: i32, _x: i32, _y: i32) -> bool {
        matches!(self, Self::True)
    }
    #[inline]
    fn eval3(&self, _seed: i32, _x: i32, _y: i32, _z: i32) -> bool {
        matches!(self, Self::True)
    }
}

/// Java `condition.ICoordinateRndCondition` — `Random` becomes [`FastRandom`]
/// (Java worldgen passes a `FastRandom`, which *is* a `java.util.Random`).
pub trait ICoordinateRndCondition {
    /// Java `eval(int seed, int x, int z, int y, Random random)`.
    fn eval(&self, seed: i32, x: i32, z: i32, y: i32, random: &mut FastRandom) -> bool;
}

/// Java `condition.DefaultCoordinateRndCondition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultCoordinateRndCondition {
    /// Java `DEFAULT_TRUE`.
    True,
    /// Java `DEFAULT_FALSE`.
    False,
}

impl ICoordinateRndCondition for DefaultCoordinateRndCondition {
    #[inline]
    fn eval(&self, _seed: i32, _x: i32, _z: i32, _y: i32, _random: &mut FastRandom) -> bool {
        matches!(self, Self::True)
    }
}

/// Java `condition.HeightCondition` — height-threshold gated by a random roll.
pub struct HeightCondition {
    /// Java `protected final IHeightThresholdInterpreter interpreter`.
    pub interpreter: Rc<dyn IHeightThresholdInterpreter>,
}

impl HeightCondition {
    /// Java `HeightCondition(IHeightThresholdInterpreter)`.
    pub fn new(interpreter: Rc<dyn IHeightThresholdInterpreter>) -> Self {
        Self { interpreter }
    }
}

impl ICoordinateRndCondition for HeightCondition {
    fn eval(&self, seed: i32, x: i32, z: i32, y: i32, random: &mut FastRandom) -> bool {
        let threshold = self.interpreter.get_threshold(seed, x as f64, z as f64, y);
        // Java: `threshold > random.nextDouble()` — float compared against double
        // widens the float.
        threshold > 0.0 && (threshold >= 1.0 || (threshold as f64) > random.next_double())
    }
}

/// Java `condition.HeightThresholdCoordinateCondition`.
pub struct HeightThresholdCoordinateCondition {
    /// Java `private final IHeightThresholdInterpreter interpreter`.
    pub interpreter: Rc<dyn IHeightThresholdInterpreter>,
}

impl HeightThresholdCoordinateCondition {
    /// Java `HeightThresholdCoordinateCondition(IHeightThresholdInterpreter)`.
    pub fn new(interpreter: Rc<dyn IHeightThresholdInterpreter>) -> Self {
        Self { interpreter }
    }
}

impl ICoordinateCondition for HeightThresholdCoordinateCondition {
    fn eval2(&self, _seed: i32, _x: i32, _y: i32) -> bool {
        // Java: throw new UnsupportedOperationException("This needs a height to operate.")
        panic!("This needs a height to operate.");
    }

    fn eval3(&self, seed: i32, x: i32, y: i32, z: i32) -> bool {
        // Java: HashUtil.random(seed, fastFloor(x), fastFloor(y), fastFloor(z))
        // (ints widen to the long overload)
        self.interpreter.get_threshold(seed, x as f64, z as f64, y)
            >= hash_util::random4(
                seed as i64,
                fast_floor(x as f64) as i64,
                fast_floor(y as f64) as i64,
                fast_floor(z as f64) as i64,
            ) as f32
    }
}

/// Java `condition.NoiseMaskCondition`.
pub struct NoiseMaskCondition {
    /// Java `protected final NoiseProperty noiseMask`.
    pub noise_mask: Rc<dyn NoiseProperty>,
    /// Java `protected final IDoubleCondition condition`.
    pub condition: Rc<dyn IDoubleCondition>,
}

impl NoiseMaskCondition {
    /// Java `NoiseMaskCondition(NoiseProperty, IDoubleCondition)`.
    pub fn new(noise_mask: Rc<dyn NoiseProperty>, condition: Rc<dyn IDoubleCondition>) -> Self {
        Self {
            noise_mask,
            condition,
        }
    }
}

impl ICoordinateCondition for NoiseMaskCondition {
    fn eval2(&self, seed: i32, x: i32, y: i32) -> bool {
        self.condition
            .eval(self.noise_mask.get(seed, x as f64, y as f64))
    }

    fn eval3(&self, seed: i32, x: i32, y: i32, z: i32) -> bool {
        self.condition
            .eval(self.noise_mask.get_3d(seed, x as f64, y as f64, z as f64))
    }
}

// ---------------------------------------------------------------------
// block/fluid conditions
// ---------------------------------------------------------------------

/// Java `condition.IBlockFluidCondition`.
pub trait IBlockFluidCondition {
    /// Java `eval(int block, int fluid)`.
    fn eval(&self, block: i32, fluid: i32) -> bool;
}

/// Java `condition.ConstantBlockFluidCondition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstantBlockFluidCondition {
    /// Java `DEFAULT_TRUE`.
    True,
    /// Java `DEFAULT_FALSE`.
    False,
}

impl IBlockFluidCondition for ConstantBlockFluidCondition {
    #[inline]
    fn eval(&self, _block: i32, _fluid: i32) -> bool {
        matches!(self, Self::True)
    }
}

// ---------------------------------------------------------------------
// height threshold interpreters
// ---------------------------------------------------------------------

/// Java `condition.IHeightThresholdInterpreter`.
pub trait IHeightThresholdInterpreter {
    /// Java `getLowestNonOne()`.
    fn get_lowest_non_one(&self) -> i32;
    /// Java `getHighestNonZero()`.
    fn get_highest_non_zero(&self) -> i32;
    /// Java `getThreshold(int seed, double x, double z, int height)`.
    fn get_threshold(&self, seed: i32, x: f64, z: f64, height: i32) -> f32;
    /// Java `getThreshold(int seed, double x, double z, int height, double context)`.
    fn get_threshold_with_context(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        height: i32,
        context: f64,
    ) -> f32;
    /// Java `getContext(int seed, double x, double z)`.
    fn get_context(&self, seed: i32, x: f64, z: f64) -> f64;
    /// Java `getLength()`.
    fn get_length(&self) -> i32;

    /// Java default `isSpawnable(int height)`.
    fn is_spawnable(&self, height: i32) -> bool {
        height >= self.get_lowest_non_one() && height <= self.get_highest_non_zero()
    }
}

/// Java static `IHeightThresholdInterpreter.lerp(float, float, float)`.
#[inline]
pub fn height_threshold_lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Java `condition.BasicHeightThresholdInterpreter`.
pub struct BasicHeightThresholdInterpreter {
    /// Java `protected final float[] interpolatedThresholds`.
    pub interpolated_thresholds: Vec<f32>,
    /// Java `protected final int lowestNonOne`.
    lowest_non_one: i32,
    /// Java `protected final int highestNonZero`.
    highest_non_zero: i32,
}

impl BasicHeightThresholdInterpreter {
    /// Java `BasicHeightThresholdInterpreter(int[] positions, float[] thresholds, int length)`.
    pub fn new(positions: &[i32], thresholds: &[f32], length: i32) -> Self {
        if positions.len() != thresholds.len() {
            panic!(
                "Mismatching array lengths! positions: {}, thresholds: {}",
                positions.len(),
                thresholds.len()
            );
        }
        let length = length as usize;

        let mut interpolated = vec![0f32; length];
        for (y, slot) in interpolated.iter_mut().enumerate() {
            let y = y as i32;
            let mut threshold = thresholds[thresholds.len() - 1];
            for (i, &pos) in positions.iter().enumerate() {
                if y < pos {
                    if i == 0 {
                        threshold = thresholds[i];
                    } else {
                        let distance = (y - positions[i - 1]) as f32 / (pos - positions[i - 1]) as f32;
                        threshold = height_threshold_lerp(thresholds[i - 1], thresholds[i], distance);
                    }
                    break;
                }
            }
            *slot = threshold;
        }

        // Java: scan up for the first index whose threshold < 1
        let mut lowest_non_one = 0i32;
        while (lowest_non_one as usize) < length {
            if interpolated[lowest_non_one as usize] < 1.0 {
                break;
            }
            lowest_non_one += 1;
        }

        // Java: scan down for the last index whose threshold > 0
        let mut highest_non_zero = length as i32 - 1;
        while highest_non_zero >= 0 {
            if interpolated[highest_non_zero as usize] > 0.0 {
                break;
            }
            highest_non_zero -= 1;
        }

        Self {
            interpolated_thresholds: interpolated,
            lowest_non_one,
            highest_non_zero,
        }
    }
}

impl IHeightThresholdInterpreter for BasicHeightThresholdInterpreter {
    fn get_lowest_non_one(&self) -> i32 {
        self.lowest_non_one
    }

    fn get_highest_non_zero(&self) -> i32 {
        self.highest_non_zero
    }

    fn get_context(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
        0.0
    }

    fn get_length(&self) -> i32 {
        self.interpolated_thresholds.len() as i32
    }

    fn get_threshold(&self, seed: i32, x: f64, y: f64, height: i32) -> f32 {
        let context = self.get_context(seed, x, y);
        self.get_threshold_with_context(seed, x, y, height, context)
    }

    fn get_threshold_with_context(&self, _seed: i32, _x: f64, _y: f64, height: i32, _context: f64) -> f32 {
        if height > self.highest_non_zero {
            return 0.0;
        }
        let mut height = height;
        if height < 0 {
            height = 0;
        }
        if height as usize >= self.interpolated_thresholds.len() {
            height = self.interpolated_thresholds.len() as i32 - 1;
        }
        self.interpolated_thresholds[height as usize]
    }
}

/// Java `condition.NoiseHeightThresholdInterpreter`.
pub struct NoiseHeightThresholdInterpreter {
    /// Java `protected final NoiseProperty noise`.
    pub noise: Rc<dyn NoiseProperty>,
    /// Java `protected final float[] keys`.
    pub keys: Vec<f32>,
    /// Java `protected final IHeightThresholdInterpreter[] values`.
    pub values: Vec<Rc<dyn IHeightThresholdInterpreter>>,
    /// Java `protected final int length`.
    length: i32,
    /// Java `protected final int lowestNonOne`.
    lowest_non_one: i32,
    /// Java `protected final int highestNonZero`.
    highest_non_zero: i32,
}

impl NoiseHeightThresholdInterpreter {
    /// Java `NoiseHeightThresholdInterpreter(NoiseProperty, float[] keys,
    /// IHeightThresholdInterpreter[] values)`.
    pub fn new(
        noise: Rc<dyn NoiseProperty>,
        keys: Vec<f32>,
        values: Vec<Rc<dyn IHeightThresholdInterpreter>>,
    ) -> Self {
        if keys.len() != values.len() {
            panic!("Length of keys and values are different!");
        }
        Self::check_interpreter_length(&values);

        let length = values[0].get_length();

        let mut lowest_non_one = length - 1;
        for value in &values {
            if value.get_lowest_non_one() < lowest_non_one {
                lowest_non_one = value.get_lowest_non_one();
            }
        }

        let mut highest_non_zero = 0;
        for value in &values {
            if value.get_highest_non_zero() > highest_non_zero {
                highest_non_zero = value.get_highest_non_zero();
            }
        }

        Self {
            noise,
            keys,
            values,
            length,
            lowest_non_one,
            highest_non_zero,
        }
    }

    /// Java `protected double noise(int seed, double x, double y)`.
    fn noise_at(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.noise.get(seed, x, y)
    }

    /// Java private static `checkInterpreterLength`.
    fn check_interpreter_length(values: &[Rc<dyn IHeightThresholdInterpreter>]) {
        let length = values[0].get_length();
        for value in &values[1..] {
            if value.get_length() != length {
                panic!("ThresholdKeyInterpreter have different size!");
            }
        }
    }
}

impl IHeightThresholdInterpreter for NoiseHeightThresholdInterpreter {
    fn get_lowest_non_one(&self) -> i32 {
        self.lowest_non_one
    }

    fn get_highest_non_zero(&self) -> i32 {
        self.highest_non_zero
    }

    fn get_context(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.noise_at(seed, x, y)
    }

    fn get_length(&self) -> i32 {
        self.length
    }

    fn get_threshold(&self, seed: i32, x: f64, z: f64, height: i32) -> f32 {
        let context = self.get_context(seed, x, z);
        self.get_threshold_with_context(seed, x, z, height, context)
    }

    fn get_threshold_with_context(&self, seed: i32, x: f64, z: f64, height: i32, context: f64) -> f32 {
        if height > self.highest_non_zero {
            return 0.0;
        }
        let length = self.keys.len();
        for (i, &key) in self.keys.iter().enumerate() {
            if context <= key as f64 {
                return if i == 0 {
                    self.values[0].get_threshold(seed, x, z, height)
                } else {
                    let distance =
                        (context as f32 - self.keys[i - 1]) / (self.keys[i] - self.keys[i - 1]);
                    height_threshold_lerp(
                        self.values[i - 1].get_threshold(seed, x, z, height),
                        self.values[i].get_threshold(seed, x, z, height),
                        distance,
                    )
                };
            }
        }
        self.values[length - 1].get_threshold(seed, x, z, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_int_conditions() {
        assert!(ConstantIntCondition::DEFAULT_TRUE.eval(0));
        assert!(!ConstantIntCondition::DEFAULT_FALSE.eval(12345));
        assert!(ConstantIntCondition::new(true).eval(-1));
    }

    #[test]
    fn single_double_condition_is_strictly_less() {
        let c = SingleDoubleCondition::new(0.5);
        assert!(c.eval(0.4999999));
        assert!(!c.eval(0.5)); // strict <
        assert!(!c.eval(0.6));
    }

    #[test]
    fn double_threshold_single_is_inclusive() {
        let t = DoubleThresholdSingle::new(0.25, 0.75);
        assert!(t.eval(0.25)); // inclusive
        assert!(t.eval(0.75)); // inclusive
        assert!(!t.eval(0.2));
        assert!(!t.eval(0.8));
        // factor shrinks the window toward the center
        assert!(t.eval_factor(0.4, 0.0)); // full window
        assert!(t.eval_factor(0.5, 1.0)); // collapsed to center
        assert!(!t.eval_factor(0.25, 1.0));
    }

    #[test]
    fn double_threshold_multiple_any_match() {
        let m = DoubleThresholdMultiple::new(vec![
            DoubleThresholdSingle::new(0.0, 0.2),
            DoubleThresholdSingle::new(0.8, 1.0),
        ]);
        assert!(m.eval(0.1));
        assert!(m.eval(0.9));
        assert!(!m.eval(0.5));
    }

    #[test]
    fn basic_interpreter_interpolates_and_clamps() {
        // positions [25, 75], thresholds [1, 0], length 100
        // y<25 → 1; y=25 → lerp(1,0,0)=1; y=50 → 0.5; y>=75 → 0
        let interp = BasicHeightThresholdInterpreter::new(&[25, 75], &[1.0, 0.0], 100);
        // y=25 interpolates to exactly 1.0, so the first threshold < 1 is y=26
        assert_eq!(interp.get_lowest_non_one(), 26);
        assert!(interp.get_threshold(0, 0.0, 0.0, 24) == 1.0);
        assert!(interp.get_threshold(0, 0.0, 0.0, 26) < 1.0);
        let mid = interp.get_threshold(0, 0.0, 0.0, 50);
        assert!((mid - 0.5).abs() < 1e-6, "mid={mid}");
        assert_eq!(interp.get_threshold(0, 0.0, 0.0, 75), 0.0);
        // clamping beyond the range
        assert_eq!(interp.get_threshold(0, 0.0, 0.0, 1000), 0.0);
        assert_eq!(interp.get_threshold(0, 0.0, 0.0, -5), 1.0);
        assert!(!interp.is_spawnable(200));
    }

    #[test]
    fn noise_interpreter_blends_by_context() {
        // positions/thresholds must have equal lengths (Java precondition)
        let low = Rc::new(BasicHeightThresholdInterpreter::new(&[25, 50], &[1.0, 0.0], 100));
        let high = Rc::new(BasicHeightThresholdInterpreter::new(&[50, 75], &[1.0, 0.0], 100));
        // ConstantNoise(0.5) → SingleNoiseProperty: 0.5*0.5+0.5 = 0.75
        // keys[i] pairs with values[i]: context <= keys[i] blends values[i-1..i]
        let interp = NoiseHeightThresholdInterpreter::new(
            constant_property(0.5),
            vec![0.5, 1.0],
            vec![low.clone(), high.clone()],
        );
        assert!((interp.get_context(3, 1.0, 1.0) - 0.75).abs() < 1e-12);
        // context 0.75 hits key[1]=1.0 → blends low/high at t=(0.75-0.5)/0.5=0.5.
        // low(26): lerp over [25,50] → 1 - 1/25 = 0.96; high(26): 1.0
        // → 0.5*(0.96+1.0) = 0.98
        let got = interp.get_threshold(3, 1.0, 1.0, 26);
        assert!((got - 0.98).abs() < 1e-6, "got {got}");
        // context 0.75 > every key (fall-through branch) → last interpreter exactly
        let interp2 = NoiseHeightThresholdInterpreter::new(
            constant_property(0.5),
            vec![0.5, 0.7],
            vec![low.clone(), high.clone()],
        );
        assert_eq!(
            interp2.get_threshold(3, 1.0, 1.0, 30).to_bits(),
            high.get_threshold(3, 1.0, 1.0, 30).to_bits()
        );
    }

    fn constant_property(v: f64) -> Rc<dyn NoiseProperty> {
        use crate::noise::property::single_noise_property::SingleNoiseProperty;
        Rc::new(SingleNoiseProperty::new(0, Rc::new(crate::noise::constant_noise::ConstantNoise::new(v))))
    }
}
