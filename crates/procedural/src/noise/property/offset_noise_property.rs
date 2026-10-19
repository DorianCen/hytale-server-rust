// Ported from: `.hytale-source/.../procedurallib/property/OffsetNoiseProperty.java`
//! Java `OffsetNoiseProperty`.

use std::rc::Rc;

use crate::noise::support::NoiseProperty;

/// Java `OffsetNoiseProperty`.
pub struct OffsetNoiseProperty {
    /// Java `protected final NoiseProperty noiseProperty`.
    pub noise_property: Rc<dyn NoiseProperty>,
    /// Java `protected final double offsetX, offsetY, offsetZ`.
    pub offset_x: f64,
    pub offset_y: f64,
    pub offset_z: f64,
}

impl OffsetNoiseProperty {
    /// Java `OffsetNoiseProperty(NoiseProperty, double offset)` — uniform.
    pub fn uniform(noise_property: Rc<dyn NoiseProperty>, offset: f64) -> Self {
        Self::new(noise_property, offset, offset, offset)
    }

    /// Java `OffsetNoiseProperty(NoiseProperty, double, double, double)`.
    pub fn new(
        noise_property: Rc<dyn NoiseProperty>,
        offset_x: f64,
        offset_y: f64,
        offset_z: f64,
    ) -> Self {
        Self {
            noise_property,
            offset_x,
            offset_y,
            offset_z,
        }
    }
}

impl NoiseProperty for OffsetNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.noise_property.get(seed, x + self.offset_x, y + self.offset_y)
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.noise_property.get_3d(
            seed,
            x + self.offset_x,
            y + self.offset_y,
            z + self.offset_z,
        )
    }
}
