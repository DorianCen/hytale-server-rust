// Ported from: `.hytale-source/.../worldgen/climate/DirectGrid.java`
//! Java `climate.DirectGrid` — a no-op `CellDistanceFunction` (returns the
//! query point itself).

use procedural::noise::cell::cell_distance_function::CellDistanceFunction;
use procedural::noise::cell::evaluator::point_evaluator::PointEvaluator;
use procedural::noise::result_buffer::{Bounds2d, ResultBuffer2d, ResultBuffer3d};

/// Java `climate.DirectGrid implements CellDistanceFunction`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DirectGrid;

impl CellDistanceFunction for DirectGrid {
    /// Java `nearest2D` — records the query point.
    fn nearest_2d(
        &self,
        _seed: i32,
        x: f64,
        y: f64,
        _cell_x: i32,
        _cell_y: i32,
        buffer: &mut ResultBuffer2d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
        buffer.x = x;
        buffer.y = y;
    }

    /// Java `nearest3D` — records the query point.
    fn nearest_3d(
        &self,
        _seed: i32,
        x: f64,
        y: f64,
        z: f64,
        _cell_x: i32,
        _cell_y: i32,
        _cell_z: i32,
        buffer: &mut ResultBuffer3d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
        buffer.x = x;
        buffer.y = y;
        buffer.z = z;
    }

    /// Java `transition2D` — no-op.
    fn transition_2d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _cell_x: i32,
        _cell_y: i32,
        _buffer: &mut ResultBuffer2d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
    }

    /// Java `transition3D` — no-op.
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
    }

    /// Java `evalPoint` (2d) — no-op.
    fn eval_point_2d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _cell_x: i32,
        _cell_y: i32,
        _buffer: &mut ResultBuffer2d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
    }

    /// Java `evalPoint` (3d) — no-op.
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
    }

    /// Java `evalPoint2` (2d) — no-op.
    fn eval_point2_2d(
        &self,
        _seed: i32,
        _x: f64,
        _y: f64,
        _cell_x: i32,
        _cell_y: i32,
        _buffer: &mut ResultBuffer2d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
    }

    /// Java `evalPoint2` (3d) — no-op.
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
    }

    /// Java `collect` — no-op.
    fn collect(
        &self,
        _original_seed: i32,
        _seed: i32,
        _min_x: i32,
        _min_y: i32,
        _max_x: i32,
        _max_y: i32,
        _bounds: &Bounds2d,
        _collector: &mut dyn FnMut(f64, f64),
        _point_evaluator: &dyn PointEvaluator,
    ) {
    }
}

/// Java `DirectGrid.INSTANCE`.
pub fn direct_grid() -> std::rc::Rc<dyn CellDistanceFunction> {
    std::rc::Rc::new(DirectGrid)
}
