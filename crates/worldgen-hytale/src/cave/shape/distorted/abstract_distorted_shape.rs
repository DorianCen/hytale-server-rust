// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/AbstractDistortedShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted.AbstractDistortedShape`
//! — the shared bounds of the distorted shapes.
//!
//! The Java class only stores the six integer bound fields computed by one of
//! its two constructors and the [`clamp_pitch`] helper. The Rust port stores
//! them as a [`WorldBounds`] (the same flattened representation the 4c shapes
//! use) and exposes the individual accessors through `IChunkBounds` /
//! `IWorldBounds`.
//!
//! # The two bound constructions
//!
//! * [`AbstractDistortedShape::new`] — a **body** centred on its origin: the
//!   bounds are `origin ± radius` on each axis.
//! * [`AbstractDistortedShape::from_segment`] — an **extrusion** from `o` to
//!   `o + v`: the bounds are the segment's AABB inflated by the *width* on
//!   `x` / `z` and by the *height* on `y`.
//!
//! Java computes `MathUtil.floor` / `MathUtil.ceil` (the saturating `(int)`
//! casts), reproduced by [`hytale_math::math_util`].

use hytale_math::math_util;

use crate::loader::util::Vector3d;
use crate::util::bounds::{IChunkBounds, IWorldBounds, WorldBounds};

/// Java `private static final double PITCH_MIN = Math.toRadians(5)`.
const PITCH_MIN: f64 = 5.0 / 180.0 * std::f64::consts::PI;
/// Java `private static final double PITCH_MAX = Math.toRadians(175)`.
const PITCH_MAX: f64 = 175.0 / 180.0 * std::f64::consts::PI;

/// Java `abstract class AbstractDistortedShape implements DistortedShape`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbstractDistortedShape {
    /// Java `private final int lowBoundX, lowBoundY, lowBoundZ, highBoundX,
    /// highBoundY, highBoundZ`.
    bounds: WorldBounds,
}

impl AbstractDistortedShape {
    /// Java `public AbstractDistortedShape(@Nonnull Vector3d o, double
    /// radiusX, double radiusY, double radiusZ)` — a shape centred on its
    /// origin.
    pub fn new(o: Vector3d, radius_x: f64, radius_y: f64, radius_z: f64) -> Self {
        Self {
            bounds: WorldBounds::with_bounds(
                math_util::floor(o.x - radius_x),
                math_util::floor(o.y - radius_y),
                math_util::floor(o.z - radius_z),
                math_util::ceil(o.x + radius_x),
                math_util::ceil(o.y + radius_y),
                math_util::ceil(o.z + radius_z),
            ),
        }
    }

    /// Java `public AbstractDistortedShape(@Nonnull Vector3d o, @Nonnull
    /// Vector3d v, double width, double height)` — an extrusion from `o` to
    /// `o + v`.
    pub fn from_segment(o: Vector3d, v: Vector3d, width: f64, height: f64) -> Self {
        Self {
            bounds: WorldBounds::with_bounds(
                math_util::floor(o.x.min(o.x + v.x) - width),
                math_util::floor(o.y.min(o.y + v.y) - height),
                math_util::floor(o.z.min(o.z + v.z) - width),
                math_util::ceil(o.x.max(o.x + v.x) + width),
                math_util::ceil(o.y.max(o.y + v.y) + height),
                math_util::ceil(o.z.max(o.z + v.z) + width),
            ),
        }
    }

    /// The bounds computed by the Java constructors.
    pub fn get_bounds(&self) -> &WorldBounds {
        &self.bounds
    }

    /// Java `public static double clampPitch(double pitch)` —
    /// `MathUtil.clamp(pitch, PITCH_MIN, PITCH_MAX)`.
    pub fn clamp_pitch(pitch: f64) -> f64 {
        math_util::clamp_f64(pitch, PITCH_MIN, PITCH_MAX)
    }
}

// The `IWorldBounds` implementation of the Java class (inherited by every
// distorted shape) — kept so tests can call the bound accessors directly.
impl IChunkBounds for AbstractDistortedShape {
    fn get_low_bound_x(&self) -> i32 {
        self.bounds.get_low_bound_x()
    }

    fn get_low_bound_z(&self) -> i32 {
        self.bounds.get_low_bound_z()
    }

    fn get_high_bound_x(&self) -> i32 {
        self.bounds.get_high_bound_x()
    }

    fn get_high_bound_z(&self) -> i32 {
        self.bounds.get_high_bound_z()
    }
}

impl IWorldBounds for AbstractDistortedShape {
    fn get_low_bound_y(&self) -> i32 {
        self.bounds.get_low_bound_y()
    }

    fn get_high_bound_y(&self) -> i32 {
        self.bounds.get_high_bound_y()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_bounded_by_its_radii() {
        let shape = AbstractDistortedShape::new(Vector3d::new(10.0, 40.0, -5.0), 2.0, 3.0, 4.0);
        assert_eq!(shape.get_low_bound_x(), 8);
        assert_eq!(shape.get_low_bound_y(), 37);
        assert_eq!(shape.get_low_bound_z(), -9);
        assert_eq!(shape.get_high_bound_x(), 12);
        assert_eq!(shape.get_high_bound_y(), 43);
        assert_eq!(shape.get_high_bound_z(), -1);
    }

    #[test]
    fn an_extrusion_is_bounded_by_the_segment_and_width_height() {
        let shape = AbstractDistortedShape::from_segment(
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(10.0, -5.0, 4.0),
            3.0,
            2.0,
        );
        assert_eq!(shape.get_low_bound_x(), -3);
        assert_eq!(shape.get_low_bound_y(), 33);
        assert_eq!(shape.get_low_bound_z(), -3);
        assert_eq!(shape.get_high_bound_x(), 13);
        assert_eq!(shape.get_high_bound_y(), 42);
        assert_eq!(shape.get_high_bound_z(), 7);
    }

    #[test]
    fn the_pitch_is_clamped_to_five_degrees() {
        assert_eq!(AbstractDistortedShape::clamp_pitch(0.0), PITCH_MIN);
        assert_eq!(AbstractDistortedShape::clamp_pitch(PITCH_MIN), PITCH_MIN);
        assert_eq!(
            AbstractDistortedShape::clamp_pitch(std::f64::consts::PI),
            PITCH_MAX
        );
        assert_eq!(AbstractDistortedShape::clamp_pitch(PITCH_MAX), PITCH_MAX);
        // The bounds are the exact `Math.toRadians` values.
        assert!((PITCH_MIN - 5.0f64.to_radians()).abs() < 1e-15);
        assert!((PITCH_MAX - 175.0f64.to_radians()).abs() < 1e-14);
    }
}
