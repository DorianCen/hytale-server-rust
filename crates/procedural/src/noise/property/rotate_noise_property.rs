// Ported from: `.hytale-source/.../procedurallib/property/RotateNoiseProperty.java`
//! Java `RotateNoiseProperty`.

use std::rc::Rc;

use crate::noise::random::{CoordinateRotator, ICoordinateRandomizer};
use crate::noise::support::NoiseProperty;

/// Java `RotateNoiseProperty` — rotates the sample coordinates before sampling.
pub struct RotateNoiseProperty {
    /// Java `protected final NoiseProperty noise`.
    pub noise: Rc<dyn NoiseProperty>,
    /// Java `protected final CoordinateRotator rotation`.
    ///
    /// ⚠ NOTE: Java declares the field as `CoordinateRotator` but only calls
    /// the `ICoordinateRandomizer` rotate methods on it; the Rust port takes
    /// any randomizer (use [`CoordinateRotator`] for the Java behavior).
    pub rotation: Rc<dyn ICoordinateRandomizer>,
}

impl RotateNoiseProperty {
    /// Java `RotateNoiseProperty(NoiseProperty, CoordinateRotator)`.
    pub fn new(noise: Rc<dyn NoiseProperty>, rotation: Rc<dyn ICoordinateRandomizer>) -> Self {
        Self { noise, rotation }
    }

    /// Java overload taking the concrete `CoordinateRotator`.
    pub fn with_rotator(noise: Rc<dyn NoiseProperty>, rotation: CoordinateRotator) -> Self {
        Self {
            noise,
            rotation: Rc::new(rotation),
        }
    }
}

impl NoiseProperty for RotateNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        let px = self.rotation.random_double_x2(0, x, y); // rotation.rotateX(x, y)
        let py = self.rotation.random_double_y2(0, x, y);
        self.noise.get(seed, px, py)
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let px = self.rotation.random_double_x3(0, x, y, z);
        let py = self.rotation.random_double_y3(0, x, y, z);
        let pz = self.rotation.random_double_z3(0, x, y, z);
        self.noise.get_3d(seed, px, py, pz)
    }
}
