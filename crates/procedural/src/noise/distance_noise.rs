// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/DistanceNoise.java`
//! Java `DistanceNoise` (abstract in Java; the Rust port is a concrete struct
//! since the only abstract members were the local-buffer accessors which
//! become explicit per-call buffers) and its `Distance2Function` interface +
//! nested `Distance2Mode` enum.

use std::rc::Rc;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::limit;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

/// Java `DistanceNoise.Distance2Function` (`@FunctionalInterface`).
pub trait Distance2Function {
    /// Java `double eval(double distance, double distance2)`.
    fn eval(&self, distance: f64, distance2: f64) -> f64;
}

/// Java `DistanceNoise.Distance2Mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Distance2Mode {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
}

impl Distance2Function for Distance2Mode {
    fn eval(&self, distance: f64, distance2: f64) -> f64 {
        match self {
            Distance2Mode::Add => distance + distance2,
            Distance2Mode::Sub => distance2 - distance,
            Distance2Mode::Mul => distance * distance2,
            Distance2Mode::Div => distance / distance2,
            Distance2Mode::Min => distance,
            Distance2Mode::Max => distance2,
        }
    }
}

/// Java `DistanceNoise`.
#[derive(Clone)]
pub struct DistanceNoise {
    /// Java `protected final CellDistanceFunction cellDistanceFunction`.
    pub cell_distance_function: Rc<dyn CellDistanceFunction>,
    /// Java `protected final PointEvaluator pointEvaluator`.
    pub point_evaluator: Rc<dyn PointEvaluator>,
    /// Java `protected final Distance2Function distance2Function`.
    pub distance2_function: Rc<dyn Distance2Function>,
}

impl DistanceNoise {
    /// Java `DistanceNoise(CellDistanceFunction, PointEvaluator, Distance2Function)`.
    pub fn new(
        cell_distance_function: Rc<dyn CellDistanceFunction>,
        point_evaluator: Rc<dyn PointEvaluator>,
        distance2_function: Rc<dyn Distance2Function>,
    ) -> Self {
        Self {
            cell_distance_function,
            point_evaluator,
            distance2_function,
        }
    }

    /// Java `CellDistanceFunction getCellDistanceFunction()`.
    pub fn get_cell_distance_function(&self) -> &dyn CellDistanceFunction {
        self.cell_distance_function.as_ref()
    }

    /// Java `Distance2Function getDistance2Function()`.
    pub fn get_distance2_function(&self) -> &dyn Distance2Function {
        self.distance2_function.as_ref()
    }
}

impl NoiseFunction2d for DistanceNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        #[allow(unused_variables)]
        let (seed, offset_seed) = (seed, offset_seed);

        let x = self.cell_distance_function.scale(x);
        let y = self.cell_distance_function.scale(y);

        let xr = self.cell_distance_function.get_cell_x2(x, y);
        let yr = self.cell_distance_function.get_cell_y2(x, y);

        let mut buffer = ResultBuffer2d::new();
        buffer.distance = f64::INFINITY;
        buffer.distance2 = f64::INFINITY;

        self.cell_distance_function.transition_2d(
            offset_seed,
            x,
            y,
            xr,
            yr,
            &mut buffer,
            self.point_evaluator.as_ref(),
        );
        buffer.distance = f64::sqrt(buffer.distance);
        buffer.distance2 = f64::sqrt(buffer.distance2);
        limit(self.distance2_function.eval(buffer.distance, buffer.distance2)) * 2.0 - 1.0
    }
}

impl NoiseFunction3d for DistanceNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        #[allow(unused_variables)]
        let (seed, offset_seed) = (seed, offset_seed);

        let x = self.cell_distance_function.scale(x);
        let y = self.cell_distance_function.scale(y);
        let z = self.cell_distance_function.scale(z);

        let xr = self.cell_distance_function.get_cell_x3(x, y, z);
        let yr = self.cell_distance_function.get_cell_y3(x, y, z);
        let zr = self.cell_distance_function.get_cell_z3(x, y, z);

        let mut buffer = ResultBuffer3d::new();
        buffer.distance = f64::INFINITY;
        buffer.distance2 = f64::INFINITY;

        self.cell_distance_function.transition_3d(
            offset_seed,
            x,
            y,
            z,
            xr,
            yr,
            zr,
            &mut buffer,
            self.point_evaluator.as_ref(),
        );
        buffer.distance = f64::sqrt(buffer.distance);
        buffer.distance2 = f64::sqrt(buffer.distance2);
        limit(self.distance2_function.eval(buffer.distance, buffer.distance2)) * 2.0 - 1.0
    }
}
