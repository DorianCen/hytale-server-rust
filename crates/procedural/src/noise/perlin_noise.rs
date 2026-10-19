// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/PerlinNoise.java`
//! Java `PerlinNoise`.

use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::{
    fast_floor, grad_coord_2d, grad_coord_3d, lerp, InterpolationFunction, InterpolationMode,
};

/// Java `PerlinNoise`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerlinNoise {
    /// Java `protected final GeneralNoise.InterpolationFunction interpolationFunction`.
    pub interpolation_function: InterpolationMode,
}

impl PerlinNoise {
    /// Java `PerlinNoise(InterpolationFunction)`.
    pub fn new(interpolation_function: InterpolationMode) -> Self {
        Self {
            interpolation_function,
        }
    }
}

impl Default for PerlinNoise {
    fn default() -> Self {
        // Java callers always pass an InterpolationMode explicitly; mirror the
        // most common one used through the noise loaders (quintic reflection of
        // Perlin's improved-noise curve).
        Self::new(InterpolationMode::Quintic)
    }
}

impl NoiseFunction2d for PerlinNoise {
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        let f = self.interpolation_function;
        let x0 = fast_floor(x);
        let y0 = fast_floor(y);
        let x1 = x0 + 1;
        let y1 = y0 + 1;

        let xs = f.interpolate(x - x0 as f64);
        let ys = f.interpolate(y - y0 as f64);

        let xd0 = x - x0 as f64;
        let yd0 = y - y0 as f64;
        let xd1 = xd0 - 1.0;
        let yd1 = yd0 - 1.0;

        let xf0 = lerp(
            grad_coord_2d(offset_seed, x0, y0, xd0, yd0),
            grad_coord_2d(offset_seed, x1, y0, xd1, yd0),
            xs,
        );
        let xf1 = lerp(
            grad_coord_2d(offset_seed, x0, y1, xd0, yd1),
            grad_coord_2d(offset_seed, x1, y1, xd1, yd1),
            xs,
        );

        lerp(xf0, xf1, ys)
    }
}

#[allow(clippy::too_many_arguments)]
impl NoiseFunction3d for PerlinNoise {
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let f = self.interpolation_function;
        let x0 = fast_floor(x);
        let y0 = fast_floor(y);
        let z0 = fast_floor(z);
        let x1 = x0 + 1;
        let y1 = y0 + 1;
        let z1 = z0 + 1;

        let xs = f.interpolate(x - x0 as f64);
        let ys = f.interpolate(y - y0 as f64);
        let zs = f.interpolate(z - z0 as f64);

        let xd0 = x - x0 as f64;
        let yd0 = y - y0 as f64;
        let zd0 = z - z0 as f64;
        let xd1 = xd0 - 1.0;
        let yd1 = yd0 - 1.0;
        let zd1 = zd0 - 1.0;

        let xf00 = lerp(
            grad_coord_3d(offset_seed, x0, y0, z0, xd0, yd0, zd0),
            grad_coord_3d(offset_seed, x1, y0, z0, xd1, yd0, zd0),
            xs,
        );
        let xf10 = lerp(
            grad_coord_3d(offset_seed, x0, y1, z0, xd0, yd1, zd0),
            grad_coord_3d(offset_seed, x1, y1, z0, xd1, yd1, zd0),
            xs,
        );
        let xf01 = lerp(
            grad_coord_3d(offset_seed, x0, y0, z1, xd0, yd0, zd1),
            grad_coord_3d(offset_seed, x1, y0, z1, xd1, yd0, zd1),
            xs,
        );
        let xf11 = lerp(
            grad_coord_3d(offset_seed, x0, y1, z1, xd0, yd1, zd1),
            grad_coord_3d(offset_seed, x1, y1, z1, xd1, yd1, zd1),
            xs,
        );

        let yf0 = lerp(xf00, xf10, ys);
        let yf1 = lerp(xf01, xf11, ys);

        lerp(yf0, yf1, zs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_deterministic_and_bounded() {
        let noise = PerlinNoise::new(InterpolationMode::Quintic);
        for &seed in &[7i32, -7, 0x5925_fad1] {
            for i in 0..300 {
                let x = (i as f64) * 0.17527 - 45.0;
                let y = (i as f64) * 0.35111 + 61.0;
                let z = (i as f64) * 0.51111 - 13.0;
                let a2 = NoiseFunction2d::get(&noise, seed, 55, x, y);
                assert_eq!(a2, NoiseFunction2d::get(&noise, seed, 55, x, y));
                assert!(a2.is_finite());
                assert!(a2.abs() <= 2.0, "2d |v|={a2}");

                let a3 = NoiseFunction3d::get(&noise, seed, 55, x, y, z);
                assert_eq!(a3, NoiseFunction3d::get(&noise, seed, 55, x, y, z));
                assert!(a3.is_finite());
                assert!(a3.abs() <= 2.0, "3d |v|={a3}");
            }
        }
    }
}
