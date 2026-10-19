// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/ConstantNoise.java`
//! Java `ConstantNoise`.

use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};

/// Java `ConstantNoise`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConstantNoise {
    /// Java `protected final double value`.
    pub value: f64,
}

impl ConstantNoise {
    /// Java `ConstantNoise(double value)`.
    pub fn new(value: f64) -> Self {
        Self { value }
    }
}

impl NoiseFunction2d for ConstantNoise {
    #[inline]
    fn get(&self, _seed: i32, _offset_seed: i32, _x: f64, _y: f64) -> f64 {
        self.value
    }
}

impl NoiseFunction3d for ConstantNoise {
    #[inline]
    fn get(&self, _seed: i32, _offset_seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
        self.value
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_returns_value() {
        let n = ConstantNoise::new(0.75);
        assert_eq!(NoiseFunction2d::get(&n, 1, 2, 3.0, 4.0), 0.75);
        assert_eq!(NoiseFunction3d::get(&n, 1, 2, 3.0, 4.0, 5.0), 0.75);
        let n2 = ConstantNoise::new(-0.25);
        assert_eq!(NoiseFunction2d::get(&n2, 9, 9, 0.0, 0.0), -0.25);
    }
}
