// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/DensityPointEvaluator.java`
//! Java `DensityPointEvaluator`.
//!
//! ⚠ NOTE(subagent): `condition.IIntCondition` / `condition.IDoubleCondition`
//! live in [`crate::noise::support`] until the `condition/` package port
//! lands. The Java ctor overload taking a `@Nullable IDoubleCondition` becomes
//! [`DensityPointEvaluator::with_double_condition`]; `null` maps to `None`
//! and Java's `seed -> true` lambda maps to `support::AlwaysTrueIntCondition`.

use std::rc::Rc;

use hytale_math::hash_util;

use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
use crate::noise::support::{AlwaysTrueIntCondition, IDoubleCondition, IIntCondition};

/// Java `DensityPointEvaluator` — `protected final PointEvaluator pointEvaluator`.
#[derive(Clone)]
pub struct DensityPointEvaluator {
    /// Java `pointEvaluator`.
    pub point_evaluator: Rc<dyn PointEvaluator>,
    /// Java `protected final IIntCondition density`.
    pub density: Rc<dyn IIntCondition>,
}

impl DensityPointEvaluator {
    /// Java `DensityPointEvaluator(PointEvaluator, IIntCondition)`.
    pub fn new(point_evaluator: Rc<dyn PointEvaluator>, density: Rc<dyn IIntCondition>) -> Self {
        Self {
            point_evaluator,
            density,
        }
    }

    /// Java `DensityPointEvaluator(PointEvaluator, @Nullable IDoubleCondition)` —
    /// wraps the threshold with `getDensityCondition` first.
    pub fn with_double_condition(
        point_evaluator: Rc<dyn PointEvaluator>,
        threshold: Option<Rc<dyn IDoubleCondition>>,
    ) -> Self {
        Self::new(point_evaluator, Self::get_density_condition(threshold))
    }

    /// Java static `getDensityCondition(@Nullable IDoubleCondition)`:
    /// `null` → `seed -> true`; otherwise `seed -> threshold.eval(randomDensityCondition(seed))`.
    pub fn get_density_condition(
        threshold: Option<Rc<dyn IDoubleCondition>>,
    ) -> Rc<dyn IIntCondition> {
        match threshold {
            None => Rc::new(AlwaysTrueIntCondition),
            Some(threshold) => Rc::new(DensityCondition { threshold }),
        }
    }
}

/// The anonymous `seed -> threshold.eval(randomDensityCondition(seed))` lambda
/// from `getDensityCondition`.
#[derive(Clone)]
struct DensityCondition {
    threshold: Rc<dyn IDoubleCondition>,
}

impl IIntCondition for DensityCondition {
    fn eval(&self, seed: i32) -> bool {
        self.threshold.eval(Self::random_density_condition(seed))
    }
}

impl DensityCondition {
    /// Java static `randomDensityCondition(int seed)`.
    #[inline]
    fn random_density_condition(seed: i32) -> f64 {
        hash_util::random2(seed as i64, DENSITY_RANDOM_SEED)
    }
}

/// Java `0x9afc_33ae` — an **int** literal (negative, `-1693957202`) that
/// widens to `long` in `HashUtil.random(int, int)`.
const DENSITY_RANDOM_SEED: i64 = 0x9afc_33ae_u32 as i32 as i64;

/// Java static `randomDensityCondition(int seed)` (public form).
#[inline]
pub fn random_density_condition(seed: i32) -> f64 {
    hash_util::random2(seed as i64, DENSITY_RANDOM_SEED)
}

impl PointEvaluator for DensityPointEvaluator {
    /// Java `getJitter()` — delegate to the wrapped evaluator.
    fn get_jitter(&self) -> crate::noise::cell::jitter::CellJitter {
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
        if !self.density.eval(cell_hash) {
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
        if !self.density.eval(cell_hash) {
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
        if !self.density.eval(cell_hash) {
            return;
        }

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
        if !self.density.eval(cell_hash) {
            return;
        }

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
        x: f64,
        y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        if !self.density.eval(cell_hash) {
            return;
        }

        self.point_evaluator
            .collect_point(cell_hash, cell_x, cell_y, x, y, consumer);
    }
}
