// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/SkipCellPointEvaluator.java`
//! Java `SkipCellPointEvaluator`.
//!
//! ⚠ NOTE(subagent): `it.unimi.dsi.fastutil.HashCommon.nextPowerOfTwo(int)` is
//! in [`crate::noise::support`] as
//! `fastutil_next_power_of_two`.

use std::rc::Rc;

use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::jitter::CellJitter;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
use crate::noise::support::fastutil_next_power_of_two;

/// Java `SkipCellPointEvaluator.Mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkipCellMode {
    Checkerboard,
    Grid,
}

/// Java statics `SkipCellPointEvaluator.DEFAULT_NO_SKIP`.
pub const DEFAULT_NO_SKIP: i32 = 0;
/// Java statics `SkipCellPointEvaluator.DEFAULT_MODE`.
pub const DEFAULT_MODE: SkipCellMode = SkipCellMode::Checkerboard;

/// Java `SkipCellPointEvaluator`.
#[derive(Clone)]
pub struct SkipCellPointEvaluator {
    /// Java `@Nonnull protected final PointEvaluator pointEvaluator`.
    pub point_evaluator: Rc<dyn PointEvaluator>,
    /// Java `@Nonnull protected final Mode mode`.
    pub mode: SkipCellMode,
    /// Java `protected final int mask`.
    pub mask: i32,
    /// Java `protected final int mid`.
    pub mid: i32,
}

impl SkipCellPointEvaluator {
    /// Java `SkipCellPointEvaluator(PointEvaluator, Mode, int period)`.
    pub fn new(point_evaluator: Rc<dyn PointEvaluator>, mode: SkipCellMode, period: i32) -> Self {
        // Java: final int interval = HashCommon.nextPowerOfTwo(Math.max(0, period) + 1);
        let interval = fastutil_next_power_of_two(period.max(0).wrapping_add(1));
        Self {
            point_evaluator,
            mode,
            mask: interval - 1,
            mid: interval >> 1,
        }
    }

    /// Java `protected boolean skip(Mode, int cx, int cy)`.
    fn skip(&self, mode: SkipCellMode, cx: i32, cy: i32) -> bool {
        let x0 = cx & self.mask;
        let y0 = cy & self.mask;

        // Is cell on one of the major lattice points.
        let result = (x0 == 0 && y0 == 0)
            // Include the diagonal mid-points for checkerboard mode.
            || (mode == SkipCellMode::Checkerboard && x0 == self.mid && y0 == self.mid);

        // Skip if cell is not on the lattice.
        !result
    }
}

impl PointEvaluator for SkipCellPointEvaluator {
    /// Java `getJitter()` — delegate to the wrapped evaluator.
    fn get_jitter(&self) -> CellJitter {
        self.point_evaluator.get_jitter()
    }

    fn eval_point_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_hash: i32,
        cell_x: i32,
        cell_y: i32,
        cell_point_x: f64,
        cell_point_y: f64,
        buffer: &mut ResultBuffer2d,
    ) {
        if self.skip(self.mode, cell_x, cell_y) {
            return;
        }

        self.point_evaluator.eval_point_2d(
            seed,
            x,
            y,
            cell_hash,
            cell_x,
            cell_y,
            cell_point_x,
            cell_point_y,
            buffer,
        );
    }

    fn eval_point2_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_hash: i32,
        cell_x: i32,
        cell_y: i32,
        cell_point_x: f64,
        cell_point_y: f64,
        buffer: &mut ResultBuffer2d,
    ) {
        if self.skip(self.mode, cell_x, cell_y) {
            return;
        }

        self.point_evaluator.eval_point2_2d(
            seed,
            x,
            y,
            cell_hash,
            cell_x,
            cell_y,
            cell_point_x,
            cell_point_y,
            buffer,
        );
    }

    fn eval_point_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_hash: i32,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _cell_point_x: f64,
        _cell_point_y: f64,
        _cell_point_z: f64,
        _buffer: &mut ResultBuffer3d,
    ) {
        // Not implemented (Java leaves the body empty)
    }

    fn eval_point2_3d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        _cell_hash: i32,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        _cell_point_x: f64,
        _cell_point_y: f64,
        _cell_point_z: f64,
        _buffer: &mut ResultBuffer3d,
    ) {
        // Not implemented (Java leaves the body empty)
    }

    fn collect_point(
        &self,
        cell_hash: i32,
        cell_x: i32,
        cell_y: i32,
        cell_centre_x: f64,
        cell_centre_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        if self.skip(self.mode, cell_x, cell_y) {
            return;
        }

        self.point_evaluator.collect_point(
            cell_hash,
            cell_x,
            cell_y,
            cell_centre_x,
            cell_centre_y,
            consumer,
        );
    }
}
