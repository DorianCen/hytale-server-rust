//! Waypoint following.
//!
//! Ported from `NPC/.../navigation/PathFollower.java` without the motion
//! controller smoothing probe: waypoint advancement, the passed-waypoint
//! projection, path rejection and final-stage freezing are deterministic and
//! covered by tests. `MotionController`-backed `smoothPath`/`canMoveTo` remain
//! later T58 work.

use hytale_world::collision::Vec3;

use crate::steering::Steering;

/// Java `CollisionModuleConfig.MOVEMENT_THRESHOLD_SQUARED`.
pub const MOVEMENT_THRESHOLD_SQUARED: f64 = 1e-5 * 1e-5;

/// Java `PathFollower`.
#[derive(Debug, Clone, PartialEq)]
pub struct PathFollower {
    waypoints: Vec<Vec3>,
    index: usize,
    last_waypoint_position: Vec3,
    current_waypoint_distance_squared: f64,
    frozen: bool,
    relative_speed: f64,
    relative_speed_waypoint: f64,
    waypoint_radius: f64,
    waypoint_radius_squared: f64,
    rejection_weight: f64,
    direction: Vec3,
    component_selector: Vec3,
}

impl Default for PathFollower {
    fn default() -> Self {
        Self {
            waypoints: Vec::new(),
            index: 0,
            last_waypoint_position: Vec3::new(0.0, 0.0, 0.0),
            current_waypoint_distance_squared: f64::MAX,
            frozen: false,
            relative_speed: 1.0,
            relative_speed_waypoint: 1.0,
            waypoint_radius: 0.0,
            waypoint_radius_squared: 0.0,
            rejection_weight: 3.0,
            direction: Vec3::new(0.0, 0.0, 0.0),
            component_selector: Vec3::new(1.0, 1.0, 1.0),
        }
    }
}

impl PathFollower {
    /// Java `setPath(firstWaypoint, startPosition)`.
    pub fn set_path(&mut self, waypoints: Vec<Vec3>, start_position: Vec3) {
        self.waypoints = waypoints;
        self.index = 0;
        self.last_waypoint_position = start_position;
        self.current_waypoint_distance_squared = f64::MAX;
        self.frozen = false;
    }

    /// Java `clearPath`.
    pub fn clear_path(&mut self) {
        self.waypoints.clear();
        self.index = 0;
        self.frozen = false;
    }

    /// Java `getCurrentWaypointPosition`.
    pub fn current_waypoint_position(&self) -> Option<Vec3> {
        self.waypoints.get(self.index).copied()
    }

    /// Java `getNextWaypointPosition`.
    pub fn next_waypoint_position(&self) -> Option<Vec3> {
        self.waypoints.get(self.index + 1).copied()
    }

    /// Number of waypoints in the current path.
    pub fn len(&self) -> usize {
        self.waypoints.len()
    }

    /// Whether no path is set.
    pub fn is_empty(&self) -> bool {
        self.waypoints.is_empty()
    }

    /// The current waypoint index.
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Whether the current waypoint is the last one.
    pub fn path_in_final_stage(&self) -> bool {
        self.waypoints.is_empty() || self.index + 1 >= self.waypoints.len()
    }

    /// Drop the remaining waypoints after the current one.
    pub fn freeze_waypoint(&mut self) {
        if self.waypoints.is_empty() {
            return;
        }
        let keep = (self.index + 1).min(self.waypoints.len());
        self.waypoints.truncate(keep);
        self.frozen = true;
    }

    /// Whether the current waypoint was frozen.
    pub const fn is_waypoint_frozen(&self) -> bool {
        self.frozen
    }

    /// Java `setRelativeSpeed`.
    pub fn set_relative_speed(&mut self, relative_speed: f64) {
        self.relative_speed = relative_speed;
    }

    /// Java `setRelativeSpeedWaypoint`.
    pub fn set_relative_speed_waypoint(&mut self, relative_speed_waypoint: f64) {
        self.relative_speed_waypoint = relative_speed_waypoint;
    }

    /// Java `setWaypointRadius`.
    pub fn set_waypoint_radius(&mut self, waypoint_radius: f64) {
        self.waypoint_radius = waypoint_radius;
        self.waypoint_radius_squared = waypoint_radius * waypoint_radius;
    }

    /// Java `setRejectionWeight`.
    pub fn set_rejection_weight(&mut self, rejection_weight: f64) {
        self.rejection_weight = rejection_weight;
    }

    /// Java `setComponentSelector`.
    pub fn set_component_selector(&mut self, component_selector: Vec3) {
        self.component_selector = component_selector;
    }

    /// Java `executePath`.
    pub fn execute_path(&mut self, current_position: Vec3, desired_steering: &mut Steering) {
        let Some(target) = self.current_waypoint_position() else {
            return;
        };
        let temp = target.minus(current_position);
        let length = temp.length_squared().sqrt();
        desired_steering.set_max_distance(length);
        if length > self.waypoint_radius {
            self.direction = temp;
            let rejection = self.compute_rejection(current_position, target);
            self.direction = self.direction.minus(rejection);
            if length > f64::EPSILON {
                desired_steering
                    .set_translation(self.direction.scale(self.relative_speed / length));
            }
            return;
        }

        if length > 0.1 {
            self.direction = temp;
        }
        if length > f64::EPSILON {
            desired_steering
                .set_translation(self.direction.scale(self.relative_speed_waypoint / length));
        }
    }

    /// Java `computeRejection`.
    pub fn compute_rejection(&self, current_position: Vec3, target: Vec3) -> Vec3 {
        let path = mul_component(
            target.minus(self.last_waypoint_position),
            self.component_selector,
        );
        let current = mul_component(
            current_position.minus(self.last_waypoint_position),
            self.component_selector,
        );
        let dot_dd = path.length_squared();
        if dot_dd <= f64::EPSILON {
            return Vec3::new(0.0, 0.0, 0.0);
        }
        let dot_dp = path.x * current.x + path.y * current.y + path.z * current.z;
        let projection = path.scale(dot_dp / dot_dd);
        mul_component(
            current.minus(projection),
            Vec3::new(
                self.rejection_weight,
                self.rejection_weight,
                self.rejection_weight,
            ),
        )
    }

    /// Java `updateCurrentTarget` without the motion-controller projection.
    ///
    /// Returns `false` when the path has no waypoint left.
    pub fn update_current_target(&mut self, entity_position: Vec3) -> bool {
        let Some(waypoint_position) = self.current_waypoint_position() else {
            return false;
        };
        let distance_squared = waypoint_position.minus(entity_position).length_squared();
        let reached = distance_squared <= MOVEMENT_THRESHOLD_SQUARED
            || (distance_squared < 0.01
                && distance_squared > self.current_waypoint_distance_squared)
            || projection_length(
                self.last_waypoint_position,
                waypoint_position,
                entity_position,
                self.component_selector,
            ) < 0.0;
        self.current_waypoint_distance_squared = distance_squared;

        if !reached {
            return true;
        }
        self.last_waypoint_position = waypoint_position;
        self.index += 1;
        if self.index >= self.waypoints.len() {
            self.index = self.waypoints.len();
            self.frozen = false;
            return false;
        }
        self.current_waypoint_distance_squared = f64::MAX;
        true
    }

    /// Whether the current waypoint is within the configured radius.
    pub fn current_waypoint_within_radius(&self, entity_position: Vec3) -> bool {
        self.current_waypoint_position().is_some_and(|waypoint| {
            waypoint.minus(entity_position).length_squared() <= self.waypoint_radius_squared
        })
    }
}

fn mul_component(vector: Vec3, selector: Vec3) -> Vec3 {
    Vec3::new(
        vector.x * selector.x,
        vector.y * selector.y,
        vector.z * selector.z,
    )
}

fn projection_length(base: Vec3, point_on_ray: Vec3, point: Vec3, selector: Vec3) -> f64 {
    let direction = mul_component(point_on_ray.minus(base), selector);
    let offset = mul_component(point.minus(base), selector);
    direction.x * offset.x + direction.y * offset.y + direction.z * offset.z
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waypoint_advancement_uses_distance_and_the_passed_projection() {
        let mut follower = PathFollower::default();
        follower.set_path(
            vec![
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(2.0, 0.0, 0.0),
                Vec3::new(3.0, 0.0, 0.0),
            ],
            Vec3::new(0.0, 0.0, 0.0),
        );
        assert!(follower.update_current_target(Vec3::new(0.5, 0.0, 0.0)));
        assert_eq!(follower.index(), 0);
        assert!(follower.update_current_target(Vec3::new(1.0, 0.0, 0.0)));
        assert_eq!(follower.index(), 1);
        assert!(follower.update_current_target(Vec3::new(2.0, 0.0, 0.0)));
        assert_eq!(follower.index(), 2);
        assert!(!follower.update_current_target(Vec3::new(3.0, 0.0, 0.0)));
        assert_eq!(follower.index(), 3);
        assert!(follower.path_in_final_stage());
    }

    #[test]
    fn execute_path_scales_by_relative_speed_and_rejects_the_previous_segment() {
        let mut follower = PathFollower::default();
        follower.set_rejection_weight(0.0);
        follower.set_relative_speed(2.0);
        follower.set_path(vec![Vec3::new(0.0, 0.0, 10.0)], Vec3::new(0.0, 0.0, 0.0));
        let mut steering = Steering::default();
        follower.execute_path(Vec3::new(0.0, 0.0, 0.0), &mut steering);
        assert!((steering.translation().z - 2.0).abs() < 1e-12);
        assert!((steering.max_distance() - 10.0).abs() < 1e-12);
        assert!((steering.translation().x).abs() < 1e-12);
    }
}
