//! Very fast lookup tables for `cos`, `sin` and `atan2`.
//!
//! Ported from: `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/TrigMathUtil.java`
//!
//! The lookup tables are quite small so that they usually stay in the CPU
//! cache and speed up calculation for very small precision loss.
//!
//! Precision loss of the lookup (documented in the Java sources):
//! - `cos`: max `0.002301108092069626`, avg `0.0006102947235925367`
//! - `sin`: max `0.0023007746785879135`, avg `0.0006102554030407974`
//! - `atan2`: max `0.000010251998901367188`, avg `0.0000039337042763789955`
//!
//! The SIN/COS tables are the "Riven" implementation from java-gaming.org,
//! the ATAN2 table the "Icecore" implementation. Both are lazily built on
//! first use (the Java version builds them in static initializers).

use std::f64::consts::PI as PI_F64;
use std::sync::OnceLock;

/// Java `TrigMathUtil.PI` (= `(float) Math.PI`).
pub const PI: f32 = std::f32::consts::PI;
/// Java `TrigMathUtil.PI_HALF`.
pub const PI_HALF: f32 = PI / 2.0;
/// Java `TrigMathUtil.PI_QUARTER`.
pub const PI_QUARTER: f32 = PI / 4.0;
/// Java `TrigMathUtil.PI2`.
pub const PI2: f32 = PI * 2.0;
/// Java `TrigMathUtil.PI4`.
pub const PI4: f32 = PI * 4.0;

/// Java `TrigMathUtil.radToDeg`.
pub const RAD_TO_DEG: f32 = 180.0 / PI;
/// Java `TrigMathUtil.degToRad`.
pub const DEG_TO_RAD: f32 = PI / 180.0;

const SIN_BITS: u32 = 12;
const SIN_MASK: i32 = !(!0i32 << SIN_BITS); // 4095
const SIN_COUNT: usize = (SIN_MASK as usize) + 1; // 4096

const SIZE_AC: usize = 100_000;
const SIZE_AR: usize = SIZE_AC + 1;

/// Java private static class `TrigMathUtil.Riven` — the sin/cos tables.
struct Riven {
    sin: &'static [f32],
    cos: &'static [f32],
    rad_to_index: f32,
}

/// Java private static class `TrigMathUtil.Icecore` — the atan2 table.
struct Icecore {
    atan2: &'static [f32],
}

static RIVEN: OnceLock<Riven> = OnceLock::new();
static ICECORE: OnceLock<Icecore> = OnceLock::new();

// Java static initializer of `Riven`:
//   SIN_BITS = 12; SIN_MASK = ~(-1 << SIN_BITS); SIN_COUNT = SIN_MASK + 1;
//   radFull = (float) (Math.PI * 2.0); degFull = (float) 360.0;
//   radToIndex = SIN_COUNT / radFull; degToIndex = SIN_COUNT / degFull;
//   for i in 0..SIN_COUNT: SIN[i] = (float) Math.sin((i + 0.5f) / SIN_COUNT * radFull)
//                          COS[i] = (float) Math.cos((i + 0.5f) / SIN_COUNT * radFull)
//   // Four cardinal directions (credits: Nate)
//   for i in 0..360 step 90: SIN[(int) (i * degToIndex) & SIN_MASK] = (float) Math.sin(i * PI / 180)
//                            COS[(int) (i * degToIndex) & SIN_MASK] = (float) Math.cos(i * PI / 180)
fn riven() -> &'static Riven {
    RIVEN.get_or_init(|| {
        // Java computes these lookup arguments with f32 arithmetic and then
        // widens to f64 for `Math.sin`/`Math.cos`, so the port keeps the
        // f32 intermediate (`rad` here).
        let rad_full = (PI_F64 * 2.0) as f32;
        let deg_full = 360.0f32;
        let rad_to_index = SIN_COUNT as f32 / rad_full;
        let deg_to_index = SIN_COUNT as f32 / deg_full;

        let mut sin = vec![0.0f32; SIN_COUNT];
        let mut cos = vec![0.0f32; SIN_COUNT];

        for i in 0..SIN_COUNT {
            let rad = (i as f32 + 0.5) / SIN_COUNT as f32 * rad_full;
            sin[i] = ((rad as f64).sin()) as f32;
            cos[i] = ((rad as f64).cos()) as f32;
        }

        // Four cardinal directions (credits: Nate)
        for i in (0..360).step_by(90) {
            let rad = i as f64 * PI_F64 / 180.0;
            sin[(((i as f32 * deg_to_index) as i32) & SIN_MASK) as usize] = rad.sin() as f32;
            cos[(((i as f32 * deg_to_index) as i32) & SIN_MASK) as usize] = rad.cos() as f32;
        }

        Riven {
            sin: Box::leak(sin.into_boxed_slice()),
            cos: Box::leak(cos.into_boxed_slice()),
            rad_to_index,
        }
    })
}

// Java static initializer of `Icecore`:
//   SIZE_AC = 100000; SIZE_AR = SIZE_AC + 1;
//   ATAN2[i] = (float) Math.atan2(i / SIZE_AC, 1)
fn icecore() -> &'static Icecore {
    ICECORE.get_or_init(|| {
        let mut atan2_table = vec![0.0f32; SIZE_AR];
        for (i, slot) in atan2_table.iter_mut().enumerate() {
            let d = i as f64 / SIZE_AC as f64;
            let x = 1.0f64;
            let y = x * d;
            *slot = f64::atan2(y, x) as f32;
        }
        Icecore {
            atan2: Box::leak(atan2_table.into_boxed_slice()),
        }
    })
}

/// Java `TrigMathUtil.sin(float)`.
#[inline]
pub fn sin(radians: f32) -> f32 {
    // Java: SIN[(int) (rad * radToIndex) & SIN_MASK]
    let riven = riven();
    let table = riven.sin;
    table[((radians * riven.rad_to_index) as i32 & SIN_MASK) as usize]
}

/// Java `TrigMathUtil.cos(float)`.
#[inline]
pub fn cos(radians: f32) -> f32 {
    let riven = riven();
    let table = riven.cos;
    table[((radians * riven.rad_to_index) as i32 & SIN_MASK) as usize]
}

/// Java package-private use of the `degToIndex` factor (kept public only where
/// needed later; currently unused).
///
/// Java `TrigMathUtil.sin(double)` — casts to `float` first, then looks up.
#[inline]
pub fn sin_f64(radians: f64) -> f32 {
    sin(radians as f32)
}

/// Java `TrigMathUtil.cos(double)` — casts to `float` first, then looks up.
#[inline]
pub fn cos_f64(radians: f64) -> f32 {
    cos(radians as f32)
}

/// Java `TrigMathUtil.atan2(float, float)` — the "Icecore" approximation.
///
/// Index math in Java: `(int) (ratio * SIZE_AC)` where the ratio is an f32
/// division; negative ratios are impossible in the reachable branches (each
/// branch negates its arguments first), matching the Rust `as usize` cast.
pub fn atan2(y: f32, x: f32) -> f32 {
    let atan2_table = icecore().atan2;
    if y < 0.0 {
        if x < 0.0 {
            // (y < x) because == (-y > -x)
            if y < x {
                -atan2_table[((x / y) * SIZE_AC as f32) as i32 as usize] - PI_HALF
            } else {
                atan2_table[((y / x) * SIZE_AC as f32) as i32 as usize] - PI
            }
        } else {
            let y = -y;
            if y > x {
                atan2_table[((x / y) * SIZE_AC as f32) as i32 as usize] - PI_HALF
            } else {
                -atan2_table[((y / x) * SIZE_AC as f32) as i32 as usize]
            }
        }
    } else if x < 0.0 {
        let x = -x;
        if y > x {
            atan2_table[((x / y) * SIZE_AC as f32) as i32 as usize] + PI_HALF
        } else {
            -atan2_table[((y / x) * SIZE_AC as f32) as i32 as usize] + PI
        }
    } else if y > x {
        -atan2_table[((x / y) * SIZE_AC as f32) as i32 as usize] + PI_HALF
    } else {
        atan2_table[((y / x) * SIZE_AC as f32) as i32 as usize]
    }
}

/// Java `TrigMathUtil.atan2(double, double)` — casts to `float` first.
#[inline]
pub fn atan2_f64(y: f64, x: f64) -> f32 {
    atan2(y as f32, x as f32)
}

/// Java `TrigMathUtil.atan(double)`: `(float) Math.atan(d)`.
#[inline]
pub fn atan(d: f64) -> f32 {
    d.atan() as f32
}

/// Java `TrigMathUtil.asin(double)`: `(float) Math.asin(d)`.
#[inline]
pub fn asin(d: f64) -> f32 {
    d.asin() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cardinal_directions_are_exact() {
        // The Riven implementation overrides the four cardinal directions
        // with the exact libm results (credits: Nate in the Java source).
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
        assert_eq!(sin(PI_HALF), 1.0);
        assert_eq!(cos(PI), -1.0);
    }

    #[test]
    fn lookup_accuracy_matches_documented_precision_loss() {
        // From the JMH header in the Java file: sin/cos max abs error ≈
        // 0.00231 (≲ 0.0025 for the sampled lattice), atan2 ≈ 0.00001025.
        for &rad in &[0.1f32, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, -1.5] {
            assert!(
                (sin(rad) as f64 - (rad as f64).sin()).abs() < 0.0025,
                "sin lookup err at {rad}"
            );
            assert!(
                (cos(rad) as f64 - (rad as f64).cos()).abs() < 0.0025,
                "cos lookup err at {rad}"
            );
        }
    }

    #[test]
    fn double_entry_points_cast_to_float_like_java() {
        // sin(double) == sin((float) value)
        assert_eq!(sin_f64(0.5), sin(0.5f64 as f32));
        assert_eq!(cos_f64(0.5), cos(0.5f64 as f32));
        assert_eq!(atan2_f64(0.5, 1.0), atan2(0.5f32, 1.0f32));
    }

    #[test]
    fn atan2_matches_native_closely() {
        // Icecore max abs error ≈ 0.00001025 (documented above).
        for &(y, x) in &[
            (1.0f32, 1.0),
            (0.5, 1.0),
            (-0.3, -2.0),
            (2.0, -0.1),
            (-1.0, 0.7),
        ] {
            let approx = atan2(y, x) as f64;
            let exact = (y as f64).atan2(x as f64);
            assert!(
                (approx - exact).abs() < 1.1e-5,
                "atan2({y},{x}): {approx} vs {exact}"
            );
        }
        // Known exact value: atan2(0.5, 1.0) ≈ 0.4636476090.
        assert!((atan2(0.5, 1.0) as f64 - 0.4636476090).abs() < 1.1e-5);
    }

    #[test]
    fn constants_are_floats_as_in_java() {
        assert_eq!(PI, std::f32::consts::PI);
        assert_eq!(RAD_TO_DEG, 180.0f32 / PI);
        assert_eq!(DEG_TO_RAD, PI / 180.0f32);
    }
}
