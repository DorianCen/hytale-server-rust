// Ported from: `.hytale-source/.../procedurallib/property/MaxNoiseProperty.java`
//! Java `MaxNoiseProperty`.

use std::rc::Rc;

use crate::noise::property::NoiseProperty;

/// Java `MaxNoiseProperty.MAX_EPSILON` — "treat the noisemap as 1" constant.
pub const MAX_EPSILON: f64 = 0.999999;

/// Java `MaxNoiseProperty`.
pub struct MaxNoiseProperty {
    /// Java `protected final NoiseProperty[] noiseProperties`.
    pub noise_properties: Vec<Rc<dyn NoiseProperty>>,
}

impl MaxNoiseProperty {
    /// Java `MaxNoiseProperty(NoiseProperty[])`.
    pub fn new(noise_properties: Vec<Rc<dyn NoiseProperty>>) -> Self {
        Self { noise_properties }
    }
}

impl NoiseProperty for MaxNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        let mut val = self.noise_properties[0].get(seed, x, y);
        for property in &self.noise_properties[1..] {
            if val > MAX_EPSILON {
                return 1.0;
            }
            let d = property.get(seed, x, y);
            if val < d {
                val = d;
            }
        }
        val
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut val = self.noise_properties[0].get_3d(seed, x, y, z);
        for property in &self.noise_properties[1..] {
            if val > MAX_EPSILON {
                return 1.0;
            }
            let d = property.get_3d(seed, x, y, z);
            if val < d {
                val = d;
            }
        }
        val
    }
}
