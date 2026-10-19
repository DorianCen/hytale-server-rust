// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/ValueNoise.java`
//! Java `ValueNoise`.

use hytale_math::hash_util;

use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::{fast_floor, lerp, InterpolationFunction, InterpolationMode};

/// Java `ValueNoise`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValueNoise {
    /// Java `protected final GeneralNoise.InterpolationFunction interpolationFunction`.
    pub interpolation_function: InterpolationMode,
}

impl ValueNoise {
    /// Java `ValueNoise(InterpolationFunction)`.
    pub fn new(interpolation_function: InterpolationMode) -> Self {
        Self {
            interpolation_function,
        }
    }
}

impl Default for ValueNoise {
    fn default() -> Self {
        Self::new(InterpolationMode::Hermite)
    }
}

impl NoiseFunction2d for ValueNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        let f = self.interpolation_function;
        #[allow(unused_variables)]
        let seed = seed;

        let x0 = fast_floor(x);
        let y0 = fast_floor(y);
        let x1 = x0 + 1;
        let y1 = y0 + 1;

        let xs = f.interpolate(x - x0 as f64);
        let ys = f.interpolate(y - y0 as f64);

        // Java: HashUtil.random(offsetSeed, x0, y0) — int args widened to long
        let xf0 = lerp(
            hash_util::random3(offset_seed as i64, x0 as i64, y0 as i64),
            hash_util::random3(offset_seed as i64, x1 as i64, y0 as i64),
            xs,
        );
        let xf1 = lerp(
            hash_util::random3(offset_seed as i64, x0 as i64, y1 as i64),
            hash_util::random3(offset_seed as i64, x1 as i64, y1 as i64),
            xs,
        );

        lerp(xf0, xf1, ys) * 2.0 - 1.0
    }
}

impl NoiseFunction3d for ValueNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let f = self.interpolation_function;
        #[allow(unused_variables)]
        let seed = seed;

        let x0 = fast_floor(x);
        let y0 = fast_floor(y);
        let z0 = fast_floor(z);
        let x1 = x0 + 1;
        let y1 = y0 + 1;
        let z1 = z0 + 1;

        let xs = f.interpolate(x - x0 as f64);
        let ys = f.interpolate(y - y0 as f64);
        let zs = f.interpolate(z - z0 as f64);

        let xf00 = lerp(
            hash_util::random4(offset_seed as i64, x0 as i64, y0 as i64, z0 as i64),
            hash_util::random4(offset_seed as i64, x1 as i64, y0 as i64, z0 as i64),
            xs,
        );
        let xf10 = lerp(
            hash_util::random4(offset_seed as i64, x0 as i64, y1 as i64, z0 as i64),
            hash_util::random4(offset_seed as i64, x1 as i64, y1 as i64, z0 as i64),
            xs,
        );
        let xf01 = lerp(
            hash_util::random4(offset_seed as i64, x0 as i64, y0 as i64, z1 as i64),
            hash_util::random4(offset_seed as i64, x1 as i64, y0 as i64, z1 as i64),
            xs,
        );
        let xf11 = lerp(
            hash_util::random4(offset_seed as i64, x0 as i64, y1 as i64, z1 as i64),
            hash_util::random4(offset_seed as i64, x1 as i64, y1 as i64, z1 as i64),
            xs,
        );

        let yf0 = lerp(xf00, xf10, ys);
        let yf1 = lerp(xf01, xf11, ys);

        lerp(yf0, yf1, zs) * 2.0 - 1.0
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_unit_range_invariant() {
        // random() ∈ [0, 1]; lerping and `* 2 - 1` stays within [-1, 1].
        let noise = ValueNoise::new(InterpolationMode::Hermite);
        for &seed in &[3i32, -305, 0xdead_beef_u32 as i32] {
            for i in 0..400 {
                let x = (i as f64) * 0.21870 - 63.0;
                let y = (i as f64) * 1.90538 + 22.0;
                let z = (i as f64) * 0.81355 - 41.0;
                let a = NoiseFunction2d::get(&noise, seed, 1234, x, y);
                assert!(a.is_finite());
                assert!((-1.0..=1.0).contains(&a), "2d v={a} out of [-1,1]");

                let a3 = NoiseFunction3d::get(&noise, seed, 1234, x, y, z);
                assert!(a3.is_finite());
                assert!((-1.0..=1.0).contains(&a3), "3d v={a3} out of [-1,1]");
            }
        }
    }

    #[test]
    fn deterministic_and_seed_sensitive() {
        let noise = ValueNoise::new(InterpolationMode::Linear);
        let (x, y) = (3.75, -11.25);
        assert_eq!(NoiseFunction2d::get(&noise, 42, 0, x, y), NoiseFunction2d::get(&noise, 42, 0, x, y));
        assert_ne!(NoiseFunction2d::get(&noise, 42, 1, x, y), NoiseFunction2d::get(&noise, 42, 2, x, y));
        let (_, _) = (NoiseFunction3d::get(&noise, 42, 0, x, y, 0.5), NoiseFunction3d::get(&noise, 42, 0, x, y, 1.5));
    }
}
