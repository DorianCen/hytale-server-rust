// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/SimplexNoise.java`
//! Java `SimplexNoise` (the two- and three-dimensional simplex noise used as a
//! singleton, `SimplexNoise.INSTANCE`).

use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::{fast_floor, grad_coord_2d, grad_coord_3d};

// Java fields:
// private final static double F2 = 1.0 / 2.0;
// private final static double P1_F2 = F2 - 1;
// private final static double G2 = 1.0 / 4.0;
// private final static double F3 = 1.0 / 3.0;
// private final static double G3 = 1.0 / 6.0;
// private final static double G33 = G3 * 3 - 1;
const F2: f64 = 1.0 / 2.0;
const P1_F2: f64 = F2 - 1.0;
const G2: f64 = 1.0 / 4.0;
const F3: f64 = 1.0 / 3.0;
const G3: f64 = 1.0 / 6.0;
const G33: f64 = G3 * 3.0 - 1.0;

/// Java `SimplexNoise` — private-constructor singleton; the Rust port is a
/// stateless struct (`SimplexNoise::INSTANCE` is a zero-cost constant).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SimplexNoise;

impl SimplexNoise {
    /// Java `SimplexNoise.INSTANCE`.
    pub const INSTANCE: SimplexNoise = SimplexNoise;

    pub fn new() -> Self {
        Self
    }
}

impl NoiseFunction2d for SimplexNoise {
    #[inline]
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        let mut t = (x + y) * F2;
        let i = fast_floor(x + t);
        let j = fast_floor(y + t);

        t = (i.wrapping_add(j)) as f64 * G2;
        let x0_big = i as f64 - t;
        let y0_big = j as f64 - t;

        let x0 = x - x0_big;
        let y0 = y - y0_big;

        let (i1, j1): (i32, i32) = if x0 > y0 { (1, 0) } else { (0, 1) };

        // Java: t = (float) 0.5 - x0 * x0 - y0 * y0;
        // (the (float) cast applies to the literal 0.5 → exact 0.5; rest is double)
        t = 0.5 - x0 * x0 - y0 * y0;
        let n0 = if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord_2d(offset_seed, i, j, x0, y0)
        };

        let x1 = x0 - i1 as f64 + G2;
        let y1 = y0 - j1 as f64 + G2;
        t = 0.5 - x1 * x1 - y1 * y1;
        let n1 = if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord_2d(offset_seed, i + i1, j + j1, x1, y1)
        };

        let x2 = x0 + P1_F2;
        let y2 = y0 + P1_F2;
        t = 0.5 - x2 * x2 - y2 * y2;
        let n2 = if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord_2d(offset_seed, i + 1, j + 1, x2, y2)
        };

        50.0 * (n0 + n1 + n2)
    }
}

impl NoiseFunction3d for SimplexNoise {
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let mut t = (x + y + z) * F3;
        let i = fast_floor(x + t);
        let j = fast_floor(y + t);
        let k = fast_floor(z + t);

        t = (i.wrapping_add(j).wrapping_add(k)) as f64 * G3;
        let x0 = x - (i as f64 - t);
        let y0 = y - (j as f64 - t);
        let z0 = z - (k as f64 - t);

        let (i1, j1, k1, i2, j2, k2): (i32, i32, i32, i32, i32, i32);

        if x0 >= y0 {
            if y0 >= z0 {
                i1 = 1;
                j1 = 0;
                k1 = 0;
                i2 = 1;
                j2 = 1;
                k2 = 0;
            } else if x0 >= z0 {
                i1 = 1;
                j1 = 0;
                k1 = 0;
                i2 = 1;
                j2 = 0;
                k2 = 1;
            } else {
                // x0 < z0
                i1 = 0;
                j1 = 0;
                k1 = 1;
                i2 = 1;
                j2 = 0;
                k2 = 1;
            }
        } else {
            // x0 < y0
            if y0 < z0 {
                i1 = 0;
                j1 = 0;
                k1 = 1;
                i2 = 0;
                j2 = 1;
                k2 = 1;
            } else if x0 < z0 {
                i1 = 0;
                j1 = 1;
                k1 = 0;
                i2 = 0;
                j2 = 1;
                k2 = 1;
            } else {
                // x0 >= z0
                i1 = 0;
                j1 = 1;
                k1 = 0;
                i2 = 1;
                j2 = 1;
                k2 = 0;
            }
        }

        let x1 = x0 - i1 as f64 + G3;
        let y1 = y0 - j1 as f64 + G3;
        let z1 = z0 - k1 as f64 + G3;
        let x2 = x0 - i2 as f64 + F3;
        let y2 = y0 - j2 as f64 + F3;
        let z2 = z0 - k2 as f64 + F3;
        let x3 = x0 + G33;
        let y3 = y0 + G33;
        let z3 = z0 + G33;

        t = 0.6 - x0 * x0 - y0 * y0 - z0 * z0;
        let n0 = if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord_3d(offset_seed, i, j, k, x0, y0, z0)
        };

        t = 0.6 - x1 * x1 - y1 * y1 - z1 * z1;
        let n1 = if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord_3d(offset_seed, i + i1, j + j1, k + k1, x1, y1, z1)
        };

        t = 0.6 - x2 * x2 - y2 * y2 - z2 * z2;
        let n2 = if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord_3d(offset_seed, i + i2, j + j2, k + k2, x2, y2, z2)
        };

        t = 0.6 - x3 * x3 - y3 * y3 - z3 * z3;
        let n3 = if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord_3d(offset_seed, i + 1, j + 1, k + 1, x3, y3, z3)
        };

        32.0 * (n0 + n1 + n2 + n3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOISE: SimplexNoise = SimplexNoise::INSTANCE;

    #[test]
    fn output_is_finite_and_deterministic() {
        for &seed in &[42i32, -98765, i32::MAX, i32::MIN, 0x5925_fad1] {
            for i in 0..200 {
                let x = (i as f64) * 0.27183 - 41.7;
                let y = (i as f64) * 1.31471 + 19.3;
                let z = (i as f64) * 0.61803 - 88.1;
                let a = NoiseFunction2d::get(&NOISE, seed, 7777, x, y);
                let b = NoiseFunction2d::get(&NOISE, seed, 7777, x, y);
                assert_eq!(a, b, "2d not deterministic at seed={seed} x={x} y={y}");
                assert!(a.is_finite(), "2d NaN/inf at seed={seed} x={x} y={y}");

                let a3 = NoiseFunction3d::get(&NOISE, seed, 7777, x, y, z);
                let b3 = NoiseFunction3d::get(&NOISE, seed, 7777, x, y, z);
                assert_eq!(a3, b3, "3d not deterministic at seed={seed}");
                assert!(
                    a3.is_finite(),
                    "3d NaN/inf at seed={seed} x={x} y={y} z={z}"
                );
                assert!(a3.abs() <= 2.0, "3d out of expected |v|<=2 range: {a3}");
            }
        }
    }

    #[test]
    fn range_invariants_2d() {
        // Theoretical max for this implementation is well within ±2
        // (empirically ±1); 2 bound is safe without hard-coding goldens.
        for &seed in &[1i32, 0x1234_5678, -1] {
            let mut max_v = 0.0f64;
            for i in 0..500 {
                let x = (i as f64) * 0.1137 - 28.0;
                let y = (i as f64) * 0.2357 + 14.0;
                let v = NoiseFunction2d::get(&NOISE, seed, 99, x, y);
                assert!(v.is_finite());
                max_v = max_v.max(v.abs());
            }
            assert!(max_v <= 2.0, "2d max |v| = {max_v}");
        }
    }

    #[test]
    fn int_extremes_do_not_panic() {
        // Same-position enormous coordinates exercising the fastFloor casts.
        let big = 1.0e18;
        assert!(NoiseFunction2d::get(&NOISE, 1, 2, big, 0.0).is_finite());
        assert!(NoiseFunction3d::get(&NOISE, 1, 2, big, big, big).is_finite());
        assert!(NoiseFunction2d::get(&NOISE, 1, 2, -big, -big).is_finite());
        assert!(NoiseFunction3d::get(&NOISE, 1, 2, f64::NAN, 0.0, 0.0).is_nan());
    }
}
