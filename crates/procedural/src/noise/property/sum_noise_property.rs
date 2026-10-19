// Ported from: `.hytale-source/.../procedurallib/property/SumNoiseProperty.java`
//! Java `SumNoiseProperty`.

use std::rc::Rc;

use crate::noise::general_noise::limit;
use crate::noise::support::NoiseProperty;

/// Java `SumNoiseProperty.Entry`.
pub struct SumNoisePropertyEntry {
    /// Java `private NoiseProperty noiseProperty`.
    pub noise_property: Rc<dyn NoiseProperty>,
    /// Java `private double factor`.
    pub factor: f64,
}

impl SumNoisePropertyEntry {
    /// Java `Entry(NoiseProperty, double)`.
    pub fn new(noise_property: Rc<dyn NoiseProperty>, factor: f64) -> Self {
        Self { noise_property, factor }
    }
}

/// Java `SumNoiseProperty`.
pub struct SumNoiseProperty {
    /// Java `protected final Entry[] entries`.
    pub entries: Vec<SumNoisePropertyEntry>,
}

impl SumNoiseProperty {
    /// Java `SumNoiseProperty(Entry[])`.
    pub fn new(entries: Vec<SumNoisePropertyEntry>) -> Self {
        Self { entries }
    }
}

impl NoiseProperty for SumNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        let mut val = 0.0f64;
        for entry in &self.entries {
            val += entry.noise_property.get(seed, x, y) * entry.factor;
        }
        limit(val)
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut val = 0.0f64;
        for entry in &self.entries {
            val += entry.noise_property.get_3d(seed, x, y, z) * entry.factor;
        }
        limit(val)
    }
}
