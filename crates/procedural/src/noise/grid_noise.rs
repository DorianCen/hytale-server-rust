// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/GridNoise.java`
//! Java `GridNoise` — unit-cell border/edge noise.

use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::math_util;

/// Java `GridNoise` — `protected final double thicknessX, thicknessY, thicknessZ, thicknessX_m1, thicknessY_m1, thicknessZ_m1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridNoise {
    pub thickness_x: f64,
    pub thickness_y: f64,
    pub thickness_z: f64,
    thickness_x_m1: f64,
    thickness_y_m1: f64,
    thickness_z_m1: f64,
}

impl GridNoise {
    /// Java `GridNoise(double thicknessX, double thicknessY, double thicknessZ)`.
    pub fn new(thickness_x: f64, thickness_y: f64, thickness_z: f64) -> Self {
        Self {
            thickness_x,
            thickness_y,
            thickness_z,
            thickness_x_m1: 1.0 - thickness_x,
            thickness_y_m1: 1.0 - thickness_y,
            thickness_z_m1: 1.0 - thickness_z,
        }
    }
}

impl NoiseFunction2d for GridNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        #[allow(unused_variables)]
        let (seed, offset_seed) = (seed, offset_seed);

        // Java: x = x - MathUtil.floor(x);
        let x = x - math_util::floor(x) as f64;
        let y = y - math_util::floor(y) as f64;

        let mut d = 1.0f64;
        if x < self.thickness_x {
            d = x / self.thickness_x;
        } else if x > self.thickness_x_m1 {
            d = (1.0 - x) / self.thickness_x;
        }
        if y < self.thickness_y {
            let t = y / self.thickness_y;
            if t < d {
                d = t;
            }
        } else if y > self.thickness_y_m1 {
            let t = (1.0 - y) / self.thickness_y;
            if t < d {
                d = t;
            }
        }
        d * 2.0 - 1.0
    }
}

impl NoiseFunction3d for GridNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        #[allow(unused_variables)]
        let (seed, offset_seed) = (seed, offset_seed);

        let x = x - math_util::floor(x) as f64;
        let y = y - math_util::floor(y) as f64;
        let z = z - math_util::floor(z) as f64;

        let mut d = 1.0;
        if x < self.thickness_x {
            d = x / self.thickness_x;
        } else if x > self.thickness_x_m1 {
            d = (1.0 - x) / self.thickness_x;
        }
        if y < self.thickness_y {
            let t = y / self.thickness_y;
            if t < d {
                d = t;
            }
        } else if y > self.thickness_y_m1 {
            let t = (1.0 - y) / self.thickness_y;
            if t < d {
                d = t;
            }
        }
        if z < self.thickness_z {
            let t = z / self.thickness_z;
            if t < d {
                d = t;
            }
        } else if z > self.thickness_z_m1 {
            let t = (1.0 - z) / self.thickness_z;
            if t < d {
                d = t;
            }
        }
        d * 2.0 - 1.0
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_within_unit_range_and_deterministic() {
        let noise = GridNoise::new(0.2, 0.3, 0.15);
        for &seed in &[11i32, -11] {
            for i in 0..400 {
                let x = (i as f64) * 0.2873 - 61.9;
                let y = (i as f64) * 1.7709 + 32.1;
                let z = (i as f64) * 0.6519 - 8.3;
                let a2 = NoiseFunction2d::get(&noise, seed, 5, x, y);
                assert_eq!(a2, NoiseFunction2d::get(&noise, seed, 5, x, y));
                assert!(a2.is_finite());
                assert!((-1.0..=1.0).contains(&a2), "2d v={a2}");

                let a3 = NoiseFunction3d::get(&noise, seed, 5, x, y, z);
                assert_eq!(a3, NoiseFunction3d::get(&noise, seed, 5, x, y, z));
                assert!((-1.0..=1.0).contains(&a3), "3d v={a3}");
            }
        }
    }

    #[test]
    fn periodic_per_unit_cell() {
        let noise = GridNoise::new(0.25, 0.25, 0.25);
        // integer offsets leave the fractional part unchanged
        assert_eq!(NoiseFunction2d::get(&noise, 1, 2, 3.5, 4.5), NoiseFunction2d::get(&noise, 1, 2, -6.5, 101.5));
    }

    #[test]
    fn centre_of_cell_is_far_edge() {
        let noise = GridNoise::new(0.5, 0.5, 0.5);
        // at cell center, d = 1 → 2*1 - 1 = 1
        assert_eq!(NoiseFunction2d::get(&noise, 1, 2, 0.5, 0.5), 1.0);
    }
}
