// Ported from: `.hytale-source/.../procedurallib/property/CurveNoiseProperty.java`
//! Java `CurveNoiseProperty`.

use std::rc::Rc;

use hytale_math::math_util;

use crate::noise::support::NoiseProperty;

/// Java `DoubleUnaryOperator` as used by `CurveNoiseProperty`.
pub type DoubleUnaryOperator = Rc<dyn Fn(f64) -> f64>;

/// Java `CurveNoiseProperty`.
pub struct CurveNoiseProperty {
    /// Java `protected final NoiseProperty noise`.
    pub noise: Rc<dyn NoiseProperty>,
    /// Java `protected final DoubleUnaryOperator function`.
    pub function: DoubleUnaryOperator,
}

impl CurveNoiseProperty {
    /// Java `CurveNoiseProperty(NoiseProperty, DoubleUnaryOperator)`.
    pub fn new(noise: Rc<dyn NoiseProperty>, function: DoubleUnaryOperator) -> Self {
        Self { noise, function }
    }
}

impl NoiseProperty for CurveNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        let value = self.noise.get(seed, x, y);
        (self.function)(value)
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let value = self.noise.get_3d(seed, x, y, z);
        (self.function)(value)
    }
}

/// Java `CurveNoiseProperty.PowerCurve` (implements `DoubleUnaryOperator`).
pub struct PowerCurve {
    /// Java `protected final double a`.
    pub a: f64,
    /// Java `protected final double b`.
    pub b: f64,
}

impl PowerCurve {
    /// Java `protected static final double MAX = 10.0`.
    pub const MAX: f64 = 10.0;

    /// Java `PowerCurve(double a, double b)`.
    pub fn new(a: f64, b: f64) -> Self {
        let a = math_util::clamp_f64(a, 0.0, Self::MAX);
        Self {
            a,
            b: math_util::clamp_f64(b, -a, Self::MAX),
        }
    }
}

impl PowerCurve {
    /// Java `PowerCurve.applyAsDouble(double operand)`.
    pub fn apply(&self, operand: f64) -> f64 {
        // Java: operand = 1 - operand; return 1 - Math.pow(operand, a + b * operand);
        let operand = 1.0 - operand;
        1.0 - f64::powf(operand, self.a + self.b * operand)
    }
}

/// Convenience: wraps a [`PowerCurve`] as a `DoubleUnaryOperator` closure
/// (Java: `new PowerCurve(a, b)` is itself a `DoubleUnaryOperator`).
pub fn power_curve(a: f64, b: f64) -> DoubleUnaryOperator {
    let curve = PowerCurve::new(a, b);
    Rc::new(move |operand| curve.apply(operand))
}
