// Ported from: `.hytale-source/.../procedurallib/property/SingleNoiseProperty.java`
//! Java `SingleNoiseProperty`.

use std::rc::Rc;

use crate::noise::function::{NoiseFunction, NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::limit;
use crate::noise::property::NoiseProperty;

/// Java `SingleNoiseProperty` — samples a `NoiseFunction` on `seed + seedOffset`,
/// remapped from `[-1,1]` to `[0,1]`.
pub struct SingleNoiseProperty {
    /// Java `protected final int seedOffset`.
    pub seed_offset: i32,
    /// Java `protected final NoiseFunction function`.
    pub function: Rc<dyn NoiseFunction>,
}

impl SingleNoiseProperty {
    /// Java `SingleNoiseProperty(NoiseFunction)` — `seedOffset = 0`.
    pub fn of_function(function: Rc<dyn NoiseFunction>) -> Self {
        Self::new(0, function)
    }

    /// Java `SingleNoiseProperty(int seedOffset, NoiseFunction function)`.
    pub fn new(seed_offset: i32, function: Rc<dyn NoiseFunction>) -> Self {
        Self {
            seed_offset,
            function,
        }
    }

    /// Java `getSeedOffset()`.
    pub fn get_seed_offset(&self) -> i32 {
        self.seed_offset
    }

    /// Java `getFunction()`.
    pub fn get_function(&self) -> Rc<dyn NoiseFunction> {
        self.function.clone()
    }
}

impl NoiseProperty for SingleNoiseProperty {
    fn get(&self, seed: i32, x: f64, y: f64) -> f64 {
        limit(
            NoiseFunction2d::get(
                self.function.as_ref(),
                seed,
                wrapping_add_i32(seed, self.seed_offset),
                x,
                y,
            ) * 0.5
                + 0.5,
        )
    }

    fn get_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> f64 {
        limit(
            NoiseFunction3d::get(
                self.function.as_ref(),
                seed,
                wrapping_add_i32(seed, self.seed_offset),
                x,
                y,
                z,
            ) * 0.5
                + 0.5,
        )
    }
}

/// Java `+` on ints wraps (e.g. `seed + seedOffset` at `i32::MAX`).
#[inline]
fn wrapping_add_i32(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}
