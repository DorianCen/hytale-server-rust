// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/JitterPointEvaluator.java`
//! Java `JitterPointEvaluator`.

use std::rc::Rc;

use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::jitter::CellJitter;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

/// Java `JitterPointEvaluator` — `protected final PointEvaluator pointEvaluator`.
#[derive(Clone)]
pub struct JitterPointEvaluator {
    /// Java `pointEvaluator`.
    pub point_evaluator: Rc<dyn PointEvaluator>,
    /// Java `protected final CellJitter jitter`.
    pub jitter: CellJitter,
}

impl JitterPointEvaluator {
    /// Java `JitterPointEvaluator(PointEvaluator, CellJitter)`.
    pub fn new(point_evaluator: Rc<dyn PointEvaluator>, jitter: CellJitter) -> Self {
        Self {
            point_evaluator,
            jitter,
        }
    }
}

impl PointEvaluator for JitterPointEvaluator {
    /// Java `getJitter()` — always returns the stored jitter.
    fn get_jitter(&self) -> CellJitter {
        self.jitter
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
    ) {
        self.point_evaluator.eval_point_3d(
            seed,
            x,
            y,
            z,
            cell_hash,
            cell_x,
            cell_y,
            cell_z,
            cell_point_x,
            cell_point_y,
            cell_point_z,
            buffer,
        );
    }

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
    ) {
        self.point_evaluator.eval_point2_3d(
            seed,
            x,
            y,
            z,
            cell_hash,
            cell_x,
            cell_y,
            cell_z,
            cell_point_x,
            cell_point_y,
            cell_point_z,
            buffer,
        );
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
