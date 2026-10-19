// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/BranchNoise.java`
//! Java `BranchNoise`.
//!
//! ⚠ NOTE(subagent): `supplier.IDoubleRange` / `condition.IIntCondition` live
//! in [`crate::noise::support`] until those packages are ported;
//! `property.NoiseFormulaProperty.NoiseFormula.Formula` is
//! `support::NoiseFormula`; `DistanceNoise.Distance2Function` is the trait
//! re-exported from [`crate::noise::distance_noise`]. The Java static scratch
//! buffer (`ResultBuffer.buffer2d`) is shared between the parent and line
//! phases *within one `get` call* — the Rust port models this with a single
//! per-call buffer passed through both phases (call order matters: the parent
//! values are read from the buffer *before* `get_line_value` overwrites the
//! second-order fields).

use std::rc::Rc;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::distance_noise::Distance2Function;
use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::math_util;
use crate::noise::result_buffer::ResultBuffer2d;
use crate::noise::support::{IIntCondition, IDoubleRange, NoiseFormula};

/// Java `BranchNoise implements NoiseFunction`.
pub struct BranchNoise {
    /// Java `protected final CellDistanceFunction parentFunction`.
    pub parent_function: Rc<dyn CellDistanceFunction>,
    /// Java `protected final PointEvaluator parentEvaluator`.
    pub parent_evaluator: Rc<dyn PointEvaluator>,
    /// Java `protected final double parentValue`.
    pub parent_value: f64,
    /// Java `protected final double emptyValue` (precomputed in the ctor).
    pub empty_value: f64,
    /// Java `protected final IDoubleRange parentFade`.
    pub parent_fade: Rc<dyn IDoubleRange>,
    /// Java `protected final IIntCondition parentDensity`.
    pub parent_density: Rc<dyn IIntCondition>,
    /// Java `protected final DistanceNoise.Distance2Function distance2Function`.
    pub distance2_function: Rc<dyn Distance2Function>,
    /// Java `protected final NoiseFormulaProperty.NoiseFormula.Formula noiseFormula`.
    pub noise_formula: Rc<dyn NoiseFormula>,

    /// Java `protected final CellDistanceFunction lineFunction`.
    pub line_function: Rc<dyn CellDistanceFunction>,
    /// Java `protected final PointEvaluator lineEvaluator`.
    pub line_evaluator: Rc<dyn PointEvaluator>,
    /// Java `protected final double lineScale`.
    pub line_scale: f64,
    /// Java `protected final IDoubleRange lineThickness`.
    pub line_thickness: Rc<dyn IDoubleRange>,
}

impl std::fmt::Debug for BranchNoise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BranchNoise")
            .field("parent_value", &self.parent_value)
            .field("empty_value", &self.empty_value)
            .field("line_scale", &self.line_scale)
            .finish_non_exhaustive()
    }
}

impl BranchNoise {
    /// Java `BranchNoise(...)` constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent_function: Rc<dyn CellDistanceFunction>,
        parent_evaluator: Rc<dyn PointEvaluator>,
        parent_value: f64,
        parent_fade: Rc<dyn IDoubleRange>,
        parent_density: Rc<dyn IIntCondition>,
        distance2_function: Rc<dyn Distance2Function>,
        noise_formula: Rc<dyn NoiseFormula>,
        line_function: Rc<dyn CellDistanceFunction>,
        line_evaluator: Rc<dyn PointEvaluator>,
        line_scale: f64,
        line_thickness: Rc<dyn IDoubleRange>,
    ) -> Self {
        Self {
            parent_function,
            parent_evaluator,
            parent_value,
            // Java: this.emptyValue = toOutputRange(parentValue);
            empty_value: Self::to_output_range(parent_value),
            parent_fade,
            parent_density,
            distance2_function,
            noise_formula,
            line_function,
            line_evaluator,
            line_scale,
            line_thickness,
        }
    }

    /// Java `getParentNoise(int seed, double x, double y)`.
    fn get_parent_noise(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        buffer: &mut ResultBuffer2d,
    ) {
        buffer.distance = f64::INFINITY;
        buffer.distance2 = f64::INFINITY;

        let x = self.parent_function.scale(x);
        let y = self.parent_function.scale(y);

        let cell_x = self.parent_function.get_cell_x2(x, y);
        let cell_y = self.parent_function.get_cell_y2(x, y);
        self.parent_function.transition_2d(
            seed,
            x,
            y,
            cell_x,
            cell_y,
            buffer,
            self.parent_evaluator.as_ref(),
        );
    }

    /// Java `getLineValue(int seed, double x, double y, int parentHash,
    /// double parentX, double parentY, double parentDistance, ResultBuffer2d buffer)`.
    #[allow(clippy::too_many_arguments)]
    fn get_line_value(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        parent_hash: i32,
        parent_x: f64,
        parent_y: f64,
        parent_distance: f64,
        buffer: &mut ResultBuffer2d,
    ) -> f64 {
        // Select the max line thickness value based on the distance from the
        // parent cell's centre
        let thickness = self.line_thickness.get_value(parent_distance);
        if thickness == 0.0 {
            return 1.0;
        }

        buffer.distance = f64::INFINITY;

        // Store info about the parent region on the second order buffer fields
        // (these are only used on eval2() functions which we aren't calling)
        buffer.x2 = parent_x;
        buffer.y2 = parent_y;
        buffer.ix2 = parent_hash;
        buffer.distance2 = thickness;

        let mut x = x * self.line_scale;
        let mut y = y * self.line_scale;

        x = self.line_function.scale(x);
        y = self.line_function.scale(y);

        let cell_x = self.line_function.get_cell_x2(x, y);
        let cell_y = self.line_function.get_cell_y2(x, y);

        self.line_function.nearest_2d(
            seed,
            x,
            y,
            cell_x,
            cell_y,
            buffer,
            self.line_evaluator.as_ref(),
        );

        let distance = buffer.distance;
        if distance >= thickness * thickness {
            return 1.0;
        }

        f64::sqrt(distance) / thickness
    }

    /// Java `protected static double toOutputRange(double value)` — converts a
    /// `[0.0, 1.0]` value to `[-1.0, 1.0]`.
    #[inline]
    fn to_output_range(value: f64) -> f64 {
        2.0 * value - 1.0
    }
}

impl NoiseFunction2d for BranchNoise {
    fn get(&self, _seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        // A ResultBuffer2d containing the data of the parent cell region.
        // Java shares the static buffer between get_parent_noise/get_line_value;
        // the Rust port passes one buffer through both phases.
        let mut buffer = ResultBuffer2d::new();
        self.get_parent_noise(offset_seed, x, y, &mut buffer);
        if !self.parent_density.eval(buffer.hash) {
            return self.empty_value;
        }

        let parent_distance = self
            .noise_formula
            .eval(self.distance2_function.eval(buffer.distance, buffer.distance2));

        // The distance to the nearest line segment
        let line_value = self.get_line_value(
            offset_seed,
            x,
            y,
            buffer.hash,
            buffer.x,
            buffer.y,
            parent_distance,
            &mut buffer,
        );

        // The fade alpha value derived from x,y's distance to center of the
        // parent cell
        let parent_fade = self.parent_fade.get_value(parent_distance);

        // The noise result derived from fading between the parent's and
        // line's noise values
        let noise_value = math_util::lerp(self.parent_value, line_value, parent_fade);

        Self::to_output_range(noise_value)
    }
}

impl NoiseFunction3d for BranchNoise {
    fn get(&self, _seed: i32, _offset_seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
        // Java: throw new UnsupportedOperationException();
        panic!("BranchNoise.get 3d is not supported (Java UnsupportedOperationException)");
    }
}
