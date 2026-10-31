//! Role motion-controller descriptors.
//!
//! Java `MotionControllerList` entries select the movement model (`Walk`, `Fly`
//! or `Dive`) and its physical limits. The ECS actor schedule asks this
//! descriptor for speed/acceleration/vertical limits; the path follower supplies
//! waypoints for ground movement and the fly/dive controllers move in 3D.

use serde_json::Value;

use crate::role::ResolvedRole;

/// Movement controller kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionMode {
    /// Ground walking.
    #[default]
    Walk,
    /// Flying.
    Fly,
    /// Swimming/diving.
    Dive,
}

/// One resolved motion controller.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionController {
    /// Controller kind.
    pub mode: MotionMode,
    /// Horizontal acceleration.
    pub acceleration: f64,
    /// Horizontal deceleration.
    pub deceleration: f64,
    /// Gravity.
    pub gravity: f64,
    /// Maximum drop height.
    pub max_drop_height: f64,
    /// Maximum fall speed.
    pub max_fall_speed: f64,
    /// Maximum horizontal/walk speed.
    pub max_speed: f64,
    /// Minimum air speed.
    pub min_air_speed: f64,
    /// Maximum climb speed.
    pub max_climb_speed: f64,
    /// Maximum sink speed.
    pub max_sink_speed: f64,
    /// Maximum height over ground (fly).
    pub max_height_over_ground: f64,
    /// Minimum height over ground (fly).
    pub min_height_over_ground: f64,
    /// Minimum dive depth.
    pub min_dive_depth: f64,
    /// Maximum dive depth.
    pub max_dive_depth: f64,
    /// Dive/swim depth.
    pub swim_depth: f64,
    /// Dive sink ratio.
    pub sink_ratio: f64,
    /// Maximum turn speed.
    pub max_turn_speed: f64,
    /// Maximum body rotation speed.
    pub max_rotation_speed: f64,
    /// Maximum swim speed.
    pub max_swim_speed: f64,
    /// Maximum dive speed.
    pub max_dive_speed: f64,
    /// Speed fraction that starts running.
    pub run_threshold: f64,
    /// Desired altitude weight (fly).
    pub desired_altitude_weight: f64,
}

impl Default for MotionController {
    fn default() -> Self {
        Self {
            mode: MotionMode::Walk,
            acceleration: 10.0,
            deceleration: 10.0,
            gravity: 10.0,
            max_drop_height: 5.0,
            max_fall_speed: 8.0,
            max_speed: 1.0,
            min_air_speed: 0.0,
            max_climb_speed: 1.0,
            max_sink_speed: 1.0,
            max_height_over_ground: 64.0,
            min_height_over_ground: 3.0,
            min_dive_depth: 0.0,
            max_dive_depth: 100.0,
            swim_depth: 0.6,
            sink_ratio: 0.5,
            max_turn_speed: 3600.0,
            max_rotation_speed: 360.0,
            max_swim_speed: 1.0,
            max_dive_speed: 1.0,
            run_threshold: 0.3,
            desired_altitude_weight: 1.0,
        }
    }
}

impl MotionController {
    /// Resolve the first `MotionControllerList` entry, falling back to the
    /// role's `MaxSpeed` and a walk controller.
    pub fn from_role(role: &ResolvedRole) -> Self {
        let Some(value) = role.motion_controllers().first() else {
            return Self {
                max_speed: role
                    .raw
                    .get("MaxSpeed")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0),
                ..Self::default()
            };
        };
        let mode = match value.get("Type").and_then(Value::as_str) {
            Some("Fly") => MotionMode::Fly,
            Some("Dive") => MotionMode::Dive,
            _ => MotionMode::Walk,
        };
        let number = |name: &str| resolve_number(value.get(name), role);
        let role_speed = role.raw.get("MaxSpeed").and_then(Value::as_f64);
        let mut controller = Self {
            mode,
            acceleration: number("Acceleration").unwrap_or(10.0),
            deceleration: number("Deceleration").unwrap_or(10.0),
            gravity: number("Gravity").unwrap_or(10.0),
            max_drop_height: number("MaxDropHeight").unwrap_or(5.0),
            max_fall_speed: number("MaxFallSpeed").unwrap_or(8.0),
            max_speed: number("MaxWalkSpeed")
                .or_else(|| number("MaxHorizontalSpeed"))
                .or(role_speed)
                .unwrap_or(1.0),
            min_air_speed: number("MinAirSpeed").unwrap_or(0.0),
            max_climb_speed: number("MaxClimbSpeed").unwrap_or(1.0),
            max_sink_speed: number("MaxSinkSpeed").unwrap_or(1.0),
            max_height_over_ground: number("MaxHeightOverGround").unwrap_or(64.0),
            min_height_over_ground: number("MinHeightOverGround").unwrap_or(3.0),
            min_dive_depth: number("MinDiveDepth").unwrap_or(0.0),
            max_dive_depth: number("MaxDiveDepth").unwrap_or(100.0),
            swim_depth: number("SwimDepth").unwrap_or(0.6),
            sink_ratio: number("SinkRatio").unwrap_or(0.5),
            max_turn_speed: number("MaxTurnSpeed").unwrap_or(3600.0),
            max_rotation_speed: number("MaxRotationSpeed").unwrap_or(360.0),
            max_swim_speed: number("MaxSwimSpeed").unwrap_or(1.0),
            max_dive_speed: number("MaxDiveSpeed").unwrap_or(1.0),
            run_threshold: number("RunThreshold").unwrap_or(0.3),
            desired_altitude_weight: number("DesiredAltitudeWeight").unwrap_or(1.0),
        };
        if mode == MotionMode::Dive {
            controller.max_speed = controller.max_swim_speed;
        }
        controller
    }

    /// Speed used for one chase step, scaled by the role's chase fraction.
    pub fn chase_speed(&self, chase_relative_speed: f64) -> f64 {
        let speed = match self.mode {
            MotionMode::Walk | MotionMode::Fly => self.max_speed,
            MotionMode::Dive => self
                .max_swim_speed
                .max(self.max_dive_speed * self.sink_ratio),
        };
        (speed * chase_relative_speed).max(0.0)
    }

    /// Java controller type name.
    pub fn as_str(&self) -> &'static str {
        match self.mode {
            MotionMode::Walk => "Walk",
            MotionMode::Fly => "Fly",
            MotionMode::Dive => "Dive",
        }
    }

    /// Whether this controller moves along the vertical axis.
    pub fn moves_vertically(&self) -> bool {
        matches!(self.mode, MotionMode::Fly | MotionMode::Dive)
    }

    /// Vertical speed cap in one direction (positive = climb).
    pub fn vertical_speed(&self, positive: bool) -> f64 {
        match self.mode {
            MotionMode::Fly if positive => self.max_climb_speed,
            MotionMode::Fly => self.max_sink_speed,
            MotionMode::Dive if positive => self.max_climb_speed.max(self.max_swim_speed),
            MotionMode::Dive => self.max_dive_speed,
            MotionMode::Walk => 0.0,
        }
    }

    /// Clamp the yaw change to `MaxTurnSpeed` (degrees per second).
    pub fn turn_towards(&self, current: f32, desired: f32, dt: f64) -> f32 {
        let max_step = (self.max_turn_speed.max(0.0) * dt.max(0.0)).to_radians() as f32;
        if max_step <= 0.0 {
            return current;
        }
        let mut delta = (desired - current).rem_euclid(std::f32::consts::TAU);
        if delta > std::f32::consts::PI {
            delta -= std::f32::consts::TAU;
        }
        current + delta.clamp(-max_step, max_step)
    }

    /// Clamp a desired Y to the fly controller's ground-relative altitude band.
    pub fn clamp_altitude(&self, desired_y: f64, ground_y: f64) -> f64 {
        let min = ground_y + self.min_height_over_ground;
        let max = ground_y + self.max_height_over_ground;
        desired_y.clamp(min.min(max), max.max(min))
    }

    /// Clamp a desired Y to the dive controller's depth band below the water
    /// surface (`MinDiveDepth` ..= `MaxDiveDepth`).
    pub fn clamp_dive_depth(&self, desired_y: f64, surface_y: f64) -> f64 {
        let shallow = surface_y - self.min_dive_depth;
        let deep = surface_y - self.max_dive_depth;
        desired_y.clamp(deep.min(shallow), deep.max(shallow))
    }

    /// Blend the desired altitude toward the current altitude using
    /// `DesiredAltitudeWeight`.
    pub fn blend_altitude(&self, desired_y: f64, current_y: f64) -> f64 {
        let weight = self.desired_altitude_weight.clamp(0.0, 1.0);
        current_y + (desired_y - current_y) * weight
    }
}

fn resolve_number(value: Option<&Value>, role: &ResolvedRole) -> Option<f64> {
    let value = value?;
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    let parameter = value.get("Compute").and_then(Value::as_str)?;
    role.parameter(parameter).and_then(Value::as_f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_speed_clamps_yaw_changes() {
        let controller = MotionController {
            max_turn_speed: 45.0,
            ..MotionController::default()
        };
        let quarter = std::f32::consts::FRAC_PI_2;
        let turned = controller.turn_towards(0.0, quarter, 1.0);
        assert!((turned - quarter / 2.0).abs() < 1e-4);
        let wrapped = controller.turn_towards(3.0, -3.0, 1.0);
        assert!(
            (wrapped - (3.0 + (std::f32::consts::TAU - 6.0))).abs() < 1e-4,
            "wrapped turn {wrapped}"
        );
    }

    #[test]
    fn defaults_are_walk_controller_limits() {
        let controller = MotionController::default();
        assert_eq!(controller.mode, MotionMode::Walk);
        assert_eq!(controller.chase_speed(0.5), 0.5);
        assert!(!controller.moves_vertically());
    }

    #[test]
    fn a_real_walk_role_resolves_max_speed_from_parameters() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = crate::role::RoleCatalog::load_root(&root).expect("roles");
        let role = catalog
            .resolve("Coffer_Goblin_Scrapper")
            .expect("resolve")
            .expect("role");
        let controller = MotionController::from_role(&role);
        assert_eq!(controller.mode, MotionMode::Walk);
        assert!(controller.max_speed > 0.0);
        assert!(controller.chase_speed(1.0) > 0.0);
    }

    #[test]
    fn real_fly_and_dive_roles_resolve_vertical_limits() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = crate::role::RoleCatalog::load_root(&root).expect("roles");
        let fly = catalog
            .resolve("Void_Anomaly_Crystal_Stutter")
            .expect("resolve")
            .expect("fly role");
        let controller = MotionController::from_role(&fly);
        assert_eq!(controller.mode, MotionMode::Fly);
        assert!(controller.moves_vertically());
        assert!(controller.vertical_speed(true) > 0.0);
        assert!(controller.max_height_over_ground > controller.min_height_over_ground);

        let dive = catalog
            .resolve("Template_Swimming_Aggressive")
            .expect("resolve")
            .expect("dive role");
        let controller = MotionController::from_role(&dive);
        assert_eq!(controller.mode, MotionMode::Dive);
        assert!(controller.moves_vertically());
        assert!(controller.max_dive_depth >= controller.min_dive_depth);
        assert!(controller.max_swim_speed > 0.0);
    }
}
