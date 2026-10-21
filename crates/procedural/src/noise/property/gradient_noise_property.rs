// Ported from: `.hytale-source/.../procedurallib/property/GradientNoiseProperty.java`
//! Java `GradientNoiseProperty`.

use std::rc::Rc;

use hytale_math::math_util;
use hytale_math::trig_math_util;

use crate::noise::property::NoiseProperty;

/// Java `GradientNoiseProperty.GradientMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GradientMode {
    /// Java `MAGNITUDE`.
    Magnitude,
    /// Java `ANGLE`.
    Angle,
    /// Java `ANGLE_ABS`.
    AngleAbs,
}

/// Java `GradientNoiseProperty`.
///
/// ⚠ NOTE: 3d sampling throws `UnsupportedOperationException` in Java —
/// mirrored as a panic.
pub struct GradientNoiseProperty {
    /// Java `protected final NoiseProperty noise`.
    pub noise: Rc<dyn NoiseProperty>,
    /// Java `protected final GradientMode mode`.
    pub mode: GradientMode,
    /// Java `protected final double distance`.
    pub distance: f64,
    /// Java `protected final double invNormalize` (`1 / distance / normalize`).
    pub inv_normalize: f64,
}

impl GradientNoiseProperty {
    /// Java `GradientNoiseProperty(NoiseProperty, GradientMode, double distance, double normalize)`.
    pub fn new(
        noise: Rc<dyn NoiseProperty>,
        mode: GradientMode,
        distance: f64,
        normalize: f64,
    ) -> Self {
        Self {
            noise,
            mode,
            distance,
            inv_normalize: 1.0 / distance / normalize,
        }
    }
}

impl NoiseProperty for GradientNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        // Note: This is a very rough approximation to reduce number of samples per-point
        let v = self.noise.get(seed, x, y);
        let s = self.noise.get(seed, x, y + self.distance);
        let e = self.noise.get(seed, x + self.distance, y);

        let dx = e - v;
        let dy = s - v;

        match self.mode {
            GradientMode::Angle => Self::get_angle(dx, dy),
            GradientMode::AngleAbs => Self::get_abs_angle(dx, dy),
            GradientMode::Magnitude => Self::get_magnitude(dx, dy, self.inv_normalize),
        }
    }

    fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
        // Java: throw new UnsupportedOperationException();
        panic!("GradientNoiseProperty 3d is not supported (Java UnsupportedOperationException)");
    }
}

impl GradientNoiseProperty {
    /// Java `getAngle(double dx, double dy)` (protected static).
    fn get_angle(dx: f64, dy: f64) -> f64 {
        // Java: float angle = TrigMathUtil.atan2(dy, dx);
        let mut angle: f32 = trig_math_util::atan2_f64(dy, dx);
        angle = (angle + trig_math_util::PI) / trig_math_util::PI2;
        Self::convert_range(angle) as f64
    }

    /// Java `getAbsAngle(double dx, double dy)` (protected static).
    fn get_abs_angle(dx: f64, dy: f64) -> f64 {
        let angle: f32 = trig_math_util::atan2_f64(dy, dx);
        (angle.abs() / trig_math_util::PI) as f64
    }

    /// Java `getMagnitude(double dx, double dy, double invNormalize)` (protected static).
    fn get_magnitude(dx: f64, dy: f64, inv_normalize: f64) -> f64 {
        let mag = math_util::length2_f64(dx, dy);
        math_util::clamp_f64(mag * inv_normalize, 0.0, 1.0)
    }

    /// Java `convertRange(float angle)` (protected static).
    fn convert_range(mut angle: f32) -> f32 {
        // Cardinal directions on the original angle lie at 0.0, 0.25, 0.5, 0.75.
        // We rotate it a further 1/8 so the axes instead lie mid-way between
        // these values, giving more easily threshold-able ranges of
        // [0.0-0.25], [0.25-0.5], [0.5-0.75] and [0.75-1.0].
        angle += 0.125f32;

        // Wrap the angle if it exceeds 1.0
        if angle > 1.0 {
            angle - 1.0
        } else {
            angle
        }
    }
}
