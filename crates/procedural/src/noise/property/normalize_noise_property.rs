// Ported from: `.hytale-source/.../procedurallib/property/NormalizeNoiseProperty.java`
//! Java `NormalizeNoiseProperty`.

use std::rc::Rc;

use crate::noise::general_noise::limit;
use crate::noise::property::NoiseProperty;

/// Java `NormalizeNoiseProperty`.
pub struct NormalizeNoiseProperty {
    /// Java `protected final NoiseProperty noiseProperty`.
    pub noise_property: Rc<dyn NoiseProperty>,
    /// Java `protected final double min`.
    pub min: f64,
    /// Java `protected final double range`.
    pub range: f64,
}

impl NormalizeNoiseProperty {
    /// Java `NormalizeNoiseProperty(NoiseProperty, double min, double range)`.
    pub fn new(noise_property: Rc<dyn NoiseProperty>, min: f64, range: f64) -> Self {
        Self {
            noise_property,
            min,
            range,
        }
    }
}

impl NoiseProperty for NormalizeNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        limit((self.noise_property.get(seed, x, y) - self.min) / self.range)
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        limit((self.noise_property.get_3d(seed, x, y, z) - self.min) / self.range)
    }
}
