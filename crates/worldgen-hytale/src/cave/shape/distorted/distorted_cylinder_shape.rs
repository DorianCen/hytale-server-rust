// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/DistortedCylinderShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.DistortedCylinderShape`
//! — an extrusion with a circular cross-section that varies along its length.
//!
//! The radius is interpolated through three control points — start, middle and
//! end — with the [`InterpolationMode`] curve applied to the half-way segments:
//!
//! ```text
//! t <= 0.0   start
//! t <= 0.5   lerpUnclamped(start, mid, interpolate(t * 2))
//! t <= 1.0   lerpUnclamped(mid, end, interpolate((t - 0.5) * 2))
//! otherwise  end
//! ```
//!
//! The factory applies the *pitch compensation*: a steep segment gets a taller
//! cross-section (`1 + factor³ * 3`, factor = `|pitch| / (π/2)`), so that the
//! vertical projection of an inclined tube does not pinch.
//!
//! # Java → Rust
//!
//! The Java base classes become
//! [`AbstractDistortedShape`](super::abstract_distorted_shape::AbstractDistortedShape)
//! (bounds) and
//! [`AbstractDistortedExtrusion`](super::abstract_distorted_extrusion::AbstractDistortedExtrusion)
//! (segment + interpolation + the shared height-at-projection body), owned by
//! composition.

use procedural::noise::{InterpolationFunction, InterpolationMode};

use crate::cave::cave_type::CaveType;
use crate::cave::shape::distorted::abstract_distorted_extrusion::{
    get_height_at_projection as extrusion_height_at_projection, AbstractDistortedExtrusion,
    DistortedExtrusion,
};
use crate::cave::shape::distorted::distorted_shape::DistortedShape;
use crate::cave::shape::distorted::distorted_shapes::DistortedShapeFactory;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use crate::loader::util::Vector3d;
use crate::util::bounds::WorldBounds;
use hytale_math::math_util;
use hytale_math::trig_math_util;

/// Java `protected static final double PITCH_COMPENSATION_MIN = 1.0`.
pub(crate) const PITCH_COMPENSATION_MIN: f64 = 1.0;
/// Java `protected static final double PITCH_COMPENSATION_RANGE = 3.0`.
pub(crate) const PITCH_COMPENSATION_RANGE: f64 = 3.0;

/// Java `public class DistortedCylinderShape extends AbstractDistortedExtrusion`.
pub struct DistortedCylinderShape {
    /// The `AbstractDistortedExtrusion` state (segment, interpolation, bounds).
    extrusion: AbstractDistortedExtrusion,
    /// Java `protected final double startWidth`.
    start_width: f64,
    /// Java `protected final double startHeight`.
    start_height: f64,
    /// Java `protected final double midWidth`.
    mid_width: f64,
    /// Java `protected final double midHeight`.
    mid_height: f64,
    /// Java `protected final double endWidth`.
    end_width: f64,
    /// Java `protected final double endHeight`.
    end_height: f64,
}

impl DistortedCylinderShape {
    /// Java `public DistortedCylinderShape(@Nonnull Vector3d o, @Nonnull
    /// Vector3d v, double startWidth, double startHeight, double midWidth,
    /// double midHeight, double endWidth, double endHeight,
    /// GeneralNoise.InterpolationFunction interpolation)`.
    #[allow(clippy::too_many_arguments)] // The Java constructor has nine parameters.
    pub fn new(
        o: Vector3d,
        v: Vector3d,
        start_width: f64,
        start_height: f64,
        mid_width: f64,
        mid_height: f64,
        end_width: f64,
        end_height: f64,
        interpolation: InterpolationMode,
    ) -> Self {
        // Java:
        // ```java
        // this(o, v, startWidth, startHeight, midWidth, midHeight, endWidth,
        //      endHeight, MathUtil.maxValue(startWidth, midWidth, endWidth),
        //      MathUtil.maxValue(startHeight, midHeight, endHeight), interpolation);
        // ```
        Self::with_max(
            o,
            v,
            start_width,
            start_height,
            mid_width,
            mid_height,
            end_width,
            end_height,
            math_util::max_value3_f64(start_width, mid_width, end_width),
            math_util::max_value3_f64(start_height, mid_height, end_height),
            interpolation,
        )
    }

    /// Java `public DistortedCylinderShape(@Nonnull Vector3d o, @Nonnull
    /// Vector3d v, double startWidth, double startHeight, double midWidth,
    /// double midHeight, double endWidth, double endHeight, double maxWidth,
    /// double maxHeight, GeneralNoise.InterpolationFunction interpolation)`.
    #[allow(clippy::too_many_arguments)] // The Java constructor has eleven parameters.
    pub fn with_max(
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
        interpolation: InterpolationMode,
    ) -> Self {
        Self {
            extrusion: AbstractDistortedExtrusion::new(o, v, max_width, max_height, interpolation),
            start_width,
            start_height,
            mid_width,
            mid_height,
            end_width,
            end_height,
        }
    }

    /// Java `protected final double startWidth` — read by the pipe subclass.
    pub(crate) fn start_width(&self) -> f64 {
        self.start_width
    }

    /// Java `protected final double startHeight` — read by the pipe subclass.
    pub(crate) fn start_height(&self) -> f64 {
        self.start_height
    }

    /// Java `protected final double midWidth` — read by the pipe subclass.
    pub(crate) fn mid_width(&self) -> f64 {
        self.mid_width
    }

    /// Java `protected final double midHeight` — read by the pipe subclass.
    pub(crate) fn mid_height(&self) -> f64 {
        self.mid_height
    }

    /// Java `protected final double endWidth` — read by the pipe subclass.
    pub(crate) fn end_width(&self) -> f64 {
        self.end_width
    }

    /// Java `protected final double endHeight` — read by the pipe subclass.
    pub(crate) fn end_height(&self) -> f64 {
        self.end_height
    }

    /// The shared extrusion state (segment / interpolation / bounds).
    pub(crate) fn extrusion_state(&self) -> &AbstractDistortedExtrusion {
        &self.extrusion
    }

    /// Java `protected abstract double getDistanceSq(double x, double z,
    /// double t)`.
    pub fn get_distance_sq(&self, x: f64, z: f64, t: f64) -> f64 {
        let (mut x, mut z) = (x, z);
        let o = self.extrusion.o();
        let v = self.extrusion.v();
        if t <= 0.0 {
            // Java: "xz is before the start of the line"
            x -= o.x;
            z -= o.z;
        } else if t >= 1.0 {
            // Java: "xz is past the end of the line"
            x -= o.x + v.x;
            z -= o.z + v.z;
        } else {
            // Java: "xz is somewhere along the line"
            x -= o.x + v.x * t;
            z -= o.z + v.z * t;
        }
        x * x + z * z
    }

    /// Java `protected abstract double getHeightComponent(double width, double
    /// width2, double dist2)`.
    pub fn get_height_component(&self, width: f64, width2: f64, dist2: f64) -> f64 {
        (width2 - dist2).sqrt() / width
    }
}

/// Java `protected static double getDimAt(double t, double startDim, double
/// midDim, double endDim, @Nonnull GeneralNoise.InterpolationFunction
/// interpolation)`.
pub(crate) fn get_dim_at(
    t: f64,
    start_dim: f64,
    mid_dim: f64,
    end_dim: f64,
    interpolation: InterpolationMode,
) -> f64 {
    if t <= 0.0 {
        start_dim
    } else if t >= 1.0 {
        end_dim
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

/// Java `protected static double getCompensationFactor(@Nonnull Vector3d
/// direction)`.
pub(crate) fn get_compensation_factor(direction: Vector3d) -> f64 {
    let ny = direction.y
        / (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z)
            .sqrt();
    let pitch = trig_math_util::asin(-ny);
    // Java: `Math.abs(pitch) / TrigMathUtil.PI_HALF` — the `float` pitch is
    // widened to `double`, the `float` PI_HALF constant likewise.
    (pitch as f64).abs() / trig_math_util::PI_HALF as f64
}

/// Java `protected static double getHeightCompensation(double factor)` —
/// `PITCH_COMPENSATION_MIN + (factor * factor * factor *
/// PITCH_COMPENSATION_RANGE)`.
pub(crate) fn get_height_compensation(factor: f64) -> f64 {
    PITCH_COMPENSATION_MIN + (factor * factor * factor * PITCH_COMPENSATION_RANGE)
}

impl DistortedShape for DistortedCylinderShape {
    /// Java `@Nonnull @Override public Vector3d getStart()` — the segment
    /// origin.
    fn get_start(&self) -> Vector3d {
        self.extrusion.o()
    }

    /// Java `@Nonnull @Override public Vector3d getEnd()` — the segment end.
    fn get_end(&self) -> Vector3d {
        let o = self.extrusion.o();
        let v = self.extrusion.v();
        Vector3d::new(o.x + v.x, o.y + v.y, o.z + v.z)
    }

    /// Java `@Nonnull @Override public Vector3d getAnchor(@Nonnull Vector3d
    /// vector, double t, double tv, double th)`.
    fn get_anchor(&self, vector: Vector3d, t: f64, tv: f64, th: f64) -> Vector3d {
        let radius_y = self.get_height_at(t);
        let radius_xz = self.get_width_at(t);
        crate::cave::shape::cave_node_shape_utils::get_pipe_anchor(
            vector,
            self.extrusion.o(),
            self.extrusion.v(),
            radius_xz,
            radius_y,
            radius_xz,
            t,
            tv,
            th,
        )
    }

    /// Java `@Override public double getProjection(double x, double z)`.
    fn get_projection(&self, x: f64, z: f64) -> f64 {
        let o = self.extrusion.o();
        let v = self.extrusion.v();
        let mut t = (x - o.x) * v.x + (z - o.z) * v.z;
        t /= v.x * v.x + v.z * v.z;
        t
    }

    /// Java `@Override public boolean isValidProjection(double t)`.
    fn is_valid_projection(&self, t: f64) -> bool {
        t > 0.0 && t < 1.0
    }

    /// Java `@Override public double getYAt(double t)`.
    fn get_y_at(&self, t: f64) -> f64 {
        self.extrusion.o().y + self.extrusion.v().y * t
    }

    /// Java `@Override public double getWidthAt(double t)`.
    fn get_width_at(&self, t: f64) -> f64 {
        get_dim_at(
            t,
            self.start_width,
            self.mid_width,
            self.end_width,
            self.extrusion.interpolation(),
        )
    }

    /// Java `@Override public double getHeightAt(double t)`.
    fn get_height_at(&self, t: f64) -> f64 {
        get_dim_at(
            t,
            self.start_height,
            self.mid_height,
            self.end_height,
            self.extrusion.interpolation(),
        )
    }

    /// Java `@Override public double getHeightAtProjection(int seed, double x,
    /// double z, double t, double centerY, CaveType caveType, ShapeDistortion
    /// distortion)`.
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

    /// Java `@Nonnull @Override public IWorldBounds getBounds()`.
    fn get_bounds(&self) -> &WorldBounds {
        self.extrusion.get_bounds()
    }
}

impl DistortedExtrusion for DistortedCylinderShape {
    fn get_distance_sq(&self, x: f64, z: f64, t: f64) -> f64 {
        DistortedCylinderShape::get_distance_sq(self, x, z, t)
    }

    fn get_height_component(&self, width: f64, width_squared: f64, distance_squared: f64) -> f64 {
        DistortedCylinderShape::get_height_component(self, width, width_squared, distance_squared)
    }

    fn extrusion(&self) -> &AbstractDistortedExtrusion {
        &self.extrusion
    }
}

/// Java `public static class Factory implements DistortedShape.Factory` — the
/// cylinder factory (with the pitch compensation).
pub struct DistortedCylinderShapeFactory;

impl DistortedShapeFactory for DistortedCylinderShapeFactory {
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
        let comp = get_compensation_factor(direction);
        let scale = get_height_compensation(comp);
        let start_height = start_height * scale;
        let mid_height = mid_height * scale;
        let end_height = end_height * scale;
        Box::new(DistortedCylinderShape::new(
            origin,
            direction,
            start_width,
            start_height,
            mid_width,
            mid_height,
            end_width,
            end_height,
            interpolation,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dimension_curve_interpolates_through_the_middle() {
        // Linear first half: lerp(2, 6, t * 2).
        assert_eq!(
            get_dim_at(0.0, 2.0, 6.0, 4.0, InterpolationMode::Linear),
            2.0
        );
        assert_eq!(
            get_dim_at(0.25, 2.0, 6.0, 4.0, InterpolationMode::Linear),
            4.0
        );
        assert_eq!(
            get_dim_at(0.5, 2.0, 6.0, 4.0, InterpolationMode::Linear),
            6.0
        );
        // Linear second half: lerp(6, 4, (t - 0.5) * 2).
        assert_eq!(
            get_dim_at(0.75, 2.0, 6.0, 4.0, InterpolationMode::Linear),
            5.0
        );
        assert_eq!(
            get_dim_at(1.0, 2.0, 6.0, 4.0, InterpolationMode::Linear),
            4.0
        );
        // Outside of the segment the end dimensions are clamped.
        assert_eq!(
            get_dim_at(-1.0, 2.0, 6.0, 4.0, InterpolationMode::Linear),
            2.0
        );
        assert_eq!(
            get_dim_at(2.0, 2.0, 6.0, 4.0, InterpolationMode::Linear),
            4.0
        );
    }

    #[test]
    fn the_dimension_curve_applies_the_interpolation_mode() {
        // t = 0.1 maps to u = 0.2 on the first half.
        // Linear:   2 + 0.2    * 4 = 2.8
        // Hermite:  2 + 0.104  * 4 = 2.416
        // Quintic:  2 + 0.05792 * 4 = 2.23168
        let linear = get_dim_at(0.1, 2.0, 6.0, 4.0, InterpolationMode::Linear);
        let hermite = get_dim_at(0.1, 2.0, 6.0, 4.0, InterpolationMode::Hermite);
        let quintic = get_dim_at(0.1, 2.0, 6.0, 4.0, InterpolationMode::Quintic);
        assert!((linear - 2.8).abs() < 1e-15);
        assert!((hermite - 2.416).abs() < 1e-15);
        assert!((quintic - 2.23168).abs() < 1e-15);
    }

    #[test]
    fn the_pitch_compensation_grows_the_height() {
        // A horizontal segment has no pitch compensation.
        let horizontal = get_compensation_factor(Vector3d::new(10.0, 0.0, 0.0));
        assert_eq!(horizontal, 0.0);
        assert_eq!(get_height_compensation(horizontal), 1.0);

        // A vertical segment is fully compensated: 1 + 1 * 3 = 4.
        let vertical = get_compensation_factor(Vector3d::new(0.0, 10.0, 0.0));
        assert_eq!(vertical, 1.0);
        assert_eq!(get_height_compensation(vertical), 4.0);

        // The compensation is invariant under the segment length.
        assert_eq!(
            get_compensation_factor(Vector3d::new(0.0, 2.0, 0.0)),
            vertical
        );
    }

    #[test]
    fn the_factory_scales_the_heights_by_the_pitch_compensation() {
        let shape = DistortedCylinderShapeFactory.create(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(0.0, 10.0, 0.0),
            10.0,
            2.0,
            3.0,
            2.0,
            3.0,
            2.0,
            3.0,
            InterpolationMode::Linear,
        );
        // The height is multiplied by four, the width is untouched.
        assert_eq!(shape.get_width_at(0.5), 2.0);
        assert_eq!(shape.get_height_at(0.5), 12.0);
    }

    #[test]
    fn the_projection_is_the_xz_projection_on_the_segment() {
        let shape = DistortedCylinderShape::new(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(16.0, 0.0, 0.0),
            3.0,
            3.0,
            3.0,
            3.0,
            3.0,
            3.0,
            InterpolationMode::Linear,
        );
        assert_eq!(shape.get_projection(8.0, 0.0), 0.5);
        assert!(shape.is_valid_projection(0.5));
        assert!(!shape.is_valid_projection(0.0));
        assert!(!shape.is_valid_projection(1.0));
        assert_eq!(shape.get_y_at(0.5), 40.0);
        // The distance to the centre-line is zero on the line and the radius
        // at that column otherwise.
        assert_eq!(shape.get_distance_sq(8.0, 0.0, 0.5), 0.0);
        assert_eq!(shape.get_distance_sq(8.0, 2.0, 0.5), 4.0);
        // Before the start the distance is measured from the start point.
        assert_eq!(shape.get_distance_sq(-2.0, 2.0, 0.0), 8.0);
    }
}
