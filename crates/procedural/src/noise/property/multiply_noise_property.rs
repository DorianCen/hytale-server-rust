// Ported from: `.hytale-source/.../procedurallib/property/MultiplyNoiseProperty.java`
//! Java `MultiplyNoiseProperty`.

use std::rc::Rc;

use crate::noise::property::NoiseProperty;

/// Java `MultiplyNoiseProperty`.
pub struct MultiplyNoiseProperty {
    /// Java `protected final NoiseProperty[] noiseProperties`.
    pub noise_properties: Vec<Rc<dyn NoiseProperty>>,
}

impl MultiplyNoiseProperty {
    /// Java `MultiplyNoiseProperty(NoiseProperty[])`.
    pub fn new(noise_properties: Vec<Rc<dyn NoiseProperty>>) -> Self {
        Self { noise_properties }
    }
}

impl NoiseProperty for MultiplyNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        let mut val = self.noise_properties[0].get(seed, x, y);
        for property in &self.noise_properties[1..] {
            val *= property.get(seed, x, y);
        }
        val
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut val = self.noise_properties[0].get_3d(seed, x, y, z);
        for property in &self.noise_properties[1..] {
            val *= property.get_3d(seed, x, y, z);
        }
        val
    }
}
