// Ported from: `.hytale-source/.../Math/.../vector/Rotation3f.java` and
// `.hytale-source/.../Math/.../vector/Transform.java`
//! Java `com.hypixel.hytale.math.vector.Rotation3f` and
//! `com.hypixel.hytale.math.vector.Transform` — a 3-dimensional Euler
//! rotation (single precision) and a position + rotation in 3D space.
//!
//! # Deviations
//!
//! * The Rust port keeps only the parts the world generator observes (the
//!   three rotation components, the position and the rotation); the
//!   quaternion helpers, the `lerp` family, the `Axis` operations, the
//!   codec and the relative-transform masks are not ported (they belong to
//!   the CoreServer / the client).

use crate::loader::util::Vector3d;

/// Java `class Rotation3f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation3f {
    /// Java `public float x` — the pitch.
    pub x: f32,
    /// Java `public float y` — the yaw.
    pub y: f32,
    /// Java `public float z` — the roll.
    pub z: f32,
}

impl Rotation3f {
    /// Java `public static final Rotation3fc ZERO` = `new Rotation3f(0, 0, 0)`.
    pub const ZERO: Rotation3f = Rotation3f {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Java `public Rotation3f()` — the zero rotation.
    pub const fn new() -> Self {
        Self::ZERO
    }

    /// Java `public Rotation3f(final float pitch, final float yaw, final
    /// float roll)`.
    pub const fn new_with(pitch: f32, yaw: f32, roll: f32) -> Self {
        Self {
            x: pitch,
            y: yaw,
            z: roll,
        }
    }

    /// Java `float x()` / `float pitch()`.
    pub const fn pitch(&self) -> f32 {
        self.x
    }

    /// Java `float y()` / `float yaw()`.
    pub const fn yaw(&self) -> f32 {
        self.y
    }

    /// Java `float z()` / `float roll()`.
    pub const fn roll(&self) -> f32 {
        self.z
    }

    /// Java `public void addYaw(final float yaw)` — `this.y += yaw`.
    pub fn add_yaw(&mut self, yaw: f32) {
        self.y += yaw;
    }

    /// Java `public void addPitch(final float pitch)` — `this.x += pitch`.
    pub fn add_pitch(&mut self, pitch: f32) {
        self.x += pitch;
    }

    /// Java `public void addRoll(final float roll)` — `this.z += roll`.
    pub fn add_roll(&mut self, roll: f32) {
        self.z += roll;
    }
}

impl Default for Rotation3f {
    fn default() -> Self {
        Self::ZERO
    }
}

// Ported from: `.hytale-source/.../Math/.../vector/Transform.java`
/// Java `class Transform`.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    /// Java `protected Vector3d position`.
    position: Vector3d,
    /// Java `protected Rotation3f rotation`.
    rotation: Rotation3f,
}

impl Transform {
    /// Java `public Transform(@Nonnull final Vector3d position, @Nonnull final
    /// Rotation3f rotation)`.
    pub fn new(position: Vector3d, rotation: Rotation3f) -> Self {
        Self { position, rotation }
    }

    /// Java `public Transform(final double x, final double y, final double
    /// z)` — the zero rotation.
    pub fn from_position(x: f64, y: f64, z: f64) -> Self {
        Self {
            position: Vector3d::new(x, y, z),
            rotation: Rotation3f::ZERO,
        }
    }

    /// Java `@Nonnull Vector3d getPosition()`.
    pub const fn get_position(&self) -> &Vector3d {
        &self.position
    }

    /// Java `@Nonnull Rotation3f getRotation()`.
    pub const fn get_rotation(&self) -> &Rotation3f {
        &self.rotation
    }
}

/// Java `com.hypixel.hytale.server.core.universe.world.chunk.heightmap.HeightmapColumn.NO_HEIGHT`
/// = `Integer.MIN_VALUE`.
pub const NO_HEIGHT: i32 = i32::MIN;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rotation_is_the_zero_rotation_by_default() {
        let rotation = Rotation3f::new();
        assert_eq!(rotation.pitch(), 0.0);
        assert_eq!(rotation.yaw(), 0.0);
        assert_eq!(rotation.roll(), 0.0);
    }

    #[test]
    fn the_yaw_is_added_in_place() {
        let mut rotation = Rotation3f::new();
        rotation.add_yaw(1.5);
        assert_eq!(rotation.yaw(), 1.5);
        rotation.add_yaw(-0.5);
        assert_eq!(rotation.yaw(), 1.0);
    }

    #[test]
    fn the_transform_carries_its_position_and_rotation() {
        let transform = Transform::new(
            Vector3d::new(1.0, 2.0, 3.0),
            Rotation3f::new_with(0.0, 1.0, 0.0),
        );
        assert_eq!(*transform.get_position(), Vector3d::new(1.0, 2.0, 3.0));
        assert_eq!(transform.get_rotation().yaw(), 1.0);
    }

    #[test]
    fn the_no_height_constant_matches_the_java_one() {
        assert_eq!(NO_HEIGHT, i32::MIN);
    }
}
