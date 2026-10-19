// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/BorderPointEvaluator.java`
//! Java `BorderPointEvaluator`.

use std::rc::Rc;

use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::math_util;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

/// Java `BorderPointEvaluator`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BorderPointEvaluator;

impl BorderPointEvaluator {
    /// Java `public static final BorderPointEvaluator INSTANCE` — as an
    /// `Rc<dyn PointEvaluator>` handle (Java statics become constructors /
    /// factory functions because Rust has no class-lifetime statics).
    pub fn instance() -> Rc<dyn PointEvaluator> {
        Rc::new(BorderPointEvaluator)
    }

    /// Java `protected static boolean isOrigin(int cellX, int cellY, ResultBuffer2d buffer)`.
    #[inline]
    fn is_origin(cell_x: i32, cell_y: i32, buffer: &ResultBuffer2d) -> bool {
        cell_x == buffer.ix2 && cell_y == buffer.iy2
    }

    /// Java `protected static double getBorderDistance(double x, double y,
    /// double originX, double originY, double cellPointX, double cellPointY)`.
    fn get_border_distance(
        x: f64,
        y: f64,
        origin_x: f64,
        origin_y: f64,
        cell_point_x: f64,
        cell_point_y: f64,
    ) -> f64 {
        // Mid-point between the current cell point and the origin cell point.
        let ax = (cell_point_x + origin_x) * 0.5;
        let ay = (cell_point_y + origin_y) * 0.5;

        // Normals to the line between the two cell points
        let norm_x = -(cell_point_y - origin_y);
        let norm_y = cell_point_x - origin_x;

        let bx = ax + norm_x;
        let by = ay + norm_y;

        math_util::distance_to_inf_line_sq(x, y, ax, ay, bx, by)
    }
}

impl PointEvaluator for BorderPointEvaluator {
    fn eval_point_2d(
        &self,
        _seed: i32,
        x: f64,
        y: f64,
        _cell_hash: i32,
        cell_x: i32,
        cell_y: i32,
        cell_point_x: f64,
        cell_point_y: f64,
        buffer: &mut ResultBuffer2d,
    ) {
        // Don't evaluate the origin against itself
        if Self::is_origin(cell_x, cell_y, buffer) {
            return;
        }

        let distance = Self::get_border_distance(x, y, buffer.x2, buffer.y2, cell_point_x, cell_point_y);
        if distance < buffer.distance {
            buffer.distance = distance;
        }
    }

    fn eval_point2_2d(
        &self,
        _seed: i32,
        x: f64,
        y: f64,
        _cell_hash: i32,
        cell_x: i32,
        cell_y: i32,
        cell_point_x: f64,
        cell_point_y: f64,
        buffer: &mut ResultBuffer2d,
    ) {
        // Don't evaluate the origin against itself
        if Self::is_origin(cell_x, cell_y, buffer) {
            return;
        }

        let distance = Self::get_border_distance(x, y, buffer.x2, buffer.y2, cell_point_x, cell_point_y);
        if distance < buffer.distance {
            buffer.distance2 = buffer.distance;
            buffer.distance = distance;
        } else if distance < buffer.distance2 {
            buffer.distance2 = distance;
        }
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
        // Java: throw new UnsupportedOperationException();
        panic!("BorderPointEvaluator.evalPoint 3d is unsupported (Java UnsupportedOperationException)");
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
        // Java: throw new UnsupportedOperationException();
        panic!("BorderPointEvaluator.evalPoint2 3d is unsupported (Java UnsupportedOperationException)");
    }
}
