// Ported from: `.hytale-source/.../procedurallib/property/InvertNoiseProperty.java`
//! Java `InvertNoiseProperty`.

use std::rc::Rc;

use crate::noise::support::NoiseProperty;

/// Java `InvertNoiseProperty`.
pub struct InvertNoiseProperty {
    /// Java `protected final NoiseProperty noiseProperty`.
    pub noise_property: Rc<dyn NoiseProperty>,
}

impl InvertNoiseProperty {
    /// Java `InvertNoiseProperty(NoiseProperty)`.
    pub fn new(noise_property: Rc<dyn NoiseProperty>) -> Self {
        Self { noise_property }
    }
}

impl NoiseProperty for InvertNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        1.0 - self.noise_property.get(seed, x, y)
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        1.0 - self.noise_property.get_3d(seed, x, y, z)
    }
}
