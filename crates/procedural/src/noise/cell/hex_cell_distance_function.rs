// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/HexCellDistanceFunction.java`
//! Java `HexCellDistanceFunction` (+ its anonymous `CellPointFunction`
//! singleton, ported as [`HexCellPointFunction`], and the nested
//! `SquirrelHash`).
//!
//! ⚠ NOTE(subagent): 3d methods `throw new UnsupportedOperationException()` —
//! ported as panics. `Java stream` table derivation for `HEX_CELL_2D` becomes
//! const arithmetic over [`CELL_2D`].

use std::rc::Rc;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::cell_point_function::CellPointFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cellular_noise::CELL_2D;
use crate::noise::double_array::Double2;
use crate::noise::math_util;
use crate::noise::result_buffer::{Bounds2d, ResultBuffer2d, ResultBuffer3d};

/// Java `Math.sqrt(3.0)` — held as a literal because `f64::sqrt` is not
/// const-evaluable and `std::f64::consts::SQRT_3` is still nightly-only.
/// 1.7320508075688772 is the exact `f64` nearest to √3 (same as Java).
const SQRT_3: f64 = 1.7320508075688772;

/// Java `X_TO_GRID_X = Math.sqrt(3.0) / 3.0`.
const X_TO_GRID_X: f64 = SQRT_3 / 3.0;
/// Java `Y_TO_GRID_X = -1.0 / 3.0`.
const Y_TO_GRID_X: f64 = -1.0 / 3.0;
/// Java `Y_TO_GRID_Y = 2.0 / 3.0`.
const Y_TO_GRID_Y: f64 = 2.0 / 3.0;

/// Java `X_TO_HEX_X = Math.sqrt(3.0)`.
const X_TO_HEX_X: f64 = SQRT_3;
/// Java `Y_TO_HEX_X = Math.sqrt(3.0) / 2.0`.
const Y_TO_HEX_X: f64 = SQRT_3 / 2.0;
/// Java `Y_TO_HEX_Y = 3.0 / 2.0`.
const Y_TO_HEX_Y: f64 = 3.0 / 2.0;

/// Normalizes distance measurements to the range 0.0-1.0.
/// The distance between un-jittered hex-cells is 3.0.
/// (Java `NORMALIZATION = 1.0 / 3.0`.)
const NORMALIZATION: f64 = 1.0 / 3.0;

/// Used to rescale the hex-based cells so that they more closely match the
/// size of regular square-based cells, making the two types more easily
/// interchangeable. The scale is derived from the average of the horizontal
/// and vertical scales of the hexagon.
/// (Java `SCALE = (X_TO_HEX_X + Y_TO_HEX_Y) / 2.0`.)
pub const SCALE: f64 = (X_TO_HEX_X + Y_TO_HEX_Y) / 2.0;

/// Java `HEX_CELL_2D` — the square-cell offset table shifted by `-0.5` so
/// negative coordinates work (the `toGrid` methods round rather than floor).
/// (Java: `Stream.of(CellularNoise.CELL_2D).map(d -> new Double2(d.x - 0.5, d.y - 0.5)).toArray(...)`.)
pub const HEX_CELL_2D: [Double2; 256] = {
    let mut out = [Double2 { x: 0.0, y: 0.0 }; 256];
    let mut i = 0;
    while i < 256 {
        out[i] = Double2 {
            x: CELL_2D[i].x - 0.5,
            y: CELL_2D[i].y - 0.5,
        };
        i += 1;
    }
    out
};

/// Java `HexCellDistanceFunction` — the hex-cell distance function.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HexCellDistanceFunction;

impl HexCellDistanceFunction {
    /// Java `public static final HexCellDistanceFunction DISTANCE_FUNCTION`.
    pub fn distance_function() -> Rc<dyn CellDistanceFunction> {
        Rc::new(HexCellDistanceFunction)
    }

    /// Java `public static int getHash(int seed, int x, int y)`.
    #[inline]
    pub fn get_hash(seed: i32, x: i32, y: i32) -> i32 {
        SquirrelHash::hash(seed, x, y)
    }

    /// Java `public static int toGridX(double x, double y)`.
    #[inline]
    pub fn to_grid_x(x: f64, y: f64) -> i32 {
        // Java: (int) MathUtil.fastRound(X_TO_GRID_X * x + Y_TO_GRID_X * y)
        math_util::fast_round(X_TO_GRID_X * x + Y_TO_GRID_X * y) as i32
    }

    /// Java `public static int toGridY(double x, double y)`.
    #[inline]
    pub fn to_grid_y(_x: f64, y: f64) -> i32 {
        // Java: (int) MathUtil.fastRound(Y_TO_GRID_Y * y)
        math_util::fast_round(Y_TO_GRID_Y * y) as i32
    }

    /// Java `public static double toHexX(double hx, double hy)`.
    #[inline]
    pub fn to_hex_x(hx: f64, hy: f64) -> f64 {
        X_TO_HEX_X * hx + Y_TO_HEX_X * hy
    }

    /// Java `public static double toHexY(double hx, double hy)`.
    #[inline]
    pub fn to_hex_y(_hx: f64, hy: f64) -> f64 {
        Y_TO_HEX_Y * hy
    }
}

impl CellDistanceFunction for HexCellDistanceFunction {
    /// Java `scale(double)` — `value * SCALE`.
    fn scale(&self, value: f64) -> f64 {
        value * SCALE
    }

    /// Java `invScale(double)` — `value / SCALE`.
    fn inv_scale(&self, value: f64) -> f64 {
        value / SCALE
    }

    /// Java `getCellX(double x, double y)` — `toGridX(x, y)`.
    fn get_cell_x2(&self, x: f64, y: f64) -> i32 {
        Self::to_grid_x(x, y)
    }

    /// Java `getCellY(double x, double y)` — `toGridY(x, y)`.
    fn get_cell_y2(&self, x: f64, y: f64) -> i32 {
        Self::to_grid_y(x, y)
    }

    /// Java `nearest2D` — 3x3 neighbourhood, unrolled; then the distance is
    /// normalized into 0.0-1.0.
    fn nearest_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    ) {
        // 3x3 neighbourhood. Unrolled loops benchmarked ~12% faster than the
        // traditional nested loops.
        self.eval_point_2d(seed, x, y, cell_x - 1, cell_y - 1, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x + 0, cell_y - 1, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x + 1, cell_y - 1, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x - 1, cell_y + 0, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x + 0, cell_y + 0, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x + 1, cell_y + 0, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x - 1, cell_y + 1, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x + 0, cell_y + 1, buffer, point_evaluator);
        self.eval_point_2d(seed, x, y, cell_x + 1, cell_y + 1, buffer, point_evaluator);

        // Normalize the distance output to the range 0.0-1.0
        buffer.distance *= NORMALIZATION;
    }

    /// Java `nearest3D` — unsupported.
    fn nearest_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _buffer: &mut ResultBuffer3d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
        panic!("HexCellDistanceFunction.nearest3D is not supported (Java UnsupportedOperationException)");
    }

    /// Java `transition2D` — 3x3 neighbourhood; extra rings for jitter > 0.5;
    /// then both distances are normalized into 0.0-1.0.
    fn transition_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    ) {
        // 3x3 neighbourhood. Similar performance gains as explained in
        // nearest2D comments.
        self.eval_point2_2d(seed, x, y, cell_x - 1, cell_y - 1, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x + 0, cell_y - 1, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x + 1, cell_y - 1, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x - 1, cell_y + 0, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x + 0, cell_y + 0, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x + 1, cell_y + 0, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x - 1, cell_y + 1, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x + 0, cell_y + 1, buffer, point_evaluator);
        self.eval_point2_2d(seed, x, y, cell_x + 1, cell_y + 1, buffer, point_evaluator);

        // We need to check distant cells when using high jitter to prevent artifacts.
        let jitter = point_evaluator.get_jitter();
        if jitter.get_max_x() > 0.5 {
            self.eval_point2_2d(seed, x, y, cell_x - 2, cell_y - 1, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x - 2, cell_y + 0, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x - 2, cell_y + 1, buffer, point_evaluator);

            self.eval_point2_2d(seed, x, y, cell_x + 2, cell_y + 0, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x + 2, cell_y - 1, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x + 2, cell_y + 1, buffer, point_evaluator);
        }

        if jitter.get_max_y() > 0.5 {
            self.eval_point2_2d(seed, x, y, cell_x - 1, cell_y - 2, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x + 0, cell_y - 2, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x + 1, cell_y - 2, buffer, point_evaluator);

            self.eval_point2_2d(seed, x, y, cell_x - 1, cell_y + 2, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x + 0, cell_y + 2, buffer, point_evaluator);
            self.eval_point2_2d(seed, x, y, cell_x + 1, cell_y + 2, buffer, point_evaluator);
        }

        // Normalize the distance outputs to the range 0.0-1.0
        buffer.distance *= NORMALIZATION;
        buffer.distance2 *= NORMALIZATION;
    }

    /// Java `transition3D` — unsupported.
    fn transition_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _buffer: &mut ResultBuffer3d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
        panic!("HexCellDistanceFunction.transition3D is not supported (Java UnsupportedOperationException)");
    }

    fn eval_point_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    ) {
        let cell_hash = Self::get_hash(seed, cell_x, cell_y);
        let vec = HEX_CELL_2D[(cell_hash & 255) as usize];

        let jitter = point_evaluator.get_jitter();
        let px = jitter.get_point_x2(cell_x, vec);
        let py = jitter.get_point_y2(cell_y, vec);
        let hx = Self::to_hex_x(px, py);
        let hy = Self::to_hex_y(px, py);

        point_evaluator.eval_point_2d(seed, x, y, cell_hash, cell_x, cell_y, hx, hy, buffer);
    }

    /// Java `evalPoint` (3d overload) — unsupported.
    fn eval_point_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _buffer: &mut ResultBuffer3d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
        panic!("HexCellDistanceFunction.evalPoint 3d is not supported (Java UnsupportedOperationException)");
    }

    fn eval_point2_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    ) {
        let cell_hash = Self::get_hash(seed, cell_x, cell_y);
        let vec = HEX_CELL_2D[(cell_hash & 255) as usize];

        let jitter = point_evaluator.get_jitter();
        let px = jitter.get_point_x2(cell_x, vec);
        let py = jitter.get_point_y2(cell_y, vec);
        let hx = Self::to_hex_x(px, py);
        let hy = Self::to_hex_y(px, py);

        point_evaluator.eval_point2_2d(seed, x, y, cell_hash, cell_x, cell_y, hx, hy, buffer);
    }

    /// Java `evalPoint2` (3d overload) — unsupported.
    fn eval_point2_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _buffer: &mut ResultBuffer3d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
        panic!("HexCellDistanceFunction.evalPoint2 3d is not supported (Java UnsupportedOperationException)");
    }

    /// Java `collect` — iterates the skewed hex grid, scaled back to regular
    /// coordinates, filtered by the bounds.
    fn collect(
        &self,
        _original_seed: i32,
        seed: i32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        bounds: &Bounds2d,
        collector: &mut dyn FnMut(f64, f64),
        point_evaluator: &dyn PointEvaluator,
    ) {
        // Get the nearest hex-cell coords at the corners with padding
        let min_x = min_x - 1;
        let min_y = min_y - 1;
        let max_x = max_x + 1;
        let max_y = max_y + 1;

        let height = max_y - min_y;

        // Adjust width for skewed grid
        let width = (max_x - min_x) + (height >> 1);

        let jitter = point_evaluator.get_jitter();
        let mut dy = 0;
        while dy <= height {
            let cy = min_y + dy;

            // Shift the starting index of each row to the left by half the
            // y-delta to account for the skewed grid.
            let start_x = min_x - (dy >> 1);

            let mut dx = 0;
            while dx <= width {
                let cx = start_x + dx;
                let cell_hash = Self::get_hash(seed, cx, cy);
                let vec = HEX_CELL_2D[(cell_hash & 255) as usize];

                let px = jitter.get_point_x2(cx, vec);
                let py = jitter.get_point_y2(cy, vec);
                let mut hx = Self::to_hex_x(px, py);
                let mut hy = Self::to_hex_y(px, py);

                if !bounds.contains(hx, hy) {
                    dx += 1;
                    continue;
                }

                // Scale back to regular coordinates
                hx /= SCALE;
                hy /= SCALE;

                point_evaluator.collect_point(cell_hash, cx, cy, hx, hy, collector);
                dx += 1;
            }
            dy += 1;
        }
    }
}

/// Java anonymous `POINT_FUNCTION` on `HexCellDistanceFunction`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HexCellPointFunction;

impl HexCellPointFunction {
    /// Java static `HexCellDistanceFunction.POINT_FUNCTION` as an `Rc`.
    pub fn point_function() -> Rc<dyn CellPointFunction> {
        Rc::new(HexCellPointFunction)
    }
}

impl CellPointFunction for HexCellPointFunction {
    /// Java `scale(double)` — `value * SCALE`.
    fn scale(&self, value: f64) -> f64 {
        value * SCALE
    }

    /// Java `normalize(double)` — `value * NORMALIZATION`.
    fn normalize(&self, value: f64) -> f64 {
        value * NORMALIZATION
    }

    /// Java `getHash(int seed, int cellX, int cellY)`.
    fn get_hash(&self, seed: i32, cell_x: i32, cell_y: i32) -> i32 {
        HexCellDistanceFunction::get_hash(seed, cell_x, cell_y)
    }

    /// Java `getX(double x, double y)` — `toHexX(x, y)`.
    fn get_x(&self, x: f64, y: f64) -> f64 {
        HexCellDistanceFunction::to_hex_x(x, y)
    }

    /// Java `getY(double x, double y)` — `toHexY(x, y)`.
    fn get_y(&self, x: f64, y: f64) -> f64 {
        HexCellDistanceFunction::to_hex_y(x, y)
    }

    /// Java `getOffsets(int hash)`.
    fn get_offsets(&self, hash: i32) -> Double2 {
        HEX_CELL_2D[(hash & 255) as usize]
    }
}

/// Java nested `HexCellDistanceFunction.SquirrelHash`.
///
/// Hash function adapted from: https://youtu.be/LWFzPP8ZbdU?t=2799
///
/// This hash function benchmarks a decent amount faster than the standard
/// `HashUtil.rehash` method. The speedup is helpful for hex noise as it needs
/// to calculate a few extra cell hashes per invocation.
pub struct SquirrelHash;

impl SquirrelHash {
    /// Java `HASH0 = 198491317`.
    const HASH0: i32 = 198_491_317;
    /// Java `BIT_NOISE1 = 0xB5297A4D`.
    const BIT_NOISE1: i32 = 0xB529_7A4Du32 as i32;
    /// Java `BIT_NOISE2 = 0xB5297A4D`.
    const BIT_NOISE2: i32 = 0xB529_7A4Du32 as i32;
    /// Java `BIT_NOISE3 = 0xB5297A4D`.
    const BIT_NOISE3: i32 = 0xB529_7A4Du32 as i32;

    /// Java `public static int hash(int seed, int x, int y)` — all int
    /// arithmetic wraps, so the Rust port uses `wrapping_*` ops.
    #[inline]
    pub fn hash(seed: i32, x: i32, y: i32) -> i32 {
        let mut hash = x.wrapping_add(y.wrapping_mul(Self::HASH0));
        hash = hash.wrapping_mul(Self::BIT_NOISE1);
        hash = hash.wrapping_add(seed);
        hash ^= hash >> 8;
        hash = hash.wrapping_add(Self::BIT_NOISE2);
        hash ^= hash << 8;
        hash = hash.wrapping_mul(Self::BIT_NOISE3);
        hash ^= hash >> 8;
        hash
    }
}
