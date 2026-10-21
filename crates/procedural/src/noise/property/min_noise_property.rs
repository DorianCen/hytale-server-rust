// Ported from: `.hytale-source/.../procedurallib/property/MinNoiseProperty.java`
//! Java `MinNoiseProperty`.

use std::rc::Rc;

use crate::noise::property::NoiseProperty;

/// Java `MinNoiseProperty.MIN_EPSILON` — "treat the noisemap as 0" constant.
pub const MIN_EPSILON: f64 = 0.00001;

/// Java `MinNoiseProperty`.
pub struct MinNoiseProperty {
    /// Java `protected final NoiseProperty[] noiseProperties`.
    pub noise_properties: Vec<Rc<dyn NoiseProperty>>,
}

impl MinNoiseProperty {
    /// Java `MinNoiseProperty(NoiseProperty[])`.
    pub fn new(noise_properties: Vec<Rc<dyn NoiseProperty>>) -> Self {
        Self { noise_properties }
    }
}

impl NoiseProperty for MinNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        let mut val = self.noise_properties[0].get(seed, x, y);
        for property in &self.noise_properties[1..] {
            if val < MIN_EPSILON {
                return 0.0;
            }
            let d = property.get(seed, x, y);
            if val > d {
                val = d;
            }
        }
        val
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut val = self.noise_properties[0].get_3d(seed, x, y, z);
        for property in &self.noise_properties[1..] {
            if val < MIN_EPSILON {
                return 0.0;
            }
            let d = property.get_3d(seed, x, y, z);
            if val > d {
                val = d;
            }
        }
        val
    }
}
