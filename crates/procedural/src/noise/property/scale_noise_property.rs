// Ported from: `.hytale-source/.../procedurallib/property/ScaleNoiseProperty.java`
//! Java `ScaleNoiseProperty`.

use std::rc::Rc;

use crate::noise::property::NoiseProperty;

/// Java `ScaleNoiseProperty`.
pub struct ScaleNoiseProperty {
    /// Java `protected final NoiseProperty noiseProperty`.
    pub noise_property: Rc<dyn NoiseProperty>,
    /// Java `protected final double scaleX, scaleY, scaleZ`.
    pub scale_x: f64,
    pub scale_y: f64,
    pub scale_z: f64,
}

impl ScaleNoiseProperty {
    /// Java `ScaleNoiseProperty(NoiseProperty, double scale)` — uniform.
    pub fn uniform(noise_property: Rc<dyn NoiseProperty>, scale: f64) -> Self {
        Self::new(noise_property, scale, scale, scale)
    }

    /// Java `ScaleNoiseProperty(NoiseProperty, double, double, double)`.
    pub fn new(
        noise_property: Rc<dyn NoiseProperty>,
        scale_x: f64,
        scale_y: f64,
        scale_z: f64,
    ) -> Self {
        Self {
            noise_property,
            scale_x,
            scale_y,
            scale_z,
        }
    }
}

impl NoiseProperty for ScaleNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.noise_property
            .get(seed, x * self.scale_x, y * self.scale_y)
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.noise_property
            .get_3d(seed, x * self.scale_x, y * self.scale_y, z * self.scale_z)
    }
}
