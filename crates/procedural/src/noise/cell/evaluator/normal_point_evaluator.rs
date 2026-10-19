// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/NormalPointEvaluator.java`
//! Java `NormalPointEvaluator`.

use std::rc::Rc;

use crate::noise::cell::distance_calculation_mode::DistanceCalculationMode;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::point_distance_function::PointDistanceFunction;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

/// Java `NormalPointEvaluator` — `protected final PointDistanceFunction distanceFunction`.
#[derive(Clone)]
pub struct NormalPointEvaluator {
    pub distance_function: Rc<dyn PointDistanceFunction>,
}

impl NormalPointEvaluator {
    /// Java `NormalPointEvaluator(PointDistanceFunction)`.
    pub fn new(distance_function: Rc<dyn PointDistanceFunction>) -> Self {
        Self { distance_function }
    }

    /// Java statics `NormalPointEvaluator.EUCLIDEAN / MANHATTAN / NATURAL / MAX`
    /// (singletons; the Rust port exposes them as `Rc<dyn PointEvaluator>`).
    pub fn euclidean() -> Rc<dyn PointEvaluator> {
        Rc::new(Self::new(Rc::new(DistanceCalculationMode::Euclidean)))
    }

    /// Java `MANHATTAN`.
    pub fn manhattan() -> Rc<dyn PointEvaluator> {
        Rc::new(Self::new(Rc::new(DistanceCalculationMode::Manhattan)))
    }

    /// Java `NATURAL`.
    pub fn natural() -> Rc<dyn PointEvaluator> {
        Rc::new(Self::new(Rc::new(DistanceCalculationMode::Natural)))
    }

    /// Java `MAX`.
    pub fn max() -> Rc<dyn PointEvaluator> {
        Rc::new(Self::new(Rc::new(DistanceCalculationMode::Max)))
    }
}

#[allow(clippy::too_many_arguments)]
impl PointEvaluator for NormalPointEvaluator {
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
        let distance = self.distance_function.distance_2d_at_cell(
            seed,
            cell_x,
            cell_y,
            cell_point_x,
            cell_point_y,
            cell_point_x - x,
            cell_point_y - y,
        );
        buffer.register(
            cell_hash,
            cell_x,
            cell_y,
            distance,
            cell_point_x,
            cell_point_y,
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
        let distance = self.distance_function.distance_2d_at_cell(
            seed,
            cell_x,
            cell_y,
            cell_point_x,
            cell_point_y,
            cell_point_x - x,
            cell_point_y - y,
        );
        buffer.register2(
            cell_hash,
            cell_x,
            cell_y,
            distance,
            cell_point_x,
            cell_point_y,
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
        let distance = self.distance_function.distance_3d_at_cell(
            seed,
            cell_x,
            cell_y,
            cell_z,
            cell_point_x,
            cell_point_y,
            cell_point_z,
            cell_point_x - x,
            cell_point_y - y,
            cell_point_z - z,
        );
        buffer.register(
            cell_hash,
            cell_x,
            cell_y,
            cell_z,
            distance,
            cell_point_x,
            cell_point_y,
            cell_point_z,
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
        let distance = self.distance_function.distance_3d_at_cell(
            seed,
            cell_x,
            cell_y,
            cell_z,
            cell_point_x,
            cell_point_y,
            cell_point_z,
            cell_point_x - x,
            cell_point_y - y,
            cell_point_z - z,
        );
        buffer.register2(
            cell_hash,
            cell_x,
            cell_y,
            cell_z,
            distance,
            cell_point_x,
            cell_point_y,
            cell_point_z,
        );
    }
}
