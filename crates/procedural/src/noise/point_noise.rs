// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/PointNoise.java`
//! Java `PointNoise` — spherical falloff around a static point.

use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::math_util;

/// Java `PointNoise`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointNoise {
    x: f64,
    y: f64,
    z: f64,
    inner_radius2: f64,
    outer_radius2: f64,
    /// Java `private transient final double invRange2`.
    inv_range2: f64,
}

impl PointNoise {
    /// Java `PointNoise(double x, double y, double z, double innerRadius, double outerRadius)`.
    pub fn new(x: f64, y: f64, z: f64, inner_radius: f64, outer_radius: f64) -> Self {
        let inner_radius2 = inner_radius * inner_radius;
        let outer_radius2 = outer_radius * outer_radius;
        let range = outer_radius2 - inner_radius2;
        let inv_range2 = if range == 0.0 { 1.0 } else { 1.0 / range };
        Self {
            x,
            y,
            z,
            inner_radius2,
            outer_radius2,
            inv_range2,
        }
    }
}

impl NoiseFunction2d for PointNoise {
    fn get(&self, seed: i32, seed_offset: i32, x: f64, y: f64) -> f64 {
        #[allow(unused_variables)]
        let (seed, seed_offset) = (seed, seed_offset);

        let dist2 = math_util::length_squared(x - self.x, y - self.y);

        if dist2 <= self.inner_radius2 {
            return -1.0;
        }

        if dist2 >= self.outer_radius2 {
            return 1.0;
        }

        -1.0 + 2.0 * (dist2 - self.inner_radius2) * self.inv_range2
    }
}

impl NoiseFunction3d for PointNoise {
    fn get(&self, seed: i32, seed_offset: i32, x: f64, y: f64, z: f64) -> f64 {
        #[allow(unused_variables)]
        let (seed, seed_offset) = (seed, seed_offset);

        // NOTE: Java computes `lengthSquared(x - this.x, y - this.y, this.z - z)`
        // — the z-delta is `(this.z - z)`, not `(z - this.z)`. Squaring makes
        // this value-identical, but the transcription is kept faithful.
        let dist2 = math_util::length_squared3(x - self.x, y - self.y, self.z - z);

        if dist2 <= self.inner_radius2 {
            return -1.0;
        }

        if dist2 >= self.outer_radius2 {
            return 1.0;
        }

        -1.0 + 2.0 * (dist2 - self.inner_radius2) * self.inv_range2
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falloff_matches_specification() {
        // point at origin, inner radius 1, outer radius 3
        let n = PointNoise::new(0.0, 0.0, 0.0, 1.0, 3.0);
        assert_eq!(NoiseFunction2d::get(&n, 1, 2, 0.0, 0.0), -1.0);
        assert_eq!(NoiseFunction3d::get(&n, 1, 2, 0.0, 0.0, 0.0), -1.0);
        assert_eq!(NoiseFunction2d::get(&n, 1, 2, 5.0, 0.0), 1.0);
        assert_eq!(NoiseFunction3d::get(&n, 1, 2, 10.0, 0.0, 0.0), 1.0);
        // dist 2: between inner (1) and outer (3)
        let v = NoiseFunction2d::get(&n, 1, 2, 2.0, 0.0);
        assert!((-1.0..1.0).contains(&v));
        assert_eq!(v, -1.0 + 2.0 * (4.0 - 1.0) * (1.0 / 8.0));
    }

    #[test]
    fn range_and_determinism() {
        let n = PointNoise::new(-4.5, 3.25, 1.5, 0.75, 2.75);
        for i in 0..400 {
            let x = (i as f64) * 0.33081 - 94.0;
            let y = (i as f64) * 1.54077 + 29.0;
            let z = (i as f64) * 0.70232 - 53.0;
            let a = NoiseFunction2d::get(&n, 123, 321, x, y);
            assert_eq!(a, NoiseFunction2d::get(&n, 123, 321, x, y));
            assert!(a.is_finite());
            assert!((-1.0..=1.0).contains(&a), "2d v={a}");

            let a3 = NoiseFunction3d::get(&n, 123, 321, x, y, z);
            assert_eq!(a3, NoiseFunction3d::get(&n, 123, 321, x, y, z));
            assert!((-1.0..=1.0).contains(&a3), "3d v={a3}");
        }
    }
}
