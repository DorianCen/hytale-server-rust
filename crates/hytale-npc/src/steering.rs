//! Java `Steering` output and the deterministic steering forces.
//!
//! Ported from `NPC/.../movement/Steering.java` and
//! `movement/steeringforces/{SteeringForcePursue,SteeringForceEvade,SteeringForceRotate}.java`.
//! The random wander force, collision avoidance and the group forces are later
//! T58/T59 work.

use hytale_math::trig_math_util;
use hytale_world::collision::Vec3;

/// The desired direction and rotation produced by a steering force.
///
/// The translation is a direction vector with a maximum length of one; a
/// length of one means 100% of the maximum speed.
#[derive(Debug, Clone, PartialEq)]
pub struct Steering {
    translation: Vec3,
    has_translation: bool,
    max_distance: f64,
    yaw: f32,
    has_yaw: bool,
    pitch: f32,
    has_pitch: bool,
    roll: f32,
    has_roll: bool,
}

impl Default for Steering {
    fn default() -> Self {
        Self {
            translation: Vec3::new(0.0, 0.0, 0.0),
            has_translation: false,
            max_distance: f64::MAX,
            yaw: 0.0,
            has_yaw: false,
            pitch: 0.0,
            has_pitch: false,
            roll: 0.0,
            has_roll: false,
        }
    }
}

impl Steering {
    /// Clear translation and rotation.
    pub fn clear(&mut self) -> &mut Self {
        self.clear_translation();
        self.clear_rotation()
    }

    /// Java `clearTranslation`.
    pub fn clear_translation(&mut self) -> &mut Self {
        self.translation = Vec3::new(0.0, 0.0, 0.0);
        self.has_translation = false;
        self.max_distance = f64::MAX;
        self
    }

    /// Java `clearRotation`.
    pub fn clear_rotation(&mut self) -> &mut Self {
        self.yaw = 0.0;
        self.has_yaw = false;
        self.pitch = 0.0;
        self.has_pitch = false;
        self.roll = 0.0;
        self.has_roll = false;
        self
    }

    /// Current translation.
    pub const fn translation(&self) -> Vec3 {
        self.translation
    }

    /// Mutable translation; marks the translation present.
    pub fn translation_mut(&mut self) -> &mut Vec3 {
        self.has_translation = true;
        &mut self.translation
    }

    /// Whether a translation is present.
    pub const fn has_translation(&self) -> bool {
        self.has_translation
    }

    /// Replace the translation with a vector.
    pub fn set_translation(&mut self, translation: Vec3) -> &mut Self {
        self.has_translation = true;
        self.translation = translation;
        self
    }

    /// Replace the translation with components.
    pub fn set_translation_xyz(&mut self, x: f64, y: f64, z: f64) -> &mut Self {
        self.set_translation(Vec3::new(x, y, z))
    }

    /// Java `setTranslationRelativeSpeed`.
    pub fn set_translation_relative_speed(&mut self, relative_speed: f64) -> &mut Self {
        if let Some(normalized) = normalize(self.translation, relative_speed) {
            self.translation = normalized;
        }
        self
    }

    /// Java `scaleTranslation`.
    pub fn scale_translation(&mut self, factor: f64) -> &mut Self {
        self.translation = self.translation.scale(factor);
        self
    }

    /// Java `setMaxDistance`.
    pub fn set_max_distance(&mut self, max_distance: f64) -> &mut Self {
        self.max_distance = max_distance;
        self
    }

    /// Java `getMaxDistance`.
    pub const fn max_distance(&self) -> f64 {
        self.max_distance
    }

    /// Java `setYaw`.
    pub fn set_yaw(&mut self, yaw: f32) -> &mut Self {
        self.yaw = yaw;
        self.has_yaw = true;
        self
    }

    /// Current yaw.
    pub const fn yaw(&self) -> f32 {
        self.yaw
    }

    /// Whether yaw is present.
    pub const fn has_yaw(&self) -> bool {
        self.has_yaw
    }

    /// Current pitch.
    pub const fn pitch(&self) -> f32 {
        self.pitch
    }

    /// Whether pitch is present.
    pub const fn has_pitch(&self) -> bool {
        self.has_pitch
    }

    /// Current roll.
    pub const fn roll(&self) -> f32 {
        self.roll
    }

    /// Whether roll is present.
    pub const fn has_roll(&self) -> bool {
        self.has_roll
    }
}

/// Java `SteeringForcePursue`.
#[derive(Debug, Clone, PartialEq)]
pub struct SteeringForcePursue {
    self_position: Vec3,
    target_position: Vec3,
    component_selector: Vec3,
    stop_distance: f64,
    slowdown_distance: f64,
    falloff: f64,
}

impl Default for SteeringForcePursue {
    fn default() -> Self {
        Self::new(20.0, 25.0)
    }
}

impl SteeringForcePursue {
    /// Java `new SteeringForcePursue(stopDistance, slowdownDistance)`.
    pub fn new(stop_distance: f64, slowdown_distance: f64) -> Self {
        let mut force = Self {
            self_position: Vec3::new(0.0, 0.0, 0.0),
            target_position: Vec3::new(0.0, 0.0, 0.0),
            component_selector: Vec3::new(1.0, 1.0, 1.0),
            stop_distance: 0.0,
            slowdown_distance: 0.0,
            falloff: 3.0,
        };
        force.set_distances(slowdown_distance, stop_distance);
        force
    }

    /// Java `setPositions`.
    pub fn set_positions(&mut self, self_position: Vec3, target_position: Vec3) {
        self.self_position = self_position;
        self.target_position = target_position;
    }

    /// Java `setComponentSelector`.
    pub fn set_component_selector(&mut self, component_selector: Vec3) {
        self.component_selector = component_selector;
    }

    /// Java `setDistances(slowdown, stop)`.
    pub fn set_distances(&mut self, slowdown_distance: f64, stop_distance: f64) {
        self.stop_distance = stop_distance;
        self.slowdown_distance = slowdown_distance;
    }

    /// Java `setFalloff`.
    pub fn set_falloff(&mut self, falloff: f64) {
        self.falloff = falloff;
    }

    /// Java `compute`.
    pub fn compute(&self, output: &mut Steering) -> bool {
        let self_position = mul_component(self.self_position, self.component_selector);
        let target_position = mul_component(self.target_position, self.component_selector);
        output.set_translation(target_position);
        let translation = *output.translation_mut();
        let translation = translation.minus(self_position);
        output.set_translation(translation);

        let distance_squared = translation.length_squared();
        if distance_squared <= self.stop_distance * self.stop_distance {
            output.clear();
            return false;
        }
        let distance = distance_squared.sqrt();
        if distance_squared >= self.slowdown_distance * self.slowdown_distance {
            if let Some(normalized) = normalize(translation, 1.0) {
                output.set_translation(normalized);
            }
            output.clear_rotation();
            return true;
        }

        let distance_delta = self.slowdown_distance - self.stop_distance;
        let scale = if distance_delta.abs() <= f64::EPSILON {
            1.0
        } else {
            ((distance - self.stop_distance) / distance_delta).powf(1.0 / self.falloff)
        };
        if let Some(normalized) = normalize(translation, scale) {
            output.set_translation(normalized);
        }
        output.clear_rotation();
        true
    }
}

/// Java `SteeringForceEvade`.
#[derive(Debug, Clone, PartialEq)]
pub struct SteeringForceEvade {
    self_position: Vec3,
    target_position: Vec3,
    component_selector: Vec3,
    slowdown_distance: f64,
    stop_distance: f64,
    falloff: f64,
    direction_hint: f32,
    adhere_to_direction_hint: bool,
}

impl Default for SteeringForceEvade {
    fn default() -> Self {
        Self::new(20.0, 25.0)
    }
}

impl SteeringForceEvade {
    /// Java `new SteeringForceEvade(slowdownDistance, stopDistance)`.
    pub fn new(slowdown_distance: f64, stop_distance: f64) -> Self {
        let mut force = Self {
            self_position: Vec3::new(0.0, 0.0, 0.0),
            target_position: Vec3::new(0.0, 0.0, 0.0),
            component_selector: Vec3::new(1.0, 1.0, 1.0),
            slowdown_distance: 0.0,
            stop_distance: 0.0,
            falloff: 3.0,
            direction_hint: 0.0,
            adhere_to_direction_hint: false,
        };
        force.set_distances(slowdown_distance, stop_distance);
        force
    }

    /// Java `setPositions`.
    pub fn set_positions(&mut self, self_position: Vec3, target_position: Vec3) {
        self.self_position = self_position;
        self.target_position = target_position;
    }

    /// Java `setComponentSelector`.
    pub fn set_component_selector(&mut self, component_selector: Vec3) {
        self.component_selector = component_selector;
    }

    /// Java `setDistances(min, max)` — note the Java argument names are
    /// `slowdownDistance, stopDistance`.
    pub fn set_distances(&mut self, slowdown_distance: f64, stop_distance: f64) {
        self.slowdown_distance = slowdown_distance;
        self.stop_distance = stop_distance;
    }

    /// Java `setFalloff`.
    pub fn set_falloff(&mut self, falloff: f64) {
        self.falloff = falloff;
    }

    /// Java `setDirectionHint`.
    pub fn set_direction_hint(&mut self, direction_hint: f32) {
        self.direction_hint = direction_hint;
    }

    /// Java `setAdhereToDirectionHint`.
    pub fn set_adhere_to_direction_hint(&mut self, adhere: bool) {
        self.adhere_to_direction_hint = adhere;
    }

    /// Java `compute`.
    pub fn compute(&self, output: &mut Steering) -> bool {
        let self_position = mul_component(self.self_position, self.component_selector);
        let target_position = mul_component(self.target_position, self.component_selector);
        output.set_translation(self_position);
        let translation = *output.translation_mut();
        output.set_translation(translation.minus(target_position));
        let distance_squared = output.translation().length_squared();

        if distance_squared >= self.stop_distance * self.stop_distance {
            output.clear();
            return false;
        }
        output.clear_rotation();

        if distance_squared < 1e-6 {
            output.set_translation_xyz(
                f64::from(heading_x(self.direction_hint)),
                0.0,
                f64::from(heading_z(self.direction_hint)),
            );
            return true;
        }
        if self.adhere_to_direction_hint {
            output.set_translation_xyz(
                f64::from(heading_x(self.direction_hint)),
                0.0,
                f64::from(heading_z(self.direction_hint)),
            );
        }

        let distance_delta = self.stop_distance - self.slowdown_distance;
        if distance_squared < self.slowdown_distance * self.slowdown_distance
            || distance_delta.abs() <= f64::EPSILON
        {
            if let Some(normalized) = normalize(output.translation(), 1.0) {
                output.set_translation(normalized);
            }
            return true;
        }

        let distance = distance_squared.sqrt();
        let scale = ((self.stop_distance - distance) / distance_delta).powf(1.0 / self.falloff);
        if let Some(normalized) = normalize(output.translation(), scale) {
            output.set_translation(normalized);
        }
        true
    }
}

/// Java `SteeringForceRotate`.
#[derive(Debug, Clone, PartialEq)]
pub struct SteeringForceRotate {
    desired_heading: f32,
    heading: f32,
    tolerance: f64,
}

impl Default for SteeringForceRotate {
    fn default() -> Self {
        Self {
            desired_heading: 0.0,
            heading: 0.0,
            tolerance: f64::from(trig_math_util::DEG_TO_RAD) * 3.0,
        }
    }
}

impl SteeringForceRotate {
    /// Java `setDesiredHeading`.
    pub fn set_desired_heading(&mut self, desired_heading: f32) {
        self.desired_heading = desired_heading;
    }

    /// Java `setHeading`.
    pub fn set_heading(&mut self, heading: f32) {
        self.heading = heading;
    }

    /// Java `setTolerance`.
    pub fn set_tolerance(&mut self, tolerance: f64) {
        self.tolerance = tolerance;
    }

    /// Java `compute`.
    pub fn compute(&self, output: &mut Steering) -> bool {
        output.clear();
        let turn_angle = turn_angle(self.desired_heading, self.heading);
        if f64::from(turn_angle).abs() >= self.tolerance {
            output.set_yaw(self.desired_heading);
            return true;
        }
        false
    }
}

fn mul_component(vector: Vec3, selector: Vec3) -> Vec3 {
    Vec3::new(
        vector.x * selector.x,
        vector.y * selector.y,
        vector.z * selector.z,
    )
}

fn normalize(vector: Vec3, scale: f64) -> Option<Vec3> {
    let length = vector.length_squared().sqrt();
    if length <= f64::EPSILON {
        return None;
    }
    Some(vector.scale(scale / length))
}

/// Java `PhysicsMath.headingX`.
pub fn heading_x(heading: f32) -> f32 {
    -trig_math_util::sin(heading)
}

/// Java `PhysicsMath.headingZ`.
pub fn heading_z(heading: f32) -> f32 {
    -trig_math_util::cos(heading)
}

/// Java `PhysicsMath.normalizeAngle`.
pub fn normalize_angle(radians: f32) -> f32 {
    let mut angle = radians % trig_math_util::PI2;
    if angle < 0.0 {
        angle += trig_math_util::PI2;
    }
    angle
}

/// Java `NPCPhysicsMath.turnAngle`.
pub fn turn_angle(from: f32, to: f32) -> f32 {
    let mut delta = normalize_angle(to) - normalize_angle(from);
    if delta < -trig_math_util::PI {
        delta += trig_math_util::PI2;
    } else if delta > trig_math_util::PI {
        delta -= trig_math_util::PI2;
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pursue_stops_and_slows_at_the_java_distances() {
        let mut force = SteeringForcePursue::new(5.0, 20.0);
        let mut steering = Steering::default();

        force.set_positions(Vec3::new(0.0, 0.0, 0.0), Vec3::new(30.0, 0.0, 0.0));
        assert!(force.compute(&mut steering));
        assert!((steering.translation().x - 1.0).abs() < 1e-12);

        force.set_positions(Vec3::new(0.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0));
        assert!(!force.compute(&mut steering));
        assert!(!steering.has_translation());

        force.set_positions(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0));
        assert!(force.compute(&mut steering));
        let expected = ((10.0 - 5.0) / 15.0f64).powf(1.0 / 3.0);
        assert!((steering.translation().x - expected).abs() < 1e-12);
    }

    #[test]
    fn rotate_uses_the_java_turn_angle() {
        let mut force = SteeringForceRotate::default();
        force.set_desired_heading(0.5);
        force.set_heading(0.1);
        let mut steering = Steering::default();
        assert!(force.compute(&mut steering));
        assert_eq!(steering.yaw(), 0.5);

        force.set_desired_heading(0.01);
        force.set_heading(0.0);
        assert!(!force.compute(&mut steering));
        assert!(!steering.has_yaw());
    }
}
