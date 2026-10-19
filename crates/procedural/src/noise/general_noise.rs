// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/GeneralNoise.java`
//! Shared low-level plumbing for the noise generators: coordinate hashing,
//! gradient tables, floor/lerp helpers, output limiting and the interpolation
//! functions shared by Perlin/Value noise.

use crate::noise::double_array::{Double2, Double3};

/// Java `GeneralNoise.X_PRIME`.
pub const X_PRIME: i32 = 1619;
/// Java `GeneralNoise.Y_PRIME`.
pub const Y_PRIME: i32 = 31337;
/// Java `GeneralNoise.Z_PRIME`.
pub const Z_PRIME: i32 = 6971;

/// Java `GeneralNoise.GRAD_2D` — indexed with `hash & 7`.
pub const GRAD_2D: [Double2; 8] = [
    Double2 { x: -1.0, y: -1.0 },
    Double2 { x: 1.0, y: -1.0 },
    Double2 { x: -1.0, y: 1.0 },
    Double2 { x: 1.0, y: 1.0 },
    Double2 { x: 0.0, y: -1.0 },
    Double2 { x: -1.0, y: 0.0 },
    Double2 { x: 0.0, y: 1.0 },
    Double2 { x: 1.0, y: 0.0 },
];

/// Java `GeneralNoise.GRAD_3D` — indexed with `hash & 15`.
/// (Last four entries intentionally repeat the `{1,1,0}` shape pattern of the
/// first four — transcribed exactly as in Java.)
pub const GRAD_3D: [Double3; 16] = [
    Double3 { x: 1.0, y: 1.0, z: 0.0 },
    Double3 { x: -1.0, y: 1.0, z: 0.0 },
    Double3 { x: 1.0, y: -1.0, z: 0.0 },
    Double3 { x: -1.0, y: -1.0, z: 0.0 },
    Double3 { x: 1.0, y: 0.0, z: 1.0 },
    Double3 { x: -1.0, y: 0.0, z: 1.0 },
    Double3 { x: 1.0, y: 0.0, z: -1.0 },
    Double3 { x: -1.0, y: 0.0, z: -1.0 },
    Double3 { x: 0.0, y: 1.0, z: 1.0 },
    Double3 { x: 0.0, y: -1.0, z: 1.0 },
    Double3 { x: 0.0, y: 1.0, z: -1.0 },
    Double3 { x: 0.0, y: -1.0, z: -1.0 },
    Double3 { x: 1.0, y: 1.0, z: 0.0 },
    Double3 { x: 0.0, y: -1.0, z: 1.0 },
    Double3 { x: -1.0, y: 1.0, z: 0.0 },
    Double3 { x: 0.0, y: -1.0, z: -1.0 },
];

/// Java `GeneralNoise.fastFloor(double)`.
#[inline]
pub fn fast_floor(f: f64) -> i32 {
    if f >= 0.0 {
        f as i32
    } else {
        // Java: `(int) f - 1` on int arithmetic → wraps.
        (f as i32).wrapping_sub(1)
    }
}

/// Java `GeneralNoise.fastCeil(double)`.
#[inline]
pub fn fast_ceil(f: f64) -> i32 {
    if f >= 0.0 {
        // Java: `(int) f + 1` on int arithmetic → wraps.
        (f as i32).wrapping_add(1)
    } else {
        f as i32
    }
}

/// Java `GeneralNoise.lerp(double, double, double)`.
#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}

/// Java `GeneralNoise.hash2D(int, int, int)`.
#[inline]
pub fn hash2d(seed: i32, x: i32, y: i32) -> i32 {
    let mut hash = seed;
    hash ^= X_PRIME.wrapping_mul(x);
    hash ^= Y_PRIME.wrapping_mul(y);

    // Java: hash * hash * hash * 60493
    hash = hash
        .wrapping_mul(hash)
        .wrapping_mul(hash)
        .wrapping_mul(60493);
    (hash >> 13) ^ hash
}

/// Java `GeneralNoise.hash3D(int, int, int, int)`.
#[inline]
pub fn hash3d(seed: i32, x: i32, y: i32, z: i32) -> i32 {
    let mut hash = seed;
    hash ^= X_PRIME.wrapping_mul(x);
    hash ^= Y_PRIME.wrapping_mul(y);
    hash ^= Z_PRIME.wrapping_mul(z);

    hash = hash
        .wrapping_mul(hash)
        .wrapping_mul(hash)
        .wrapping_mul(60493);
    (hash >> 13) ^ hash
}

/// Java `GeneralNoise.gradCoord2D(int, int, int, double, double)`.
#[inline]
pub fn grad_coord_2d(seed: i32, x: i32, y: i32, xd: f64, yd: f64) -> f64 {
    let hash = hash2d(seed, x, y);
    let g = GRAD_2D[(hash & 7) as usize];
    xd * g.x + yd * g.y
}

/// Java `GeneralNoise.gradCoord3D(int, int, int, int, double, double, double)`.
#[inline]
pub fn grad_coord_3d(seed: i32, x: i32, y: i32, z: i32, xd: f64, yd: f64, zd: f64) -> f64 {
    let hash = hash3d(seed, x, y, z);
    let g = GRAD_3D[(hash & 15) as usize];
    xd * g.x + yd * g.y + zd * g.z
}

/// Java `GeneralNoise.limit(double)` — clamps into `[0, 1]`.
#[inline]
pub fn limit(val: f64) -> f64 {
    if val < 0.0 {
        0.0
    } else if val > 1.0 {
        1.0
    } else {
        val
    }
}

/// Java `GeneralNoise.InterpolationFunction` (`@FunctionalInterface`).
pub trait InterpolationFunction {
    /// Java `InterpolationFunction.interpolate(double t)`.
    fn interpolate(&self, t: f64) -> f64;
}

/// Java `GeneralNoise.InterpolationMode` — the built-in interpolation curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterpolationMode {
    Linear,
    Hermite,
    Quintic,
}

impl InterpolationFunction for InterpolationMode {
    #[inline]
    fn interpolate(&self, t: f64) -> f64 {
        match self {
            // Java: return t;
            InterpolationMode::Linear => t,
            // Java: return t * t * (3 - 2 * t);
            InterpolationMode::Hermite => t * t * (3.0 - 2.0 * t),
            // Java: return t * t * t * (t * (t * 6 - 15) + 10);
            InterpolationMode::Quintic => t * t * t * (t * (t * 6.0 - 15.0) + 10.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_floor_matches_java_cast_semantics() {
        assert_eq!(fast_floor(0.0), 0);
        assert_eq!(fast_floor(1.9), 1);
        assert_eq!(fast_floor(-0.1), -1);
        // Java fastFloor(-2.0): -2 >= 0 is false → (int)(-2.0) - 1 = -3
        assert_eq!(fast_floor(-2.0), -3);
        assert_eq!(fast_floor(123456.789), 123456);
    }

    #[test]
    fn limit_clamps_to_unit_range() {
        assert_eq!(limit(-5.0), 0.0);
        assert_eq!(limit(0.25), 0.25);
        assert_eq!(limit(2.0), 1.0);
    }

    #[test]
    fn interpolation_curves_endpoints() {
        for mode in [
            InterpolationMode::Linear,
            InterpolationMode::Hermite,
            InterpolationMode::Quintic,
        ] {
            assert_eq!(mode.interpolate(0.0), 0.0);
            assert_eq!(mode.interpolate(1.0), 1.0);
            // monotonic inside [0,1]
            let mut prev = mode.interpolate(0.0);
            for i in 1..=10 {
                let v = mode.interpolate(i as f64 / 10.0);
                assert!(v >= prev);
                assert!((0.0..=1.0).contains(&v));
                prev = v;
            }
        }
    }
}
