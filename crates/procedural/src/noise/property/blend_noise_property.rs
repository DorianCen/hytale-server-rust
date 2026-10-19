// Ported from: `.hytale-source/.../procedurallib/property/BlendNoiseProperty.java`
//! Java `BlendNoiseProperty`.

use std::rc::Rc;

use hytale_math::math_util;

use crate::noise::support::NoiseProperty;

/// Java `BlendNoiseProperty`.
pub struct BlendNoiseProperty {
    /// Java `private final NoiseProperty alpha`.
    pub alpha: Rc<dyn NoiseProperty>,
    /// Java `private final NoiseProperty[] noises`.
    pub noises: Vec<Rc<dyn NoiseProperty>>,
    /// Java `private final double[] thresholds`.
    pub thresholds: Vec<f64>,
    /// Java `private transient final double[] normalize` — precomputed
    /// normalization factors for each threshold range.
    normalize: Vec<f64>,
}

impl BlendNoiseProperty {
    /// Java `BlendNoiseProperty(NoiseProperty alpha, NoiseProperty[] noises, double[] thresholds)`.
    pub fn new(
        alpha: Rc<dyn NoiseProperty>,
        noises: Vec<Rc<dyn NoiseProperty>>,
        thresholds: Vec<f64>,
    ) -> Self {
        let mut normalize = vec![0.0f64; thresholds.len()];
        // Precompute normalization factors for each threshold range.
        for i in 1..thresholds.len() {
            if thresholds[i] <= thresholds[i - 1] {
                panic!("Thresholds must be in ascending order");
            }
            normalize[i] = 1.0 / (thresholds[i] - thresholds[i - 1]);
        }
        Self {
            alpha,
            noises,
            thresholds,
            normalize,
        }
    }
}

impl NoiseProperty for BlendNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        if self.noises.is_empty() {
            return 0.0;
        }

        let alpha = self.alpha.get(seed, x, y);

        if alpha <= self.thresholds[0] {
            return self.noises[0].get(seed, x, y);
        }

        if alpha >= self.thresholds[self.thresholds.len() - 1] {
            return self.noises[self.noises.len() - 1].get(seed, x, y);
        }

        for i in 1..self.noises.len() {
            if alpha <= self.thresholds[i] {
                let t = (alpha - self.thresholds[i - 1]) * self.normalize[i];
                let lower = self.noises[i - 1].get(seed, x, y);
                let upper = self.noises[i].get(seed, x, y);
                return math_util::lerp_f64(lower, upper, t);
            }
        }

        0.0
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        if self.noises.is_empty() {
            return 0.0;
        }

        let alpha = self.alpha.get_3d(seed, x, y, z);

        if alpha <= self.thresholds[0] {
            return self.noises[0].get_3d(seed, x, y, z);
        }

        if alpha >= self.thresholds[self.thresholds.len() - 1] {
            return self.noises[self.noises.len() - 1].get_3d(seed, x, y, z);
        }

        for i in 1..self.noises.len() {
            if alpha <= self.thresholds[i] {
                let t = (alpha - self.thresholds[i - 1]) * self.normalize[i];
                let lower = self.noises[i - 1].get_3d(seed, x, y, z);
                let upper = self.noises[i].get_3d(seed, x, y, z);
                return math_util::lerp_f64(lower, upper, t);
            }
        }

        0.0
    }
}
