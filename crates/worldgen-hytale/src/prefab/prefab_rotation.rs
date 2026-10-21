// Ported from: `.hytale-source/.../CoreServer/.../server/core/prefab/PrefabRotation.java`
//! Java `com.hypixel.hytale.server.core.prefab.PrefabRotation` — the four
//! quarter turns a prefab (and a prefab cave node) can be rotated by.
//!
//! The rotation *geometry* (`getX` / `getZ` / `rotate`), the yaw, the block
//! rotation helper (`getRotation(int)`) and the filler rotation helper
//! (`getFiller(int)`) are ported (the cave package needs the geometry —
//! `CaveYawMode` — and the prefab paste needs the block/filler rotation).

use hytale_math::trig_math_util;

use worldgen_model::block_key::{FillerBlockUtil, Rotation, RotationTuple};

use crate::loader::util::Vector3d;

/// Java `Rotation.rotateY(int filler)` — the filler offset rotated around
/// the Y axis.
///
/// ```java
/// case Ninety -> FillerBlockUtil.pack(unpackZ(filler), unpackY(filler), -unpackX(filler));
/// case OneEighty -> FillerBlockUtil.pack(-unpackX(filler), unpackY(filler), -unpackZ(filler));
/// case TwoSeventy -> FillerBlockUtil.pack(-unpackZ(filler), unpackY(filler), unpackX(filler));
/// ```
fn rotate_filler_y(degrees: i32, filler: i32) -> i32 {
    match degrees {
        90 => FillerBlockUtil::pack(
            FillerBlockUtil::unpack_z(filler),
            FillerBlockUtil::unpack_y(filler),
            -FillerBlockUtil::unpack_x(filler),
        ),
        180 => FillerBlockUtil::pack(
            -FillerBlockUtil::unpack_x(filler),
            FillerBlockUtil::unpack_y(filler),
            -FillerBlockUtil::unpack_z(filler),
        ),
        270 => FillerBlockUtil::pack(
            -FillerBlockUtil::unpack_z(filler),
            FillerBlockUtil::unpack_y(filler),
            FillerBlockUtil::unpack_x(filler),
        ),
        _ => filler,
    }
}

/// Java `enum PrefabRotation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrefabRotation {
    /// Java `ROTATION_0` (`Rotation.None`).
    Rotation0,
    /// Java `ROTATION_90` (`Rotation.Ninety`).
    Rotation90,
    /// Java `ROTATION_180` (`Rotation.OneEighty`).
    Rotation180,
    /// Java `ROTATION_270` (`Rotation.TwoSeventy`).
    Rotation270,
}

impl PrefabRotation {
    /// Java `PrefabRotation.VALUES`.
    pub const VALUES: [PrefabRotation; 4] = [
        PrefabRotation::Rotation0,
        PrefabRotation::Rotation90,
        PrefabRotation::Rotation180,
        PrefabRotation::Rotation270,
    ];

    /// Java `PrefabRotation.PREFIX`.
    pub const PREFIX: &'static str = "ROTATION_";

    /// Java `Rotation.getDegrees()` — `0`, `90`, `180` or `270`.
    pub const fn get_degrees(self) -> i32 {
        match self {
            PrefabRotation::Rotation0 => 0,
            PrefabRotation::Rotation90 => 90,
            PrefabRotation::Rotation180 => 180,
            PrefabRotation::Rotation270 => 270,
        }
    }

    /// Java `float getYaw()` — `-TrigMathUtil.degToRad * degrees`.
    ///
    /// The Java expression is `(-degToRad) * degrees` in `float` precision.
    pub fn get_yaw(self) -> f32 {
        -trig_math_util::DEG_TO_RAD * self.get_degrees() as f32
    }

    /// Java `int getX(int x, int z)`.
    pub const fn get_x(self, x: i32, z: i32) -> i32 {
        match self {
            PrefabRotation::Rotation0 => x,
            PrefabRotation::Rotation90 => z,
            PrefabRotation::Rotation180 => -x,
            PrefabRotation::Rotation270 => -z,
        }
    }

    /// Java `int getZ(int x, int z)`.
    pub const fn get_z(self, x: i32, z: i32) -> i32 {
        match self {
            PrefabRotation::Rotation0 => z,
            PrefabRotation::Rotation90 => -x,
            PrefabRotation::Rotation180 => -z,
            PrefabRotation::Rotation270 => x,
        }
    }

    /// Java `double rotateDoubleX(double x, double z)` / `rotateDoubleZ` —
    /// the rotated `(x, z)` pair.
    pub const fn rotate_x_z(self, x: f64, z: f64) -> (f64, f64) {
        match self {
            PrefabRotation::Rotation0 => (x, z),
            PrefabRotation::Rotation90 => (z, -x),
            PrefabRotation::Rotation180 => (-x, -z),
            PrefabRotation::Rotation270 => (-z, x),
        }
    }

    /// Java `void rotate(Vector3d v)` — rotates the vector around the Y axis.
    pub fn rotate(self, v: &mut Vector3d) {
        let (x, z) = self.rotate_x_z(v.x, v.z);
        v.x = x;
        v.z = z;
    }

    /// Java `PrefabRotation add(PrefabRotation other)` —
    /// `VALUES[(degrees + other.degrees) % 360 / 90]`.
    #[allow(clippy::should_implement_trait)] // Java method name `add`
    pub fn add(self, other: PrefabRotation) -> PrefabRotation {
        let val = self.get_degrees() + other.get_degrees();
        PrefabRotation::VALUES[((val % 360) / 90) as usize]
    }

    /// Java `String name()` — `ROTATION_0`, `ROTATION_90`, …
    pub const fn name(self) -> &'static str {
        match self {
            PrefabRotation::Rotation0 => "ROTATION_0",
            PrefabRotation::Rotation90 => "ROTATION_90",
            PrefabRotation::Rotation180 => "ROTATION_180",
            PrefabRotation::Rotation270 => "ROTATION_270",
        }
    }

    /// Java `int getRotation(int rotation)` — the rotation index of a block
    /// after the prefab rotation.
    ///
    /// ```java
    /// if (this.rotation == Rotation.None) return rotation;
    /// var inRotation = RotationTuple.get(rotation);
    /// return RotationTuple.of(inRotation.yaw().add(this.rotation), inRotation.pitch(), inRotation.roll()).index();
    /// ```
    pub fn get_rotation(self, rotation: i32) -> i32 {
        if self == PrefabRotation::Rotation0 {
            return rotation;
        }
        let in_rotation = RotationTuple::get(rotation);
        let yaw = Rotation::of_degrees(in_rotation.yaw.get_degrees() + self.get_degrees());
        RotationTuple::index(yaw, in_rotation.pitch, in_rotation.roll)
    }

    /// Java `int getFiller(int filler)` — the filler offset after the
    /// prefab rotation.
    ///
    /// ```java
    /// if (rotation == Rotation.None) return filler;
    /// return BlockRotationUtil.getRotatedFiller(filler, Axis.Y, rotation);
    /// ```
    pub fn get_filler(self, filler: i32) -> i32 {
        if self == PrefabRotation::Rotation0 {
            return filler;
        }
        rotate_filler_y(self.get_degrees(), filler)
    }

    /// Java `static PrefabRotation valueOfExtended(String s)` — accepts both
    /// `ROTATION_90` and the short `90` form.
    pub fn value_of_extended(s: &str) -> Option<PrefabRotation> {
        let name = if s.starts_with(PrefabRotation::PREFIX) {
            s.to_string()
        } else {
            format!("{}{}", PrefabRotation::PREFIX, s)
        };
        PrefabRotation::VALUES
            .into_iter()
            .find(|rotation| rotation.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_yaw_is_the_negative_radian_angle() {
        assert_eq!(PrefabRotation::Rotation0.get_yaw(), 0.0);
        assert_eq!(
            PrefabRotation::Rotation90.get_yaw(),
            -trig_math_util::DEG_TO_RAD * 90.0
        );
        assert_eq!(
            PrefabRotation::Rotation180.get_yaw(),
            -trig_math_util::DEG_TO_RAD * 180.0
        );
        assert_eq!(
            PrefabRotation::Rotation270.get_yaw(),
            -trig_math_util::DEG_TO_RAD * 270.0
        );
    }

    #[test]
    fn the_rotation_rotates_the_xz_plane() {
        assert_eq!(PrefabRotation::Rotation0.get_x(1, 2), 1);
        assert_eq!(PrefabRotation::Rotation0.get_z(1, 2), 2);
        assert_eq!(PrefabRotation::Rotation90.get_x(1, 2), 2);
        assert_eq!(PrefabRotation::Rotation90.get_z(1, 2), -1);
        assert_eq!(PrefabRotation::Rotation180.get_x(1, 2), -1);
        assert_eq!(PrefabRotation::Rotation180.get_z(1, 2), -2);
        assert_eq!(PrefabRotation::Rotation270.get_x(1, 2), -2);
        assert_eq!(PrefabRotation::Rotation270.get_z(1, 2), 1);
    }

    #[test]
    fn rotating_a_vector_uses_the_original_components() {
        let mut v = Vector3d::new(1.0, 5.0, 2.0);
        PrefabRotation::Rotation90.rotate(&mut v);
        assert_eq!(v, Vector3d::new(2.0, 5.0, -1.0));

        let mut v = Vector3d::new(1.0, 5.0, 2.0);
        PrefabRotation::Rotation180.rotate(&mut v);
        assert_eq!(v, Vector3d::new(-1.0, 5.0, -2.0));

        let mut v = Vector3d::new(1.0, 5.0, 2.0);
        PrefabRotation::Rotation270.rotate(&mut v);
        assert_eq!(v, Vector3d::new(-2.0, 5.0, 1.0));
    }

    #[test]
    fn adding_two_rotations_wraps_around() {
        assert_eq!(
            PrefabRotation::Rotation0.add(PrefabRotation::Rotation0),
            PrefabRotation::Rotation0
        );
        assert_eq!(
            PrefabRotation::Rotation90.add(PrefabRotation::Rotation90),
            PrefabRotation::Rotation180
        );
        assert_eq!(
            PrefabRotation::Rotation270.add(PrefabRotation::Rotation90),
            PrefabRotation::Rotation0
        );
        assert_eq!(
            PrefabRotation::Rotation180.add(PrefabRotation::Rotation270),
            PrefabRotation::Rotation90
        );
    }

    #[test]
    fn the_extended_value_of_accepts_both_spellings() {
        assert_eq!(
            PrefabRotation::value_of_extended("ROTATION_90"),
            Some(PrefabRotation::Rotation90)
        );
        assert_eq!(
            PrefabRotation::value_of_extended("90"),
            Some(PrefabRotation::Rotation90)
        );
        assert_eq!(PrefabRotation::value_of_extended("45"), None);
    }
}
