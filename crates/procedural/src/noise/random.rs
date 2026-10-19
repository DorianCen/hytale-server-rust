// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/random/*.java`
//! Java `procedurallib.random` package — coordinate randomizers / rotators
//! (the fuzzy domain-warp primitives used by `DistortedNoiseProperty` and the
//! worldgen `CoordinateRandomizer`).

use std::rc::Rc;

use hytale_math::trig_math_util;

// Java `procedurallib.property.NoiseProperty` (the interface) lives in
// `support.rs` in the Rust port.
use crate::noise::support::NoiseProperty;

/// Java `random.ICoordinateRandomizer`.
pub trait ICoordinateRandomizer {
    /// Java `randomDoubleX(int seed, double x, double y)`.
    fn random_double_x2(&self, seed: i32, x: f64, y: f64) -> f64;
    /// Java `randomDoubleY(int seed, double x, double y)`.
    fn random_double_y2(&self, seed: i32, x: f64, y: f64) -> f64;
    /// Java `randomDoubleX(int seed, double x, double y, double z)`.
    fn random_double_x3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64;
    /// Java `randomDoubleY(int seed, double x, double y, double z)`.
    fn random_double_y3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64;
    /// Java `randomDoubleZ(int seed, double x, double y, double z)`.
    fn random_double_z3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64;
}

/// Java `CoordinateRandomizer.AmplitudeNoiseProperty`.
pub struct AmplitudeNoiseProperty {
    /// Java `protected NoiseProperty property`.
    pub property: Rc<dyn NoiseProperty>,
    /// Java `protected double amplitude`.
    pub amplitude: f64,
}

impl AmplitudeNoiseProperty {
    /// Java `AmplitudeNoiseProperty(NoiseProperty, double)`.
    pub fn new(property: Rc<dyn NoiseProperty>, amplitude: f64) -> Self {
        Self { property, amplitude }
    }
}

/// Java `CoordinateRandomizer` — sums amplitude-weighted noise offsets per axis.
pub struct CoordinateRandomizer {
    /// Java `protected final AmplitudeNoiseProperty[] xNoise`.
    pub x_noise: Vec<AmplitudeNoiseProperty>,
    /// Java `protected final AmplitudeNoiseProperty[] yNoise`.
    pub y_noise: Vec<AmplitudeNoiseProperty>,
    /// Java `protected final AmplitudeNoiseProperty[] zNoise`.
    pub z_noise: Vec<AmplitudeNoiseProperty>,
}

impl CoordinateRandomizer {
    /// Java `CoordinateRandomizer(AmplitudeNoiseProperty[], ...)` .
    pub fn new(
        x_noise: Vec<AmplitudeNoiseProperty>,
        y_noise: Vec<AmplitudeNoiseProperty>,
        z_noise: Vec<AmplitudeNoiseProperty>,
    ) -> Self {
        Self { x_noise, y_noise, z_noise }
    }
}

impl ICoordinateRandomizer for CoordinateRandomizer {
    fn random_double_x2(&self, seed: i32, x: f64, y: f64) -> f64 {
        let mut offset_x = 0.0;
        for property in &self.x_noise {
            offset_x += (property.property.get(seed, x, y) * 2.0 - 1.0) * property.amplitude;
        }
        x + offset_x
    }

    fn random_double_y2(&self, seed: i32, x: f64, y: f64) -> f64 {
        let mut offset_y = 0.0;
        for property in &self.y_noise {
            offset_y += (property.property.get(seed, x, y) * 2.0 - 1.0) * property.amplitude;
        }
        y + offset_y
    }

    fn random_double_x3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut offset_x = 0.0;
        for property in &self.x_noise {
            offset_x += (property.property.get_3d(seed, x, y, z) * 2.0 - 1.0) * property.amplitude;
        }
        x + offset_x
    }

    fn random_double_y3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut offset_y = 0.0;
        for property in &self.y_noise {
            offset_y += (property.property.get_3d(seed, x, y, z) * 2.0 - 1.0) * property.amplitude;
        }
        y + offset_y
    }

    fn random_double_z3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut offset_z = 0.0;
        for property in &self.z_noise {
            offset_z += (property.property.get_3d(seed, x, y, z) * 2.0 - 1.0) * property.amplitude;
        }
        z + offset_z
    }
}

/// Java private `EmptyCoordinateRandomizer` — identity warp.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmptyCoordinateRandomizer;

impl ICoordinateRandomizer for EmptyCoordinateRandomizer {
    fn random_double_x2(&self, _seed: i32, x: f64, _y: f64) -> f64 {
        x
    }
    fn random_double_y2(&self, _seed: i32, _x: f64, y: f64) -> f64 {
        y
    }
    fn random_double_x3(&self, _seed: i32, x: f64, _y: f64, _z: f64) -> f64 {
        x
    }
    fn random_double_y3(&self, _seed: i32, _x: f64, y: f64, _z: f64) -> f64 {
        y
    }
    fn random_double_z3(&self, _seed: i32, _x: f64, _y: f64, z: f64) -> f64 {
        z
    }
}

/// Java `CoordinateRandomizer.EMPTY_RANDOMIZER`.
pub fn empty_randomizer() -> Rc<dyn ICoordinateRandomizer> {
    Rc::new(EmptyCoordinateRandomizer)
}

/// Java `random.CoordinateRotator` — rotates coordinates about the origin.
///
/// ⚠ NOTE: Java builds the matrix with `double` fields but computes sin/cos
/// through `TrigMathUtil.sin(double)` which returns **float** precision —
/// preserved exactly (the float results are widened back to f64).
#[derive(Debug, Clone)]
pub struct CoordinateRotator {
    /// Java `protected final double pitch`.
    pub pitch: f64,
    /// Java `protected final double yaw`.
    pub yaw: f64,
    /// Java `protected final double[] matrix` (row-major 3x3).
    pub matrix: [f64; 9],
}

// Java statics X0..Z2 — matrix indices.
pub const X0: usize = 0;
pub const Y0: usize = 1;
pub const Z0: usize = 2;
pub const X1: usize = 3;
pub const Y1: usize = 4;
pub const Z1: usize = 5;
pub const X2: usize = 6;
pub const Y2: usize = 7;
pub const Z2: usize = 8;

impl CoordinateRotator {
    /// Java `CoordinateRotator.NONE`.
    pub fn none() -> Self {
        Self::new(0.0, 0.0)
    }

    /// Java `CoordinateRotator(double pitch, double yaw)`.
    pub fn new(pitch: f64, yaw: f64) -> Self {
        Self {
            pitch,
            yaw,
            matrix: Self::create_rotation_matrix(pitch, yaw),
        }
    }

    /// Java `rotateX(double x, double y)`.
    #[inline]
    pub fn rotate_x2(&self, x: f64, y: f64) -> f64 {
        x * self.matrix[X0] + y * self.matrix[Z0]
    }

    /// Java `rotateY(double x, double y)`.
    #[inline]
    pub fn rotate_y2(&self, x: f64, y: f64) -> f64 {
        x * self.matrix[X2] + y * self.matrix[Z2]
    }

    /// Java `rotateX(double x, double y, double z)`.
    #[inline]
    pub fn rotate_x3(&self, x: f64, y: f64, z: f64) -> f64 {
        x * self.matrix[X0] + y * self.matrix[Y0] + z * self.matrix[Z0]
    }

    /// Java `rotateY(double x, double y, double z)`.
    #[inline]
    pub fn rotate_y3(&self, x: f64, y: f64, z: f64) -> f64 {
        x * self.matrix[X1] + y * self.matrix[Y1] + z * self.matrix[Z1]
    }

    /// Java `rotateZ(double x, double y, double z)`.
    #[inline]
    pub fn rotate_z3(&self, x: f64, y: f64, z: f64) -> f64 {
        x * self.matrix[X2] + y * self.matrix[Y2] + z * self.matrix[Z2]
    }
}

impl ICoordinateRandomizer for CoordinateRotator {
    fn random_double_x2(&self, _seed: i32, x: f64, y: f64) -> f64 {
        self.rotate_x2(x, y)
    }
    fn random_double_y2(&self, _seed: i32, x: f64, y: f64) -> f64 {
        self.rotate_y2(x, y)
    }
    fn random_double_x3(&self, _seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.rotate_x3(x, y, z)
    }
    fn random_double_y3(&self, _seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.rotate_y3(x, y, z)
    }
    fn random_double_z3(&self, _seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.rotate_z3(x, y, z)
    }
}

impl CoordinateRotator {
    /// Java `createRotationMatrix(double pitch, double yaw)`.
    pub fn create_rotation_matrix(pitch: f64, yaw: f64) -> [f64; 9] {
        // Java: TrigMathUtil.sin(yaw) etc. — the double overloads return float
        // precision, widened back to double here.
        let sin_yaw = trig_math_util::sin_f64(yaw) as f64;
        let cos_yaw = trig_math_util::cos_f64(yaw) as f64;
        let sin_pitch = trig_math_util::sin_f64(pitch) as f64;
        let cos_pitch = trig_math_util::cos_f64(pitch) as f64;

        // rotation about x-axis (pitch)
        let (px1, px2, px3) = (1.0, 0.0, 0.0);
        let (py1, py2, py3) = (0.0, cos_pitch, -sin_pitch);
        let (pz1, pz2, pz3) = (0.0, sin_pitch, cos_pitch);
        // rotation about y-axis (yaw)
        let (yx1, yx2, yx3) = (cos_yaw, 0.0, sin_yaw);
        let (yy1, yy2, yy3) = (0.0, 1.0, 0.0);
        let (yz1, yz2, yz3) = (-sin_yaw, 0.0, cos_yaw);

        [
            dot(px1, px2, px3, yx1, yy1, yz1),
            dot(px1, px2, px3, yx2, yy2, yz2),
            dot(px1, px2, px3, yx3, yy3, yz3),
            dot(py1, py2, py3, yx1, yy1, yz1),
            dot(py1, py2, py3, yx2, yy2, yz2),
            dot(py1, py2, py3, yx3, yy3, yz3),
            dot(pz1, pz2, pz3, yx1, yy1, yz1),
            dot(pz1, pz2, pz3, yx2, yy2, yz2),
            dot(pz1, pz2, pz3, yx3, yy3, yz3),
        ]
    }
}

#[inline]
fn dot(x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> f64 {
    x1 * x2 + y1 * y2 + z1 * z2
}

/// Java `random.CoordinateOriginRotator` — rotations around a defined origin.
#[derive(Debug, Clone)]
pub struct CoordinateOriginRotator {
    /// Java `super` (pitch/yaw/matrix).
    pub base: CoordinateRotator,
    /// Java `private final double originX`.
    pub origin_x: f64,
    /// Java `private final double originY`.
    pub origin_y: f64,
    /// Java `private final double originZ`.
    pub origin_z: f64,
}

impl CoordinateOriginRotator {
    /// Java `CoordinateOriginRotator(pitch, yaw, originX, originY, originZ)`.
    pub fn new(pitch: f64, yaw: f64, origin_x: f64, origin_y: f64, origin_z: f64) -> Self {
        Self {
            base: CoordinateRotator::new(pitch, yaw),
            origin_x,
            origin_y,
            origin_z,
        }
    }
}

impl ICoordinateRandomizer for CoordinateOriginRotator {
    fn random_double_x2(&self, _seed: i32, x: f64, y: f64) -> f64 {
        let x = x - self.origin_x;
        let y = y - self.origin_y;
        self.origin_x + self.base.rotate_x2(x, y)
    }

    fn random_double_y2(&self, _seed: i32, x: f64, y: f64) -> f64 {
        let x = x - self.origin_x;
        let y = y - self.origin_y;
        self.origin_y + self.base.rotate_y2(x, y)
    }

    fn random_double_x3(&self, _seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let x = x - self.origin_x;
        let y = y - self.origin_y;
        let z = z - self.origin_z;
        self.origin_x + self.base.rotate_x3(x, y, z)
    }

    fn random_double_y3(&self, _seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let x = x - self.origin_x;
        let y = y - self.origin_y;
        let z = z - self.origin_z;
        self.origin_y + self.base.rotate_y3(x, y, z)
    }

    fn random_double_z3(&self, _seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let x = x - self.origin_x;
        let y = y - self.origin_y;
        let z = z - self.origin_z;
        self.origin_z + self.base.rotate_z3(x, y, z)
    }
}

/// Java `random.RotatedCoordinateRandomizer` — warps through a rotation first.
pub struct RotatedCoordinateRandomizer {
    /// Java `protected final ICoordinateRandomizer randomizer`.
    pub randomizer: Rc<dyn ICoordinateRandomizer>,
    /// Java `protected final CoordinateRotator rotation`.
    pub rotation: CoordinateRotator,
}

impl RotatedCoordinateRandomizer {
    /// Java `RotatedCoordinateRandomizer(ICoordinateRandomizer, CoordinateRotator)`.
    pub fn new(randomizer: Rc<dyn ICoordinateRandomizer>, rotation: CoordinateRotator) -> Self {
        Self { randomizer, rotation }
    }
}

impl ICoordinateRandomizer for RotatedCoordinateRandomizer {
    fn random_double_x2(&self, seed: i32, x: f64, y: f64) -> f64 {
        let px = self.rotation.rotate_x2(x, y);
        let py = self.rotation.rotate_y2(x, y);
        self.randomizer.random_double_x2(seed, px, py)
    }

    fn random_double_y2(&self, seed: i32, x: f64, y: f64) -> f64 {
        let px = self.rotation.rotate_x2(x, y);
        let py = self.rotation.rotate_y2(x, y);
        self.randomizer.random_double_y2(seed, px, py)
    }

    fn random_double_x3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let px = self.rotation.rotate_x3(x, y, z);
        let py = self.rotation.rotate_y3(x, y, z);
        let pz = self.rotation.rotate_z3(x, y, z);
        self.randomizer.random_double_x3(seed, px, py, pz)
    }

    fn random_double_y3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let px = self.rotation.rotate_x3(x, y, z);
        let py = self.rotation.rotate_y3(x, y, z);
        let pz = self.rotation.rotate_z3(x, y, z);
        self.randomizer.random_double_y3(seed, px, py, pz)
    }

    fn random_double_z3(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let px = self.rotation.rotate_x3(x, y, z);
        let py = self.rotation.rotate_y3(x, y, z);
        let pz = self.rotation.rotate_z3(x, y, z);
        self.randomizer.random_double_z3(seed, px, py, pz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::constant_noise::ConstantNoise;
    use crate::noise::property::single_noise_property::SingleNoiseProperty;

    fn constant(v: f64) -> Rc<dyn NoiseProperty> {
        Rc::new(SingleNoiseProperty::new(0, Rc::new(ConstantNoise::new(v))))
    }

    #[test]
    fn empty_randomizer_is_identity() {
        let r = EmptyCoordinateRandomizer;
        assert_eq!(r.random_double_x2(1, 1.5, 2.5), 1.5);
        assert_eq!(r.random_double_y2(1, 1.5, 2.5), 2.5);
        assert_eq!(r.random_double_z3(1, 1.5, 2.5, 3.5), 3.5);
    }

    #[test]
    fn coordinate_randomizer_offsets_by_amplitude() {
        // SingleNoiseProperty maps ConstantNoise(0.75) from [-1,1] to [0,1]:
        // 0.75*0.5+0.5 = 0.875 → (0.875*2-1) = 0.75 → offset = 0.75 * 2.0 amplitude
        let prop = || AmplitudeNoiseProperty::new(constant(0.75), 2.0);
        let r = CoordinateRandomizer::new(vec![prop()], vec![prop()], vec![prop()]);
        assert_eq!(r.random_double_x2(7, 10.0, 0.0), 11.5);
        assert_eq!(r.random_double_y2(7, 10.0, 20.0), 21.5);
        assert_eq!(r.random_double_z3(7, 0.0, 0.0, 5.0), 6.5);
    }

    #[test]
    fn rotator_none_is_identity() {
        let r = CoordinateRotator::none();
        assert_eq!(r.random_double_x3(0, 1.0, 2.0, 3.0), 1.0);
        assert_eq!(r.random_double_y3(0, 1.0, 2.0, 3.0), 2.0);
        assert_eq!(r.random_double_z3(0, 1.0, 2.0, 3.0), 3.0);
    }

    #[test]
    fn yaw_quarter_swaps_axes() {
        // Deterministic sanity: rotating (1,0) by yaw=pi/2 maps onto ~(0,-1)
        // in this matrix convention (rotation about the y-axis maps +x onto
        // -z, and the 2d path uses the x/z rows). Float-precision sin/cos
        // introduce the usual tiny error.
        let r = CoordinateRotator::new(0.0, std::f64::consts::FRAC_PI_2);
        let x = r.random_double_x2(0, 1.0, 0.0);
        let y = r.random_double_y2(0, 1.0, 0.0);
        assert!((x - 0.0).abs() < 1e-6, "x={x}");
        assert!((y + 1.0).abs() < 1e-6, "y={y}");
    }
}
