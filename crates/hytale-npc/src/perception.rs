//! Minimal NPC perception math.
//!
//! Ported from `NPC/.../util/NPCPhysicsMath.isInViewCone`. Range checks,
//! occlusion and blackboard sensor state are later T58/T59 work.

use hytale_world::collision::Vec3;

/// Java `NPCPhysicsMath.EPSILON_LENGTH`.
pub const EPSILON_LENGTH: f64 = 1e-6;

/// Java `NPCPhysicsMath.isInViewCone(viewer, viewDirection, cosHalfAngle, object)`.
pub fn is_in_view_cone(
    viewer: Vec3,
    view_direction: Vec3,
    cos_half_angle: f32,
    object: Vec3,
) -> bool {
    is_in_view_cone_relative(view_direction, cos_half_angle, object.minus(viewer))
}

/// Java `NPCPhysicsMath.isInViewCone` with a relative object position.
pub fn is_in_view_cone_relative(
    view_direction: Vec3,
    cos_half_angle: f32,
    relative_object: Vec3,
) -> bool {
    if cos_half_angle >= 1.0 {
        return true;
    }
    let length = relative_object.length_squared().sqrt();
    if length < EPSILON_LENGTH {
        return true;
    }
    let dot = view_direction.x * relative_object.x
        + view_direction.y * relative_object.y
        + view_direction.z * relative_object.z;
    dot > f64::from(cos_half_angle) * length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_cone_uses_the_java_strict_comparison() {
        let viewer = Vec3::new(0.0, 0.0, 0.0);
        let direction = Vec3::new(1.0, 0.0, 0.0);
        assert!(is_in_view_cone(
            viewer,
            direction,
            0.707_106_77,
            Vec3::new(5.0, 0.0, 1.0)
        ));
        // The fixture's float cosine is just below cos(45°), so Java treats
        // the exact-diagonal object as inside the cone.
        assert!(is_in_view_cone(
            viewer,
            direction,
            0.707_106_77,
            Vec3::new(5.0, 0.0, 5.0)
        ));
        assert!(!is_in_view_cone(
            viewer,
            direction,
            0.707_106_77,
            Vec3::new(-5.0, 0.0, 0.0)
        ));
        assert!(is_in_view_cone(
            viewer,
            direction,
            0.5,
            Vec3::new(0.0, 0.0, 0.0)
        ));
    }
}
