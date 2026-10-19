// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/cell/evaluator/DistancePointEvaluator.java`
//! Java `DistancePointEvaluator`.
//!
//! ⚠ NOTE(subagent): the Java ctor overload taking a `@Nullable IDoubleRange`
//! plus the static `getDistanceModifier(...)` factory become
//! [`DistancePointEvaluator::with_double_range`] /
//! [`DistancePointEvaluator::get_distance_modifier`]; `null` maps to
//! `Option<...>::None`, `ISeedDoubleRange.DIRECT` maps to
//! `support::DirectSeedDoubleRange`.

use std::rc::Rc;

use hytale_math::hash_util;

use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::point_distance_function::PointDistanceFunction;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
use crate::noise::support::{DirectSeedDoubleRange, IDoubleRange, ISeedDoubleRange};

/// Java `DistancePointEvaluator` — `protected final PointDistanceFunction distanceFunction`.
#[derive(Clone)]
pub struct DistancePointEvaluator {
    /// Java `distanceFunction`.
    pub distance_function: Rc<dyn PointDistanceFunction>,
    /// Java `protected final ISeedDoubleRange distanceMod`.
    pub distance_mod: Rc<dyn ISeedDoubleRange>,
}

impl DistancePointEvaluator {
    /// Java `DistancePointEvaluator(PointDistanceFunction, ISeedDoubleRange)`.
    pub fn new(
        distance_function: Rc<dyn PointDistanceFunction>,
        distance_mod: Rc<dyn ISeedDoubleRange>,
    ) -> Self {
        Self {
            distance_function,
            distance_mod,
        }
    }

    /// Java `DistancePointEvaluator(PointDistanceFunction, @Nullable IDoubleRange)` —
    /// wraps the range with `getDistanceModifier` first.
    pub fn with_double_range(
        distance_function: Rc<dyn PointDistanceFunction>,
        range: Option<Rc<dyn IDoubleRange>>,
    ) -> Self {
        Self::new(distance_function, Self::get_distance_modifier(range))
    }

    /// Java static `getDistanceModifier(@Nullable IDoubleRange)`:
    /// `null` → `ISeedDoubleRange.DIRECT`; otherwise a seed-aware range that
    /// scales the value by `range.getValue(randomDistanceModification(seed))`.
    pub fn get_distance_modifier(range: Option<Rc<dyn IDoubleRange>>) -> Rc<dyn ISeedDoubleRange> {
        match range {
            None => Rc::new(DirectSeedDoubleRange),
            Some(range) => Rc::new(DistanceModifier { range }),
        }
    }
}

/// The anonymous `(seed, value) -> value * range.getValue(randomDistanceModification(seed))`
/// lambda from `getDistanceModifier`.
#[derive(Clone)]
struct DistanceModifier {
    range: Rc<dyn IDoubleRange>,
}

impl ISeedDoubleRange for DistanceModifier {
    fn get_value(&self, seed: i32, value: f64) -> f64 {
        value
            * self
                .range
                .get_value(Self::random_distance_modification(seed))
    }
}

impl DistanceModifier {
    /// Java static `randomDistanceModification(int seed)`.
    #[inline]
    fn random_distance_modification(seed: i32) -> f64 {
        hash_util::random2(seed as i64, 0x5925_fad1)
    }
}

/// Java static `randomDistanceModification(int seed)` (public form).
#[inline]
pub fn random_distance_modification(seed: i32) -> f64 {
    hash_util::random2(seed as i64, 0x5925_fad1)
}

#[allow(clippy::too_many_arguments)]
impl PointEvaluator for DistancePointEvaluator {
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
        let distance = self.distance_mod.get_value(cell_hash, distance);
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
        let distance = self.distance_mod.get_value(cell_hash, distance);
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
        let distance = self.distance_mod.get_value(cell_hash, distance);
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
        let distance = self.distance_mod.get_value(cell_hash, distance);
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
