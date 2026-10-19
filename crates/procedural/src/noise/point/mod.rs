// Ported from: `.hytale-source/.../procedurallib/logic/point/*.java`
//! Java `logic.point` package — jittered-grid point generators.
//!
//! - [`PointGenerator`]: the core 3x3(/3x3x3) cell-search generator.
//! - [`OffsetPointGenerator`], [`ScaledPointGenerator`],
//!   [`DistortedPointGenerator`]: decorators.
//!
//! ⚠ NOTE: Java reuses shared static `ResultBuffer` instances (thread-local
//! pools). The Rust port allocates a fresh buffer per call — identical
//! results, thread-safe. The generic
//! `collect0(..., PointConsumer<PointConsumer2d>, PointConsumer2d)` adapter
//! collapses into a single `&mut dyn FnMut(f64, f64)` consumer (the scale
//! transform composes into the closure).

use std::rc::Rc;

use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::random::ICoordinateRandomizer;
use crate::noise::result_buffer::{Bounds2d, ResultBuffer2d, ResultBuffer3d};

/// Java `IPointGenerator` — `PointConsumer2d` becomes `&mut dyn FnMut(f64, f64)`.
pub trait IPointGenerator {
    /// Java `nearest2D(int seed, double x, double y)`.
    fn nearest_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d;
    /// Java `nearest3D(int seed, double x, double y, double z)`.
    fn nearest_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d;
    /// Java `transition2D(int seed, double x, double y)`.
    fn transition_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d;
    /// Java `transition3D(int seed, double x, double y, double z)`.
    fn transition_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d;
    /// Java `collect(int seed, double minX, double minY, double maxX, double maxY, PointConsumer2d)`.
    fn collect(
        &self,
        seed: i32,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    );
    /// Java `getInterval()`.
    fn get_interval(&self) -> f64;
}

/// Java `PointGenerator`.
pub struct PointGenerator {
    /// Java `protected final int seedOffset`.
    pub seed_offset: i32,
    /// Java `protected final CellDistanceFunction cellDistanceFunction`.
    pub cell_distance_function: Rc<dyn CellDistanceFunction>,
    /// Java `protected final PointEvaluator pointEvaluator`.
    pub point_evaluator: Rc<dyn PointEvaluator>,
}

impl PointGenerator {
    /// Java `PointGenerator(int, CellDistanceFunction, PointEvaluator)`.
    pub fn new(
        seed_offset: i32,
        cell_distance_function: Rc<dyn CellDistanceFunction>,
        point_evaluator: Rc<dyn PointEvaluator>,
    ) -> Self {
        Self {
            seed_offset,
            cell_distance_function,
            point_evaluator,
        }
    }

    /// Java `collect0(int seed, double minX, double minY, double maxX, double maxY,
    /// PointConsumer<PointConsumer2d> pointConsumer, PointConsumer2d consumer)`.
    ///
    /// The Java `(PointConsumer<PointConsumer2d>, PointConsumer2d)` pair is a
    /// coordinate-transforming adapter; the Rust port takes the final consumer
    /// closure directly (callers compose the transform into the closure).
    pub fn collect0(
        &self,
        seed: i32,
        mut min_x: f64,
        mut min_y: f64,
        mut max_x: f64,
        mut max_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        let df = self.cell_distance_function.as_ref();
        min_x = df.scale(min_x);
        min_y = df.scale(min_y);
        max_x = df.scale(max_x);
        max_y = df.scale(max_y);

        let x0 = df.get_cell_x2(min_x, min_y);
        let y0 = df.get_cell_y2(min_x, min_y);
        let x1 = df.get_cell_x2(max_x, max_y);
        let y1 = df.get_cell_y2(max_x, max_y);

        // Java: shared static bounds, assigned here, used by collect.
        let mut bounds = Bounds2d::new();
        bounds.assign(min_x, min_y, max_x, max_y);

        df.collect(
            seed,
            seed.wrapping_add(self.seed_offset), // Java seed + seedOffset
            x0,
            y0,
            x1,
            y1,
            &bounds,
            consumer,
            self.point_evaluator.as_ref(),
        );
    }
}

impl IPointGenerator for PointGenerator {
    fn nearest_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        let df = self.cell_distance_function.as_ref();
        let x = df.scale(x);
        let y = df.scale(y);

        let xr = df.get_cell_x2(x, y);
        let yr = df.get_cell_y2(x, y);

        let mut buffer = ResultBuffer2d::new();
        buffer.distance = f64::INFINITY;

        df.nearest_2d(
            seed.wrapping_add(self.seed_offset),
            x,
            y,
            xr,
            yr,
            &mut buffer,
            self.point_evaluator.as_ref(),
        );
        buffer
    }

    fn nearest_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        let df = self.cell_distance_function.as_ref();
        let x = df.scale(x);
        let y = df.scale(y);
        let z = df.scale(z);

        let xr = df.get_cell_x3(x, y, z);
        let yr = df.get_cell_y3(x, y, z);
        let zr = df.get_cell_z3(x, y, z);

        let mut buffer = ResultBuffer3d::new();
        buffer.distance = f64::INFINITY;

        df.nearest_3d(
            seed.wrapping_add(self.seed_offset),
            x,
            y,
            z,
            xr,
            yr,
            zr,
            &mut buffer,
            self.point_evaluator.as_ref(),
        );
        buffer
    }

    fn transition_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        let df = self.cell_distance_function.as_ref();
        let x = df.scale(x);
        let y = df.scale(y);

        let xr = df.get_cell_x2(x, y);
        let yr = df.get_cell_y2(x, y);

        let mut buffer = ResultBuffer2d::new();
        buffer.distance = f64::INFINITY;
        buffer.distance2 = f64::INFINITY;

        df.transition_2d(
            seed.wrapping_add(self.seed_offset),
            x,
            y,
            xr,
            yr,
            &mut buffer,
            self.point_evaluator.as_ref(),
        );
        buffer
    }

    fn transition_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        let df = self.cell_distance_function.as_ref();
        let x = df.scale(x);
        let y = df.scale(y);
        let z = df.scale(z);

        let xr = df.get_cell_x3(x, y, z);
        let yr = df.get_cell_y3(x, y, z);
        let zr = df.get_cell_z3(x, y, z);

        let mut buffer = ResultBuffer3d::new();
        buffer.distance = f64::INFINITY;
        buffer.distance2 = f64::INFINITY;

        df.transition_3d(
            seed.wrapping_add(self.seed_offset),
            x,
            y,
            z,
            xr,
            yr,
            zr,
            &mut buffer,
            self.point_evaluator.as_ref(),
        );
        buffer
    }

    fn get_interval(&self) -> f64 {
        1.0
    }

    fn collect(
        &self,
        seed: i32,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        self.collect0(seed, min_x, min_y, max_x, max_y, consumer);
    }
}

/// Java `OffsetPointGenerator`.
pub struct OffsetPointGenerator {
    /// Java `private final IPointGenerator generator`.
    pub generator: Rc<dyn IPointGenerator>,
    /// Java `private final double offsetX, offsetY, offsetZ`.
    pub offset_x: f64,
    pub offset_y: f64,
    pub offset_z: f64,
}

impl OffsetPointGenerator {
    /// Java `OffsetPointGenerator(IPointGenerator, double, double, double)`.
    pub fn new(
        generator: Rc<dyn IPointGenerator>,
        offset_x: f64,
        offset_y: f64,
        offset_z: f64,
    ) -> Self {
        Self {
            generator,
            offset_x,
            offset_y,
            offset_z,
        }
    }
}

impl IPointGenerator for OffsetPointGenerator {
    fn nearest_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        self.generator
            .nearest_2d(seed, x + self.offset_x, y + self.offset_y)
    }

    fn nearest_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        self.generator.nearest_3d(
            seed,
            x + self.offset_x,
            y + self.offset_y,
            z + self.offset_z,
        )
    }

    fn transition_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        self.generator
            .transition_2d(seed, x + self.offset_x, y + self.offset_y)
    }

    fn transition_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        self.generator.transition_3d(
            seed,
            x + self.offset_x,
            y + self.offset_y,
            z + self.offset_z,
        )
    }

    fn get_interval(&self) -> f64 {
        self.generator.get_interval()
    }

    fn collect(
        &self,
        seed: i32,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        let (ox, oy) = (self.offset_x, self.offset_y);
        self.generator.collect(seed, min_x, min_y, max_x, max_y, &mut |x, y| {
            consumer(x + ox, y + oy);
        });
    }
}

/// Java `ScaledPointGenerator` — wraps a concrete [`PointGenerator`].
pub struct ScaledPointGenerator {
    /// Java `protected final PointGenerator pointGenerator`.
    pub point_generator: PointGenerator,
    /// Java `protected final double scale`.
    pub scale: f64,
}

impl ScaledPointGenerator {
    /// Java `ScaledPointGenerator(PointGenerator, double scale)`.
    pub fn new(point_generator: PointGenerator, scale: f64) -> Self {
        Self {
            point_generator,
            scale,
        }
    }
}

impl IPointGenerator for ScaledPointGenerator {
    fn nearest_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        let mut buf = self
            .point_generator
            .nearest_2d(seed, x * self.scale, y * self.scale);
        buf.x /= self.scale;
        buf.y /= self.scale;
        buf.distance = f64::sqrt(buf.distance) / self.scale;
        buf
    }

    fn nearest_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        let mut buf = self.point_generator.nearest_3d(
            seed,
            x * self.scale,
            y * self.scale,
            z * self.scale,
        );
        buf.x /= self.scale;
        buf.y /= self.scale;
        buf.z /= self.scale;
        buf.distance = f64::sqrt(buf.distance) / self.scale;
        buf
    }

    fn transition_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        let mut buf = self
            .point_generator
            .transition_2d(seed, x * self.scale, y * self.scale);
        buf.x /= self.scale;
        buf.x2 /= self.scale;
        buf.y /= self.scale;
        buf.y2 /= self.scale;
        buf.distance = f64::sqrt(buf.distance) / self.scale;
        buf.distance2 = f64::sqrt(buf.distance2) / self.scale;
        buf
    }

    fn transition_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        let mut buf = self.point_generator.transition_3d(
            seed,
            x * self.scale,
            y * self.scale,
            z * self.scale,
        );
        buf.x /= self.scale;
        buf.x2 /= self.scale;
        buf.y /= self.scale;
        buf.y2 /= self.scale;
        buf.z /= self.scale;
        buf.z2 /= self.scale;
        buf.distance = f64::sqrt(buf.distance) / self.scale;
        buf.distance2 = f64::sqrt(buf.distance2) / self.scale;
        buf
    }

    fn get_interval(&self) -> f64 {
        self.point_generator.get_interval() / self.scale
    }

    fn collect(
        &self,
        seed: i32,
        mut min_x: f64,
        mut min_y: f64,
        mut max_x: f64,
        mut max_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        min_x *= self.scale;
        min_y *= self.scale;
        max_x *= self.scale;
        max_y *= self.scale;
        let scale = self.scale;
        self.point_generator.collect0(seed, min_x, min_y, max_x, max_y, &mut |x, y| {
            consumer(x / scale, y / scale);
        });
    }
}

/// Java `DistortedPointGenerator`.
pub struct DistortedPointGenerator {
    /// Java `protected final IPointGenerator pointGenerator`.
    pub point_generator: Rc<dyn IPointGenerator>,
    /// Java `protected final ICoordinateRandomizer coordinateRandomizer`.
    pub coordinate_randomizer: Rc<dyn ICoordinateRandomizer>,
}

impl DistortedPointGenerator {
    /// Java `DistortedPointGenerator(IPointGenerator, ICoordinateRandomizer)`.
    pub fn new(
        point_generator: Rc<dyn IPointGenerator>,
        coordinate_randomizer: Rc<dyn ICoordinateRandomizer>,
    ) -> Self {
        Self {
            point_generator,
            coordinate_randomizer,
        }
    }
}

impl IPointGenerator for DistortedPointGenerator {
    fn nearest_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        self.point_generator.nearest_2d(
            seed,
            self.coordinate_randomizer.random_double_x2(seed, x, y),
            self.coordinate_randomizer.random_double_y2(seed, x, y),
        )
    }

    fn nearest_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        self.point_generator.nearest_3d(
            seed,
            self.coordinate_randomizer.random_double_x3(seed, x, y, z),
            self.coordinate_randomizer.random_double_y3(seed, x, y, z),
            self.coordinate_randomizer.random_double_z3(seed, x, y, z),
        )
    }

    fn transition_2d(&self, seed: i32, x: f64, y: f64) -> ResultBuffer2d {
        self.point_generator.transition_2d(
            seed,
            self.coordinate_randomizer.random_double_x2(seed, x, y),
            self.coordinate_randomizer.random_double_y2(seed, x, y),
        )
    }

    fn transition_3d(&self, seed: i32, x: f64, y: f64, z: f64) -> ResultBuffer3d {
        self.point_generator.transition_3d(
            seed,
            self.coordinate_randomizer.random_double_x3(seed, x, y, z),
            self.coordinate_randomizer.random_double_y3(seed, x, y, z),
            self.coordinate_randomizer.random_double_z3(seed, x, y, z),
        )
    }

    fn get_interval(&self) -> f64 {
        self.point_generator.get_interval()
    }

    fn collect(
        &self,
        seed: i32,
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        self.point_generator
            .collect(seed, min_x, min_y, max_x, max_y, consumer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::cell::evaluator::normal_point_evaluator::NormalPointEvaluator;
    use crate::noise::cell::grid_cell_distance_function::GridCellDistanceFunction;

    fn generator() -> PointGenerator {
        PointGenerator::new(
            0,
            GridCellDistanceFunction::distance_function(),
            NormalPointEvaluator::euclidean(),
        )
    }

    #[test]
    fn nearest_2d_is_deterministic_and_finds_a_point() {
        let g = generator();
        let a = g.nearest_2d(42, 1.3, -0.7);
        let b = g.nearest_2d(42, 1.3, -0.7);
        assert_eq!(a.distance, b.distance);
        assert_eq!(a.hash, b.hash);
        assert_eq!(a.ix, b.ix);
        assert_eq!(a.iy, b.iy);
        assert!(a.distance.is_finite());
        assert!(a.distance >= 0.0);
    }

    #[test]
    fn nearest_3d_is_deterministic() {
        let g = generator();
        let a = g.nearest_3d(7, 0.5, 0.5, 0.5);
        let b = g.nearest_3d(7, 0.5, 0.5, 0.5);
        assert_eq!(a.distance, b.distance);
        assert!(a.distance.is_finite());
    }

    #[test]
    fn transition_2d_fills_both_distances() {
        let g = generator();
        let buf = g.transition_2d(1, 10.25, 3.75);
        assert!(buf.distance.is_finite());
        assert!(buf.distance2.is_finite());
        assert!(buf.distance <= buf.distance2, "closest <= second closest");
    }

    #[test]
    fn collect_visits_cells_in_range() {
        let g = generator();
        let mut points = Vec::new();
        g.collect(5, 0.0, 0.0, 3.0, 3.0, &mut |x, y| {
            // Cells overlapping the range are visited; jittered points may lie
            // up to one cell outside the requested range (Java passes the
            // bounds but the square-grid collect does not filter by them).
            assert!(
                (-1.0..=4.0).contains(&x) && (-1.0..=4.0).contains(&y),
                "({x},{y}) far out of bounds"
            );
            points.push((x, y));
        });
        assert!(!points.is_empty(), "collect must visit at least one point");
        // Determinism
        let mut points2 = Vec::new();
        g.collect(5, 0.0, 0.0, 3.0, 3.0, &mut |x, y| points2.push((x, y)));
        assert_eq!(points, points2);
    }

    #[test]
    fn interval_matches_configuration() {
        let g = generator();
        assert_eq!(IPointGenerator::get_interval(&g), 1.0);
        let s = ScaledPointGenerator::new(generator(), 2.0);
        assert_eq!(IPointGenerator::get_interval(&s), 0.5);
    }

    #[test]
    fn scaled_generator_scales_distances() {
        let s = ScaledPointGenerator::new(generator(), 2.0);
        let a = s.nearest_2d(42, 1.3, -0.7);
        assert!(a.distance.is_finite() && a.distance >= 0.0);
        // Determinism through the decorator
        let b = s.nearest_2d(42, 1.3, -0.7);
        assert_eq!(a.distance.to_bits(), b.distance.to_bits());
    }

    #[test]
    fn offset_generator_shifts_query() {
        let o = OffsetPointGenerator::new(Rc::new(generator()), 10.0, 0.0, 0.0);
        let shifted = o.nearest_2d(42, 1.3, -0.7);
        let direct = generator().nearest_2d(42, 11.3, -0.7);
        assert_eq!(shifted.distance.to_bits(), direct.distance.to_bits());
    }
}
