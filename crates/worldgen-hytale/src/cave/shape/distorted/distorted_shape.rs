// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/DistortedShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.DistortedShape`
//! — the geometry interface of the distorted shape family.
//!
//! A distorted shape is either
//!
//! * an **extrusion** (a cylinder / a pipe) — a cross-section swept along a
//!   straight segment, or
//! * a **body** (an ellipsoid) — a blob centred on its origin.
//!
//! Both share the `IWorldBounds` contract (exposed as [`DistortedShape::get_bounds`]
//! here), the start / end anchors and the height-at-projection math.
//!
//! # Java → Rust
//!
//! * Java `extends IWorldBounds` → the bounds are reachable through
//!   [`DistortedShape::get_bounds`]. The concrete shapes (and
//!   `DistortedCaveNodeShape`, which forwards its bounds to the shape) use
//!   that instead of duplicating the six bound accessors on the trait.
//! * Java `@FunctionalInterface GeneralNoise.InterpolationFunction` is only
//!   ever instantiated from `GeneralNoise.InterpolationMode.function` (see
//!   `DistortedCaveNodeShapeGeneratorJsonLoader`), so the port stores the
//!   [`InterpolationMode`] enum directly. Same values, same `interpolate`
//!   arithmetic; a custom user-supplied function is not representable.
//! * `Vector3d.set(...)` (joml, in-place) becomes a returned copy.

use hytale_math::trig_math_util;
use procedural::noise::InterpolationMode;

use crate::cave::cave_type::CaveType;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use crate::loader::util::Vector3d;
use crate::util::bounds::WorldBounds;

/// Java `GeneralNoise.InterpolationFunction DEFAULT_INTERPOLATION =
/// GeneralNoise.InterpolationMode.LINEAR.function`.
pub const DEFAULT_INTERPOLATION: InterpolationMode = InterpolationMode::Linear;

/// Java `interface DistortedShape extends IWorldBounds`.
pub trait DistortedShape {
    /// Java `Vector3d getStart()`.
    fn get_start(&self) -> Vector3d;

    /// Java `Vector3d getEnd()`.
    fn get_end(&self) -> Vector3d;

    /// Java `Vector3d getAnchor(Vector3d vector, double tx, double ty, double
    /// tz)`.
    fn get_anchor(&self, vector: Vector3d, tx: f64, ty: f64, tz: f64) -> Vector3d;

    /// Java `double getProjection(double x, double z)`.
    fn get_projection(&self, x: f64, z: f64) -> f64;

    /// Java `boolean isValidProjection(double t)`.
    fn is_valid_projection(&self, t: f64) -> bool;

    /// Java `double getYAt(double t)`.
    fn get_y_at(&self, t: f64) -> f64;

    /// Java `double getWidthAt(double t)`.
    fn get_width_at(&self, t: f64) -> f64;

    /// Java `double getHeightAt(double t)`.
    fn get_height_at(&self, t: f64) -> f64;

    /// Java `double getHeightAtProjection(int seed, double x, double z, double
    /// t, double centerY, CaveType caveType, ShapeDistortion distortion)`.
    #[allow(clippy::too_many_arguments)] // The Java signature has seven parameters.
    fn get_height_at_projection(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        t: f64,
        center_y: f64,
        cave_type: &CaveType,
        distortion: &ShapeDistortion,
    ) -> f64;

    /// Java `@Nonnull IWorldBounds getBounds()` — the shape *is* its own
    /// bounds in Java (`DistortedCaveNodeShape.getBounds()` returns it).
    fn get_bounds(&self) -> &WorldBounds;

    /// Java `default double getCeiling(double x, double z, double centerY,
    /// double height)`.
    fn get_ceiling(&self, _x: f64, _z: f64, center_y: f64, height: f64) -> f64 {
        center_y + height
    }

    /// Java `default double getFloor(double x, double z, double centerY,
    /// double height)`.
    fn get_floor(&self, _x: f64, _z: f64, center_y: f64, height: f64) -> f64 {
        center_y - height
    }
}

/// The Java `org.joml.Vector3d.length()` the shape math uses
/// (`Math.sqrt(x*x + y*y + z*z)`).
pub(crate) fn vector_length(v: Vector3d) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

/// The Java `TrigMathUtil.atan2(double, double)` / `TrigMathUtil.asin(double)`
/// pair the distorted-body factory uses to turn a direction into a yaw /
/// pitch (`float` precision widened back to `double`).
pub(crate) fn direction_to_yaw_pitch(direction: Vector3d) -> (f64, f64) {
    let scale = 1.0 / vector_length(direction);
    let nx = direction.x * scale;
    let ny = direction.y * scale;
    let nz = direction.z * scale;
    let yaw = trig_math_util::atan2_f64(nx, nz) as f64;
    let pitch = trig_math_util::asin(-ny) as f64;
    (yaw, pitch)
}
