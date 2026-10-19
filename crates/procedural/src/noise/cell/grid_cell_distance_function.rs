// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/GridCellDistanceFunction.java`
//! Java `GridCellDistanceFunction` (+ its anonymous `CellPointFunction`
//! singleton, ported as the [`GridCellPointFunction`] struct).

use std::rc::Rc;

use hytale_math::hash_util;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::cell_point_function::CellPointFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cellular_noise::{CELL_2D, CELL_3D};
use crate::noise::double_array::{Double2, Double3};
use crate::noise::result_buffer::{Bounds2d, ResultBuffer2d, ResultBuffer3d};

/// Java `GridCellDistanceFunction` — the grid (square-cell) distance function.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GridCellDistanceFunction;

impl GridCellDistanceFunction {
    /// Java `public static final GridCellDistanceFunction DISTANCE_FUNCTION`.
    pub fn distance_function() -> Rc<dyn CellDistanceFunction> {
        Rc::new(GridCellDistanceFunction)
    }

    /// Java `public static int getHash(int seed, int cellX, int cellY)`.
    #[inline]
    pub fn get_hash(seed: i32, cell_x: i32, cell_y: i32) -> i32 {
        // Java: (int) HashUtil.rehash(seed, cellX, cellY)
        hash_util::rehash3(seed as i64, cell_x as i64, cell_y as i64) as i32
    }

    /// Java `public static int getHash(int seed, int cellX, int cellY, int cellZ)`.
    #[inline]
    pub fn get_hash3(seed: i32, cell_x: i32, cell_y: i32, cell_z: i32) -> i32 {
        // Java: (int) HashUtil.rehash(seed, cellX, cellY, cellZ)
        hash_util::rehash4(seed as i64, cell_x as i64, cell_y as i64, cell_z as i64) as i32
    }
}

/// Java anonymous `POINT_FUNCTION` on `GridCellDistanceFunction`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GridCellPointFunction;

impl GridCellPointFunction {
    /// Java static `GridCellDistanceFunction.POINT_FUNCTION` as an `Rc`.
    pub fn point_function() -> Rc<dyn CellPointFunction> {
        Rc::new(GridCellPointFunction)
    }
}

impl CellPointFunction for GridCellPointFunction {
    fn get_hash(&self, seed: i32, cell_x: i32, cell_y: i32) -> i32 {
        GridCellDistanceFunction::get_hash(seed, cell_x, cell_y)
    }

    fn get_offsets(&self, hash: i32) -> Double2 {
        CELL_2D[(hash & 255) as usize]
    }

    fn get_x(&self, x: f64, _y: f64) -> f64 {
        x
    }

    fn get_y(&self, _x: f64, y: f64) -> f64 {
        y
    }
}

#[allow(clippy::too_many_arguments)]
impl CellDistanceFunction for GridCellDistanceFunction {
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
        let mut cy = cell_y - 1;
        while cy <= cell_y + 1 {
            let mut cx = cell_x - 1;
            while cx <= cell_x + 1 {
                self.eval_point_2d(seed, x, y, cx, cy, buffer, point_evaluator);
                cx += 1;
            }
            cy += 1;
        }
    }

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
    ) {
        let mut cx = cell_x - 1;
        while cx <= cell_x + 1 {
            let mut cy = cell_y - 1;
            while cy <= cell_y + 1 {
                let mut cz = cell_z - 1;
                while cz <= cell_z + 1 {
                    self.eval_point_3d(seed, x, y, z, cx, cy, cz, buffer, point_evaluator);
                    cz += 1;
                }
                cy += 1;
            }
            cx += 1;
        }
    }

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
        let mut cy = cell_y - 1;
        while cy <= cell_y + 1 {
            let mut cx = cell_x - 1;
            while cx <= cell_x + 1 {
                self.eval_point2_2d(seed, x, y, cx, cy, buffer, point_evaluator);
                cx += 1;
            }
            cy += 1;
        }
    }

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
    ) {
        let mut cx = cell_x - 1;
        while cx <= cell_x + 1 {
            let mut cy = cell_y - 1;
            while cy <= cell_y + 1 {
                let mut cz = cell_z - 1;
                while cz <= cell_z + 1 {
                    self.eval_point2_3d(seed, x, y, z, cx, cy, cz, buffer, point_evaluator);
                    cz += 1;
                }
                cy += 1;
            }
            cx += 1;
        }
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
        let vec = CELL_2D[(cell_hash & 255) as usize];

        let jitter = point_evaluator.get_jitter();
        let px = jitter.get_point_x2(cell_x, vec);
        let py = jitter.get_point_y2(cell_y, vec);

        point_evaluator.eval_point_2d(seed, x, y, cell_hash, cell_x, cell_y, px, py, buffer);
    }

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
    ) {
        let cell_hash = Self::get_hash3(seed, cell_x, cell_y, cell_z);
        let vec: Double3 = CELL_3D[(cell_hash & 255) as usize];

        let jitter = point_evaluator.get_jitter();
        let px = jitter.get_point_x3(cell_x, vec);
        let py = jitter.get_point_y3(cell_y, vec);
        let pz = jitter.get_point_z3(cell_z, vec);

        point_evaluator.eval_point_3d(
            seed, x, y, z, cell_hash, cell_x, cell_y, cell_z, px, py, pz, buffer,
        );
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
        let vec = CELL_2D[(cell_hash & 255) as usize];

        let jitter = point_evaluator.get_jitter();
        let px = jitter.get_point_x2(cell_x, vec);
        let py = jitter.get_point_y2(cell_y, vec);

        point_evaluator.eval_point2_2d(seed, x, y, cell_hash, cell_x, cell_y, px, py, buffer);
    }

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
    ) {
        let cell_hash = Self::get_hash3(seed, cell_x, cell_y, cell_z);
        let vec: Double3 = CELL_3D[(cell_hash & 255) as usize];

        let jitter = point_evaluator.get_jitter();
        let px = jitter.get_point_x3(cell_x, vec);
        let py = jitter.get_point_y3(cell_y, vec);
        let pz = jitter.get_point_z3(cell_z, vec);

        point_evaluator.eval_point2_3d(
            seed, x, y, z, cell_hash, cell_x, cell_y, cell_z, px, py, pz, buffer,
        );
    }

    fn collect(
        &self,
        _original_seed: i32,
        seed: i32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        _bounds: &Bounds2d,
        collector: &mut dyn FnMut(f64, f64),
        point_evaluator: &dyn PointEvaluator,
    ) {
        let jitter = point_evaluator.get_jitter();
        let mut cy = min_y;
        while cy <= max_y {
            let mut cx = min_x;
            while cx <= max_x {
                let cell_hash = Self::get_hash(seed, cx, cy);

                let vec = CELL_2D[(cell_hash & 255) as usize];
                let px = jitter.get_point_x2(cx, vec);
                let py = jitter.get_point_y2(cy, vec);

                point_evaluator.collect_point(cell_hash, cx, cy, px, py, collector);
                cx += 1;
            }
            cy += 1;
        }
    }
}
