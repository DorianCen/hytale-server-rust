// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/OldSimplexNoise.java`
//! Java `OldSimplexNoise` — OpenSimplex noise (KdotJPG gist, changes by Slikey),
//! used as the `OLD_SIMPLEX` noise type singleton.

use crate::noise::double_array::{Double2, Double3};
use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::{X_PRIME, Y_PRIME, Z_PRIME};

// Java static constants:
// private static final double STRETCH_CONSTANT_2D = -0.211324865405187;
// private static final double SQUISH_CONSTANT_2D = 0.366025403784439;
// private static final double STRETCH_CONSTANT_3D = -1.0 / 6;
// private static final double SQUISH_CONSTANT_3D = 1.0 / 3;
// private static final double NORM_CONSTANT_2D = 47;
// private static final double NORM_CONSTANT_3D = 103;
const STRETCH_CONSTANT_2D: f64 = -0.211324865405187;
const SQUISH_CONSTANT_2D: f64 = 0.366025403784439;
const STRETCH_CONSTANT_3D: f64 = -1.0 / 6.0;
const SQUISH_CONSTANT_3D: f64 = 1.0 / 3.0;
const NORM_CONSTANT_2D: f64 = 47.0;
const NORM_CONSTANT_3D: f64 = 103.0;

/// Java `OldSimplexNoise` — stateless singleton (`OldSimplexNoise.INSTANCE`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OldSimplexNoise;

impl OldSimplexNoise {
    /// Java `OldSimplexNoise.INSTANCE`.
    pub const INSTANCE: OldSimplexNoise = OldSimplexNoise;

    pub fn new() -> Self {
        Self
    }
}

// Java `OldSimplexNoise.extrapolate(int seed, int x, int y, double xd, double yd)`.
fn extrapolate2d(seed: i32, x: i32, y: i32, xd: f64, yd: f64) -> f64 {
    let mut hash = seed;
    hash ^= X_PRIME.wrapping_mul(x);
    hash ^= Y_PRIME.wrapping_mul(y);

    hash = hash
        .wrapping_mul(hash)
        .wrapping_mul(hash)
        .wrapping_mul(60493);
    hash = (hash >> 13) ^ hash;

    let g = GRADIENTS_2D[(hash & 7) as usize];
    xd * g.x + yd * g.y
}

// Java `OldSimplexNoise.extrapolate(int seed, int x, int y, int z, ...)` — the
// 3D flavor indexes its gradient table with `Math.floorMod(hash, table_len)`.
fn extrapolate3d(seed: i32, x: i32, y: i32, z: i32, xd: f64, yd: f64, zd: f64) -> f64 {
    let mut hash = seed;
    hash ^= X_PRIME.wrapping_mul(x);
    hash ^= Y_PRIME.wrapping_mul(y);
    hash ^= Z_PRIME.wrapping_mul(z);

    hash = hash
        .wrapping_mul(hash)
        .wrapping_mul(hash)
        .wrapping_mul(60493);
    hash = (hash >> 13) ^ hash;

    let g = &GRADIENTS_3D[hash.rem_euclid(GRADIENTS_3D.len() as i32) as usize];
    xd * g.x + yd * g.y + zd * g.z
}

// Java `OldSimplexNoise.fastFloor(double x)` (differs from `GeneralNoise.fastFloor`
// only for NaN — preserved exactly).
fn fast_floor(x: f64) -> i32 {
    let xi = x as i32;
    if x < xi as f64 {
        // Java int subtraction wraps (e.g. fastFloor(-3e9) → i32::MIN - 1 → i32::MAX)
        xi.wrapping_sub(1)
    } else {
        xi
    }
}

// Gradients for 2D. They approximate the directions to the
// vertices of an octagon from the center.
const GRADIENTS_2D: [Double2; 8] = [
    Double2 { x: 5.0, y: 2.0 },
    Double2 { x: 2.0, y: 5.0 },
    Double2 { x: -5.0, y: 2.0 },
    Double2 { x: -2.0, y: 5.0 },
    Double2 { x: 5.0, y: -2.0 },
    Double2 { x: 2.0, y: -5.0 },
    Double2 { x: -5.0, y: -2.0 },
    Double2 { x: -2.0, y: -5.0 },
];

// Gradients for 3D. They approximate the directions to the
// vertices of a rhombicuboctahedron from the center, skewed so
// that the triangular and square facets can be inscribed inside
// circles of the same radius.
#[rustfmt::skip]
const GRADIENTS_3D: [Double3; 24] = [
    Double3 { x: -11.0, y: 4.0,   z: 4.0 },  Double3 { x: -4.0, y: 11.0,  z: 4.0 },  Double3 { x: -4.0, y: 4.0,  z: 11.0 },
    Double3 { x: 11.0,  y: 4.0,   z: 4.0 },  Double3 { x: 4.0,  y: 11.0,  z: 4.0 },  Double3 { x: 4.0,  y: 4.0,  z: 11.0 },
    Double3 { x: -11.0, y: -4.0,  z: 4.0 },  Double3 { x: -4.0, y: -11.0, z: 4.0 },  Double3 { x: -4.0, y: -4.0, z: 11.0 },
    Double3 { x: 11.0,  y: -4.0,  z: 4.0 },  Double3 { x: 4.0,  y: -11.0, z: 4.0 },  Double3 { x: 4.0,  y: -4.0, z: 11.0 },
    Double3 { x: -11.0, y: 4.0,   z: -4.0 }, Double3 { x: -4.0, y: 11.0,  z: -4.0 }, Double3 { x: -4.0, y: 4.0,  z: -11.0 },
    Double3 { x: 11.0,  y: 4.0,   z: -4.0 }, Double3 { x: 4.0,  y: 11.0,  z: -4.0 }, Double3 { x: 4.0,  y: 4.0,  z: -11.0 },
    Double3 { x: -11.0, y: -4.0,  z: -4.0 }, Double3 { x: -4.0, y: -11.0, z: -4.0 }, Double3 { x: -4.0, y: -4.0, z: -11.0 },
    Double3 { x: 11.0,  y: -4.0,  z: -4.0 }, Double3 { x: 4.0,  y: -11.0, z: -4.0 }, Double3 { x: 4.0,  y: -4.0, z: -11.0 },
];

impl NoiseFunction2d for OldSimplexNoise {
    // 2D OpenSimplex Noise.
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        // Place input coordinates onto grid.
        let stretch_offset = (x + y) * STRETCH_CONSTANT_2D;
        let xs = x + stretch_offset;
        let ys = y + stretch_offset;

        // Floor to get grid coordinates of rhombus (stretched square) super-cell origin.
        let mut xsb = fast_floor(xs);
        let mut ysb = fast_floor(ys);

        // Skew out to get actual coordinates of rhombus origin. We'll need these later.
        let squish_offset = (xsb.wrapping_add(ysb)) as f64 * SQUISH_CONSTANT_2D;
        let xb = xsb as f64 + squish_offset;
        let yb = ysb as f64 + squish_offset;

        // Compute grid coordinates relative to rhombus origin.
        let xins = xs - xsb as f64;
        let yins = ys - ysb as f64;

        // Sum those together to get a value that determines which region we're in.
        let in_sum = xins + yins;

        // Positions relative to origin point.
        let mut dx0 = x - xb;
        let mut dy0 = y - yb;

        // We'll be defining these inside the next block and using them afterwards.
        let dx_ext;
        let dy_ext;
        let xsv_ext;
        let ysv_ext;

        let mut value = 0.0;

        // Contribution (1,0)
        let dx1 = dx0 - 1.0 - SQUISH_CONSTANT_2D;
        let dy1 = dy0 - 0.0 - SQUISH_CONSTANT_2D;
        let mut attn1 = 2.0 - dx1 * dx1 - dy1 * dy1;
        if attn1 > 0.0 {
            attn1 *= attn1;
            value += attn1 * attn1 * extrapolate2d(offset_seed, xsb.wrapping_add(1), ysb, dx1, dy1);
        }

        // Contribution (0,1)
        let dx2 = dx0 - 0.0 - SQUISH_CONSTANT_2D;
        let dy2 = dy0 - 1.0 - SQUISH_CONSTANT_2D;
        let mut attn2 = 2.0 - dx2 * dx2 - dy2 * dy2;
        if attn2 > 0.0 {
            attn2 *= attn2;
            value += attn2 * attn2 * extrapolate2d(offset_seed, xsb, ysb.wrapping_add(1), dx2, dy2);
        }

        if in_sum <= 1.0 {
            // We're inside the triangle (2-Simplex) at (0,0)
            let zins = 1.0 - in_sum;
            if zins > xins || zins > yins {
                // (0,0) is one of the closest two triangular vertices
                if xins > yins {
                    xsv_ext = xsb.wrapping_add(1);
                    ysv_ext = ysb - 1;
                    dx_ext = dx0 - 1.0;
                    dy_ext = dy0 + 1.0;
                } else {
                    xsv_ext = xsb - 1;
                    ysv_ext = ysb.wrapping_add(1);
                    dx_ext = dx0 + 1.0;
                    dy_ext = dy0 - 1.0;
                }
            } else {
                // (1,0) and (0,1) are the closest two vertices.
                xsv_ext = xsb.wrapping_add(1);
                ysv_ext = ysb.wrapping_add(1);
                dx_ext = dx0 - 1.0 - 2.0 * SQUISH_CONSTANT_2D;
                dy_ext = dy0 - 1.0 - 2.0 * SQUISH_CONSTANT_2D;
            }
        } else {
            // We're inside the triangle (2-Simplex) at (1,1)
            let zins = 2.0 - in_sum;
            if zins < xins || zins < yins {
                // (0,0) is one of the closest two triangular vertices
                if xins > yins {
                    xsv_ext = xsb.wrapping_add(2);
                    ysv_ext = ysb;
                    dx_ext = dx0 - 2.0 - 2.0 * SQUISH_CONSTANT_2D;
                    dy_ext = dy0 + 0.0 - 2.0 * SQUISH_CONSTANT_2D;
                } else {
                    xsv_ext = xsb;
                    ysv_ext = ysb.wrapping_add(2);
                    dx_ext = dx0 + 0.0 - 2.0 * SQUISH_CONSTANT_2D;
                    dy_ext = dy0 - 2.0 - 2.0 * SQUISH_CONSTANT_2D;
                }
            } else {
                // (1,0) and (0,1) are the closest two vertices.
                dx_ext = dx0;
                dy_ext = dy0;
                xsv_ext = xsb;
                ysv_ext = ysb;
            }
            xsb += 1;
            ysb += 1;
            dx0 = dx0 - 1.0 - 2.0 * SQUISH_CONSTANT_2D;
            dy0 = dy0 - 1.0 - 2.0 * SQUISH_CONSTANT_2D;
        }

        // Contribution (0,0) or (1,1)
        let mut attn0 = 2.0 - dx0 * dx0 - dy0 * dy0;
        if attn0 > 0.0 {
            attn0 *= attn0;
            value += attn0 * attn0 * extrapolate2d(offset_seed, xsb, ysb, dx0, dy0);
        }

        // Extra Vertex
        let mut attn_ext = 2.0 - dx_ext * dx_ext - dy_ext * dy_ext;
        if attn_ext > 0.0 {
            attn_ext *= attn_ext;
            value += attn_ext * attn_ext * extrapolate2d(offset_seed, xsv_ext, ysv_ext, dx_ext, dy_ext);
        }

        value / NORM_CONSTANT_2D
    }
}

#[allow(clippy::too_many_arguments)]
impl NoiseFunction3d for OldSimplexNoise {
    // 3D OpenSimplex Noise.
    #[allow(unused_assignments)]
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        // Place input coordinates on simplectic honeycomb.
        let stretch_offset = (x + y + z) * STRETCH_CONSTANT_3D;
        let xs = x + stretch_offset;
        let ys = y + stretch_offset;
        let zs = z + stretch_offset;

        // Floor to get simplectic honeycomb coordinates of rhombohedron (stretched cube) super-cell origin.
        let xsb = fast_floor(xs);
        let ysb = fast_floor(ys);
        let zsb = fast_floor(zs);

        // Skew out to get actual coordinates of rhombohedron origin. We'll need these later.
        let squish_offset = (xsb.wrapping_add(ysb).wrapping_add(zsb)) as f64 * SQUISH_CONSTANT_3D;
        let xb = xsb as f64 + squish_offset;
        let yb = ysb as f64 + squish_offset;
        let zb = zsb as f64 + squish_offset;

        // Compute simplectic honeycomb coordinates relative to rhombohedral origin.
        let xins = xs - xsb as f64;
        let yins = ys - ysb as f64;
        let zins = zs - zsb as f64;

        // Sum those together to get a value that determines which region we're in.
        let in_sum = xins + yins + zins;

        // Positions relative to origin point.
        let mut dx0 = x - xb;
        let mut dy0 = y - yb;
        let mut dz0 = z - zb;

        // We'll be defining these inside the next block and using them afterwards.
        let mut dx_ext0 = 0.0;
        let mut dy_ext0 = 0.0;
        let mut dz_ext0 = 0.0;
        let mut dx_ext1 = 0.0;
        let mut dy_ext1 = 0.0;
        let mut dz_ext1 = 0.0;
        let mut xsv_ext0 = 0;
        let mut ysv_ext0 = 0;
        let mut zsv_ext0 = 0;
        let mut xsv_ext1 = 0;
        let mut ysv_ext1 = 0;
        let mut zsv_ext1 = 0;

        let mut value = 0.0;

        if in_sum <= 1.0 {
            // We're inside the tetrahedron (3-Simplex) at (0,0,0)

            // Determine which two of (0,0,1), (0,1,0), (1,0,0) are closest.
            let mut a_point: i32 = 0x01;
            let mut a_score = xins;
            let mut b_point: i32 = 0x02;
            let mut b_score = yins;
            if a_score >= b_score && zins > b_score {
                b_score = zins;
                b_point = 0x04;
            } else if a_score < b_score && zins > a_score {
                a_score = zins;
                a_point = 0x04;
            }

            // Now we determine the two lattice points not part of the tetrahedron that may contribute.
            // This depends on the closest two tetrahedral vertices, including (0,0,0)
            let wins = 1.0 - in_sum;
            if wins > a_score || wins > b_score {
                // (0,0,0) is one of the closest two tetrahedral vertices.
                let c = if b_score > a_score { b_point } else { a_point }; // Our other closest vertex is the closest out of a and b.

                if (c & 0x01) == 0 {
                    xsv_ext0 = xsb - 1;
                    xsv_ext1 = xsb;
                    dx_ext0 = dx0 + 1.0;
                    dx_ext1 = dx0;
                } else {
                    xsv_ext0 = xsb.wrapping_add(1);
                    xsv_ext1 = xsb.wrapping_add(1);
                    dx_ext0 = dx0 - 1.0;
                    dx_ext1 = dx0 - 1.0;
                }

                if (c & 0x02) == 0 {
                    ysv_ext0 = ysb;
                    ysv_ext1 = ysb;
                    dy_ext0 = dy0;
                    dy_ext1 = dy0;
                    if (c & 0x01) == 0 {
                        ysv_ext1 -= 1;
                        dy_ext1 += 1.0;
                    } else {
                        ysv_ext0 -= 1;
                        dy_ext0 += 1.0;
                    }
                } else {
                    ysv_ext0 = ysb.wrapping_add(1);
                    ysv_ext1 = ysb.wrapping_add(1);
                    dy_ext0 = dy0 - 1.0;
                    dy_ext1 = dy0 - 1.0;
                }

                if (c & 0x04) == 0 {
                    zsv_ext0 = zsb;
                    zsv_ext1 = zsb - 1;
                    dz_ext0 = dz0;
                    dz_ext1 = dz0 + 1.0;
                } else {
                    zsv_ext0 = zsb.wrapping_add(1);
                    zsv_ext1 = zsb.wrapping_add(1);
                    dz_ext0 = dz0 - 1.0;
                    dz_ext1 = dz0 - 1.0;
                }
            } else {
                // (0,0,0) is not one of the closest two tetrahedral vertices.
                let c = a_point | b_point; // Our two extra vertices are determined by the closest two.

                if (c & 0x01) == 0 {
                    xsv_ext0 = xsb;
                    xsv_ext1 = xsb - 1;
                    dx_ext0 = dx0 - 2.0 * SQUISH_CONSTANT_3D;
                    dx_ext1 = dx0 + 1.0 - SQUISH_CONSTANT_3D;
                } else {
                    xsv_ext0 = xsb.wrapping_add(1);
                    xsv_ext1 = xsb.wrapping_add(1);
                    dx_ext0 = dx0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
                    dx_ext1 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
                }

                if (c & 0x02) == 0 {
                    ysv_ext0 = ysb;
                    ysv_ext1 = ysb - 1;
                    dy_ext0 = dy0 - 2.0 * SQUISH_CONSTANT_3D;
                    dy_ext1 = dy0 + 1.0 - SQUISH_CONSTANT_3D;
                } else {
                    ysv_ext0 = ysb.wrapping_add(1);
                    ysv_ext1 = ysb.wrapping_add(1);
                    dy_ext0 = dy0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
                    dy_ext1 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
                }

                if (c & 0x04) == 0 {
                    zsv_ext0 = zsb;
                    zsv_ext1 = zsb - 1;
                    dz_ext0 = dz0 - 2.0 * SQUISH_CONSTANT_3D;
                    dz_ext1 = dz0 + 1.0 - SQUISH_CONSTANT_3D;
                } else {
                    zsv_ext0 = zsb.wrapping_add(1);
                    zsv_ext1 = zsb.wrapping_add(1);
                    dz_ext0 = dz0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
                    dz_ext1 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
                }
            }

            // Contribution (0,0,0)
            let mut attn0 = 2.0 - dx0 * dx0 - dy0 * dy0 - dz0 * dz0;
            if attn0 > 0.0 {
                attn0 *= attn0;
                value += attn0 * attn0 * extrapolate3d(offset_seed, xsb, ysb, zsb, dx0, dy0, dz0);
            }

            // Contribution (1,0,0)
            let dx1 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
            let dy1 = dy0 - 0.0 - SQUISH_CONSTANT_3D;
            let dz1 = dz0 - 0.0 - SQUISH_CONSTANT_3D;
            let mut attn1 = 2.0 - dx1 * dx1 - dy1 * dy1 - dz1 * dz1;
            if attn1 > 0.0 {
                attn1 *= attn1;
                value += attn1 * attn1 * extrapolate3d(offset_seed, xsb.wrapping_add(1), ysb, zsb, dx1, dy1, dz1);
            }

            // Contribution (0,1,0)
            let dx2 = dx0 - 0.0 - SQUISH_CONSTANT_3D;
            let dy2 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
            let dz2 = dz1;
            let mut attn2 = 2.0 - dx2 * dx2 - dy2 * dy2 - dz2 * dz2;
            if attn2 > 0.0 {
                attn2 *= attn2;
                value += attn2 * attn2 * extrapolate3d(offset_seed, xsb, ysb.wrapping_add(1), zsb, dx2, dy2, dz2);
            }

            // Contribution (0,0,1)
            let dx3 = dx2;
            let dy3 = dy1;
            let dz3 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
            let mut attn3 = 2.0 - dx3 * dx3 - dy3 * dy3 - dz3 * dz3;
            if attn3 > 0.0 {
                attn3 *= attn3;
                value += attn3 * attn3 * extrapolate3d(offset_seed, xsb, ysb, zsb.wrapping_add(1), dx3, dy3, dz3);
            }
        } else if in_sum >= 2.0 {
            // We're inside the tetrahedron (3-Simplex) at (1,1,1)

            // Determine which two tetrahedral vertices are the closest, out of (1,1,0), (1,0,1), (0,1,1) but not (1,1,1).
            let mut a_point: i32 = 0x06;
            let mut a_score = xins;
            let mut b_point: i32 = 0x05;
            let mut b_score = yins;
            if a_score <= b_score && zins < b_score {
                b_score = zins;
                b_point = 0x03;
            } else if a_score > b_score && zins < a_score {
                a_score = zins;
                a_point = 0x03;
            }

            // Now we determine the two lattice points not part of the tetrahedron that may contribute.
            // This depends on the closest two tetrahedral vertices, including (1,1,1)
            let wins = 3.0 - in_sum;
            if wins < a_score || wins < b_score {
                // (1,1,1) is one of the closest two tetrahedral vertices.
                let c = if b_score < a_score { b_point } else { a_point }; // Our other closest vertex is the closest out of a and b.

                if (c & 0x01) != 0 {
                    xsv_ext0 = xsb.wrapping_add(2);
                    xsv_ext1 = xsb.wrapping_add(1);
                    dx_ext0 = dx0 - 2.0 - 3.0 * SQUISH_CONSTANT_3D;
                    dx_ext1 = dx0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
                } else {
                    xsv_ext0 = xsb;
                    xsv_ext1 = xsb;
                    dx_ext0 = dx0 - 3.0 * SQUISH_CONSTANT_3D;
                    dx_ext1 = dx0 - 3.0 * SQUISH_CONSTANT_3D;
                }

                if (c & 0x02) != 0 {
                    ysv_ext0 = ysb.wrapping_add(1);
                    ysv_ext1 = ysb.wrapping_add(1);
                    dy_ext0 = dy0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
                    dy_ext1 = dy0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
                    if (c & 0x01) != 0 {
                        ysv_ext1 += 1;
                        dy_ext1 -= 1.0;
                    } else {
                        ysv_ext0 += 1;
                        dy_ext0 -= 1.0;
                    }
                } else {
                    ysv_ext0 = ysb;
                    ysv_ext1 = ysb;
                    dy_ext0 = dy0 - 3.0 * SQUISH_CONSTANT_3D;
                    dy_ext1 = dy0 - 3.0 * SQUISH_CONSTANT_3D;
                }

                if (c & 0x04) != 0 {
                    zsv_ext0 = zsb.wrapping_add(1);
                    zsv_ext1 = zsb.wrapping_add(2);
                    dz_ext0 = dz0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
                    dz_ext1 = dz0 - 2.0 - 3.0 * SQUISH_CONSTANT_3D;
                } else {
                    zsv_ext0 = zsb;
                    zsv_ext1 = zsb;
                    dz_ext0 = dz0 - 3.0 * SQUISH_CONSTANT_3D;
                    dz_ext1 = dz0 - 3.0 * SQUISH_CONSTANT_3D;
                }
            } else {
                // (1,1,1) is not one of the closest two tetrahedral vertices.
                let c = a_point & b_point; // Our two extra vertices are determined by the closest two.

                if (c & 0x01) != 0 {
                    xsv_ext0 = xsb.wrapping_add(1);
                    xsv_ext1 = xsb.wrapping_add(2);
                    dx_ext0 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
                    dx_ext1 = dx0 - 2.0 - 2.0 * SQUISH_CONSTANT_3D;
                } else {
                    xsv_ext0 = xsb;
                    xsv_ext1 = xsb;
                    dx_ext0 = dx0 - SQUISH_CONSTANT_3D;
                    dx_ext1 = dx0 - 2.0 * SQUISH_CONSTANT_3D;
                }

                if (c & 0x02) != 0 {
                    ysv_ext0 = ysb.wrapping_add(1);
                    ysv_ext1 = ysb.wrapping_add(2);
                    dy_ext0 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
                    dy_ext1 = dy0 - 2.0 - 2.0 * SQUISH_CONSTANT_3D;
                } else {
                    ysv_ext0 = ysb;
                    ysv_ext1 = ysb;
                    dy_ext0 = dy0 - SQUISH_CONSTANT_3D;
                    dy_ext1 = dy0 - 2.0 * SQUISH_CONSTANT_3D;
                }

                if (c & 0x04) != 0 {
                    zsv_ext0 = zsb.wrapping_add(1);
                    zsv_ext1 = zsb.wrapping_add(2);
                    dz_ext0 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
                    dz_ext1 = dz0 - 2.0 - 2.0 * SQUISH_CONSTANT_3D;
                } else {
                    zsv_ext0 = zsb;
                    zsv_ext1 = zsb;
                    dz_ext0 = dz0 - SQUISH_CONSTANT_3D;
                    dz_ext1 = dz0 - 2.0 * SQUISH_CONSTANT_3D;
                }
            }

            // Contribution (1,1,0)
            let dx3 = dx0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dy3 = dy0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dz3 = dz0 - 0.0 - 2.0 * SQUISH_CONSTANT_3D;
            let mut attn3 = 2.0 - dx3 * dx3 - dy3 * dy3 - dz3 * dz3;
            if attn3 > 0.0 {
                attn3 *= attn3;
                value += attn3 * attn3 * extrapolate3d(offset_seed, xsb.wrapping_add(1), ysb.wrapping_add(1), zsb, dx3, dy3, dz3);
            }

            // Contribution (1,0,1)
            let dx2 = dx3;
            let dy2 = dy0 - 0.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dz2 = dz0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
            let mut attn2 = 2.0 - dx2 * dx2 - dy2 * dy2 - dz2 * dz2;
            if attn2 > 0.0 {
                attn2 *= attn2;
                value += attn2 * attn2 * extrapolate3d(offset_seed, xsb.wrapping_add(1), ysb, zsb.wrapping_add(1), dx2, dy2, dz2);
            }

            // Contribution (0,1,1)
            let dx1 = dx0 - 0.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dy1 = dy3;
            let dz1 = dz2;
            let mut attn1 = 2.0 - dx1 * dx1 - dy1 * dy1 - dz1 * dz1;
            if attn1 > 0.0 {
                attn1 *= attn1;
                value += attn1 * attn1 * extrapolate3d(offset_seed, xsb, ysb.wrapping_add(1), zsb.wrapping_add(1), dx1, dy1, dz1);
            }

            // Contribution (1,1,1)
            dx0 = dx0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
            dy0 = dy0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
            dz0 = dz0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
            let mut attn0 = 2.0 - dx0 * dx0 - dy0 * dy0 - dz0 * dz0;
            if attn0 > 0.0 {
                attn0 *= attn0;
                value += attn0 * attn0 * extrapolate3d(offset_seed, xsb.wrapping_add(1), ysb.wrapping_add(1), zsb.wrapping_add(1), dx0, dy0, dz0);
            }
        } else {
            // We're inside the octahedron (Rectified 3-Simplex) in between.
            let mut a_score;
            let mut a_point: i32;
            let mut a_is_further_side;
            let mut b_score;
            let mut b_point: i32;
            let mut b_is_further_side;

            // Decide between point (0,0,1) and (1,1,0) as closest
            let p1 = xins + yins;
            if p1 > 1.0 {
                a_score = p1 - 1.0;
                a_point = 0x03;
                a_is_further_side = true;
            } else {
                a_score = 1.0 - p1;
                a_point = 0x04;
                a_is_further_side = false;
            }

            // Decide between point (0,1,0) and (1,0,1) as closest
            let p2 = xins + zins;
            if p2 > 1.0 {
                b_score = p2 - 1.0;
                b_point = 0x05;
                b_is_further_side = true;
            } else {
                b_score = 1.0 - p2;
                b_point = 0x02;
                b_is_further_side = false;
            }

            // The closest out of the two (1,0,0) and (0,1,1) will replace the furthest out of the two decided above, if closer.
            let p3 = yins + zins;
            if p3 > 1.0 {
                let score = p3 - 1.0;
                if a_score <= b_score && a_score < score {
                    a_score = score;
                    a_point = 0x06;
                    a_is_further_side = true;
                } else if a_score > b_score && b_score < score {
                    b_score = score;
                    b_point = 0x06;
                    b_is_further_side = true;
                }
            } else {
                let score = 1.0 - p3;
                if a_score <= b_score && a_score < score {
                    a_score = score;
                    a_point = 0x01;
                    a_is_further_side = false;
                } else if a_score > b_score && b_score < score {
                    b_score = score;
                    b_point = 0x01;
                    b_is_further_side = false;
                }
            }

            // Where each of the two closest points are determines how the extra two vertices are calculated.
            if a_is_further_side == b_is_further_side {
                if a_is_further_side {
                    // Both closest points on (1,1,1) side

                    // One of the two extra points is (1,1,1)
                    dx_ext0 = dx0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
                    dy_ext0 = dy0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
                    dz_ext0 = dz0 - 1.0 - 3.0 * SQUISH_CONSTANT_3D;
                    xsv_ext0 = xsb.wrapping_add(1);
                    ysv_ext0 = ysb.wrapping_add(1);
                    zsv_ext0 = zsb.wrapping_add(1);

                    // Other extra point is based on the shared axis.
                    let c = a_point & b_point;
                    if (c & 0x01) != 0 {
                        dx_ext1 = dx0 - 2.0 - 2.0 * SQUISH_CONSTANT_3D;
                        dy_ext1 = dy0 - 2.0 * SQUISH_CONSTANT_3D;
                        dz_ext1 = dz0 - 2.0 * SQUISH_CONSTANT_3D;
                        xsv_ext1 = xsb.wrapping_add(2);
                        ysv_ext1 = ysb;
                        zsv_ext1 = zsb;
                    } else if (c & 0x02) != 0 {
                        dx_ext1 = dx0 - 2.0 * SQUISH_CONSTANT_3D;
                        dy_ext1 = dy0 - 2.0 - 2.0 * SQUISH_CONSTANT_3D;
                        dz_ext1 = dz0 - 2.0 * SQUISH_CONSTANT_3D;
                        xsv_ext1 = xsb;
                        ysv_ext1 = ysb.wrapping_add(2);
                        zsv_ext1 = zsb;
                    } else {
                        dx_ext1 = dx0 - 2.0 * SQUISH_CONSTANT_3D;
                        dy_ext1 = dy0 - 2.0 * SQUISH_CONSTANT_3D;
                        dz_ext1 = dz0 - 2.0 - 2.0 * SQUISH_CONSTANT_3D;
                        xsv_ext1 = xsb;
                        ysv_ext1 = ysb;
                        zsv_ext1 = zsb.wrapping_add(2);
                    }
                } else {
                    // Both closest points on (0,0,0) side

                    // One of the two extra points is (0,0,0)
                    dx_ext0 = dx0;
                    dy_ext0 = dy0;
                    dz_ext0 = dz0;
                    xsv_ext0 = xsb;
                    ysv_ext0 = ysb;
                    zsv_ext0 = zsb;

                    // Other extra point is based on the omitted axis.
                    let c = a_point | b_point;
                    if (c & 0x01) == 0 {
                        dx_ext1 = dx0 + 1.0 - SQUISH_CONSTANT_3D;
                        dy_ext1 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
                        dz_ext1 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
                        xsv_ext1 = xsb - 1;
                        ysv_ext1 = ysb.wrapping_add(1);
                        zsv_ext1 = zsb.wrapping_add(1);
                    } else if (c & 0x02) == 0 {
                        dx_ext1 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
                        dy_ext1 = dy0 + 1.0 - SQUISH_CONSTANT_3D;
                        dz_ext1 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
                        xsv_ext1 = xsb.wrapping_add(1);
                        ysv_ext1 = ysb - 1;
                        zsv_ext1 = zsb.wrapping_add(1);
                    } else {
                        dx_ext1 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
                        dy_ext1 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
                        dz_ext1 = dz0 + 1.0 - SQUISH_CONSTANT_3D;
                        xsv_ext1 = xsb.wrapping_add(1);
                        ysv_ext1 = ysb.wrapping_add(1);
                        zsv_ext1 = zsb - 1;
                    }
                }
            } else {
                // One point on (0,0,0) side, one point on (1,1,1) side
                let c1: i32;
                let c2: i32;
                if a_is_further_side {
                    c1 = a_point;
                    c2 = b_point;
                } else {
                    c1 = b_point;
                    c2 = a_point;
                }

                // One contribution is a permutation of (1,1,-1)
                if (c1 & 0x01) == 0 {
                    dx_ext0 = dx0 + 1.0 - SQUISH_CONSTANT_3D;
                    dy_ext0 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
                    dz_ext0 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
                    xsv_ext0 = xsb - 1;
                    ysv_ext0 = ysb.wrapping_add(1);
                    zsv_ext0 = zsb.wrapping_add(1);
                } else if (c1 & 0x02) == 0 {
                    dx_ext0 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
                    dy_ext0 = dy0 + 1.0 - SQUISH_CONSTANT_3D;
                    dz_ext0 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
                    xsv_ext0 = xsb.wrapping_add(1);
                    ysv_ext0 = ysb - 1;
                    zsv_ext0 = zsb.wrapping_add(1);
                } else {
                    dx_ext0 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
                    dy_ext0 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
                    dz_ext0 = dz0 + 1.0 - SQUISH_CONSTANT_3D;
                    xsv_ext0 = xsb.wrapping_add(1);
                    ysv_ext0 = ysb.wrapping_add(1);
                    zsv_ext0 = zsb - 1;
                }

                // One contribution is a permutation of (0,0,2)
                dx_ext1 = dx0 - 2.0 * SQUISH_CONSTANT_3D;
                dy_ext1 = dy0 - 2.0 * SQUISH_CONSTANT_3D;
                dz_ext1 = dz0 - 2.0 * SQUISH_CONSTANT_3D;
                xsv_ext1 = xsb;
                ysv_ext1 = ysb;
                zsv_ext1 = zsb;
                if (c2 & 0x01) != 0 {
                    dx_ext1 -= 2.0;
                    xsv_ext1 += 2;
                } else if (c2 & 0x02) != 0 {
                    dy_ext1 -= 2.0;
                    ysv_ext1 += 2;
                } else {
                    dz_ext1 -= 2.0;
                    zsv_ext1 += 2;
                }
            }

            // Contribution (1,0,0)
            let dx1 = dx0 - 1.0 - SQUISH_CONSTANT_3D;
            let dy1 = dy0 - 0.0 - SQUISH_CONSTANT_3D;
            let dz1 = dz0 - 0.0 - SQUISH_CONSTANT_3D;
            let mut attn1 = 2.0 - dx1 * dx1 - dy1 * dy1 - dz1 * dz1;
            if attn1 > 0.0 {
                attn1 *= attn1;
                value += attn1 * attn1 * extrapolate3d(offset_seed, xsb.wrapping_add(1), ysb, zsb, dx1, dy1, dz1);
            }

            // Contribution (0,1,0)
            let dx2 = dx0 - 0.0 - SQUISH_CONSTANT_3D;
            let dy2 = dy0 - 1.0 - SQUISH_CONSTANT_3D;
            let dz2 = dz1;
            let mut attn2 = 2.0 - dx2 * dx2 - dy2 * dy2 - dz2 * dz2;
            if attn2 > 0.0 {
                attn2 *= attn2;
                value += attn2 * attn2 * extrapolate3d(offset_seed, xsb, ysb.wrapping_add(1), zsb, dx2, dy2, dz2);
            }

            // Contribution (0,0,1)
            let dx3 = dx2;
            let dy3 = dy1;
            let dz3 = dz0 - 1.0 - SQUISH_CONSTANT_3D;
            let mut attn3 = 2.0 - dx3 * dx3 - dy3 * dy3 - dz3 * dz3;
            if attn3 > 0.0 {
                attn3 *= attn3;
                value += attn3 * attn3 * extrapolate3d(offset_seed, xsb, ysb, zsb.wrapping_add(1), dx3, dy3, dz3);
            }

            // Contribution (1,1,0)
            let dx4 = dx0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dy4 = dy0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dz4 = dz0 - 0.0 - 2.0 * SQUISH_CONSTANT_3D;
            let mut attn4 = 2.0 - dx4 * dx4 - dy4 * dy4 - dz4 * dz4;
            if attn4 > 0.0 {
                attn4 *= attn4;
                value += attn4 * attn4 * extrapolate3d(offset_seed, xsb.wrapping_add(1), ysb.wrapping_add(1), zsb, dx4, dy4, dz4);
            }

            // Contribution (1,0,1)
            let dx5 = dx4;
            let dy5 = dy0 - 0.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dz5 = dz0 - 1.0 - 2.0 * SQUISH_CONSTANT_3D;
            let mut attn5 = 2.0 - dx5 * dx5 - dy5 * dy5 - dz5 * dz5;
            if attn5 > 0.0 {
                attn5 *= attn5;
                value += attn5 * attn5 * extrapolate3d(offset_seed, xsb.wrapping_add(1), ysb, zsb.wrapping_add(1), dx5, dy5, dz5);
            }

            // Contribution (0,1,1)
            let dx6 = dx0 - 0.0 - 2.0 * SQUISH_CONSTANT_3D;
            let dy6 = dy4;
            let dz6 = dz5;
            let mut attn6 = 2.0 - dx6 * dx6 - dy6 * dy6 - dz6 * dz6;
            if attn6 > 0.0 {
                attn6 *= attn6;
                value += attn6 * attn6 * extrapolate3d(offset_seed, xsb, ysb.wrapping_add(1), zsb.wrapping_add(1), dx6, dy6, dz6);
            }
        }

        // First extra vertex
        let mut attn_ext0 = 2.0 - dx_ext0 * dx_ext0 - dy_ext0 * dy_ext0 - dz_ext0 * dz_ext0;
        if attn_ext0 > 0.0 {
            attn_ext0 *= attn_ext0;
            value += attn_ext0 * attn_ext0 * extrapolate3d(offset_seed, xsv_ext0, ysv_ext0, zsv_ext0, dx_ext0, dy_ext0, dz_ext0);
        }

        // Second extra vertex
        let mut attn_ext1 = 2.0 - dx_ext1 * dx_ext1 - dy_ext1 * dy_ext1 - dz_ext1 * dz_ext1;
        if attn_ext1 > 0.0 {
            attn_ext1 *= attn_ext1;
            value += attn_ext1 * attn_ext1 * extrapolate3d(offset_seed, xsv_ext1, ysv_ext1, zsv_ext1, dx_ext1, dy_ext1, dz_ext1);
        }

        value / NORM_CONSTANT_3D
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    const NOISE: OldSimplexNoise = OldSimplexNoise::INSTANCE;

    #[test]
    fn two_d_is_finite_and_deterministic() {
        for &seed in &[1i32, -1, 0x1234_5678, i32::MIN] {
            for i in 0..300 {
                let x = (i as f64) * 0.17305 - 39.0;
                let y = (i as f64) * 0.29098 + 51.0;
                let a = NoiseFunction2d::get(&NOISE, seed, 1234, x, y);
                let b = NoiseFunction2d::get(&NOISE, seed, 1234, x, y);
                assert_eq!(a, b);
                assert!(a.is_finite());
                // / 47 normalization: generous ±3 bound.
                assert!(a.abs() <= 3.0, "2d |v|={a}");
            }
        }
    }

    #[test]
    fn three_d_is_finite_and_deterministic() {
        for &seed in &[1i32, -1, 0x9afc_33ae_u32 as i32, i32::MAX] {
            for i in 0..300 {
                let x = (i as f64) * 0.17305 - 39.0;
                let y = (i as f64) * 0.29098 + 51.0;
                let z = (i as f64) * 0.61038 - 12.0;
                let a = NoiseFunction3d::get(&NOISE, seed, 4321, x, y, z);
                let b = NoiseFunction3d::get(&NOISE, seed, 4321, x, y, z);
                assert_eq!(a, b, "3d not deterministic at seed={seed} i={i}");
                assert!(a.is_finite(), "3d non-finite at seed={seed} i={i}");
                // / 103 normalization: generous ±3 bound.
                assert!(a.abs() <= 3.0, "3d |v|={a}");
            }
        }
    }

    #[test]
    fn negative_and_large_coordinates() {
        let big = 1.0e18;
        assert!(NoiseFunction2d::get(&NOISE, 7, 8, big, -big).is_finite());
        assert!(NoiseFunction3d::get(&NOISE, 7, 8, big, -big, big).is_finite());
        assert!(NoiseFunction2d::get(&NOISE, 7, 8, -1.5, -2.5).is_finite());
    }
}
