// Provisionally ported from: `.hytale-source/hytale-shared-source/HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/TrigMathUtil.java`
//! ⚠ NOTE(subagent): only the constants and the Riven sin/cos lookup used by
//! the cell noise `DIRECTION` mode are ported here, because
//! `crates/hytale-math` (the designated home of TrigMathUtil) is being ported
//! in parallel. Transcribed exactly, including the lookup-table construction,
//! precision (Java `float`) and cardinal-direction overrides.

/// Java `TrigMathUtil.PI = 3.1415927f` (Java's own float constant — keep verbatim).
#[allow(clippy::approx_constant)]
pub const PI: f32 = 3.1415927f32;
/// Java `TrigMathUtil.PI_HALF`.
pub const PI_HALF: f32 = PI / 2.0;
/// Java `TrigMathUtil.PI_QUARTER`.
pub const PI_QUARTER: f32 = PI / 4.0;
/// Java `TrigMathUtil.PI2`.
pub const PI2: f32 = PI * 2.0;
/// Java `TrigMathUtil.PI4`.
pub const PI4: f32 = PI * 4.0;

// ------------------------------------------------------------------
// SIN / COS METHODS
// This code was published at java-gaming.org. Implementation by Riven.
// ------------------------------------------------------------------

const SIN_BITS: i32 = 12;
const SIN_MASK: i32 = !(-1i32 << SIN_BITS); // Java: ~(-1 << SIN_BITS)
const SIN_COUNT: i32 = SIN_MASK + 1;

const RAD_FULL: f32 = (std::f64::consts::PI * 2.0) as f32; // Java: (float) (Math.PI * 2.0)
const DEG_FULL: f32 = 360.0;
const RAD_TO_INDEX: f32 = SIN_COUNT as f32 / RAD_FULL;
const DEG_TO_INDEX: f32 = SIN_COUNT as f32 / DEG_FULL;

// Java static initializer:
//   for (int i = 0; i < SIN_COUNT; i++) {
//       SIN[i] = (float) Math.sin((i + 0.5f) / SIN_COUNT * radFull);
//       COS[i] = (float) Math.cos((i + 0.5f) / SIN_COUNT * radFull);
//   }
//
//   // Four cardinal directions (credits: Nate)
//   for (int i = 0; i < 360; i += 90) {
//       SIN[(int) (i * degToIndex) & SIN_MASK] = (float) Math.sin(i * Math.PI / 180.0);
//       COS[(int) (i * degToIndex) & SIN_MASK] = (float) Math.cos(i * Math.PI / 180.0);
//   }
//
// Rust `const fn` cannot evaluate `f64::sin`, so the one-time Java static
// initializer becomes an explicit lazy init (cost: exactly once per process).
type Tables = ([f32; SIN_COUNT as usize], [f32; SIN_COUNT as usize]);

static SIN_COS: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();

fn sin_cos_tables() -> &'static Tables {
    SIN_COS.get_or_init(|| {
        let mut table_sin = [0f32; SIN_COUNT as usize];
        let mut table_cos = [0f32; SIN_COUNT as usize];

        let mut i: i32 = 0;
        while i < SIN_COUNT {
            // (i + 0.5f) / SIN_COUNT * radFull — float arithmetic, widened to
            // double for Math.sin, result cast back to float.
            let x = (i as f32 + 0.5) / SIN_COUNT as f32 * RAD_FULL;
            table_sin[i as usize] = (x as f64).sin() as f32;
            table_cos[i as usize] = (x as f64).cos() as f32;
            i += 1;
        }

        // Four cardinal directions (credits: Nate)
        let mut deg = 0;
        while deg < 360 {
            let idx = (((deg as f32) * DEG_TO_INDEX) as i32 & SIN_MASK) as usize;
            let exact = (deg as f64) * std::f64::consts::PI / 180.0;
            table_sin[idx] = exact.sin() as f32;
            table_cos[idx] = exact.cos() as f32;
            deg += 90;
        }

        (table_sin, table_cos)
    })
}

/// Java `TrigMathUtil.sin(float)`.
#[inline]
pub fn sin(radians: f32) -> f32 {
    let tables = sin_cos_tables();
    tables.0[((radians * RAD_TO_INDEX) as i32 & SIN_MASK) as usize]
}

/// Java `TrigMathUtil.cos(float)`.
#[inline]
pub fn cos(radians: f32) -> f32 {
    let tables = sin_cos_tables();
    tables.1[((radians * RAD_TO_INDEX) as i32 & SIN_MASK) as usize]
}

/// Java `TrigMathUtil.sin(double)` → returns float.
#[inline]
pub fn sin_f64(radians: f64) -> f32 {
    sin(radians as f32)
}

/// Java `TrigMathUtil.cos(double)` → returns float.
#[inline]
pub fn cos_f64(radians: f64) -> f32 {
    cos(radians as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_java_initialization_for_selected_indices() {
        // The lookup must agree with the exact Java construction expression
        // (samples at several indices; endpoints and cardinal overrides).
        // NOTE: indices 0 and SIN_COUNT/2 are later overridden by the Java
        // cardinal-direction loop (deg = 0 and 180), so they are excluded here
        // and asserted separately below.
        for i in [1usize, 2, SIN_COUNT as usize - 1] {
            let x = (i as f32 + 0.5) / SIN_COUNT as f32 * RAD_FULL;
            let expected_sin = (x as f64).sin() as f32;
            let expected_cos = (x as f64).cos() as f32;
            let (sin_table, cos_table) = sin_cos_tables();
            assert_eq!(sin_table[i], expected_sin);
            assert_eq!(cos_table[i], expected_cos);
        }
        // Cardinal overrides: SIN[0] = sin(0) = 0.0 exactly,
        // SIN[SIN_COUNT/2] = sin(pi) ≈ 1.2246e-16, COS[SIN_COUNT/2] = -1.0.
        let (sin_table, cos_table) = sin_cos_tables();
        assert_eq!(sin_table[0], 0.0);
        assert_eq!(sin_table[SIN_COUNT as usize / 2], 1.2246468e-16_f32);
        assert_eq!(cos_table[SIN_COUNT as usize / 2], -1.0);
    }

    #[test]
    fn cardinal_direction_overrides() {
        // Java overrides indices for 0, 90, 180, 270 degrees with exact values.
        let (sin_table, cos_table) = sin_cos_tables();
        for deg in [0.0f64, 90.0, 180.0, 270.0] {
            let rad = (deg * std::f64::consts::PI / 180.0) as f32;
            let idx = ((rad * RAD_TO_INDEX) as i32 & SIN_MASK) as usize;
            let expected_sin = (deg * std::f64::consts::PI / 180.0).sin() as f32;
            let expected_cos = (deg * std::f64::consts::PI / 180.0).cos() as f32;
            assert_eq!(sin_table[idx], expected_sin, "sin deg={}", deg);
            assert_eq!(cos_table[idx], expected_cos, "cos deg={}", deg);
        }
    }

    #[test]
    fn sin_cos_approximation_reasonable() {
        let mut ang = -7.5f32;
        loop {
            let s = sin(ang);
            let c = cos(ang);
            assert!((s - (ang as f64).sin() as f32).abs() < 0.01, "sin {}", ang);
            assert!((c - (ang as f64).cos() as f32).abs() < 0.01, "cos {}", ang);
            ang += 0.37;
            if ang > 7.5 {
                break;
            }
        }
    }
}
