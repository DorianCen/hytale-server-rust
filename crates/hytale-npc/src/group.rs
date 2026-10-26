//! Group steering accumulation.
//!
//! Ported from `NPC/.../movement/GroupSteeringAccumulator.java`. The accumulator
//! sums neighbour distances/positions/velocities inside a range and view cone;
//! the role's flock weights consume the sums. The Java entity-component lookup
//! is replaced with [`GroupMember`] snapshots so the math is testable without
//! the ECS.

use hytale_world::collision::Vec3;

use crate::perception::is_in_view_cone_relative;

/// One neighbour snapshot for group steering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupMember {
    /// World position.
    pub position: Vec3,
    /// Linear velocity.
    pub velocity: Vec3,
}

impl GroupMember {
    /// Construct a member.
    pub const fn new(position: Vec3, velocity: Vec3) -> Self {
        Self { position, velocity }
    }
}

/// Java `GroupSteeringAccumulator`.
#[derive(Debug, Clone)]
pub struct GroupSteeringAccumulator {
    sum_of_velocities: Vec3,
    sum_of_distances: Vec3,
    sum_of_positions: Vec3,
    center: Vec3,
    view_direction: Vec3,
    component_selector: Vec3,
    max_range_squared: f64,
    max_distance: f64,
    view_cone_half_angle_cosine: f32,
    normalize_distances: bool,
    count: usize,
}

impl Default for GroupSteeringAccumulator {
    fn default() -> Self {
        Self {
            sum_of_velocities: Vec3::new(0.0, 0.0, 0.0),
            sum_of_distances: Vec3::new(0.0, 0.0, 0.0),
            sum_of_positions: Vec3::new(0.0, 0.0, 0.0),
            center: Vec3::new(0.0, 0.0, 0.0),
            view_direction: Vec3::new(0.0, 0.0, -1.0),
            component_selector: Vec3::new(1.0, 1.0, 1.0),
            max_range_squared: f64::MAX,
            max_distance: f64::MAX,
            view_cone_half_angle_cosine: 1.0,
            normalize_distances: false,
            count: 0,
        }
    }
}

impl GroupSteeringAccumulator {
    /// Java `begin(x, y, z, viewDirection)`.
    pub fn begin(&mut self, center: Vec3, view_direction: Vec3) {
        self.center = center;
        self.view_direction = view_direction;
        self.sum_of_distances = Vec3::new(0.0, 0.0, 0.0);
        self.sum_of_positions = Vec3::new(0.0, 0.0, 0.0);
        self.sum_of_velocities = Vec3::new(0.0, 0.0, 0.0);
        self.count = 0;
    }

    /// Java `setComponentSelector`.
    pub fn set_component_selector(&mut self, selector: Vec3) {
        self.component_selector = selector;
    }

    /// Java `setMaxRange`.
    pub fn set_max_range(&mut self, max_range: f64) {
        self.max_range_squared = max_range * max_range;
        self.max_distance = max_range;
    }

    /// Java `setNormalizeDistances`.
    pub fn set_normalize_distances(&mut self, normalize: bool) {
        self.normalize_distances = normalize;
    }

    /// Java `setViewConeHalfAngleCosine`.
    pub fn set_view_cone_half_angle_cosine(&mut self, cosine: f32) {
        self.view_cone_half_angle_cosine = cosine;
    }

    /// Java `processEntity(ref, ...)` without distance/position/velocity weights.
    pub fn process(&mut self, member: GroupMember) {
        let delta = member.position.minus(self.center);
        if self.accepts(delta) {
            self.sum_of_distances = self.sum_of_distances.plus(delta);
            self.sum_of_positions = self.sum_of_positions.plus(member.position);
            self.sum_of_velocities = self.sum_of_velocities.plus(member.velocity);
            self.count += 1;
        }
    }

    /// Java weighted `processEntity`.
    pub fn process_weighted(
        &mut self,
        member: GroupMember,
        distance_weight: f64,
        position_weight: f64,
        velocity_weight: f64,
    ) {
        let mut delta = member.position.minus(self.center);
        let mut distance_squared = dot_component(delta, self.component_selector);
        if distance_squared >= self.max_range_squared {
            return;
        }
        if !is_in_view_cone_relative(self.view_direction, self.view_cone_half_angle_cosine, delta) {
            return;
        }

        let length;
        if self.normalize_distances {
            if distance_squared < 1e-12 {
                // Java generates a random tiny direction here; a deterministic
                // +X direction keeps the accumulator reproducible.
                delta = Vec3::new(1e-6, 0.0, 0.0);
                distance_squared = 1e-12;
            }
            length = distance_squared.sqrt();
            delta = delta.scale(1.0 / length);
        } else {
            length = distance_squared.sqrt();
        }

        let falloff = (1.0 - length / self.max_distance).max(0.0);
        let distance_scale = falloff.powf(distance_weight);
        self.sum_of_distances = self.sum_of_distances.plus(mul_component(
            delta.scale(distance_scale),
            self.component_selector,
        ));
        let position_scale = falloff.powf(position_weight);
        self.sum_of_positions = self.sum_of_positions.plus(mul_component(
            member.position.scale(position_scale),
            self.component_selector,
        ));
        let velocity_scale = falloff.powf(velocity_weight);
        self.sum_of_velocities = self.sum_of_velocities.plus(mul_component(
            member.velocity.scale(velocity_scale),
            self.component_selector,
        ));
        self.count += 1;
    }

    fn accepts(&self, delta: Vec3) -> bool {
        dot_component(delta, self.component_selector) < self.max_range_squared
            && is_in_view_cone_relative(
                self.view_direction,
                self.view_cone_half_angle_cosine,
                delta,
            )
    }

    /// Java `end`.
    pub fn end(&mut self) {
        if self.count == 0 {
            return;
        }
        if self.normalize_distances {
            if self.sum_of_distances.length_squared() >= 1.0 {
                self.sum_of_distances = normalize(self.sum_of_distances);
            }
        } else {
            let scale = 1.0 / self.count as f64;
            self.sum_of_distances =
                mul_component(self.sum_of_distances.scale(scale), self.component_selector);
            self.sum_of_positions =
                mul_component(self.sum_of_positions.scale(scale), self.component_selector);
            self.sum_of_velocities =
                mul_component(self.sum_of_velocities.scale(scale), self.component_selector);
        }
    }

    /// Summed velocities.
    pub const fn sum_of_velocities(&self) -> Vec3 {
        self.sum_of_velocities
    }

    /// Summed distance vectors.
    pub const fn sum_of_distances(&self) -> Vec3 {
        self.sum_of_distances
    }

    /// Summed positions.
    pub const fn sum_of_positions(&self) -> Vec3 {
        self.sum_of_positions
    }

    /// Accepted neighbour count.
    pub const fn count(&self) -> usize {
        self.count
    }
}

fn dot_component(vector: Vec3, selector: Vec3) -> f64 {
    vector.x * vector.x * selector.x
        + vector.y * vector.y * selector.y
        + vector.z * vector.z * selector.z
}

fn mul_component(vector: Vec3, selector: Vec3) -> Vec3 {
    Vec3::new(
        vector.x * selector.x,
        vector.y * selector.y,
        vector.z * selector.z,
    )
}

fn normalize(vector: Vec3) -> Vec3 {
    let length = vector.length_squared().sqrt();
    if length <= f64::EPSILON {
        return Vec3::new(0.0, 0.0, 0.0);
    }
    vector.scale(1.0 / length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_outside_range_or_view_cone_are_ignored() {
        let mut accumulator = GroupSteeringAccumulator::default();
        accumulator.begin(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0));
        accumulator.set_max_range(10.0);
        accumulator.set_view_cone_half_angle_cosine(0.707_106_77);
        accumulator.process(GroupMember::new(
            Vec3::new(0.0, 0.0, -5.0),
            Vec3::new(1.0, 0.0, 0.0),
        ));
        accumulator.process(GroupMember::new(
            Vec3::new(0.0, 0.0, 15.0),
            Vec3::new(1.0, 0.0, 0.0),
        ));
        accumulator.process(GroupMember::new(
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(1.0, 0.0, 0.0),
        ));
        accumulator.end();
        assert_eq!(accumulator.count(), 1);
        assert_eq!(accumulator.sum_of_positions(), Vec3::new(0.0, 0.0, -5.0));
        assert_eq!(accumulator.sum_of_velocities(), Vec3::new(1.0, 0.0, 0.0));
    }

    #[test]
    fn weighted_normalized_distances_follow_the_java_falloff() {
        let mut accumulator = GroupSteeringAccumulator::default();
        accumulator.begin(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, -1.0));
        accumulator.set_max_range(10.0);
        accumulator.set_normalize_distances(true);
        accumulator.process_weighted(
            GroupMember::new(Vec3::new(0.0, 0.0, -5.0), Vec3::new(1.0, 0.0, 0.0)),
            1.0,
            1.0,
            1.0,
        );
        accumulator.end();
        assert_eq!(accumulator.count(), 1);
        let distance = accumulator.sum_of_distances();
        assert!((distance.x).abs() < 1e-12, "{distance:?}");
        assert!((distance.z + 0.5).abs() < 1e-12, "{distance:?}");
    }
}
