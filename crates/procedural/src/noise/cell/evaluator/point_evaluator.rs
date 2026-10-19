// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/PointEvaluator.java`
//! Java `PointEvaluator` interface.
//!
//! ⚠ NOTE(subagent): `PointEvaluator.of(distanceFunction, density, distanceMod,
//! jitter)` and `PointEvaluator.evalPoint(int seed, double x, double y, double
//! z, ...)` Java overloads become `eval_point_3d` / `eval_point2_3d`; the
//! generic `collectPoint` overload becomes `collect_point` (2d-specific; Java
//! only defines the 2d flavor). The static `of(...)` composition helpers are
//! provided as inherent constructors on the concrete evaluator structs.

use crate::noise::cell::jitter::CellJitter;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

/// Java `PointEvaluator`.
///
/// ⚠ NOTE(subagent): the generic Java method
/// `<T> void collectPoint(..., T ctx, PointConsumer<T> consumer)` is ported as
/// `collect_point(..., consumer: &mut dyn FnMut(f64, f64))` — the caller's
/// closure captures Java's `ctx`, keeping the trait object-safe (`dyn`
/// usable). Behavior is identical.
pub trait PointEvaluator {
    /// Java default `CellJitter getJitter()` (defaults to `DefaultCellJitter.DEFAULT_ONE`).
    fn get_jitter(&self) -> CellJitter {
        CellJitter::DEFAULT_ONE
    }

    /// Java `void evalPoint(int seed, double x, double y, int cellHash, int cellX, int cellY,
    /// double cellPointX, double cellPointY, ResultBuffer.ResultBuffer2d buffer)`.
    #[allow(clippy::too_many_arguments)]
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
    );

    /// Java `void evalPoint2(int seed, double x, double y, int cellHash, int cellX, int cellY,
    /// double cellPointX, double cellPointY, ResultBuffer.ResultBuffer2d buffer)`.
    #[allow(clippy::too_many_arguments)]
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
    );

    /// Java `void evalPoint(int seed, double x, double y, double z, int cellHash, int cellX, int cellY,
    /// int cellZ, double cellPointX, double cellPointY, double cellPointZ, ResultBuffer.ResultBuffer3d buffer)`.
    #[allow(clippy::too_many_arguments)]
    fn eval_point_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        cell_hash: i32,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
        cell_point_x: f64,
        cell_point_y: f64,
        cell_point_z: f64,
        buffer: &mut ResultBuffer3d,
    );

    /// Java `void evalPoint2(int seed, double x, double y, double z, ...)`.
    #[allow(clippy::too_many_arguments)]
    fn eval_point2_3d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        z: f64,
        cell_hash: i32,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
        cell_point_x: f64,
        cell_point_y: f64,
        cell_point_z: f64,
        buffer: &mut ResultBuffer3d,
    );

    /// Java `default <T> void collectPoint(int cellHash, int cellX, int cellY, double cellCentreX,
    /// double cellCentreY, T ctx, PointConsumer<T> consumer)` — the generic
    /// `(T ctx, PointConsumer<T>)` pair becomes `&mut dyn FnMut(f64, f64)`.
    #[allow(clippy::too_many_arguments)]
    fn collect_point(
        &self,
        _cell_hash: i32,
        _cell_x: i32,
        _cell_y: i32,
        cell_centre_x: f64,
        cell_centre_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        consumer(cell_centre_x, cell_centre_y);
    }
}
