// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/BorderDistanceFunction.java`
//! Java `BorderDistanceFunction`.
//!
//! ⚠ NOTE(subagent): `condition.IDoubleCondition` is `support::IDoubleCondition`
//! (`procedurallib.condition` is ported separately); the Java
//! `@Nullable IDoubleCondition` ctor parameter becomes
//! `Option<Rc<dyn IDoubleCondition>>`. 3d / direct `evalPoint` methods
//! `throw new UnsupportedOperationException()` — ported as panics.

use std::rc::Rc;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::density_point_evaluator::DensityPointEvaluator;
use crate::noise::cell::evaluator::jitter_point_evaluator::JitterPointEvaluator;
use crate::noise::cell::evaluator::normal_point_evaluator::NormalPointEvaluator;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::result_buffer::{Bounds2d, ResultBuffer2d, ResultBuffer3d};
use crate::noise::support::{IDoubleCondition, IIntCondition};

/// Java `BorderDistanceFunction`.
#[derive(Clone)]
pub struct BorderDistanceFunction {
    /// Java `protected final CellDistanceFunction distanceFunction`.
    pub distance_function: Rc<dyn CellDistanceFunction>,
    /// Java `@Nonnull protected final PointEvaluator cellEvaluator`.
    pub cell_evaluator: Rc<dyn PointEvaluator>,
    /// Java `@Nonnull protected final PointEvaluator borderEvaluator`.
    pub border_evaluator: Rc<dyn PointEvaluator>,
    /// Java `@Nonnull protected final IIntCondition density`.
    pub density: Rc<dyn IIntCondition>,
}

impl BorderDistanceFunction {
    /// Java `BorderDistanceFunction(CellDistanceFunction distanceFunction,
    /// PointEvaluator borderEvaluator, IDoubleCondition density)` — `density`
    /// may be `null` in Java (→ `None`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        distance_function: Rc<dyn CellDistanceFunction>,
        border_evaluator: Rc<dyn PointEvaluator>,
        density: Option<Rc<dyn IDoubleCondition>>,
    ) -> Self {
        let cell_evaluator: Rc<dyn PointEvaluator> = Rc::new(JitterPointEvaluator::new(
            NormalPointEvaluator::euclidean(),
            border_evaluator.get_jitter(),
        ));

        Self {
            distance_function,
            cell_evaluator,
            border_evaluator,
            density: DensityPointEvaluator::get_density_condition(density),
        }
    }
}

#[allow(clippy::too_many_arguments)]
impl CellDistanceFunction for BorderDistanceFunction {
    /// Java `scale(double)` — delegate.
    fn scale(&self, value: f64) -> f64 {
        self.distance_function.scale(value)
    }

    /// Java `invScale(double)` — delegate.
    fn inv_scale(&self, value: f64) -> f64 {
        self.distance_function.inv_scale(value)
    }

    /// Java `getCellX(double x, double y)` — delegate.
    fn get_cell_x2(&self, x: f64, y: f64) -> i32 {
        self.distance_function.get_cell_x2(x, y)
    }

    /// Java `getCellY(double x, double y)` — delegate.
    fn get_cell_y2(&self, x: f64, y: f64) -> i32 {
        self.distance_function.get_cell_y2(x, y)
    }

    /// Java `nearest2D` — delegates to `transition2D` (we only need to find
    /// the nearest cell at this stage).
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
        self.transition_2d(seed, x, y, cell_x, cell_y, buffer, point_evaluator);
    }

    /// Java `transition2D` — nearest search with the cell evaluator, then a
    /// ring search for the border with the border evaluator.
    fn transition_2d(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        mut cell_x: i32,
        mut cell_y: i32,
        buffer: &mut ResultBuffer2d,
        _point_evaluator: &dyn PointEvaluator,
    ) {
        // Note: We only need to find the nearest cell at this stage
        self.distance_function.nearest_2d(
            seed,
            x,
            y,
            cell_x,
            cell_y,
            buffer,
            self.cell_evaluator.as_ref(),
        );

        if !self.density.eval(buffer.hash) {
            buffer.distance = 0.0;
            return;
        }

        cell_x = buffer.ix;
        cell_y = buffer.iy;

        buffer.ix2 = cell_x;
        buffer.iy2 = cell_y;
        buffer.x2 = buffer.x;
        buffer.y2 = buffer.y;
        buffer.distance = f64::INFINITY;
        buffer.distance2 = f64::INFINITY;

        let dx = if self.border_evaluator.get_jitter().get_max_x() > 0.5 {
            2
        } else {
            1
        };
        let dy = if self.border_evaluator.get_jitter().get_max_y() > 0.5 {
            2
        } else {
            1
        };

        let mut cy = cell_y - dy;
        while cy <= cell_y + dy {
            let mut cx = cell_x - dx;
            while cx <= cell_x + dx {
                self.distance_function.eval_point2_2d(
                    seed,
                    x,
                    y,
                    cx,
                    cy,
                    buffer,
                    self.border_evaluator.as_ref(),
                );
                cx += 1;
            }
            cy += 1;
        }
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
        panic!("BorderDistanceFunction.nearest3D is not supported (Java UnsupportedOperationException)");
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
        panic!("BorderDistanceFunction.transition3D is not supported (Java UnsupportedOperationException)");
    }

    /// Java `evalPoint` (2d) — unsupported.
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
        panic!("BorderDistanceFunction.evalPoint is not supported (Java UnsupportedOperationException)");
    }

    /// Java `evalPoint` (3d) — unsupported.
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
        panic!("BorderDistanceFunction.evalPoint 3d is not supported (Java UnsupportedOperationException)");
    }

    /// Java `evalPoint2` (2d) — unsupported.
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
        panic!("BorderDistanceFunction.evalPoint2 is not supported (Java UnsupportedOperationException)");
    }

    /// Java `evalPoint2` (3d) — unsupported.
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
        panic!("BorderDistanceFunction.evalPoint2 3d is not supported (Java UnsupportedOperationException)");
    }

    /// Java `collect` — delegate to the wrapped distance function.
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
    ) {
        self.distance_function.collect(
            original_seed,
            seed,
            min_x,
            min_y,
            max_x,
            max_y,
            bounds,
            collector,
            point_evaluator,
        );
    }
}
