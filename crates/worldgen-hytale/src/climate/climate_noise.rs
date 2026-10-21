// Ported from: `.hytale-source/.../worldgen/climate/ClimateNoise.java`
//! Java `climate.ClimateNoise` — jittered-grid quantized climate sampling.

use procedural::noise::cell::cell_distance_function::CellDistanceFunction;
use procedural::noise::cell::evaluator::point_evaluator::PointEvaluator;
use procedural::noise::property::NoiseProperty;
use procedural::noise::result_buffer::ResultBuffer2d;
use std::rc::Rc;

use hytale_math::math_util;

use crate::climate::climate_graph::ClimateGraph;
use crate::climate::climate_type::{IS_ISLAND, IS_OCEAN, IS_SHORE};

/// Java `climate.ClimateNoise`.
pub struct ClimateNoise {
    /// Java `public final Grid grid`.
    pub grid: Grid,
    /// Java `public final NoiseProperty continent`.
    pub continent: Rc<dyn NoiseProperty>,
    /// Java `public final NoiseProperty temperature`.
    pub temperature: Rc<dyn NoiseProperty>,
    /// Java `public final NoiseProperty intensity`.
    pub intensity: Rc<dyn NoiseProperty>,
    /// Java `public final Thresholds thresholds`.
    pub thresholds: Thresholds,
}

/// Java `ClimateNoise.Buffer`.
#[derive(Debug, Clone)]
pub struct Buffer {
    /// Java `public double continent`.
    pub continent: f64,
    /// Java `public double temperature`.
    pub temperature: f64,
    /// Java `public double intensity`.
    pub intensity: f64,
    /// Java `public double fade`.
    pub fade: f64,
    /// Java `public final ResultBuffer.ResultBuffer2d pos`.
    pub pos: ResultBuffer2d,
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            continent: 0.0,
            temperature: 0.0,
            intensity: 0.0,
            fade: 0.0,
            pos: ResultBuffer2d::new(),
        }
    }
}

/// Java `ClimateNoise.Grid`.
pub struct Grid {
    /// Java `public final int seedOffset`.
    pub seed_offset: i32,
    /// Java `public final double scale`.
    pub scale: f64,
    /// Java `public final PointEvaluator evaluator`.
    pub evaluator: Rc<dyn PointEvaluator>,
    /// Java `public final CellDistanceFunction grid`.
    pub grid: Rc<dyn CellDistanceFunction>,
    /// Java `public transient final double invScale`.
    pub inv_scale: f64,
}

impl Grid {
    /// Java `Grid(int seedOffset, double scale, CellDistanceFunction, PointEvaluator)`.
    pub fn new(
        seed_offset: i32,
        scale: f64,
        grid: Rc<dyn CellDistanceFunction>,
        evaluator: Rc<dyn PointEvaluator>,
    ) -> Self {
        Self {
            seed_offset,
            scale,
            evaluator,
            grid,
            inv_scale: 1.0 / scale,
        }
    }

    /// Java `void eval(int seed, double x, double y, ResultBuffer.ResultBuffer2d buffer)`.
    pub fn eval(&self, seed: i32, x: f64, y: f64, buffer: &mut ResultBuffer2d) {
        let x = x * self.scale;
        let y = y * self.scale;
        buffer.distance = f64::MAX;
        buffer.distance2 = f64::MAX;
        self.grid.nearest_2d(
            seed.wrapping_add(self.seed_offset),
            x,
            y,
            math_util::floor(x),
            math_util::floor(y),
            buffer,
            self.evaluator.as_ref(),
        );
        buffer.x *= self.inv_scale;
        buffer.y *= self.inv_scale;
    }
}

/// Java `ClimateNoise.Thresholds`.
pub struct Thresholds {
    /// Java `public final double land`.
    pub land: f64,
    /// Java `public final double island`.
    pub island: f64,
    /// Java `public final double beachSize`.
    pub beach_size: f64,
    /// Java `public final double shallowOceanSize`.
    pub shallow_ocean_size: f64,
    /// Java `public transient final double landShoreInner`.
    pub land_shore_inner: f64,
    /// Java `public transient final double islandShoreInner`.
    pub island_shore_inner: f64,
    /// Java `public transient final double landShallowOceanOuter`.
    pub land_shallow_ocean_outer: f64,
    /// Java `public transient final double islandShallowOceanOuter`.
    pub island_shallow_ocean_outer: f64,
}

impl Thresholds {
    /// Java `Thresholds.LAND_DEFAULT`.
    pub const LAND_DEFAULT: f64 = 0.5;
    /// Java `Thresholds.ISLAND_DEFAULT`.
    pub const ISLAND_DEFAULT: f64 = 0.8;
    /// Java `Thresholds.BEACH_SIZE_DEFAULT`.
    pub const BEACH_SIZE_DEFAULT: f64 = 0.05;
    /// Java `Thresholds.SHALLOW_OCEAN_SIZE_DEFAULT`.
    pub const SHALLOW_OCEAN_SIZE_DEFAULT: f64 = 0.15;

    /// Java `Thresholds(double land, double island, double beach, double shore)`.
    pub fn new(land: f64, island: f64, beach: f64, shore: f64) -> Self {
        Self {
            land,
            island,
            beach_size: beach,
            shallow_ocean_size: shore,
            land_shore_inner: land - beach,
            island_shore_inner: island + beach,
            land_shallow_ocean_outer: land + shore,
            island_shallow_ocean_outer: island - shore * 0.5,
        }
    }
}

impl ClimateNoise {
    /// Java `ClimateNoise(...)`.
    pub fn new(
        grid: Grid,
        continent: Rc<dyn NoiseProperty>,
        temperature: Rc<dyn NoiseProperty>,
        intensity: Rc<dyn NoiseProperty>,
        thresholds: Thresholds,
    ) -> Self {
        Self {
            grid,
            temperature,
            intensity,
            continent,
            thresholds,
        }
    }

    /// Java `int generate(int seed, double x, double y, Buffer buffer, ClimateGraph climate)`.
    pub fn generate(
        &self,
        seed: i32,
        x: f64,
        y: f64,
        buffer: &mut Buffer,
        climate: &ClimateGraph,
    ) -> i32 {
        self.grid.eval(seed, x, y, &mut buffer.pos);

        let c = self.continent.get(seed, x, y);
        let t = self.temperature.get(seed, buffer.pos.x, buffer.pos.y);
        let i = self.intensity.get(seed, buffer.pos.x, buffer.pos.y);

        let index = climate.index_of(t, i);

        buffer.continent = c;
        buffer.temperature = t;
        buffer.intensity = i;
        buffer.fade = climate.get_fade_at(index);

        let id = climate.get_id_at(index);
        let flags = Self::get_continent_flags(c, &self.thresholds);

        id | flags
    }

    /// Java `private static int getContinentFlags(double value, Thresholds)`.
    fn get_continent_flags(value: f64, thresholds: &Thresholds) -> i32 {
        let is_land = value <= thresholds.land_shallow_ocean_outer;
        let is_island = value >= thresholds.island_shallow_ocean_outer;
        let is_ocean = value > thresholds.land && value < thresholds.island;
        let is_shore = (is_land && value > thresholds.land_shore_inner)
            || (is_island && value < thresholds.island_shore_inner);

        let mut flags = 0;
        if is_ocean {
            flags |= IS_OCEAN;
        }
        if is_shore {
            flags |= IS_SHORE;
        }
        if is_island {
            flags |= IS_ISLAND;
        }

        flags
    }
}
