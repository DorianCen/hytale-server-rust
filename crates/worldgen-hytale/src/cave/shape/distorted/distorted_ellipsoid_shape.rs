// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/DistortedEllipsoidShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.DistortedEllipsoidShape`
//! — a rotated ellipsoid ("body") distorted by the width noise.
//!
//! The shape is the standard ellipsoid equation
//! `(x/rx)² + (y/ry)² + (z/rz)² = 1`, solved for `y`:
//!
//! ```text
//! y = sqrt((1 - (qx + qz)) * ry²)
//! ```
//!
//! where `qx` / `qz` are the *interpolated* squared normalised horizontal
//! offsets and the width noise divides the sum (the noise can only contract
//! the shape).
//!
//! # Pitch compensation
//!
//! A distorted shape cannot be rotated perfectly, so the factory compensates
//! the pitch by trading `y` and `z` radius against the angle:
//!
//! * `|pitch| < 45°` — stretch `ry` by `1 / alpha` and squeeze `rz` by
//!   `alpha`, `alpha = 1 - |pitch| / (π/2)`;
//! * `|pitch| > 45°` — swap `ry` / `rz` and re-map the pitch with
//!   [`wrap_pitch`] (the shape degenerates towards the vertical axis).
//!
//! # Java → Rust
//!
//! The `AbstractDistortedBody` base is the
//! [`AbstractDistortedBody`](super::abstract_distorted_body::AbstractDistortedBody)
//! member; the abstract `getHeight` is passed to
//! [`AbstractDistortedBody::height_at_projection`] as a closure.

use hytale_math::trig_math_util;
use procedural::noise::{InterpolationFunction, InterpolationMode};

use crate::cave::cave_type::CaveType;
use crate::cave::shape::distorted::abstract_distorted_body::AbstractDistortedBody;
use crate::cave::shape::distorted::distorted_shape::{direction_to_yaw_pitch, DistortedShape};
use crate::cave::shape::distorted::distorted_shapes::DistortedShapeFactory;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use crate::loader::util::Vector3d;
use crate::util::bounds::WorldBounds;

/// Java `public class DistortedEllipsoidShape extends AbstractDistortedBody`.
pub struct DistortedEllipsoidShape {
    /// The `AbstractDistortedBody` state (origin, direction, rotation,
    /// bounds).
    body: AbstractDistortedBody,
    /// Java `private final double radiusX`.
    radius_x: f64,
    /// Java `private final double radiusY`.
    radius_y: f64,
    /// Java `private final double radiusZ`.
    radius_z: f64,
    /// Java `private final double radiusX2`.
    radius_x2: f64,
    /// Java `private final double radiusY2`.
    radius_y2: f64,
    /// Java `private final double radiusZ2`.
    radius_z2: f64,
    /// Java `private final double invRadiusX2`.
    inv_radius_x2: f64,
    /// Java `private final double invRadiusZ2`.
    inv_radius_z2: f64,
    /// Java `private final GeneralNoise.InterpolationFunction interpolation`.
    interpolation: InterpolationMode,
}

impl DistortedEllipsoidShape {
    /// Java `public DistortedEllipsoidShape(@Nonnull Vector3d o, Vector3d d,
    /// double yaw, double pitch, double radiusX, double radiusY, double
    /// radiusZ, GeneralNoise.InterpolationFunction interpolation)`.
    #[allow(clippy::too_many_arguments)] // The Java constructor has eight parameters.
    pub fn new(
        o: Vector3d,
        d: Vector3d,
        yaw: f64,
        pitch: f64,
        radius_x: f64,
        radius_y: f64,
        radius_z: f64,
        interpolation: InterpolationMode,
    ) -> Self {
        let radius_x2 = radius_x * radius_x;
        let radius_y2 = radius_y * radius_y;
        let radius_z2 = radius_z * radius_z;
        Self {
            body: AbstractDistortedBody::new(o, d, yaw, pitch, radius_x, radius_y, radius_z),
            radius_x,
            radius_y,
            radius_z,
            radius_x2,
            radius_y2,
            radius_z2,
            inv_radius_x2: 1.0 / radius_x2,
            inv_radius_z2: 1.0 / radius_z2,
            interpolation,
        }
    }

    /// Java `@Override protected double getHeight(int seed, double x, double
    /// z, double t, double centerY, CaveType caveType, @Nonnull
    /// ShapeDistortion distortion)`.
    ///
    /// `x / z` are already in the body's local (rotated) frame.
    #[allow(clippy::too_many_arguments)] // The Java signature has seven parameters.
    fn get_height(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        _t: f64,
        _center_y: f64,
        _cave_type: &CaveType,
        distortion: &ShapeDistortion,
    ) -> f64 {
        let o = self.body.o();
        let dx = x - o.x;
        let dz = z - o.z;

        let dx2 = dx * dx;
        if dx2 > self.radius_x2 {
            return 0.0;
        }

        let dz2 = dz * dz;
        if dz2 > self.radius_z2 {
            return 0.0;
        }

        // Java: `(x^2/rx^2) + (y2/ry2) + (z^2)/rz2 = 1 #solve for y`
        let qx = self.interpolation.interpolate(dx2 * self.inv_radius_x2);
        let qz = self.interpolation.interpolate(dz2 * self.inv_radius_z2);

        // Java: "apply noise to horizontal components"
        // "divide used to ensure the distortion only contracts the shape
        // (y decreases as qh increases, noise range 0.0-1.0)"
        let mut qh = qx + qz;
        let noise = distortion.get_width_factor(seed, x, z);
        if noise > 0.0 {
            qh /= noise;
        }

        let y2 = (1.0 - qh) * self.radius_y2;
        if y2 <= 0.0 {
            return 0.0;
        }

        y2.sqrt()
    }
}

/// Java `private static double wrapPitch(double pitch, double radiusY, double
/// radiusZ)`.
fn wrap_pitch(pitch: f64, radius_y: f64, radius_z: f64) -> f64 {
    let min = radius_y.min(radius_z);
    let max = radius_y.max(radius_z);
    let aspect = 1.0 - (min / max);

    if pitch < -trig_math_util::PI_QUARTER as f64 {
        let alpha = 1.0
            - ((pitch + trig_math_util::PI_QUARTER as f64) / trig_math_util::PI_QUARTER as f64)
                .abs();
        return -trig_math_util::PI_HALF as f64 * aspect * alpha;
    }

    if pitch > trig_math_util::PI_QUARTER as f64 {
        let alpha = 1.0
            - ((pitch - trig_math_util::PI_QUARTER as f64) / trig_math_util::PI_QUARTER as f64)
                .abs();
        return trig_math_util::PI_HALF as f64 * aspect * alpha;
    }

    pitch
}

impl DistortedShape for DistortedEllipsoidShape {
    /// Java `@Nonnull @Override public Vector3d getStart()`.
    fn get_start(&self) -> Vector3d {
        self.body.get_start()
    }

    /// Java `@Nonnull @Override public Vector3d getEnd()`.
    fn get_end(&self) -> Vector3d {
        self.body.get_end()
    }

    /// Java `@Nonnull @Override public Vector3d getAnchor(@Nonnull Vector3d
    /// vector, double tx, double ty, double tz)`.
    fn get_anchor(&self, vector: Vector3d, tx: f64, ty: f64, tz: f64) -> Vector3d {
        crate::cave::shape::cave_node_shape_utils::get_sphere_anchor(
            vector,
            self.body.o(),
            self.radius_x,
            self.radius_y,
            self.radius_z,
            tx,
            ty,
            tz,
        )
    }

    /// Java `@Override public double getProjection(double x, double z)` —
    /// "not applicable to this shape type", always `0`.
    fn get_projection(&self, _x: f64, _z: f64) -> f64 {
        0.0
    }

    /// Java `@Override public boolean isValidProjection(double t)` — always
    /// valid.
    fn is_valid_projection(&self, _t: f64) -> bool {
        true
    }

    /// Java `@Override public double getYAt(double t)`.
    fn get_y_at(&self, _t: f64) -> f64 {
        self.body.o().y
    }

    /// Java `@Override public double getWidthAt(double t)`.
    fn get_width_at(&self, _t: f64) -> f64 {
        // Java: "Child nodes might spawn off the narrow side of the ellipse
        // so they should start with a width no greater than that."
        java_min(self.radius_x, self.radius_z)
    }

    /// Java `@Override public double getHeightAt(double t)`.
    fn get_height_at(&self, _t: f64) -> f64 {
        self.radius_y
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
        self.body.height_at_projection(
            seed,
            x,
            z,
            t,
            center_y,
            cave_type,
            distortion,
            |seed, x, z, t, center_y, cave_type, distortion| {
                self.get_height(seed, x, z, t, center_y, cave_type, distortion)
            },
        )
    }

    /// Java `@Nonnull @Override public IWorldBounds getBounds()`.
    fn get_bounds(&self) -> &WorldBounds {
        self.body.get_bounds()
    }

    /// Java `@Override public double getFloor(double x, double z, double
    /// centerY, double height)`.
    fn get_floor(&self, x: f64, z: f64, center_y: f64, height: f64) -> f64 {
        self.body.get_floor(x, z, center_y, height)
    }

    /// Java `@Override public double getCeiling(double x, double z, double
    /// centerY, double height)`.
    fn get_ceiling(&self, x: f64, z: f64, center_y: f64, height: f64) -> f64 {
        self.body.get_ceiling(x, z, center_y, height)
    }
}

/// Java `Math.min(double, double)` — propagates NaN.
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// Java `public static class Factory extends AbstractDistortedBody.Factory`.
pub struct DistortedEllipsoidShapeFactory;

impl DistortedEllipsoidShapeFactory {
    /// Java `protected DistortedShape createShape(@Nonnull Vector3d origin,
    /// Vector3d direction, double yaw, double pitch, double radiusX, double
    /// radiusY, double radiusZ, GeneralNoise.InterpolationFunction
    /// interpolation)`.
    #[allow(clippy::too_many_arguments)] // The Java signature has eight parameters.
    fn create_shape(
        origin: Vector3d,
        direction: Vector3d,
        yaw: f64,
        mut pitch: f64,
        radius_x: f64,
        mut radius_y: f64,
        mut radius_z: f64,
        interpolation: InterpolationMode,
    ) -> DistortedEllipsoidShape {
        // Java: "Note: can't do perfect-math pitch rotation on distorted
        // shapes so to compensate on angles above +-45deg we flip the y & z
        // radii and offset the angle instead."
        //
        // The Java code has two *identical* branches (one per side of the
        // quarter turn); the port merges them into one condition.
        if pitch < -(trig_math_util::PI_QUARTER as f64) || pitch > trig_math_util::PI_QUARTER as f64
        {
            // Java: `double ry = radiusY; radiusY = radiusZ; radiusZ = ry;`
            std::mem::swap(&mut radius_y, &mut radius_z);
            pitch = wrap_pitch(pitch, radius_y, radius_z);
        } else {
            let alpha = 1.0 - (pitch / trig_math_util::PI_HALF as f64).abs();
            radius_y /= alpha;
            radius_z *= alpha;
        }
        DistortedEllipsoidShape::new(
            origin,
            direction,
            yaw,
            pitch,
            radius_x,
            radius_y,
            radius_z,
            interpolation,
        )
    }
}

impl DistortedShapeFactory for DistortedEllipsoidShapeFactory {
    /// Java `@Nonnull @Override public DistortedShape create(...)` (inherited
    /// from `AbstractDistortedBody.Factory`): normalise the direction into a
    /// yaw / pitch pair, then create the body.
    #[allow(clippy::too_many_arguments)] // The Java signature has ten parameters.
    fn create(
        &self,
        origin: Vector3d,
        direction: Vector3d,
        length: f64,
        start_width: f64,
        start_height: f64,
        _mid_width: f64,
        _mid_height: f64,
        _end_width: f64,
        _end_height: f64,
        interpolation: InterpolationMode,
    ) -> Box<dyn DistortedShape> {
        let (yaw, pitch) = direction_to_yaw_pitch(direction);
        Box::new(Self::create_shape(
            origin,
            direction,
            yaw,
            pitch,
            start_width,
            start_height,
            length,
            interpolation,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;
    use crate::util::bounds::{IChunkBounds, IWorldBounds};

    fn ellipsoid() -> DistortedEllipsoidShape {
        DistortedEllipsoidShape::new(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(0.0, 0.0, 1.0),
            0.0,
            0.0,
            2.0,
            3.0,
            4.0,
            InterpolationMode::Linear,
        )
    }

    #[test]
    fn the_ellipsoid_is_centred_and_always_valid() {
        let shape = ellipsoid();
        assert_eq!(shape.get_projection(123.0, -7.0), 0.0);
        assert!(shape.is_valid_projection(-100.0));
        assert!(shape.is_valid_projection(100.0));
        assert_eq!(shape.get_y_at(0.75), 40.0);
        assert_eq!(shape.get_width_at(0.5), 2.0);
        assert_eq!(shape.get_height_at(0.5), 3.0);
    }

    #[test]
    fn the_bounds_and_anchors_follow_the_radii() {
        let shape = ellipsoid();
        let bounds = shape.get_bounds();
        assert_eq!(bounds.get_low_bound_x(), -2);
        assert_eq!(bounds.get_low_bound_y(), 37);
        assert_eq!(bounds.get_low_bound_z(), -4);
        assert_eq!(bounds.get_high_bound_x(), 2);
        assert_eq!(bounds.get_high_bound_y(), 43);
        assert_eq!(bounds.get_high_bound_z(), 4);
        // The start / end are above / below the centre on the world y axis.
        assert_eq!(shape.get_start(), Vector3d::new(0.0, 43.0, 0.0));
        assert_eq!(shape.get_end(), Vector3d::new(0.0, 37.0, 0.0));
        // The anchor is the radial projection onto the ellipsoid surface.
        let anchor = shape.get_anchor(Vector3d::ZERO, 0.5, 1.0, 0.5);
        assert_eq!(anchor.x, 0.0);
        assert_eq!(anchor.y, 43.0);
        assert_eq!(anchor.z, 0.0);
    }

    #[test]
    fn the_height_solves_the_ellipsoid_equation() {
        let shape = ellipsoid();
        let distortion = ShapeDistortion::default();
        let cave_type = crate::cave::test_stubs::cave_type_with_height_factor("ellipse", 1.0);

        // The centre column: sqrt(ry²) = ry.
        assert_eq!(
            shape.get_height_at_projection(7, 0.0, 0.0, 0.0, 40.0, &cave_type, &distortion),
            3.0
        );
        // x = 1 → qx = 1/4 → y = sqrt(0.75 * 9).
        let height =
            shape.get_height_at_projection(7, 1.0, 0.0, 0.0, 40.0, &cave_type, &distortion);
        assert!((height - (6.75f64).sqrt()).abs() < 1e-12);
        // x = 2 is exactly the radius → qx = 1 → y = 0.
        assert_eq!(
            shape.get_height_at_projection(7, 2.0, 0.0, 0.0, 40.0, &cave_type, &distortion),
            0.0
        );
        // x = 3 is outside of the radius → 0 before the equation is solved.
        assert_eq!(
            shape.get_height_at_projection(7, 3.0, 0.0, 0.0, 40.0, &cave_type, &distortion),
            0.0
        );
        // z = 2 → qz = 4/16 = 1/4 → y = sqrt(0.75 * 9) as well.
        let height =
            shape.get_height_at_projection(7, 0.0, 2.0, 0.0, 40.0, &cave_type, &distortion);
        assert!((height - (6.75f64).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn the_width_noise_contracts_the_height() {
        // A width factor of 0.5 doubles qh, so x = 1 → qh = 0.5 →
        // y = sqrt(0.5 * 9).
        let shape = ellipsoid();
        let noise = Rc::new(FixedNoise(0.5));
        let distortion = ShapeDistortion::of(Some(noise), None, None);
        let cave_type = crate::cave::test_stubs::cave_type_with_height_factor("ellipse", 1.0);
        let height =
            shape.get_height_at_projection(7, 1.0, 0.0, 0.0, 40.0, &cave_type, &distortion);
        assert!((height - (4.5f64).sqrt()).abs() < 1e-12);
        // A zero noise is ignored (the Java `noise > 0` guard).
        let distortion = ShapeDistortion::of(Some(Rc::new(FixedNoise(0.0))), None, None);
        let height =
            shape.get_height_at_projection(7, 1.0, 0.0, 0.0, 40.0, &cave_type, &distortion);
        assert!((height - (6.75f64).sqrt()).abs() < 1e-12);
    }

    #[test]
    fn a_vertical_direction_swaps_the_y_and_z_radii() {
        // pitch = asin(-1) = -π/2 < -π/4 → the y/z radii are swapped and the
        // pitch is wrapped to zero (the aspect ratio maps it exactly).
        let shape = DistortedEllipsoidShapeFactory.create(
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
        // radiusX = startWidth = 2, radiusY = startHeight (was the length!
        // the body factory passes `startWidth, startHeight, length`), then
        // swapped with radiusZ = length.
        assert_eq!(shape.get_width_at(0.0), 2.0);
        assert_eq!(shape.get_height_at(0.0), 10.0);
        let bounds = shape.get_bounds();
        assert_eq!(bounds.get_low_bound_y(), 30);
        assert_eq!(bounds.get_high_bound_y(), 50);
    }

    #[test]
    fn a_shallow_pitch_stretches_the_height() {
        // A 45° direction lands on the `else` branch (alpha = 0.5):
        // ry = 3 / 0.5 = 6, rz = 10 * 0.5 = 5.
        let shape = DistortedEllipsoidShapeFactory.create(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(10.0, 10.0, 0.0),
            10.0,
            2.0,
            3.0,
            2.0,
            3.0,
            2.0,
            3.0,
            InterpolationMode::Linear,
        );
        assert_eq!(shape.get_height_at(0.0), 6.0);
        let bounds = shape.get_bounds();
        assert_eq!(bounds.get_low_bound_y(), 34);
        assert_eq!(bounds.get_high_bound_y(), 46);
    }

    struct FixedNoise(f64);

    impl procedural::noise::support::NoiseProperty for FixedNoise {
        fn get(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }

        fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }
}
