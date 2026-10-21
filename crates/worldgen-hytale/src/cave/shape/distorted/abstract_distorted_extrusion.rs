// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/AbstractDistortedExtrusion.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.AbstractDistortedExtrusion`
//! — a distorted shape with a start and an end point and an extruded
//! cross-section.
//!
//! The Java class owns the segment (`o`, `v`), the interpolation curve and the
//! full `getHeightAtProjection` body math. The cross-section itself
//! ([`DistortedExtrusion::get_distance_sq`] /
//! [`DistortedExtrusion::get_height_component`]) stays abstract and is
//! supplied by the cylinder.
//!
//! # Java → Rust
//!
//! Rust has no abstract classes, so the Java base is split into
//!
//! * [`AbstractDistortedExtrusion`] — the data (segment, interpolation,
//!   bounds), and `AbstractDistortedShape.getBounds()`, and
//! * [`DistortedExtrusion`] — the internal trait the concrete extrusion
//!   implements (the two Java abstract methods), plus the free function
//!   [`get_height_at_projection`] that transcribes the Java body verbatim.
//!
//! # The height-at-projection math (bit-exact)
//!
//! 1. `width = getWidthAt(t)`, scaled by the `CaveType` height-radius factor
//!    at `floor(centerY)`.
//! 2. Reject the column when it is outside that horizontal radius.
//! 3. Apply the *width distortion noise* and reject again when the distorted
//!    radius no longer covers the column (the noise can only contract, because
//!    the Java code divides by it and the rejection is repeated).
//! 4. `height = getHeightAt(t)` — a zero height means no cave here.
//! 5. `alpha = clamp(getHeightComponent(width, width², dist²), 0, 1)`, then
//!    pass it through the interpolation curve and scale the height by it.

use hytale_math::math_util;
use procedural::noise::{InterpolationFunction, InterpolationMode};

use crate::cave::cave_type::CaveType;
use crate::cave::shape::distorted::abstract_distorted_shape::AbstractDistortedShape;
use crate::cave::shape::distorted::distorted_shape::DistortedShape;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use crate::loader::util::Vector3d;
use crate::util::bounds::WorldBounds;

/// Java `abstract class AbstractDistortedExtrusion extends AbstractDistortedShape`.
pub struct AbstractDistortedExtrusion {
    /// The Java `AbstractDistortedShape` state (the bounds).
    base: AbstractDistortedShape,
    /// Java `@Nonnull protected final Vector3d o`.
    o: Vector3d,
    /// Java `@Nonnull protected final Vector3d v`.
    v: Vector3d,
    /// Java `protected final GeneralNoise.InterpolationFunction interpolation`.
    interpolation: InterpolationMode,
}

impl AbstractDistortedExtrusion {
    /// Java `public AbstractDistortedExtrusion(@Nonnull Vector3d o, @Nonnull
    /// Vector3d v, double width, double height, GeneralNoise.InterpolationFunction
    /// interpolation)`.
    pub fn new(
        o: Vector3d,
        v: Vector3d,
        width: f64,
        height: f64,
        interpolation: InterpolationMode,
    ) -> Self {
        Self {
            base: AbstractDistortedShape::from_segment(o, v, width, height),
            o,
            v,
            interpolation,
        }
    }

    /// The segment origin (`protected final Vector3d o`).
    pub fn o(&self) -> Vector3d {
        self.o
    }

    /// The segment direction (`protected final Vector3d v`).
    pub fn v(&self) -> Vector3d {
        self.v
    }

    /// The interpolation curve (`protected final InterpolationFunction`).
    pub fn interpolation(&self) -> InterpolationMode {
        self.interpolation
    }

    /// The bounds (`AbstractDistortedShape` state).
    pub fn get_bounds(&self) -> &WorldBounds {
        self.base.get_bounds()
    }
}

/// The two abstract members of the Java `AbstractDistortedExtrusion`
/// (`getDistanceSq` and `getHeightComponent`), implemented by
/// [`DistortedCylinderShape`](super::distorted_cylinder_shape::DistortedCylinderShape).
pub trait DistortedExtrusion: DistortedShape {
    /// Java `protected abstract double getDistanceSq(double x, double z,
    /// double t)` — the squared horizontal distance from `x/z` to the
    /// shape's centre-line at projection `t`.
    fn get_distance_sq(&self, x: f64, z: f64, t: f64) -> f64;

    /// Java `protected abstract double getHeightComponent(double width, double
    /// widthSquared, double distanceSquared)` — the `[0, 1]` height component
    /// at the column.
    fn get_height_component(&self, width: f64, width_squared: f64, distance_squared: f64) -> f64;

    /// The shared `AbstractDistortedExtrusion` state.
    fn extrusion(&self) -> &AbstractDistortedExtrusion;
}

/// Java `@Override public double getHeightAtProjection(int seed, double x,
/// double z, double t, double centerY, @Nonnull CaveType caveType, @Nonnull
/// ShapeDistortion distortion)` — the verbatim Java body.
#[allow(clippy::too_many_arguments)] // The Java signature has seven parameters.
pub fn get_height_at_projection<E: DistortedExtrusion + ?Sized>(
    shape: &E,
    seed: i32,
    x: f64,
    z: f64,
    t: f64,
    center_y: f64,
    cave_type: &CaveType,
    distortion: &ShapeDistortion,
) -> f64 {
    // Java: `double width = getWidthAt(t);`
    let mut width = shape.get_width_at(t);

    // Java: "size of caves may vary depending on the y-level"
    // `width *= caveType.getHeightRadiusFactor(seed, x, z, MathUtil.floor(centerY));`
    // (the Java factor is a float; it widens for the double multiply)
    width *= cave_type.get_height_radius_factor(seed, x, z, math_util::floor(center_y)) as f64;

    // Java: "distance from x,z and it's intersection with the shape's center-line"
    let dist2 = shape.get_distance_sq(x, z, t);
    let mut width2 = width * width;

    if dist2 > width2 {
        // Java: "xz is outside of the horizontal radius of the shape"
        return 0.0;
    }

    // Java: "distorted width"
    width *= distortion.get_width_factor(seed, x, z);
    width2 = width * width;

    if dist2 > width2 {
        // Java: "xz is outside of the distorted horizontal radius of the shape"
        return 0.0;
    }

    // Java: "height of the shape at its center"
    let height = shape.get_height_at(t);
    if height == 0.0 {
        return 0.0;
    }

    // Java: "height component of the shape at xz"
    let alpha = shape.get_height_component(width, width2, dist2);
    let alpha = math_util::clamp_f64(alpha, 0.0, 1.0);

    // Java: "interpolation function may alter the curvature"
    let alpha = shape.extrusion().interpolation().interpolate(alpha);

    height * alpha
}
