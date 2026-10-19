// Provisionally ported from: `.hytale-source/hytale-shared-source/HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/MathUtil.java`.
//!
//! ⚠ NOTE(subagent): only the subset used by `logic/**`/`logic/cell/**` is
//! ported here because `crates/hytale-math` (the designated home of MathUtil)
//! is being ported in parallel. When its `math_util` module lands, delete this
//! file and re-point imports — the functions are exact Java ports, so they can
//! be swapped without behavioral change.

/// Java `MathUtil.floor(double)` — exact port (saturating cast + correction).
pub fn floor(d: f64) -> i32 {
    let i = d as i32; // Java `(int) d` (saturating, NaN -> 0); Rust `as` matches.
    if i as f64 <= d {
        return i;
    }
    if d < i32::MIN as f64 {
        return i32::MIN;
    }
    i - 1
}

/// Java private-shape `MathUtil.fastFloor(double)` (`long` flavor).
fn fast_floor_long(d: f64) -> i64 {
    let i = d as i64;
    if i as f64 <= d {
        return i;
    }
    if d < i64::MIN as f64 {
        return i64::MIN;
    }
    i - 1
}

/// Java `MathUtil.fastRound(double)` — returns `long`.
pub fn fast_round(d: f64) -> i64 {
    fast_floor_long(d + 0.5)
}

/// Java `MathUtil.lengthSquared(double, double)`.
pub fn length_squared(x: f64, y: f64) -> f64 {
    x * x + y * y
}

/// Java `MathUtil.lengthSquared(double, double, double)`.
pub fn length_squared3(x: f64, y: f64, z: f64) -> f64 {
    x * x + y * y + z * z
}

/// Java `MathUtil.clamp(double, double, double)`.
pub fn clamp(v: f64, min: f64, max: f64) -> f64 {
    if v > max {
        if v < min {
            min
        } else {
            max
        }
    } else if v < min {
        min
    } else {
        v
    }
}

/// Java `MathUtil.lerp(double, double, double)` — note: clamps `t` to [0, 1]
/// before interpolating (unlike `GeneralNoise.lerp`).
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    lerp_unclamped(a, b, clamp(t, 0.0, 1.0))
}

/// Java `MathUtil.lerpUnclamped(double, double, double)`.
pub fn lerp_unclamped(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}

/// Java `MathUtil.sideOfLine(double, double, double, double, double, double)`.
pub fn side_of_line(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64) -> i32 {
    if (ax - x) * (by - y) - (ay - y) * (bx - x) >= 0.0 {
        1
    } else {
        -1
    }
}

/// Java `MathUtil.distanceToLineSq(double x, double y, double ax, double ay, double bx, double by)`.
pub fn distance_to_line_sq(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx0 = x - ax;
    let dy0 = y - ay;
    let dx1 = bx - ax;
    let dy1 = by - ay;
    distance_to_line_sq_deltas(x, y, ax, ay, bx, by, dx0, dy0, dx1, dy1)
}

/// Java `MathUtil.distanceToLineSq(...10 delta overload...)`.
#[allow(clippy::too_many_arguments)]
pub fn distance_to_line_sq_deltas(
    x: f64,
    y: f64,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    dx_ax: f64,
    dy_ay: f64,
    dbx_ax: f64,
    dby_ay: f64,
) -> f64 {
    // Projection on AB
    let mut t = dx_ax * dbx_ax + dy_ay * dby_ay;
    t /= dbx_ax * dbx_ax + dby_ay * dby_ay;

    // Connection point
    let mut px = ax;
    let mut py = ay;
    if t > 1.0 {
        px = bx;
        py = by;
    } else if t > 0.0 {
        px = ax + t * dbx_ax;
        py = ay + t * dby_ay;
    }

    let dbx_ax = x - px;
    let dby_ay = y - py;

    dbx_ax * dbx_ax + dby_ay * dby_ay
}

/// Java `MathUtil.distanceToInfLineSq(double x, double y, double ax, double ay, double bx, double by)`.
pub fn distance_to_inf_line_sq(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx0 = x - ax;
    let dy0 = y - ay;
    let dx1 = bx - ax;
    let dy1 = by - ay;
    distance_to_inf_line_sq_deltas(x, y, ax, ay, dx0, dy0, dx1, dy1)
}

/// Java `MathUtil.distanceToInfLineSq(...8 delta overload...)`.
#[allow(clippy::too_many_arguments)]
pub fn distance_to_inf_line_sq_deltas(
    x: f64,
    y: f64,
    ax: f64,
    ay: f64,
    dx_ax: f64,
    dy_ay: f64,
    dbx_ax: f64,
    dby_ay: f64,
) -> f64 {
    // Projection on AB
    let mut t = dx_ax * dbx_ax + dy_ay * dby_ay;
    t /= dbx_ax * dbx_ax + dby_ay * dby_ay;

    // Connection point
    let px = ax + t * dbx_ax;
    let py = ay + t * dby_ay;

    let dbx_ax = x - px;
    let dby_ay = y - py;

    dbx_ax * dbx_ax + dby_ay * dby_ay
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_semantics() {
        assert_eq!(floor(1.9), 1);
        assert_eq!(floor(2.0), 2);
        assert_eq!(floor(-0.1), -1);
        assert_eq!(floor(-2.0), -2);
        assert_eq!(floor(0.0), 0);
    }

    #[test]
    fn fast_round_semantics() {
        assert_eq!(fast_round(1.6), 2);
        assert_eq!(fast_round(-1.6), -2);
        assert_eq!(fast_round(-1.4), -1);
    }

    #[test]
    fn line_distances() {
        // Perpendicular distance from (0,1) to the x-axis is 1 (squared: 1)
        assert_eq!(distance_to_inf_line_sq(0.0, 1.0, 0.0, 0.0, 1.0, 0.0), 1.0);
        // Clamped segment distance from a far point at the end point
        let d = distance_to_line_sq(500.0, 1.0, 0.0, 0.0, 1.0, 0.0);
        assert!((d - 1.0 - 499.0 * 499.0).abs() < 1e-9);
        assert_eq!(side_of_line(0.0, 1.0, 0.0, 0.0, 1.0, 0.0), 1);
        assert_eq!(side_of_line(0.0, -1.0, 0.0, 0.0, 1.0, 0.0), -1);
    }
}
