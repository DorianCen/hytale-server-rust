// Ported from: `.hytale-source/.../procedurallib/property/FractalNoiseProperty.java`
//! Java `FractalNoiseProperty` (+ `FractalMode` / `FractalFunction`).

use std::rc::Rc;

use crate::noise::function::{NoiseFunction, NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::limit;
use crate::noise::support::NoiseProperty;

/// Java `FractalNoiseProperty.FractalFunction` — the per-mode octaves loop.
/// Ported as an enum with methods (Java used anonymous classes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FractalMode {
    /// Java `FBM`.
    Fbm,
    /// Java `BILLOW`.
    Billow,
    /// Java `MULTI_RIGID`.
    MultiRigid,
    /// Java `OLDSCHOOL`.
    Oldschool,
    /// Java `MIN`.
    Min,
    /// Java `MAX`.
    Max,
}

impl FractalMode {
    /// Java `FractalFunction.get(...)` 2d.
    #[allow(clippy::too_many_arguments)]
    pub fn get2d(
        &self,
        seed: i32,
        mut offset_seed: i32,
        mut x: f64,
        mut y: f64,
        octaves: i32,
        lacunarity: f64,
        persistence: f64,
        noise: &dyn NoiseFunction2d,
    ) -> f64 {
        match self {
            FractalMode::Fbm => {
                let mut sum = noise.get(seed, offset_seed, x, y);
                let mut amp = 1.0f64;
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    amp *= persistence;
                    offset_seed = offset_seed.wrapping_add(1); // Java ++offsetSeed
                    sum += noise.get(seed, offset_seed, x, y) * amp;
                }
                limit(sum * 0.5 + 0.5)
            }
            FractalMode::Billow => {
                let mut sum = noise.get(seed, offset_seed, x, y).abs() * 2.0 - 1.0;
                let mut amp = 1.0f64;
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    amp *= persistence;
                    offset_seed = offset_seed.wrapping_add(1);
                    sum += (noise.get(seed, offset_seed, x, y).abs() * 2.0 - 1.0) * amp;
                }
                limit(sum * 0.5 + 0.5)
            }
            FractalMode::MultiRigid => {
                let mut sum = 1.0 - noise.get(seed, offset_seed, x, y).abs();
                let mut amp = 1.0f64;
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    amp *= persistence;
                    offset_seed = offset_seed.wrapping_add(1);
                    sum -= (1.0 - noise.get(seed, offset_seed, x, y).abs()) * amp;
                }
                limit(sum * 0.5 + 0.5)
            }
            FractalMode::Oldschool => {
                let mut max_amp = 0.0f64;
                let mut amp = 1.0f64;
                let mut freq = 1i32;
                let mut sum = 0.0f64;
                let seed = seed.wrapping_sub(1); // Java seed--
                for _ in 0..octaves {
                    // Java `offsetSeed++`: the FIRST octave uses the original
                    // offset seed (the increment happens after the read).
                    sum += noise.get(
                        seed,
                        offset_seed,
                        x * freq as f64,
                        y * freq as f64,
                    ) * amp;
                    offset_seed = offset_seed.wrapping_add(1);
                    max_amp += amp;
                    amp *= persistence;
                    freq <<= 1;
                }
                sum /= max_amp;
                sum *= 0.5;
                sum + 0.5
            }
            FractalMode::Min => {
                let mut min = noise.get(seed, offset_seed, x, y);
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    offset_seed = offset_seed.wrapping_add(1);
                    let d = noise.get(seed, offset_seed, x, y);
                    if d < min {
                        min = d;
                    }
                }
                limit(min * 0.5 + 0.5)
            }
            FractalMode::Max => {
                let mut max = noise.get(seed, offset_seed, x, y);
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    offset_seed = offset_seed.wrapping_add(1);
                    let d = noise.get(seed, offset_seed, x, y);
                    if d > max {
                        max = d;
                    }
                }
                limit(max * 0.5 + 0.5)
            }
        }
    }

    /// Java `FractalFunction.get(...)` 3d.
    #[allow(clippy::too_many_arguments)]
    pub fn get3d(
        &self,
        seed: i32,
        mut offset_seed: i32,
        mut x: f64,
        mut y: f64,
        mut z: f64,
        octaves: i32,
        lacunarity: f64,
        persistence: f64,
        noise: &dyn NoiseFunction3d,
    ) -> f64 {
        match self {
            FractalMode::Fbm => {
                let mut sum = noise.get(seed, offset_seed, x, y, z);
                let mut amp = 1.0f64;
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    z *= lacunarity;
                    amp *= persistence;
                    offset_seed = offset_seed.wrapping_add(1);
                    sum += noise.get(seed, offset_seed, x, y, z) * amp;
                }
                limit(sum * 0.5 + 0.5)
            }
            FractalMode::Billow => {
                let mut sum = noise.get(seed, offset_seed, x, y, z).abs() * 2.0 - 1.0;
                let mut amp = 1.0f64;
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    z *= lacunarity;
                    amp *= persistence;
                    offset_seed = offset_seed.wrapping_add(1);
                    sum += (noise.get(seed, offset_seed, x, y, z).abs() * 2.0 - 1.0) * amp;
                }
                limit(sum * 0.5 + 0.5)
            }
            FractalMode::MultiRigid => {
                let mut sum = 1.0 - noise.get(seed, offset_seed, x, y, z).abs();
                let mut amp = 1.0f64;
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    z *= lacunarity;
                    amp *= persistence;
                    offset_seed = offset_seed.wrapping_add(1);
                    sum -= (1.0 - noise.get(seed, offset_seed, x, y, z).abs()) * amp;
                }
                limit(sum * 0.5 + 0.5)
            }
            FractalMode::Oldschool => {
                let mut max_amp = 0.0f64;
                let mut amp = 1.0f64;
                let mut freq = 1i32;
                let mut sum = 0.0f64;
                let seed = seed.wrapping_sub(1);
                for _ in 0..octaves {
                    // Java `offsetSeed++`: read before the increment.
                    sum += noise.get(
                        seed,
                        offset_seed,
                        x * freq as f64,
                        y * freq as f64,
                        z * freq as f64,
                    ) * amp;
                    offset_seed = offset_seed.wrapping_add(1);
                    max_amp += amp;
                    amp *= persistence;
                    freq <<= 1;
                }
                sum /= max_amp;
                sum *= 0.5;
                sum + 0.5
            }
            FractalMode::Min => {
                let mut min = noise.get(seed, offset_seed, x, y, z);
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    z *= lacunarity;
                    offset_seed = offset_seed.wrapping_add(1);
                    let d = noise.get(seed, offset_seed, x, y, z);
                    if d < min {
                        min = d;
                    }
                }
                limit(min * 0.5 + 0.5)
            }
            FractalMode::Max => {
                let mut max = noise.get(seed, offset_seed, x, y, z);
                for _ in 1..octaves {
                    x *= lacunarity;
                    y *= lacunarity;
                    z *= lacunarity;
                    offset_seed = offset_seed.wrapping_add(1);
                    let d = noise.get(seed, offset_seed, x, y, z);
                    if d > max {
                        max = d;
                    }
                }
                limit(max * 0.5 + 0.5)
            }
        }
    }
}

/// Java `FractalNoiseProperty`.
pub struct FractalNoiseProperty {
    /// Java `protected final int seedOffset`.
    pub seed_offset: i32,
    /// Java `protected final NoiseFunction function`.
    pub function: Rc<dyn NoiseFunction>,
    /// Java `protected final FractalFunction fractalFunction`.
    pub fractal_function: FractalMode,
    /// Java `protected final int octaves`.
    pub octaves: i32,
    /// Java `protected final double lacunarity`.
    pub lacunarity: f64,
    /// Java `protected final double persistence`.
    pub persistence: f64,
}

impl FractalNoiseProperty {
    /// Java `FractalNoiseProperty(...)`.
    pub fn new(
        seed_offset: i32,
        function: Rc<dyn NoiseFunction>,
        fractal_function: FractalMode,
        octaves: i32,
        lacunarity: f64,
        persistence: f64,
    ) -> Self {
        Self {
            seed_offset,
            function,
            fractal_function,
            octaves,
            lacunarity,
            persistence,
        }
    }
}

impl NoiseProperty for FractalNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        self.fractal_function.get2d(
            seed,
            seed.wrapping_add(self.seed_offset),
            x,
            y,
            self.octaves,
            self.lacunarity,
            self.persistence,
            self.function.as_ref(),
        )
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.fractal_function.get3d(
            seed,
            seed.wrapping_add(self.seed_offset),
            x,
            y,
            z,
            self.octaves,
            self.lacunarity,
            self.persistence,
            self.function.as_ref(),
        )
    }
}
