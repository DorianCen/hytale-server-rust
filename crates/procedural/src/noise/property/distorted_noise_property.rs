// Ported from: `.hytale-source/.../procedurallib/property/DistortedNoiseProperty.java`
//! Java `DistortedNoiseProperty`.

use std::rc::Rc;

use crate::noise::random::ICoordinateRandomizer;
use crate::noise::support::NoiseProperty;

/// Java `DistortedNoiseProperty` — warps the sample coordinates through an
/// `ICoordinateRandomizer` before sampling.
pub struct DistortedNoiseProperty {
    /// Java `protected final NoiseProperty noiseProperty`.
    pub noise_property: Rc<dyn NoiseProperty>,
    /// Java `protected final ICoordinateRandomizer randomizer`.
    pub randomizer: Rc<dyn ICoordinateRandomizer>,
}

impl DistortedNoiseProperty {
    /// Java `DistortedNoiseProperty(NoiseProperty, ICoordinateRandomizer)`.
    pub fn new(
        noise_property: Rc<dyn NoiseProperty>,
        randomizer: Rc<dyn ICoordinateRandomizer>,
    ) -> Self {
        Self {
            noise_property,
            randomizer,
        }
    }
}

impl NoiseProperty for DistortedNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.noise_property.get(
            seed,
            self.randomizer.random_double_x2(seed, x, y),
            self.randomizer.random_double_y2(seed, x, y),
        )
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.noise_property.get_3d(
            seed,
            self.randomizer.random_double_x3(seed, x, y, z),
            self.randomizer.random_double_y3(seed, x, y, z),
            self.randomizer.random_double_z3(seed, x, y, z),
        )
    }
}
