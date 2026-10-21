// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/logic/CellNoise.java`
//! Java `CellNoise`, its `CellFunction` interface and the nested `CellMode`
//! enum with the four built-in cell functions.
//!
//! ⚠ NOTE(subagent): Java's `@Nullable NoiseProperty` becomes
//! `Option<Rc<dyn NoiseProperty>>`; the FunctionInterface + enum pair
//! (`CellFunction` / `CellMode`) becomes a trait + implementing enum. The
//! shared scratch buffer becomes an explicit per-call
//! [`ResultBuffer2d`]/[`ResultBuffer3d`].

use std::rc::Rc;

use hytale_math::hash_util;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::function::{NoiseFunction2d, NoiseFunction3d};
use crate::noise::general_noise::limit;
use crate::noise::math_util;
use crate::noise::property::NoiseProperty;
use crate::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
use crate::noise::trig_math_util;

/// Java `CellNoise.CellFunction` interface.
pub trait CellFunction {
    /// Java `double eval(int seed, int offsetSeed, double x, double y,
    /// ResultBuffer2d buffer, CellDistanceFunction cellFunction, NoiseProperty noiseLookup)`.
    fn eval_2d(
        &self,
        seed: i32,
        offset_seed: i32,
        x: f64,
        y: f64,
        buffer: &ResultBuffer2d,
        cell_function: &dyn CellDistanceFunction,
        noise_lookup: Option<&dyn NoiseProperty>,
    ) -> f64;

    /// Java `double eval(int seed, int offsetSeed, double x, double y, double z,
    /// ResultBuffer3d buffer, CellDistanceFunction cellFunction, NoiseProperty noiseLookup)`.
    fn eval_3d(
        &self,
        seed: i32,
        offset_seed: i32,
        x: f64,
        y: f64,
        z: f64,
        buffer: &ResultBuffer3d,
        cell_function: &dyn CellDistanceFunction,
        noise_lookup: Option<&dyn NoiseProperty>,
    ) -> f64;
}

/// Java `CellNoise implements NoiseFunction`.
#[derive(Clone)]
pub struct CellNoise {
    /// Java `protected final CellDistanceFunction distanceFunction`.
    pub distance_function: Rc<dyn CellDistanceFunction>,
    /// Java `protected final PointEvaluator pointEvaluator`.
    pub point_evaluator: Rc<dyn PointEvaluator>,
    /// Java `protected final CellFunction cellFunction`.
    pub cell_function: Rc<dyn CellFunction>,
    /// Java `@Nullable protected final NoiseProperty noiseLookup`.
    pub noise_lookup: Option<Rc<dyn NoiseProperty>>,
}

impl CellNoise {
    /// Java `CellNoise(CellDistanceFunction, PointEvaluator, CellFunction,
    /// NoiseProperty)` — `noiseLookup` may be `null` in Java (→ `None`).
    pub fn new(
        distance_function: Rc<dyn CellDistanceFunction>,
        point_evaluator: Rc<dyn PointEvaluator>,
        cell_function: Rc<dyn CellFunction>,
        noise_lookup: Option<Rc<dyn NoiseProperty>>,
    ) -> Self {
        Self {
            distance_function,
            point_evaluator,
            cell_function,
            noise_lookup,
        }
    }

    /// Java `CellDistanceFunction getDistanceFunction()`.
    pub fn get_distance_function(&self) -> &dyn CellDistanceFunction {
        self.distance_function.as_ref()
    }

    /// Java `CellFunction getCellFunction()`.
    pub fn get_cell_function(&self) -> &dyn CellFunction {
        self.cell_function.as_ref()
    }

    /// Java `@Nullable NoiseProperty getNoiseLookup()`.
    pub fn get_noise_lookup(&self) -> Option<&dyn NoiseProperty> {
        self.noise_lookup.as_deref()
    }
}

impl NoiseFunction2d for CellNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        let x = self.distance_function.scale(x);
        let y = self.distance_function.scale(y);

        let xr = self.distance_function.get_cell_x2(x, y);
        let yr = self.distance_function.get_cell_y2(x, y);

        let mut buffer = ResultBuffer2d::new();
        buffer.distance = f64::INFINITY;

        self.distance_function.nearest_2d(
            offset_seed,
            x,
            y,
            xr,
            yr,
            &mut buffer,
            self.point_evaluator.as_ref(),
        );

        limit(self.cell_function.eval_2d(
            seed,
            offset_seed,
            x,
            y,
            &buffer,
            self.distance_function.as_ref(),
            self.noise_lookup.as_deref(),
        )) * 2.0
            - 1.0
    }
}

impl NoiseFunction3d for CellNoise {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        let x = self.distance_function.scale(x);
        let y = self.distance_function.scale(y);
        let z = self.distance_function.scale(z);

        let xr = self.distance_function.get_cell_x3(x, y, z);
        let yr = self.distance_function.get_cell_y3(x, y, z);
        let zr = self.distance_function.get_cell_z3(x, y, z);

        let mut buffer = ResultBuffer3d::new();
        buffer.distance = f64::INFINITY;

        self.distance_function.nearest_3d(
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

        limit(self.cell_function.eval_3d(
            seed,
            offset_seed,
            x,
            y,
            z,
            &buffer,
            self.distance_function.as_ref(),
            self.noise_lookup.as_deref(),
        )) * 2.0
            - 1.0
    }
}

/// Java `CellNoise.CellMode` — the four built-in `CellFunction`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellMode {
    /// Java `CELL_VALUE`.
    CellValue,
    /// Java `NOISE_LOOKUP`.
    NoiseLookup,
    /// Java `DISTANCE`.
    Distance,
    /// Java `DIRECTION`.
    Direction,
}

impl CellFunction for CellMode {
    fn eval_2d(
        &self,
        seed: i32,
        offset_seed: i32,
        x: f64,
        y: f64,
        buffer: &ResultBuffer2d,
        cell_function: &dyn CellDistanceFunction,
        noise_lookup: Option<&dyn NoiseProperty>,
    ) -> f64 {
        match self {
            CellMode::CellValue => {
                // Java: HashUtil.random(offsetSeed, buffer.ix, buffer.iy)
                hash_util::random3(offset_seed as i64, buffer.ix as i64, buffer.iy as i64)
            }
            CellMode::NoiseLookup => {
                let noise_lookup = noise_lookup
                    .expect("NoiseLookup cell function requires a noise lookup (Java @Nonnull)");
                let px = cell_function.inv_scale(buffer.x);
                let py = cell_function.inv_scale(buffer.y);
                noise_lookup.get(seed, px, py)
            }
            CellMode::Distance => f64::sqrt(buffer.distance),
            CellMode::Direction => {
                Self::direction_eval_2d(seed, offset_seed, x, y, buffer, noise_lookup)
            }
        }
    }

    fn eval_3d(
        &self,
        seed: i32,
        offset_seed: i32,
        _x: f64,
        _y: f64,
        _z: f64,
        buffer: &ResultBuffer3d,
        cell_function: &dyn CellDistanceFunction,
        noise_lookup: Option<&dyn NoiseProperty>,
    ) -> f64 {
        match self {
            CellMode::CellValue => {
                // Java: HashUtil.random(offsetSeed, buffer.ix, buffer.iy, buffer.iz)
                hash_util::random4(
                    offset_seed as i64,
                    buffer.ix as i64,
                    buffer.iy as i64,
                    buffer.iz as i64,
                )
            }
            CellMode::NoiseLookup => {
                let noise_lookup = noise_lookup
                    .expect("NoiseLookup cell function requires a noise lookup (Java @Nonnull)");
                let px = cell_function.inv_scale(buffer.x);
                let py = cell_function.inv_scale(buffer.y);
                let pz = cell_function.inv_scale(buffer.z);
                noise_lookup.get_3d(seed, px, py, pz)
            }
            CellMode::Distance => f64::sqrt(buffer.distance),
            // Java: throw new UnsupportedOperationException();
            CellMode::Direction => panic!(
                "Direction cell function does not support 3d (Java UnsupportedOperationException)"
            ),
        }
    }
}

impl CellMode {
    /// The `DIRECTION` 2d body. Hidden Java helper `getAngleNoise` is inlined
    /// below; `x`/`y` are the query point.
    fn direction_eval_2d(
        seed: i32,
        offset_seed: i32,
        x: f64,
        y: f64,
        buffer: &ResultBuffer2d,
        noise_property: Option<&dyn NoiseProperty>,
    ) -> f64 {
        // Choose a random angle for the cell based on its 'unique' hash or
        // noiseLookup if present.
        // Java: float angle = (float) getAngleNoise(seed, offsetSeed, buffer, noiseLookup) * TrigMathUtil.PI2;
        let angle = (Self::get_angle_noise(seed, offset_seed, buffer, noise_property) as f32)
            * trig_math_util::PI2;

        let dx = trig_math_util::sin(angle);
        let dy = trig_math_util::cos(angle);

        // The line AB passing through the center of the cell with the given angle.
        let ax = buffer.x;
        let ay = buffer.y;
        let bx = ax + dx as f64;
        let by = ay + dy as f64;

        let distance2 = math_util::distance_to_inf_line_sq(x, y, ax, ay, bx, by);
        let distance = math_util::clamp(f64::sqrt(distance2), 0.0, 1.0);

        // Value of -1 or +1 representing the side of the line AB that x,y lies on.
        let side = math_util::side_of_line(x, y, ax, ay, bx, by);

        // Modifies the output to produce a continuously increasing gradient
        // across the cell rather than two slopes converging at zero.
        0.5 + side as f64 * distance * 0.5
    }

    /// Private Java `CellMode.DIRECTION.getAngleNoise(...)` (Java quirk: uses
    /// `offsetSeed`, not `seed`, for the hash fallback).
    fn get_angle_noise(
        seed: i32,
        offset_seed: i32,
        buffer: &ResultBuffer2d,
        noise_property: Option<&dyn NoiseProperty>,
    ) -> f64 {
        if let Some(noise_property) = noise_property {
            return noise_property.get(seed, buffer.x, buffer.y);
        }

        // Java: HashUtil.random(offsetSeed, buffer.ix, buffer.iy)
        hash_util::random3(offset_seed as i64, buffer.ix as i64, buffer.iy as i64)
    }
}
