//! Internal `MovementConfig` assets (`Server/Entity/MovementConfig`).
//!
//! Ported from `CoreServer/.../entity/entities/player/movement/MovementConfig`.
//! Java registers the store with `IndexedLookupTableAssetMap` and sends the
//! selected config through `UpdateMovementSettings`. The Rust loader reads the
//! file-backed assets only (the Java-only `BuiltinDefault` preload is modelled
//! by [`java_default_movement`] instead). Fields that are not in the packet
//! (`FallDamagePartialMitigationPercent`, `MaxFallSpeedRollFullMitigation`) are
//! accepted but not modelled.
//!
//! A config document is usually sparse; absent fields keep the codec's Java
//! default, which is `0` for every field. The server falls back to
//! [`java_default_movement`] when the selected asset does not exist.

use hytale_protocol::generated::common::{FlyMode, MovementSettings};
use serde_json::Value;

/// Java `MovementConfig.DEFAULT_ID`.
pub const DEFAULT_MOVEMENT_CONFIG_ID: &str = "BuiltinDefault";

/// One loaded `MovementConfig` asset.
#[derive(Debug, Clone, PartialEq)]
pub struct MovementConfigAsset {
    /// Packet fields as decoded from the asset (`mass`, `dragCoefficient`,
    /// `invertedGravity` and `fly` stay at their codec defaults; the player's
    /// physics values and game mode fill those in at send time).
    pub settings: MovementSettings,
    /// Java `MovementConfig.getFly()`, unset when the document has no `Fly`.
    pub fly: Option<FlyMode>,
}

impl Default for MovementConfigAsset {
    fn default() -> Self {
        Self {
            settings: java_default_movement(),
            fly: None,
        }
    }
}

impl MovementConfigAsset {
    /// Decode one `MovementConfig` JSON document.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "MovementConfig must be an object".to_string())?;
        let mut settings = MovementSettings::default();

        macro_rules! float {
            ($field:ident, $key:literal) => {
                if let Some(value) = object.get($key) {
                    let value = value
                        .as_f64()
                        .ok_or_else(|| format!("MovementConfig.{0} must be a number", $key))?;
                    settings.$field = value as f32;
                }
            };
        }
        macro_rules! boolean {
            ($field:ident, $key:literal) => {
                if let Some(value) = object.get($key) {
                    settings.$field = value
                        .as_bool()
                        .ok_or_else(|| format!("MovementConfig.{0} must be a boolean", $key))?;
                }
            };
        }

        float!(velocity_resistance, "VelocityResistance");
        float!(jump_force, "JumpForce");
        float!(swim_jump_force, "SwimJumpForce");
        float!(jump_buffer_duration, "JumpBufferDuration");
        float!(jump_buffer_max_y_velocity, "JumpBufferMaxYVelocity");
        float!(acceleration, "Acceleration");
        float!(air_drag_min, "AirDragMin");
        float!(air_drag_max, "AirDragMax");
        float!(air_drag_min_speed, "AirDragMinSpeed");
        float!(air_drag_max_speed, "AirDragMaxSpeed");
        float!(air_friction_min, "AirFrictionMin");
        float!(air_friction_max, "AirFrictionMax");
        float!(air_friction_min_speed, "AirFrictionMinSpeed");
        float!(air_friction_max_speed, "AirFrictionMaxSpeed");
        float!(air_speed_multiplier, "AirSpeedMultiplier");
        float!(air_control_min_speed, "AirControlMinSpeed");
        float!(air_control_max_speed, "AirControlMaxSpeed");
        float!(air_control_min_multiplier, "AirControlMinMultiplier");
        float!(air_control_max_multiplier, "AirControlMaxMultiplier");
        float!(combo_air_speed_multiplier, "ComboAirSpeedMultiplier");
        float!(base_speed, "BaseSpeed");
        float!(climb_speed, "ClimbSpeed");
        float!(climb_speed_lateral, "ClimbSpeedLateral");
        float!(climb_up_sprint_speed, "ClimbUpSprintSpeed");
        float!(climb_down_sprint_speed, "ClimbDownSprintSpeed");
        float!(horizontal_fly_speed, "HorizontalFlySpeed");
        float!(vertical_fly_speed, "VerticalFlySpeed");
        float!(max_speed_multiplier, "MaxSpeedMultiplier");
        float!(min_speed_multiplier, "MinSpeedMultiplier");
        float!(wish_direction_gravity_x, "WishDirectionGravityX");
        float!(wish_direction_gravity_y, "WishDirectionGravityY");
        float!(wish_direction_weight_x, "WishDirectionWeightX");
        float!(wish_direction_weight_y, "WishDirectionWeightY");
        float!(collision_expulsion_force, "CollisionExpulsionForce");
        float!(forward_walk_speed_multiplier, "ForwardWalkSpeedMultiplier");
        float!(
            backward_walk_speed_multiplier,
            "BackwardWalkSpeedMultiplier"
        );
        float!(strafe_walk_speed_multiplier, "StrafeWalkSpeedMultiplier");
        float!(forward_run_speed_multiplier, "ForwardRunSpeedMultiplier");
        float!(backward_run_speed_multiplier, "BackwardRunSpeedMultiplier");
        float!(strafe_run_speed_multiplier, "StrafeRunSpeedMultiplier");
        float!(
            forward_crouch_speed_multiplier,
            "ForwardCrouchSpeedMultiplier"
        );
        float!(
            backward_crouch_speed_multiplier,
            "BackwardCrouchSpeedMultiplier"
        );
        float!(
            strafe_crouch_speed_multiplier,
            "StrafeCrouchSpeedMultiplier"
        );
        float!(
            forward_sprint_speed_multiplier,
            "ForwardSprintSpeedMultiplier"
        );
        float!(variable_jump_fall_force, "VariableJumpFallForce");
        float!(fall_effect_duration, "FallEffectDuration");
        float!(fall_jump_force, "FallJumpForce");
        float!(fall_momentum_loss, "FallMomentumLoss");
        float!(auto_jump_obstacle_speed_loss, "AutoJumpObstacleSpeedLoss");
        float!(
            auto_jump_obstacle_sprint_speed_loss,
            "AutoJumpObstacleSprintSpeedLoss"
        );
        float!(
            auto_jump_obstacle_effect_duration,
            "AutoJumpObstacleEffectDuration"
        );
        float!(
            auto_jump_obstacle_sprint_effect_duration,
            "AutoJumpObstacleSprintEffectDuration"
        );
        float!(auto_jump_obstacle_max_angle, "AutoJumpObstacleMaxAngle");
        boolean!(auto_jump_disable_jumping, "AutoJumpDisableJumping");
        float!(min_slide_entry_speed, "MinSlideEntrySpeed");
        float!(slide_exit_speed, "SlideExitSpeed");
        float!(min_fall_speed_to_engage_roll, "MinFallSpeedToEngageRoll");
        float!(max_fall_speed_to_engage_roll, "MaxFallSpeedToEngageRoll");
        float!(roll_start_speed_modifier, "RollStartSpeedModifier");
        float!(roll_exit_speed_modifier, "RollExitSpeedModifier");
        float!(roll_time_to_complete, "RollTimeToComplete");
        float!(max_slope_angle_degrees, "MaxSlopeAngleDegrees");
        float!(max_wall_angle_degrees, "MaxWallAngleDegrees");

        // Fields consumed by Java but not serialized into `MovementSettings`.
        for ignored in [
            "FallDamagePartialMitigationPercent",
            "MaxFallSpeedRollFullMitigation",
        ] {
            if let Some(value) = object.get(ignored) {
                if !value.is_number() {
                    return Err(format!("MovementConfig.{ignored} must be a number"));
                }
            }
        }

        let fly = match value.get("Fly").and_then(Value::as_str) {
            None => None,
            Some("Disabled") => Some(FlyMode::Disabled),
            Some("Allowed") => Some(FlyMode::Allowed),
            Some("Forced") => Some(FlyMode::Forced),
            Some(other) => return Err(format!("unknown MovementConfig.Fly `{other}`")),
        };

        Ok(Self { settings, fly })
    }
}

/// Java `MovementConfig.DEFAULT_MOVEMENT`.
///
/// This is the fallback when `GameplayConfig.Player.MovementConfig` does not
/// resolve to a loaded asset. `mass`, `dragCoefficient`, `invertedGravity` and
/// `fly` are not part of the asset.
pub fn java_default_movement() -> MovementSettings {
    MovementSettings {
        velocity_resistance: 0.242,
        jump_force: 11.8,
        swim_jump_force: 10.0,
        jump_buffer_duration: 0.3,
        jump_buffer_max_y_velocity: 3.0,
        acceleration: 0.1,
        air_drag_min: 0.96,
        air_drag_max: 0.995,
        air_drag_min_speed: 6.0,
        air_drag_max_speed: 10.0,
        air_friction_min: 0.02,
        air_friction_max: 0.045,
        air_friction_min_speed: 6.0,
        air_friction_max_speed: 10.0,
        air_speed_multiplier: 1.0,
        air_control_min_speed: 0.0,
        air_control_max_speed: 3.0,
        air_control_min_multiplier: 0.0,
        air_control_max_multiplier: 3.13,
        combo_air_speed_multiplier: 1.05,
        base_speed: 5.5,
        climb_speed: 0.035,
        climb_speed_lateral: 0.035,
        climb_up_sprint_speed: 0.5,
        climb_down_sprint_speed: 0.6,
        horizontal_fly_speed: 10.32,
        vertical_fly_speed: 10.32,
        max_speed_multiplier: 1000.0,
        min_speed_multiplier: 0.01,
        wish_direction_gravity_x: 0.5,
        wish_direction_gravity_y: 0.5,
        wish_direction_weight_x: 0.5,
        wish_direction_weight_y: 0.5,
        collision_expulsion_force: 0.04,
        forward_walk_speed_multiplier: 0.3,
        backward_walk_speed_multiplier: 0.3,
        strafe_walk_speed_multiplier: 0.3,
        forward_run_speed_multiplier: 1.0,
        backward_run_speed_multiplier: 0.65,
        strafe_run_speed_multiplier: 0.8,
        forward_crouch_speed_multiplier: 0.55,
        backward_crouch_speed_multiplier: 0.4,
        strafe_crouch_speed_multiplier: 0.45,
        forward_sprint_speed_multiplier: 1.65,
        variable_jump_fall_force: 35.0,
        fall_effect_duration: 0.6,
        fall_jump_force: 7.0,
        fall_momentum_loss: 0.1,
        auto_jump_obstacle_speed_loss: 0.95,
        auto_jump_obstacle_sprint_speed_loss: 0.75,
        auto_jump_obstacle_effect_duration: 0.2,
        auto_jump_obstacle_sprint_effect_duration: 0.1,
        auto_jump_obstacle_max_angle: 45.0,
        auto_jump_disable_jumping: true,
        min_slide_entry_speed: 8.5,
        slide_exit_speed: 2.5,
        min_fall_speed_to_engage_roll: 21.0,
        max_fall_speed_to_engage_roll: 31.0,
        roll_start_speed_modifier: 2.5,
        roll_exit_speed_modifier: 1.5,
        roll_time_to_complete: 0.9,
        max_slope_angle_degrees: 30.0,
        max_wall_angle_degrees: 15.0,
        fly: FlyMode::Disabled,
        ..MovementSettings::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn java_defaults_match_the_java_initialiser() {
        let settings = java_default_movement();
        assert_eq!(settings.jump_force, 11.8);
        assert_eq!(settings.base_speed, 5.5);
        assert_eq!(settings.climb_speed, 0.035);
        assert_eq!(settings.climb_up_sprint_speed, 0.5);
        assert_eq!(settings.max_speed_multiplier, 1000.0);
        assert_eq!(settings.forward_sprint_speed_multiplier, 1.65);
        assert_eq!(settings.max_wall_angle_degrees, 15.0);
        assert_eq!(settings.fly, FlyMode::Disabled);
    }

    #[test]
    fn sparse_documents_keep_codec_zero_defaults() {
        let asset = MovementConfigAsset::from_value(&json!({"BaseSpeed": 4.0, "Fly": "Allowed"}))
            .expect("parse");
        assert_eq!(asset.settings.base_speed, 4.0);
        assert_eq!(asset.settings.jump_force, 0.0);
        assert_eq!(asset.fly, Some(FlyMode::Allowed));
    }

    #[test]
    fn non_numeric_fields_are_rejected() {
        let error = MovementConfigAsset::from_value(&json!({"JumpForce": "high"}))
            .expect_err("must reject a string");
        assert!(error.contains("JumpForce"), "{error}");
    }
}
