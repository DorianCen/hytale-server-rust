// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/CellDistanceFunction.java`
//! Java `CellDistanceFunction` interface.
//!
//! ⚠ NOTE(subagent): Java's generic `<T> collect(...)` uses a
//! `PointConsumer<T>`; generic trait methods are not object-safe, so `collect`
//! carries `where Self: Sized`. `Cell` collection callers that go through
//! `Rc<dyn CellDistanceFunction>` can either keep a concrete handle or switch
//! to dynamic dispatch via a helper added during integration.
//!
//! Default methods are ported with their Java defaults. 2D/3D Java overloads
//! are suffixed `_2d`/`_3d` (`getCellX(x, y, z)` → `get_cell_x3`).

use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::math_util;
use crate::noise::result_buffer::{Bounds2d, ResultBuffer2d, ResultBuffer3d};

/// Java `CellDistanceFunction`.
pub trait CellDistanceFunction {
    /// Java `default double scale(double value)`.
    fn scale(&self, value: f64) -> f64 {
        value
    }

    /// Java `default double invScale(double value)`.
    fn inv_scale(&self, value: f64) -> f64 {
        value
    }

    /// Java `default int getCellX(double x, double y)`.
    fn get_cell_x2(&self, x: f64, _y: f64) -> i32 {
        math_util::floor(x)
    }

    /// Java `default int getCellY(double x, double y)`.
    fn get_cell_y2(&self, _x: f64, y: f64) -> i32 {
        math_util::floor(y)
    }

    /// Java `default int getCellX(double x, double y, double z)`.
    fn get_cell_x3(&self, x: f64, _y: f64, _z: f64) -> i32 {
        math_util::floor(x)
    }

    /// Java `default int getCellY(double x, double y, double z)`.
    fn get_cell_y3(&self, _x: f64, y: f64, _z: f64) -> i32 {
        math_util::floor(y)
    }

    /// Java `default int getCellZ(double x, double y, double z)`.
    fn get_cell_z3(&self, _x: f64, _y: f64, z: f64) -> i32 {
        math_util::floor(z)
    }

    /// Java `void nearest2D(int seed, double x, double y, int cellX, int cellY,
    /// ResultBuffer.ResultBuffer2d buffer, PointEvaluator pointEvaluator)`.
    fn nearest_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `void nearest3D(int seed, double x, double y, double z, int cellX, int cellY, int cellZ,
    /// ResultBuffer.ResultBuffer3d buffer, PointEvaluator pointEvaluator)`.
    fn nearest_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
        buffer: &mut ResultBuffer3d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `void transition2D(int seed, double x, double y, int cellX, int cellY,
    /// ResultBuffer.ResultBuffer2d buffer, PointEvaluator pointEvaluator)`.
    fn transition_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `void transition3D(...)`.
    fn transition_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
        buffer: &mut ResultBuffer3d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `void evalPoint(int seed, double x, double y, int cellX, int cellY, ...)`.
    fn eval_point_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `void evalPoint(int seed, double x, double y, double z, int cellX, int cellY, int cellZ, ...)`.
    fn eval_point_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
        buffer: &mut ResultBuffer3d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `void evalPoint2(int seed, double x, double y, int cellX, int cellY, ...)`.
    fn eval_point2_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        cell_x: i32,
        cell_y: i32,
        buffer: &mut ResultBuffer2d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `void evalPoint2(int seed, double x, double y, double z, int cellX, int cellY, int cellZ, ...)`.
    fn eval_point2_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
        buffer: &mut ResultBuffer3d,
        point_evaluator: &dyn PointEvaluator,
    );

    /// Java `<T> void collect(int originalSeed, int seed, int minX, int minY, int maxX, int maxY,
    /// ResultBuffer.Bounds2d bounds, T ctx, PointConsumer<T> collector, PointEvaluator pointEvaluator)`.
    ///
    /// The Java generic `(T ctx, PointConsumer<T>)` pair becomes a single
    /// `&mut dyn FnMut(f64, f64)` closure (the consumer captures its own context),
    /// which keeps the trait object-safe.
    #[allow(clippy::too_many_arguments)]
    fn collect(
        &self,
        original_seed: i32,
        seed: i32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        bounds: &Bounds2d,
        collector: &mut dyn FnMut(f64, f64),
        point_evaluator: &dyn PointEvaluator,
    );
}
