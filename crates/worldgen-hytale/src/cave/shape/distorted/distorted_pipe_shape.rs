// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/DistortedPipeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.DistortedPipeShape`
//! — the cylinder with a pitch-compensated *cross-section fade* at the ends.
//!
//! The pipe is the default distorted shape (`DistortedShapes.getDefault()`).
//! Inside the segment (`0 < t < 1`) it interpolates exactly like the cylinder;
//! outside of it (`t <= 0` / `t >= 1`) the Java code squashes the end radius
//! proportionally to the pitch compensation:
//!
//! ```text
//! t <= 0.0:  fade = interpolate(1 - clamp(t, -0.5, 0) * -2)
//!            lerp(startDim, startDim * fade, compensation)
//! t >= 1.0:  fade = interpolate(1 - clamp(t - 1, 0, 0.5) * 2)
//!            lerp(endDim, endDim * fade, compensation)
//! ```
//!
//! and it accepts every projection (the caps are part of the shape).
//!
//! # Java → Rust
//!
//! The Java `extends DistortedCylinderShape` becomes composition: the pipe
//! owns a [`DistortedCylinderShape`] and forwards everything it does not
//! override. `getAnchor` is re-implemented because Java's version dispatches
//! to the *overridden* `getWidthAt` / `getHeightAt`.

use hytale_math::math_util;
use procedural::noise::{InterpolationFunction, InterpolationMode};

use crate::cave::cave_type::CaveType;
use crate::cave::shape::distorted::abstract_distorted_extrusion::{
    get_height_at_projection as extrusion_height_at_projection, AbstractDistortedExtrusion,
    DistortedExtrusion,
};
use crate::cave::shape::distorted::distorted_cylinder_shape::{
    get_compensation_factor, get_height_compensation, DistortedCylinderShape,
};
use crate::cave::shape::distorted::distorted_shape::DistortedShape;
use crate::cave::shape::distorted::distorted_shapes::DistortedShapeFactory;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use crate::loader::util::Vector3d;
use crate::util::bounds::WorldBounds;

/// Java `public class DistortedPipeShape extends DistortedCylinderShape`.
pub struct DistortedPipeShape {
    /// The `DistortedCylinderShape` state (overridden dimensions, extrusion,
    /// bounds).
    cylinder: DistortedCylinderShape,
    /// Java `private final double compensation`.
    compensation: f64,
}

impl DistortedPipeShape {
    /// Java `public DistortedPipeShape(@Nonnull Vector3d o, @Nonnull Vector3d
    /// v, double startWidth, double startHeight, double midWidth, double
    /// midHeight, double endWidth, double endHeight, double maxWidth, double
    /// maxHeight, double compensation, GeneralNoise.InterpolationFunction
    /// interpolation)`.
    #[allow(clippy::too_many_arguments)] // The Java constructor has twelve parameters.
    pub fn new(
        o: Vector3d,
        v: Vector3d,
        start_width: f64,
        start_height: f64,
        mid_width: f64,
        mid_height: f64,
        end_width: f64,
        end_height: f64,
        max_width: f64,
        max_height: f64,
        compensation: f64,
        interpolation: InterpolationMode,
    ) -> Self {
        Self {
            cylinder: DistortedCylinderShape::with_max(
                o,
                v,
                start_width,
                start_height,
                mid_width,
                mid_height,
                end_width,
                end_height,
                max_width,
                max_height,
                interpolation,
            ),
            compensation,
        }
    }
}

/// Java `protected static double getCompensatedDim(double t, double startDim,
/// double midDim, double endDim, double compensation, @Nonnull
/// GeneralNoise.InterpolationFunction interpolation)`.
pub(crate) fn get_compensated_dim(
    t: f64,
    start_dim: f64,
    mid_dim: f64,
    end_dim: f64,
    compensation: f64,
    interpolation: InterpolationMode,
) -> f64 {
    if t <= 0.0 {
        // Java: "squash start radius proportionally with the amount of pitch
        // compensation"
        let fade = 1.0 - math_util::clamp_f64(t, -0.5, 0.0) * -2.0;
        let fade = interpolation.interpolate(fade);
        math_util::lerp_f64(start_dim, start_dim * fade, compensation)
    } else if t >= 1.0 {
        // Java: "squash end radius proportionally with the amount of pitch
        // compensation"
        let fade = 1.0 - math_util::clamp_f64(t - 1.0, 0.0, 0.5) * 2.0;
        let fade = interpolation.interpolate(fade);
        math_util::lerp_f64(end_dim, end_dim * fade, compensation)
    } else if t <= 0.5 {
        // Java: "map t to the range 0.0-1.0"
        let t = interpolation.interpolate(t * 2.0);
        // Java: "as t increases midDim becomes more dominant"
        math_util::lerp_unclamped_f64(start_dim, mid_dim, t)
    } else {
        // Java: "map t to the range 0.0-1.0"
        let t = interpolation.interpolate((t - 0.5) * 2.0);
        // Java: "as t increases endDim becomes more dominant"
        math_util::lerp_unclamped_f64(mid_dim, end_dim, t)
    }
}

impl DistortedShape for DistortedPipeShape {
    /// Java `@Nonnull @Override public Vector3d getStart()` (inherited).
    fn get_start(&self) -> Vector3d {
        self.cylinder.get_start()
    }

    /// Java `@Nonnull @Override public Vector3d getEnd()` (inherited).
    fn get_end(&self) -> Vector3d {
        self.cylinder.get_end()
    }

    /// Java `@Nonnull @Override public Vector3d getAnchor(@Nonnull Vector3d
    /// vector, double t, double tv, double th)` (inherited, but dispatching to
    /// the pipe's compensated dimensions).
    fn get_anchor(&self, vector: Vector3d, t: f64, tv: f64, th: f64) -> Vector3d {
        let radius_y = self.get_height_at(t);
        let radius_xz = self.get_width_at(t);
        crate::cave::shape::cave_node_shape_utils::get_pipe_anchor(
            vector,
            self.cylinder.extrusion_state().o(),
            self.cylinder.extrusion_state().v(),
            radius_xz,
            radius_y,
            radius_xz,
            t,
            tv,
            th,
        )
    }

    /// Java `@Override public double getProjection(double x, double z)`
    /// (inherited).
    fn get_projection(&self, x: f64, z: f64) -> f64 {
        self.cylinder.get_projection(x, z)
    }

    /// Java `@Override public boolean isValidProjection(double t)` — a pipe
    /// accepts every projection (its caps are part of the shape).
    fn is_valid_projection(&self, _t: f64) -> bool {
        true
    }

    /// Java `@Override public double getYAt(double t)` (inherited).
    fn get_y_at(&self, t: f64) -> f64 {
        self.cylinder.get_y_at(t)
    }

    /// Java `@Override public double getWidthAt(double t)`.
    fn get_width_at(&self, t: f64) -> f64 {
        get_compensated_dim(
            t,
            self.cylinder.start_width(),
            self.cylinder.mid_width(),
            self.cylinder.end_width(),
            self.compensation,
            self.cylinder.extrusion_state().interpolation(),
        )
    }

    /// Java `@Override public double getHeightAt(double t)`.
    fn get_height_at(&self, t: f64) -> f64 {
        get_compensated_dim(
            t,
            self.cylinder.start_height(),
            self.cylinder.mid_height(),
            self.cylinder.end_height(),
            self.compensation,
            self.cylinder.extrusion_state().interpolation(),
        )
    }

    /// Java `@Override public double getHeightAtProjection(int seed, double x,
    /// double z, double t, double centerY, CaveType caveType, ShapeDistortion
    /// distortion)` (inherited).
    fn get_height_at_projection(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        t: f64,
        center_y: f64,
        cave_type: &CaveType,
        distortion: &ShapeDistortion,
    ) -> f64 {
        extrusion_height_at_projection(self, seed, x, z, t, center_y, cave_type, distortion)
    }

    /// Java `@Nonnull @Override public IWorldBounds getBounds()` (inherited).
    fn get_bounds(&self) -> &WorldBounds {
        self.cylinder.get_bounds()
    }
}

impl DistortedExtrusion for DistortedPipeShape {
    fn get_distance_sq(&self, x: f64, z: f64, t: f64) -> f64 {
        self.cylinder.get_distance_sq(x, z, t)
    }

    fn get_height_component(&self, width: f64, width_squared: f64, distance_squared: f64) -> f64 {
        self.cylinder
            .get_height_component(width, width_squared, distance_squared)
    }

    fn extrusion(&self) -> &AbstractDistortedExtrusion {
        self.cylinder.extrusion_state()
    }
}

/// Java `public static class Factory implements DistortedShape.Factory`.
pub struct DistortedPipeShapeFactory;

impl DistortedShapeFactory for DistortedPipeShapeFactory {
    /// Java `@Nonnull @Override public DistortedShape create(...)`.
    #[allow(clippy::too_many_arguments)] // The Java signature has ten parameters.
    fn create(
        &self,
        origin: Vector3d,
        direction: Vector3d,
        _length: f64,
        start_width: f64,
        start_height: f64,
        mid_width: f64,
        mid_height: f64,
        end_width: f64,
        end_height: f64,
        interpolation: InterpolationMode,
    ) -> Box<dyn DistortedShape> {
        let compensation = get_compensation_factor(direction);
        let scale = get_height_compensation(compensation);
        let start_height = start_height * scale;
        let mid_height = mid_height * scale;
        let end_height = end_height * scale;
        let max_width = math_util::max_value3_f64(start_width, mid_width, end_width);
        let max_height = math_util::max_value3_f64(start_height, mid_height, end_height);
        Box::new(DistortedPipeShape::new(
            origin,
            direction,
            start_width,
            start_height,
            mid_width,
            mid_height,
            end_width,
            end_height,
            max_width,
            max_height,
            compensation,
            interpolation,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipe(compensation: f64) -> DistortedPipeShape {
        DistortedPipeShape::new(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(16.0, 0.0, 0.0),
            4.0,
            5.0,
            6.0,
            7.0,
            8.0,
            9.0,
            8.0,
            9.0,
            compensation,
            InterpolationMode::Linear,
        )
    }

    #[test]
    fn inside_the_segment_the_pipe_matches_the_cylinder() {
        let pipe = pipe(1.0);
        assert_eq!(pipe.get_width_at(0.25), 5.0);
        assert_eq!(pipe.get_width_at(0.5), 6.0);
        assert_eq!(pipe.get_width_at(0.75), 7.0);
        assert_eq!(pipe.get_height_at(0.25), 6.0);
        assert_eq!(pipe.get_height_at(0.5), 7.0);
        assert_eq!(pipe.get_height_at(0.75), 8.0);
    }

    #[test]
    fn the_caps_are_squashed_by_the_pitch_compensation() {
        let pipe = pipe(1.0);
        // t = 0: fade = interpolate(1) = 1 → lerp(4, 4, 1) = 4 (the cap
        // starts at the full radius and fades outwards).
        assert_eq!(pipe.get_width_at(0.0), 4.0);
        // t = -0.25: fade = 1 - 0.5 = 0.5 → lerp(4, 2, 1) = 2.
        assert_eq!(pipe.get_width_at(-0.25), 2.0);
        // t = -0.5 and below: fade = 0 → 0.
        assert_eq!(pipe.get_width_at(-0.5), 0.0);
        assert_eq!(pipe.get_width_at(-2.0), 0.0);
        // t = 1: fade = interpolate(1) = 1 → lerp(8, 8, 1) = 8.
        assert_eq!(pipe.get_width_at(1.0), 8.0);
        // t = 1.25: fade = 1 - 0.5 = 0.5 → lerp(8, 4, 1) = 4.
        assert_eq!(pipe.get_width_at(1.25), 4.0);
        // t = 1.5 and above: fade = 0 → 0.
        assert_eq!(pipe.get_width_at(1.5), 0.0);
        assert_eq!(pipe.get_width_at(3.0), 0.0);
    }

    #[test]
    fn a_zero_compensation_keeps_the_cap_radius() {
        let pipe = pipe(0.0);
        assert_eq!(pipe.get_width_at(-0.25), 4.0);
        assert_eq!(pipe.get_width_at(0.0), 4.0);
        assert_eq!(pipe.get_width_at(1.25), 8.0);
        assert_eq!(pipe.get_width_at(3.0), 8.0);
    }

    #[test]
    fn a_pipe_accepts_every_projection() {
        let pipe = pipe(0.0);
        assert!(pipe.is_valid_projection(-10.0));
        assert!(pipe.is_valid_projection(0.0));
        assert!(pipe.is_valid_projection(0.5));
        assert!(pipe.is_valid_projection(10.0));
        // The inherited projection is the xz projection on the segment.
        assert_eq!(pipe.get_projection(8.0, 0.0), 0.5);
    }
}
