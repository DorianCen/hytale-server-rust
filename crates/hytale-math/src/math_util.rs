//! Assorted math helpers used across the server and world generation.
//!
//! Ported from: `HytaleServer/Math/src/main/java/com/hypixel/hytale/math/util/MathUtil.java`
//!
//! Differences from Java (documented so the porting doc can reference them):
//! - Overloads are disambiguated by type/arity suffixes (`_f32`, `_f64`,
//!   `_i32`, `_i64`, number-of-arguments) because Rust has no method
//!   overloading.
//! - The three `rotateVectorYAxis` overloads and the two hit-normal helpers
//!   (`getRotationForHitNormal`, `getNameForHitNormal`) are **not** ported
//!   here: they operate on `org.joml` vector/`Rotation3f` types that live in a
//!   later vector crate. Everything else is ported.
//! - `randomInt(int,int)`, `randomDouble(double,double)`,
//!   `randomFloat(float,float)` are **not** ported: they draw from
//!   `ThreadLocalRandom`/`Math.random()`, which are non-deterministic and
//!   cannot reproduce bit-exact Java state. World generation code that needs
//!   randomness passes `FastRandom`/`HashUtil` instead.

/// Java `MathUtil.EPSILON_DOUBLE` = `Math.ulp(1.0)` = 2^-52.
pub const EPSILON_DOUBLE: f64 = f64::EPSILON;

/// Java `MathUtil.EPSILON_FLOAT` = `Math.ulp(1.0f)` = 2^-23.
pub const EPSILON_FLOAT: f32 = f32::EPSILON;

/// Length-squared below which a vector is treated as degenerate (effectively
/// zero length), used to guard normalization against producing a non-finite
/// result.
pub const ZERO_LENGTH_EPSILON: f64 = 1e-8;

/// Java field `MathUtil.PITCH_EDGE_PADDING` (public, mutated in Java; kept as
/// a constant here — nothing in the ported code writes to it).
pub const PITCH_EDGE_PADDING: f32 = 0.01;

/// Java `MathUtil.abs(int)` — the branch-free `((i + mask) ^ mask)` identity,
/// which returns `i32::MIN` unchanged for `i32::MIN` (same as Java).
pub fn abs(i: i32) -> i32 {
    let mask = i >> 31;
    i.wrapping_add(mask) ^ mask
}

/// Java `MathUtil.floor(double)` returning `int`, including Java's saturating
/// `(int)` cast semantics.
pub fn floor(d: f64) -> i32 {
    let i = d as i32;
    if (i as f64) <= d {
        return i;
    }
    if d < i32::MIN as f64 {
        return i32::MIN;
    }
    i - 1
}

/// Java `MathUtil.ceil(double)` returning `int`.
pub fn ceil(d: f64) -> i32 {
    let i = d as i32;
    if d > 0.0 && d != (i as f64) {
        if d > i32::MAX as f64 {
            return i32::MAX;
        }
        return i + 1;
    }
    i
}

/// Java `MathUtil.round(double, int)`: `Math.round(d * pow) / pow`.
pub fn round(d: f64, p: i32) -> f64 {
    let pow = 10f64.powi(p);
    // Java Math.round(double) == floor(x + 0.5) cast to long (saturating).
    ((d * pow + 0.5).floor() as i64) as f64 / pow
}

/// Java `MathUtil.within(double, double, double)`.
pub fn within(val: f64, min: f64, max: f64) -> bool {
    val >= min && val <= max
}

/// Java `MathUtil.minValue(double v, double a, int c)`.
pub fn min_value_f64(v: f64, a: f64, c: i32) -> f64 {
    let mut v = v;
    if a < v {
        v = a;
    }
    let c = c as f64;
    if c < v {
        v = c;
    }
    v
}

/// Java `MathUtil.minValue(int v, int a, int c)`.
pub fn min_value_i32(v: i32, a: i32, c: i32) -> i32 {
    let mut v = v;
    if a < v {
        v = a;
    }
    if c < v {
        v = c;
    }
    v
}

/// Java `MathUtil.maxValue(double v, double a, double b, double c)`.
pub fn max_value4_f64(v: f64, a: f64, b: f64, c: f64) -> f64 {
    let mut v = v;
    if a > v {
        v = a;
    }
    if b > v {
        v = b;
    }
    if c > v {
        v = c;
    }
    v
}

/// Java `MathUtil.maxValue(double v, double a, double b)` and the identical
/// `maxValue(double v, double a, double c)` overload (same logic, different
/// parameter name only).
pub fn max_value3_f64(v: f64, a: f64, b: f64) -> f64 {
    let mut v = v;
    if a > v {
        v = a;
    }
    if b > v {
        v = b;
    }
    v
}

/// Java `MathUtil.maxValue(byte v, byte a, byte b)`.
pub fn max_value3_i8(v: i8, a: i8, b: i8) -> i8 {
    let mut v = v;
    if a > v {
        v = a;
    }
    if b > v {
        v = b;
    }
    v
}

/// Java `MathUtil.maxValue(byte v, byte a, byte b, byte c)`.
pub fn max_value4_i8(v: i8, a: i8, b: i8, c: i8) -> i8 {
    let mut v = v;
    if a > v {
        v = a;
    }
    if b > v {
        v = b;
    }
    if c > v {
        v = c;
    }
    v
}

/// Java `MathUtil.maxValue(int v, int a, int b)`.
pub fn max_value3_i32(v: i32, a: i32, b: i32) -> i32 {
    let mut v = v;
    if a > v {
        v = a;
    }
    if b > v {
        v = b;
    }
    v
}

/// Java `MathUtil.maxValue(double v, double a)`.
pub fn max_value_f64(v: f64, a: f64) -> f64 {
    if a > v {
        a
    } else {
        v
    }
}

/// Java `MathUtil.lengthSquared(double x, double y)`.
pub fn length_squared2_f64(x: f64, y: f64) -> f64 {
    x * x + y * y
}

/// Java `MathUtil.length(double x, double y)`.
pub fn length2_f64(x: f64, y: f64) -> f64 {
    length_squared2_f64(x, y).sqrt()
}

/// Java `MathUtil.lengthSquared(double x, double y, double z)`.
pub fn length_squared3_f64(x: f64, y: f64, z: f64) -> f64 {
    x * x + y * y + z * z
}

/// Java `MathUtil.length(double x, double y, double z)`.
pub fn length3_f64(x: f64, y: f64, z: f64) -> f64 {
    length_squared3_f64(x, y, z).sqrt()
}

/// Java `MathUtil.clipToZero(double)` at `EPSILON_DOUBLE`.
pub fn clip_to_zero_f64(v: f64) -> f64 {
    clip_to_zero_f64_eps(v, EPSILON_DOUBLE)
}

/// Java `MathUtil.clipToZero(double, double)`.
pub fn clip_to_zero_f64_eps(v: f64, epsilon: f64) -> f64 {
    if v >= -epsilon && v <= epsilon {
        0.0
    } else {
        v
    }
}

/// Java `MathUtil.clipToZero(float)` at `EPSILON_FLOAT`.
pub fn clip_to_zero_f32(v: f32) -> f32 {
    clip_to_zero_f32_eps(v, EPSILON_FLOAT)
}

/// Java `MathUtil.clipToZero(float, float)`.
pub fn clip_to_zero_f32_eps(v: f32, epsilon: f32) -> f32 {
    if v >= -epsilon && v <= epsilon {
        0.0
    } else {
        v
    }
}

/// Java `MathUtil.closeToZero(double)` at `EPSILON_DOUBLE`.
pub fn close_to_zero_f64(v: f64) -> bool {
    close_to_zero_f64_eps(v, EPSILON_DOUBLE)
}

/// Java `MathUtil.closeToZero(double, double)`.
pub fn close_to_zero_f64_eps(v: f64, epsilon: f64) -> bool {
    v >= -epsilon && v <= epsilon
}

/// Java `MathUtil.closeToZero(float)` at `EPSILON_FLOAT`.
pub fn close_to_zero_f32(v: f32) -> bool {
    close_to_zero_f32_eps(v, EPSILON_FLOAT)
}

/// Java `MathUtil.closeToZero(float, float)`.
pub fn close_to_zero_f32_eps(v: f32, epsilon: f32) -> bool {
    v >= -epsilon && v <= epsilon
}

/// Java `MathUtil.clamp(double, double, double)` — note this is *not* the
/// `f64::clamp` NaN ordering; ported verbatim.
pub fn clamp_f64(v: f64, min: f64, max: f64) -> f64 {
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

/// Java `MathUtil.clamp(float, float, float)`.
pub fn clamp_f32(v: f32, min: f32, max: f32) -> f32 {
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

/// Java `MathUtil.clamp(int, int, int)`.
pub fn clamp_i32(v: i32, min: i32, max: i32) -> i32 {
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

/// Java `MathUtil.clamp(long, long, long)`.
pub fn clamp_i64(v: i64, min: i64, max: i64) -> i64 {
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

/// Java `MathUtil.saturatedMultiply(long, long)` — saturates at
/// `i64::MAX`/`i64::MIN` instead of overflowing.
pub fn saturated_multiply(a: i64, b: i64) -> i64 {
    a.saturating_mul(b)
}

/// Java `MathUtil.getPercentageOf(int, int)`: `(int)(((double) index / (max - 1)) * 100)`.
pub fn get_percentage_of(index: i32, max: i32) -> i32 {
    ((index as f64 / (max as f64 - 1.0)) * 100.0) as i32
}

/// Java `MathUtil.percent(int, int)`.
pub fn percent(v: i32, total: i32) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (v as f64 * 100.0) / total as f64
}

/// Java `MathUtil.fastRound(float)` returning `int`: `fastFloor(f + 0.5f)`.
pub fn fast_round_f32(f: f32) -> i32 {
    fast_floor_f32(f + 0.5)
}

/// Java `MathUtil.fastRound(double)` returning `long`: `fastFloor(d + 0.5)`.
pub fn fast_round_f64(d: f64) -> i64 {
    fast_floor_f64(d + 0.5)
}

/// Java `MathUtil.fastFloor(float)` returning `int`.
pub fn fast_floor_f32(f: f32) -> i32 {
    let i = f as i32;
    if (i as f32) <= f {
        return i;
    }
    if f < i32::MIN as f32 {
        return i32::MIN;
    }
    i - 1
}

/// Java `MathUtil.fastFloor(double)` returning `long`.
pub fn fast_floor_f64(d: f64) -> i64 {
    let i = d as i64;
    if (i as f64) <= d {
        return i;
    }
    if d < i64::MIN as f64 {
        return i64::MIN;
    }
    i - 1
}

/// Java `MathUtil.fastCeil(float)` returning `int`.
pub fn fast_ceil_f32(f: f32) -> i32 {
    let i = f as i32;
    if f > 0.0 && f != (i as f32) {
        if f > i32::MAX as f32 {
            return i32::MAX;
        }
        return i + 1;
    }
    i
}

/// Java `MathUtil.fastCeil(double)` returning `long`.
pub fn fast_ceil_f64(d: f64) -> i64 {
    let i = d as i64;
    if d > 0.0 && d != (i as f64) {
        if d > i64::MAX as f64 {
            return i64::MAX;
        }
        return i + 1;
    }
    i
}

/// Java `halfFloatToFloat(int)` — half-precision (f16) bits to `f32`.
///
/// From <http://stackoverflow.com/questions/6162651/half-precision-floating-point-in-java>;
/// ignores the higher 16 bits.
pub fn half_float_to_float(hbits: i32) -> f32 {
    /// Java `Float.intBitsToFloat`: collapses every NaN bit pattern to the
    /// canonical `Float.NaN` (0x7FC00000). Rust's `f32::from_bits` preserves
    /// the payload, so canonicalize manually for bit-exactness.
    #[inline]
    fn java_int_bits_to_float(bits: u32) -> f32 {
        let v = f32::from_bits(bits);
        if v.is_nan() { f32::NAN } else { v }
    }

    let mut mant = hbits & 0x03ff; // 10 bits mantissa
    let mut exp = hbits & 0x7c00; // 5 bits exponent
    if exp == 0x7c00 {
        // NaN/Inf
        exp = 0x3fc00; // -> NaN/Inf
    } else if exp != 0 {
        // normalized value
        exp += 0x1c000; // exp - 15 + 127
        if mant == 0 && exp > 0x1c400 {
            // smooth transition
            return java_int_bits_to_float(((hbits & 0x8000) << 16) as u32 | ((exp << 13) as u32) | 0x3ff);
        }
    } else if mant != 0 {
        // && exp==0 -> subnormal
        exp = 0x1c400; // make it normal
        loop {
            mant <<= 1; // mantissa * 2
            exp -= 0x400; // decrease exp by 1
            if (mant & 0x400) != 0 {
                break;
            }
        }
        mant &= 0x3ff; // discard subnormal bit
    } // else +/-0 -> +/-0
    java_int_bits_to_float(
        (((hbits & 0x8000) << 16) as u32) // sign << (31 - 15)
            | (((exp | mant) << 13) as u32), // value << (23 - 10)
    )
}

/// Java `halfFloatFromFloat(float)` — `f32` bits to half-precision.
///
/// Returns all higher 16 bits as 0 for all results.
pub fn half_float_from_float(fval: f32) -> i32 {
    let fbits = fval.to_bits() as i32;
    let sign = (fbits >> 16) & 0x8000; // sign only (Java >>> on value < 2^16 = plain shift)
    let val = (fbits & 0x7fff_ffff).wrapping_add(0x1000); // rounded value

    if val >= 0x4780_0000 {
        // might be or become NaN/Inf — avoid Inf due to rounding
        if (fbits & 0x7fff_ffff) >= 0x4780_0000 {
            // is or must become NaN/Inf
            if val < 0x7f80_0000 {
                // was value but too large
                return sign | 0x7c00; // make it +/-Inf
            }
            // remains +/-Inf or NaN (keep NaN (and Inf) bits)
            return sign | 0x7c00 | ((fbits & 0x007f_ffff) >> 13);
        }
        return sign | 0x7bff; // unrounded not quite Inf
    }
    if val >= 0x3880_0000 {
        // remains normalized value
        return sign | ((val - 0x3800_0000) >> 13); // exp - 127 + 15
    }
    if val < 0x3300_0000 {
        // too small for subnormal
        return sign; // becomes +/-0
    }
    let val = (fbits & 0x7fff_ffff) >> 23; // tmp exp for subnormal calc
    // add subnormal bit + round depending on cut off, then divide by
    // 2^(1-(exp-127+15)) and >> 13 | exp=0
    sign | ((((fbits & 0x007f_ffff) | 0x0080_0000) as i64
        + ((0x0080_0000_u32 >> (val - 102)) as i64))
        >> (126 - val)) as i32
}

/// Java `MathUtil.byteCount(int)`: number of bytes needed to store the value's
/// magnitude (0/1/2/4).
pub fn byte_count(i: i32) -> i32 {
    if i > 0xFFFF {
        return 4;
    }
    if i > 0xFF {
        return 2;
    }
    if i > 0x00 {
        return 1;
    }
    0
}

/// Java `MathUtil.packInt(int x, int z)`: `(x << 16) | (z & 0xFFFF)`.
pub fn pack_int(x: i32, z: i32) -> i32 {
    (x << 16) | (z & 0xFFFF)
}

/// Java `MathUtil.unpackLeft(int)`.
pub fn unpack_left_int(packed: i32) -> i32 {
    let mut i = (packed >> 16) & 0xFFFF;
    if (i & 0x8000) != 0 {
        i |= 0xFFFF_0000_u32 as i32;
    }
    i
}

/// Java `MathUtil.unpackRight(int)`.
pub fn unpack_right_int(packed: i32) -> i32 {
    let mut i = packed & 0xFFFF;
    if (i & 0x8000) != 0 {
        i |= 0xFFFF_0000_u32 as i32;
    }
    i
}

/// Java `MathUtil.packLong(int left, int right)`.
pub fn pack_long(left: i32, right: i32) -> i64 {
    ((left as i64) << 32) | (right as i64 & 0xFFFF_FFFF)
}

/// Java `MathUtil.unpackLeft(long)`.
pub fn unpack_left_long(packed: i64) -> i32 {
    (packed >> 32) as i32
}

/// Java `MathUtil.unpackRight(long)`.
pub fn unpack_right_long(packed: i64) -> i32 {
    packed as i32
}

/// Java `MathUtil.wrapAngle(float)`.
pub fn wrap_angle(angle: f32) -> f32 {
    let mut angle = angle % crate::trig_math_util::PI2;
    if angle <= -crate::trig_math_util::PI {
        angle += crate::trig_math_util::PI2;
    } else if angle > crate::trig_math_util::PI {
        angle -= crate::trig_math_util::PI2;
    }
    angle
}

/// Java `MathUtil.lerp(float, float, float)` (clamped).
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    lerp_unclamped_f32(a, b, clamp_f32(t, 0.0, 1.0))
}

/// Java `MathUtil.lerpUnclamped(float, float, float)`.
pub fn lerp_unclamped_f32(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

/// Java `MathUtil.lerp(double, double, double)` (clamped).
pub fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    lerp_unclamped_f64(a, b, clamp_f64(t, 0.0, 1.0))
}

/// Java `MathUtil.lerpUnclamped(double, double, double)`.
pub fn lerp_unclamped_f64(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}

/// Java `MathUtil.seededLerp(float a, float b, long seed)` — deterministic
/// interpolation within `[a, b]` from a 64-bit seed.
///
/// Keep byte-for-byte in sync with the client (`MathHelper.SeededLerp`) so
/// predicted values match the server.
pub fn seeded_lerp(a: f32, b: f32, seed: i64) -> f32 {
    let rnd = ((seed ^ (seed >> 32)) as i32) & 0xFF_FFFF;
    let t = rnd as f32 / 0xFF_FFFF as f32;
    a + t * (b - a)
}

/// Java `MathUtil.shortAngleDistance(float, float)`.
pub fn short_angle_distance(a: f32, b: f32) -> f32 {
    let distance = (b - a) % crate::trig_math_util::PI2;
    2.0 * distance % crate::trig_math_util::PI2 - distance
}

/// Java `MathUtil.lerpAngle(float, float, float)`.
pub fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    a + short_angle_distance(a, b) * t
}

/// Java `MathUtil.floorMod(double, double)`: `x - Math.floor(x / y) * y`.
pub fn floor_mod_f64(x: f64, y: f64) -> f64 {
    x - (x / y).floor() * y
}

/// Java `MathUtil.compareAngle(double, double)`.
pub fn compare_angle(a: f64, b: f64) -> f64 {
    let diff = b - a;
    floor_mod_f64(diff + std::f64::consts::PI, std::f64::consts::PI * 2.0) - std::f64::consts::PI
}

/// Java `MathUtil.percentile(long[], double)`, ported from
/// <https://stackoverflow.com/questions/14683467/finding-the-first-and-third-quartiles/51264621#51264621>.
///
/// `sorted_data` must be sorted ascending; Java indexes blindly here, so the
/// port indexes blindly too (panics on out-of-range instead of
/// `ArrayIndexOutOfBoundsException`, same observable contract).
pub fn percentile(sorted_data: &[i64], percentile_of: f64) -> f64 {
    if sorted_data.len() == 1 {
        return sorted_data[0] as f64;
    }
    if percentile_of >= 1.0 {
        return sorted_data[sorted_data.len() - 1] as f64;
    }

    let position = (sorted_data.len() as f64 + 1.0) * percentile_of;
    let n = percentile_of * (sorted_data.len() as f64 - 1.0) + 1.0;

    let (left, right): (i64, i64);
    if position >= 1.0 {
        left = sorted_data[floor(n) as usize - 1];
        right = sorted_data[floor(n) as usize];
    } else {
        left = sorted_data[0];
        right = sorted_data[1];
    }

    if left == right {
        return left as f64;
    }

    let part = n - floor(n) as f64;
    left as f64 + part * (right - left) as f64
}

/// Java `MathUtil.distanceToLineSq(double…)` — distance from point P to line
/// AB squared; falls back to the nearest end point when P does not project
/// onto the segment.
pub fn distance_to_line_sq(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx0 = x - ax;
    let dy0 = y - ay;
    let dx1 = bx - ax;
    let dy1 = by - ay;
    distance_to_line_sq_deltas(x, y, ax, ay, bx, by, dx0, dy0, dx1, dy1)
}

/// Java `MathUtil.distanceToLineSq(...)` with precomputed deltas.
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
    d_bx_ax: f64,
    d_by_ay: f64,
) -> f64 {
    // Projection on AB
    let mut t = dx_ax * d_bx_ax + dy_ay * d_by_ay;
    t /= d_bx_ax * d_bx_ax + d_by_ay * d_by_ay;

    // Connection point
    let mut px = ax;
    let mut py = ay;
    if t > 1.0 {
        px = bx;
        py = by;
    } else if t > 0.0 {
        px = ax + t * d_bx_ax;
        py = ay + t * d_by_ay;
    }

    let dx = x - px;
    let dy = y - py;
    dx * dx + dy * dy
}

/// Java `MathUtil.distanceToInfLineSq(double…)` — distance from point P to its
/// projection onto the infinite line through A and B, squared.
pub fn distance_to_inf_line_sq(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx0 = x - ax;
    let dy0 = y - ay;
    let dx1 = bx - ax;
    let dy1 = by - ay;
    distance_to_inf_line_sq_deltas(x, y, ax, ay, dx0, dy0, dx1, dy1)
}

/// Java `MathUtil.distanceToInfLineSq(...)` with precomputed deltas.
#[allow(clippy::too_many_arguments)]
pub fn distance_to_inf_line_sq_deltas(
    x: f64,
    y: f64,
    ax: f64,
    ay: f64,
    dx_ax: f64,
    dy_ay: f64,
    d_bx_ax: f64,
    d_by_ay: f64,
) -> f64 {
    // Projection on AB
    let mut t = dx_ax * d_bx_ax + dy_ay * d_by_ay;
    t /= d_bx_ax * d_bx_ax + d_by_ay * d_by_ay;

    // Connection point
    let px = ax + t * d_bx_ax;
    let py = ay + t * d_by_ay;

    let dx = x - px;
    let dy = y - py;
    dx * dx + dy * dy
}

/// Java `MathUtil.sideOfLine(double…)` — the side of the line AB that point P
/// lies on: `1` or `-1`.
pub fn side_of_line(x: f64, y: f64, ax: f64, ay: f64, bx: f64, by: f64) -> i32 {
    if (ax - x) * (by - y) - (ay - y) * (bx - x) >= 0.0 {
        1
    } else {
        -1
    }
}

/// Java `MathUtil.mapToRange(float…)`.
pub fn map_to_range(value: f32, value_min: f32, value_max: f32, range_min: f32, range_max: f32) -> f32 {
    let alpha = (value - value_min) / (value_max - value_min);
    range_min + alpha * (range_max - range_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Java `MathUtilTest.NUMBERS`.
    const NUMBERS: [f64; 4] = [
        (i32::MAX as f64) + 1.0,
        (i32::MIN as f64) - 1.0,
        (i64::MAX as f64) + 1.0,
        (i64::MIN as f64) - 1.0,
    ];

    /// Ported from Java `MathUtilTest.floor()`.
    #[test]
    fn floor_matches_java() {
        for i in -100..100 {
            let d = i as f64 / 4.0;
            let java_floor = d.floor();
            assert_eq!(floor(d), java_floor as i32, "floor({d})");
            assert_eq!(fast_floor_f64(d), java_floor as i64, "fastFloor({d})");
        }

        for &d in &NUMBERS {
            let java_floor = d.floor();
            assert_eq!(floor(d), java_floor as i32, "floor({d})");
            assert_eq!(fast_floor_f64(d), java_floor as i64, "fastFloor({d})");
        }
    }

    /// Ported from Java `MathUtilTest.ceil()`.
    #[test]
    fn ceil_matches_java() {
        for i in -100..100 {
            let d = i as f64 / 4.0;
            let java_ceil = d.ceil();
            assert_eq!(ceil(d), java_ceil as i32, "ceil({d})");
            assert_eq!(fast_ceil_f64(d), java_ceil as i64, "fastCeil({d})");
        }

        for &d in &NUMBERS {
            let java_ceil = d.ceil();
            assert_eq!(ceil(d), java_ceil as i32, "ceil({d})");
            assert_eq!(fast_ceil_f64(d), java_ceil as i64, "fastCeil({d})");
        }
    }

    #[test]
    fn abs_wraps_like_java() {
        assert_eq!(abs(-1), 1);
        assert_eq!(abs(0), 0);
        assert_eq!(abs(i32::MIN), i32::MIN, "Java's bit-trick abs wraps MIN");
        assert_eq!(abs(i32::MAX), i32::MAX);
    }

    #[test]
    fn pack_unpack_round_trip() {
        // Java packInt places x in bits [16,32) and z in bits [0,16); the
        // sign of each half is inferred from bit 31/bit 15 respectively, so a
        // full round trip is only guaranteed when the values fit in i16
        // (mirrors the Java code exactly, which has the same limitation).
        for x in [0i32, 1, -1, i16::MAX as i32, i16::MIN as i32] {
            for z in [0i32, 1, -1, i16::MAX as i32, i16::MIN as i32] {
                let packed = pack_int(x, z);
                assert_eq!(unpack_left_int(packed), x, "unpackLeft(packInt({x},{z}))");
                assert_eq!(unpack_right_int(packed), z, "unpackRight(packInt({x},{z}))");
            }
        }
        // packLong round trips for any i32 values (full 32-bit halves).
        for x in [0i32, 1, -1, i32::MIN, i32::MAX] {
            for z in [0i32, 1, -1, i32::MIN, i32::MAX] {
                let packed_long = pack_long(x, z);
                assert_eq!(unpack_left_long(packed_long), x);
                assert_eq!(unpack_right_long(packed_long), z);
            }
        }
        // Sanity: Java quirk preserved — packInt drops the high half of z and
        // never recovers it, so full-range int round trip does NOT hold:
        assert_eq!(pack_int(0, i32::MIN), i32::MIN & 0xFFFF);
    }

    #[test]
    fn clamp_mirrors_java_semantics() {
        // Java clamp(v, min, max): when v exceeds max it can still produce max
        // even below min if min > max (quirky but exact).
        assert_eq!(clamp_f64(5.0, 0.0, 1.0), 1.0);
        assert_eq!(clamp_f64(-5.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp_f64(0.5, 0.0, 1.0), 0.5);
        assert_eq!(clamp_i32(5, 0, 1), 1);
        // Java clamp(v, min, max) quirk: when v > max it returns `v < min ?
        // min : max` — so min > max wins the comparison:
        assert_eq!(clamp_i32(5, 10, 1), 10); // v > max: v < min ? min : max -> min = 10
        assert_eq!(clamp_i64(-5, 10, 1), 10); // v !> max, v < min -> min
    }

    #[test]
    fn saturated_multiply_saturates() {
        assert_eq!(saturated_multiply(i64::MAX, 2), i64::MAX);
        assert_eq!(saturated_multiply(i64::MIN, 2), i64::MIN);
        assert_eq!(saturated_multiply(-i64::MAX, -2), i64::MAX);
        assert_eq!(saturated_multiply(3, 4), 12);
    }

    #[test]
    fn seeded_lerp_is_deterministic() {
        // Exact formula check with a seed whose high 32 bits are zero, so
        // (seed ^ (seed >>> 32)) == low word:
        //   seed = 0x0000_0000_00AB_CD87 -> rnd = 0x00AB_CD87 & 0xFFFFFF
        let a = 1.0f32;
        let b = 3.0f32;
        let seed = 0x0000_0000_00AB_CD87u64 as i64;
        let expected_rnd = (seed as u32 as i32) & 0xFF_FFFF;
        let expected_t = expected_rnd as f32 / 0xFF_FFFF as f32;
        assert_eq!(seeded_lerp(a, b, seed), a + expected_t * (b - a));
    }
}
