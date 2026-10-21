// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/AbstractDistortedBody.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.AbstractDistortedBody`
//! — a distorted shape centred on its origin (the ellipsoid).
//!
//! The body owns the centre `o`, the direction `v` and a
//! [`CoordinateRotator`] built from the yaw / pitch. The rotation turns the
//! world `x/z` offsets into the shape's local frame before the height is
//! computed, and it turns the local `y` extent into the world floor / ceiling
//! afterwards.
//!
//! # The axis-aligned bounds of a rotated body
//!
//! Java inflates the bounds by the *largest* extent of each axis over the
//! rotation matrix: `maxX` rotates `(±rx, ry, rz)`, `maxY` rotates
//! `(rx, ±ry, rz)`, `maxZ` rotates `(rx, ry, ±rz)` and takes the absolute
//! maximum. That is exact for a full rotation of an ellipsoid only when the
//! radii are symmetric on the two sampled signs; the Java code uses this
//! cheap bound, and the port keeps it verbatim.
//!
//! # Java → Rust
//!
//! * The Java `Factory` (direction → normalised yaw / pitch → `createShape`)
//!   is the free [`direction_to_yaw_pitch`](super::distorted_shape::direction_to_yaw_pitch)
//!   helper; the ellipsoid factory supplies the shape-specific pitch
//!   compensation.
//! * The abstract `getHeight(...)` is a closure parameter of
//!   [`AbstractDistortedBody::height_at_projection`].

use hytale_math::math_util;
use procedural::noise::random::CoordinateRotator;

use crate::cave::cave_type::CaveType;
use crate::cave::shape::distorted::abstract_distorted_shape::AbstractDistortedShape;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use crate::loader::util::Vector3d;
use crate::util::bounds::{IWorldBounds, WorldBounds};

/// Java `abstract class AbstractDistortedBody extends AbstractDistortedShape`.
pub struct AbstractDistortedBody {
    /// The Java `AbstractDistortedShape` state (the bounds).
    base: AbstractDistortedShape,
    /// Java `@Nonnull protected final Vector3d o`.
    o: Vector3d,
    /// Java `protected final Vector3d v`.
    v: Vector3d,
    /// Java `@Nonnull protected final CoordinateRotator rotation`.
    rotation: CoordinateRotator,
}

impl AbstractDistortedBody {
    /// Java `public AbstractDistortedBody(@Nonnull Vector3d o, Vector3d v,
    /// double yaw, double pitch, double radiusX, double radiusY, double
    /// radiusZ)`.
    pub fn new(
        o: Vector3d,
        v: Vector3d,
        yaw: f64,
        pitch: f64,
        radius_x: f64,
        radius_y: f64,
        radius_z: f64,
    ) -> Self {
        // Java: `this(o, v, new CoordinateRotator(pitch, yaw), radiusX,
        // radiusY, radiusZ);`
        let rotation = CoordinateRotator::new(pitch, yaw);
        let base = AbstractDistortedShape::new(
            o,
            max_x(&rotation, radius_x, radius_y, radius_z),
            max_y(&rotation, radius_x, radius_y, radius_z),
            max_z(&rotation, radius_x, radius_y, radius_z),
        );
        Self {
            base,
            o,
            v,
            rotation,
        }
    }

    /// The centre (`protected final Vector3d o`).
    pub fn o(&self) -> Vector3d {
        self.o
    }

    /// The direction (`protected final Vector3d v`).
    pub fn v(&self) -> Vector3d {
        self.v
    }

    /// The rotation (`protected final CoordinateRotator rotation`).
    pub fn rotation(&self) -> &CoordinateRotator {
        &self.rotation
    }

    /// The bounds (`AbstractDistortedShape` state).
    pub fn get_bounds(&self) -> &WorldBounds {
        self.base.get_bounds()
    }

    /// Java `@Nonnull @Override public Vector3d getStart()` — the point above
    /// the origin at the high vertical bound.
    pub fn get_start(&self) -> Vector3d {
        let bounds = self.base.get_bounds();
        Vector3d::new(self.o.x, bounds.get_high_bound_y() as f64, self.o.z)
    }

    /// Java `@Nonnull @Override public Vector3d getEnd()` — the point below
    /// the origin at the low vertical bound.
    pub fn get_end(&self) -> Vector3d {
        let bounds = self.base.get_bounds();
        Vector3d::new(self.o.x, bounds.get_low_bound_y() as f64, self.o.z)
    }

    /// Java `@Override public double getFloor(double x, double z, double
    /// centerY, double height)`.
    pub fn get_floor(&self, x: f64, z: f64, center_y: f64, height: f64) -> f64 {
        let dx = x - self.o.x;
        let dz = z - self.o.z;
        let dy = self.rotation.rotate_y3(dx, -height, dz);
        center_y + dy
    }

    /// Java `@Override public double getCeiling(double x, double z, double
    /// centerY, double height)`.
    pub fn get_ceiling(&self, x: f64, z: f64, center_y: f64, height: f64) -> f64 {
        let dx = x - self.o.x;
        let dz = z - self.o.z;
        let dy = self.rotation.rotate_y3(dx, height, dz);
        center_y + dy
    }

    /// Java `@Override public double getHeightAtProjection(int caveSeed,
    /// double x, double z, double t, double centerY, CaveType caveType,
    /// ShapeDistortion distortion)`.
    ///
    /// The body rotates `x/z` into its local frame and delegates to the
    /// concrete `getHeight` (the closure parameter).
    #[allow(clippy::too_many_arguments)] // The Java signature has seven parameters.
    pub fn height_at_projection<F>(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        t: f64,
        center_y: f64,
        cave_type: &CaveType,
        distortion: &ShapeDistortion,
        get_height: F,
    ) -> f64
    where
        F: Fn(i32, f64, f64, f64, f64, &CaveType, &ShapeDistortion) -> f64,
    {
        let dx = x - self.o.x;
        let dz = z - self.o.z;
        let x = self.o.x + self.rotation.rotate_x3(dx, 0.0, dz);
        let z = self.o.z + self.rotation.rotate_z3(dx, 0.0, dz);
        get_height(seed, x, z, t, center_y, cave_type, distortion)
    }
}

/// Java `private static double maxX(@Nonnull CoordinateRotator rotation, double
/// radiusX, double radiusY, double radiusZ)`.
fn max_x(rotation: &CoordinateRotator, radius_x: f64, radius_y: f64, radius_z: f64) -> f64 {
    let x1 = rotation.rotate_x3(radius_x, radius_y, radius_z).abs();
    let x2 = rotation.rotate_x3(-radius_x, radius_y, radius_z).abs();
    math_util::max_value_f64(x1, x2)
}

/// Java `private static double maxY(@Nonnull CoordinateRotator rotation, double
/// radiusX, double radiusY, double radiusZ)`.
fn max_y(rotation: &CoordinateRotator, radius_x: f64, radius_y: f64, radius_z: f64) -> f64 {
    let y1 = rotation.rotate_y3(radius_x, radius_y, radius_z).abs();
    let y2 = rotation.rotate_y3(radius_x, -radius_y, radius_z).abs();
    java_max(y1, y2)
}

/// Java `private static double maxZ(@Nonnull CoordinateRotator rotation, double
/// radiusX, double radiusY, double radiusZ)`.
fn max_z(rotation: &CoordinateRotator, radius_x: f64, radius_y: f64, radius_z: f64) -> f64 {
    let z1 = rotation.rotate_z3(radius_x, radius_y, radius_z).abs();
    let z2 = rotation.rotate_z3(radius_x, radius_y, -radius_z).abs();
    math_util::max_value_f64(z1, z2)
}

/// Java `Math.max(double, double)` — propagates NaN (Rust's `f64::max`
/// returns the non-NaN operand).
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}
