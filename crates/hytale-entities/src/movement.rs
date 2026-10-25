//! Player collision-authority and kinematic movement.
//!
//! Ported behavior from `CoreServer/.../entity/entities/player/movement`,
//! `PlayerInput`, `PlayerProcessMovementSystem` and
//! `modules/physics/util/PhysicsConstants`.
//!
//! Java keeps walking client-authoritative: `PlayerInput.AbsoluteMovement`
//! calls `Player.moveTo`, and the server only rejects non-finite values and
//! processes collision triggers afterwards. The same model is used here for
//! network input:
//!
//! - [`resolve_target`] turns a client target into a collision-safe position
//!   (Y → X → Z sweep, one-block step-up, exact passthrough when free);
//! - [`MovementBody`] is a deterministic integrator with Java gravity, jump,
//!   fly and climb constants for server-side simulation and NPC work.
//!
//! The collision query lives in [`hytale_world::collision`]; this module does
//! not depend on assets or protocol packet types.

use hytale_protocol::generated::common::MovementSettings;
use hytale_world::collision::{aabb_has_collision, Aabb, CollisionWorld, Vec3};

/// Java `PhysicsConstants.GRAVITY_ACCELERATION`.
pub const GRAVITY_ACCELERATION: f64 = 32.0;
/// Java `PlayerProcessMovementSystem` refuses offsets of 10 blocks or more.
pub const MAX_COLLISION_OFFSET: f64 = 10.0;
/// One-block auto-step used by the client's auto-jump (Java
/// `AutoJumpObstacleMaxAngle`/`MaxWallAngleDegrees` are the slope/wall limits,
/// not a height; the block grid makes one cell the meaningful ceiling).
pub const MAX_STEP_HEIGHT: f64 = 1.0;
/// Sweep bisection iterations: 48 halvings resolve a 512-block move below 1e-12.
const SWEEP_ITERATIONS: usize = 48;
/// Ground probe distance for `on_ground` after a move.
const GROUND_PROBE: f64 = 1e-4;

/// Default player model hitbox (`Server/Models/Human/Player.json`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerShape {
    /// Half of the X/Z extent (`0.325`).
    pub half_width: f64,
    /// Standing height (`1.85`).
    pub height: f64,
    /// Crouch offset (`-0.4`), applied to the height.
    pub crouch_offset: f64,
}

impl Default for PlayerShape {
    fn default() -> Self {
        Self {
            half_width: 0.325,
            height: 1.85,
            crouch_offset: -0.4,
        }
    }
}

impl PlayerShape {
    /// World-space AABB with the feet at `position`.
    pub fn aabb(&self, position: Vec3, crouching: bool) -> Aabb {
        let height = if crouching {
            self.height + self.crouch_offset
        } else {
            self.height
        };
        Aabb::player(position, self.half_width, height.max(0.1))
    }
}

/// Movement flags from `ClientMovement.movementStates`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MovementState {
    /// `walking`.
    pub walking: bool,
    /// `running`.
    pub running: bool,
    /// `sprinting`.
    pub sprinting: bool,
    /// `crouching` or `forcedCrouching`.
    pub crouching: bool,
    /// `flying`.
    pub flying: bool,
    /// `climbing`.
    pub climbing: bool,
    /// `inFluid`.
    pub in_fluid: bool,
    /// `swimming`.
    pub swimming: bool,
    /// `sliding`.
    pub sliding: bool,
    /// `rolling` (safety roll).
    pub rolling: bool,
    /// `mantling`.
    pub mantling: bool,
}

impl MovementState {
    /// Java crouch-height states: crouching, forced crouching, sliding and
    /// rolling all use the model's crouch bounding box.
    pub fn low_profile(self) -> bool {
        self.crouching || self.sliding || self.rolling
    }
}

/// One simulation step's local input.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MovementInput {
    /// Desired horizontal direction in world space (X/Z used).
    pub wish: Vec3,
    /// Desired vertical direction (`-1`, `0`, `1`) for fly/climb.
    pub vertical: f64,
    /// Jump/climb-up request.
    pub jump: bool,
    /// Crouch-slide request (sprinting + entry speed gates the transition).
    pub slide: bool,
    /// Safety-roll request on landing.
    pub roll: bool,
    /// Mantle a ledge in front of the body.
    pub mantle: bool,
    /// Body yaw in radians (game heading, `-sin`/`-cos` convention).
    pub yaw: f64,
    /// Movement flags.
    pub state: MovementState,
}

/// Server bounds applied before a target reaches the world thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementBounds {
    /// Maximum absolute X/Z coordinate.
    pub max_absolute: f64,
    /// Minimum Y (below this is out of world).
    pub min_y: f64,
    /// Maximum Y.
    pub max_y: f64,
    /// Maximum distance between two accepted positions.
    pub max_step: f64,
}

impl Default for MovementBounds {
    fn default() -> Self {
        Self {
            max_absolute: 30_000_000.0,
            min_y: -2048.0,
            max_y: 2048.0,
            max_step: 512.0,
        }
    }
}

/// Why a target position was refused before collision resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementRejection {
    /// A coordinate was NaN or infinite.
    NonFinite,
    /// A coordinate is outside the world limits.
    OutOfBounds,
    /// The move is larger than the configured per-packet distance.
    StepTooLarge,
}

impl MovementBounds {
    /// Validate one absolute/relative target.
    pub fn validate(&self, from: Vec3, to: Vec3) -> Result<(), MovementRejection> {
        if !from.is_finite() || !to.is_finite() {
            return Err(MovementRejection::NonFinite);
        }
        if to.x.abs() > self.max_absolute
            || to.z.abs() > self.max_absolute
            || !(self.min_y..=self.max_y).contains(&to.y)
        {
            return Err(MovementRejection::OutOfBounds);
        }
        if to.minus(from).length_squared() > self.max_step * self.max_step {
            return Err(MovementRejection::StepTooLarge);
        }
        Ok(())
    }
}

/// Result of resolving a client target against block collision.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetResolution {
    /// The safe position (the target itself when it was free).
    pub position: Vec3,
    /// The resolved position differs from the requested target.
    pub corrected: bool,
    /// The X move hit a block.
    pub hit_x: bool,
    /// The Y move hit a block.
    pub hit_y: bool,
    /// The Z move hit a block.
    pub hit_z: bool,
    /// The player was raised onto a one-block obstacle.
    pub stepped: bool,
    /// The start position already overlapped a block, so no correction ran.
    pub overlapping: bool,
}

/// Resolve a target position using the Java axis order (Y, X, Z) with a
/// one-block step-up attempt.
///
/// A free target is returned bit-for-bit, which is what keeps a normal client
/// loop free of rubber-banding. A blocked target is bisected onto the contact
/// face. When the start position is already overlapping (a spawn inside a
/// block), the target is accepted so the client can walk out.
pub fn resolve_target(
    world: &dyn CollisionWorld,
    from: Vec3,
    to: Vec3,
    shape: PlayerShape,
    crouching: bool,
    step_height: f64,
) -> TargetResolution {
    let mut scratch = Vec::new();
    let from_box = shape.aabb(from, crouching);

    if aabb_has_collision(world, from_box, &mut scratch) {
        return TargetResolution {
            position: to,
            corrected: false,
            hit_x: false,
            hit_y: false,
            hit_z: false,
            stepped: false,
            overlapping: true,
        };
    }

    // Normal axis order: vertical first so a fall/land resolves before the
    // horizontal component slides along the floor.
    let (after_y, hit_y) = sweep_axis(world, shape, from, 1, to.y, crouching, &mut scratch);
    let (after_x, hit_x) = sweep_axis(world, shape, after_y, 0, to.x, crouching, &mut scratch);
    let (after_z, hit_z) = sweep_axis(world, shape, after_x, 2, to.z, crouching, &mut scratch);
    let direct = TargetResolution {
        position: after_z,
        corrected: after_z != to,
        hit_x,
        hit_y,
        hit_z,
        stepped: false,
        overlapping: false,
    };

    if !(hit_x || hit_z) || step_height <= 0.0 {
        return direct;
    }

    // Auto-step: raise by at most one block, move horizontally, then settle to
    // the requested height. The final X/Z must reach the requested X/Z.
    let raised = from.with_axis(1, from.y + step_height);
    if aabb_has_collision(world, shape.aabb(raised, crouching), &mut scratch) {
        return direct;
    }
    let (step_x, _) = sweep_axis(world, shape, raised, 0, to.x, crouching, &mut scratch);
    let (step_z, _) = sweep_axis(world, shape, step_x, 2, to.z, crouching, &mut scratch);
    let (settled, _) = sweep_axis(world, shape, step_z, 1, to.y, crouching, &mut scratch);
    if (settled.x - to.x).abs() <= 1e-6 && (settled.z - to.z).abs() <= 1e-6 {
        return TargetResolution {
            position: settled,
            corrected: settled != to,
            hit_x: false,
            hit_y: false,
            hit_z: false,
            stepped: true,
            overlapping: false,
        };
    }
    direct
}

fn sweep_axis(
    world: &dyn CollisionWorld,
    shape: PlayerShape,
    start: Vec3,
    axis: usize,
    target: f64,
    crouching: bool,
    scratch: &mut Vec<Aabb>,
) -> (Vec3, bool) {
    let current = start.axis(axis);
    let delta = target - current;
    if delta.abs() <= f64::EPSILON {
        return (start, false);
    }

    // Continuous check along the path. The player box is larger than the
    // sampling interval, so a thin block cannot be tunnelled through; once a
    // colliding sample is found, bisect between the last free and that sample.
    const SAMPLE: f64 = 0.25;
    let steps = (delta.abs() / SAMPLE).ceil().max(1.0) as usize;
    let mut previous = 0.0;
    for index in 1..=steps {
        let travelled = delta * index as f64 / steps as f64;
        let probe = start.with_axis(axis, current + travelled);
        if aabb_has_collision(world, shape.aabb(probe, crouching), scratch) {
            let mut good = previous;
            let mut bad = travelled;
            for _ in 0..SWEEP_ITERATIONS {
                let mid = (good + bad) * 0.5;
                let probe = start.with_axis(axis, current + mid);
                if aabb_has_collision(world, shape.aabb(probe, crouching), scratch) {
                    bad = mid;
                } else {
                    good = mid;
                }
            }
            return (start.with_axis(axis, current + good), true);
        }
        previous = travelled;
    }
    (start.with_axis(axis, target), false)
}

/// Runtime tuning used by [`MovementBody`].
///
/// Most values come straight from the selected `MovementConfig`; gravity and
/// the step height come from the Java physics constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementTuning {
    /// Base speed (`BaseSpeed`).
    pub base_speed: f64,
    /// Jump force (`JumpForce`).
    pub jump_force: f64,
    /// Swim jump force (`SwimJumpForce`).
    pub swim_jump_force: f64,
    /// Horizontal fly speed (`HorizontalFlySpeed`).
    pub horizontal_fly_speed: f64,
    /// Vertical fly speed (`VerticalFlySpeed`).
    pub vertical_fly_speed: f64,
    /// Climb up/down speed (`ClimbSpeed`).
    pub climb_speed: f64,
    /// Climb lateral speed (`ClimbSpeedLateral`).
    pub climb_speed_lateral: f64,
    /// Sprint climb up speed.
    pub climb_up_sprint_speed: f64,
    /// Sprint climb down speed.
    pub climb_down_sprint_speed: f64,
    /// Forward walk multiplier.
    pub forward_walk: f64,
    /// Backward walk multiplier.
    pub backward_walk: f64,
    /// Strafe walk multiplier.
    pub strafe_walk: f64,
    /// Forward run multiplier.
    pub forward_run: f64,
    /// Backward run multiplier.
    pub backward_run: f64,
    /// Strafe run multiplier.
    pub strafe_run: f64,
    /// Forward crouch multiplier.
    pub forward_crouch: f64,
    /// Backward crouch multiplier.
    pub backward_crouch: f64,
    /// Strafe crouch multiplier.
    pub strafe_crouch: f64,
    /// Forward sprint multiplier.
    pub forward_sprint: f64,
    /// Air control multiplier (`AirControlMaxMultiplier`).
    pub air_control: f64,
    /// Minimum slide-entry speed.
    pub min_slide_entry_speed: f64,
    /// Speed at/below which a slide ends.
    pub slide_exit_speed: f64,
    /// Minimum landing speed that can start a safety roll.
    pub min_fall_speed_to_engage_roll: f64,
    /// Maximum landing speed that can start a safety roll.
    pub max_fall_speed_to_engage_roll: f64,
    /// Safety-roll start speed modifier.
    pub roll_start_speed_modifier: f64,
    /// Safety-roll exit speed modifier.
    pub roll_exit_speed_modifier: f64,
    /// Safety-roll duration in seconds.
    pub roll_time_to_complete: f64,
    /// Maximum mantle height (T55 server-side model).
    pub max_mantle_height: f64,
    /// Gravity (`PhysicsConstants.GRAVITY_ACCELERATION`).
    pub gravity: f64,
    /// Maximum auto-step height.
    pub step_height: f64,
}

impl Default for MovementTuning {
    fn default() -> Self {
        Self {
            base_speed: 5.5,
            jump_force: 11.8,
            swim_jump_force: 10.0,
            horizontal_fly_speed: 10.32,
            vertical_fly_speed: 10.32,
            climb_speed: 0.035,
            climb_speed_lateral: 0.035,
            climb_up_sprint_speed: 0.5,
            climb_down_sprint_speed: 0.6,
            forward_walk: 0.3,
            backward_walk: 0.3,
            strafe_walk: 0.3,
            forward_run: 1.0,
            backward_run: 0.65,
            strafe_run: 0.8,
            forward_crouch: 0.55,
            backward_crouch: 0.4,
            strafe_crouch: 0.45,
            forward_sprint: 1.65,
            air_control: 3.13,
            min_slide_entry_speed: 8.5,
            slide_exit_speed: 2.5,
            min_fall_speed_to_engage_roll: 21.0,
            max_fall_speed_to_engage_roll: 31.0,
            roll_start_speed_modifier: 2.5,
            roll_exit_speed_modifier: 1.5,
            roll_time_to_complete: 0.9,
            max_mantle_height: 2.0,
            gravity: GRAVITY_ACCELERATION,
            step_height: MAX_STEP_HEIGHT,
        }
    }
}

impl MovementTuning {
    /// Copy the movement fields from one selected `MovementSettings`.
    pub fn from_settings(settings: &MovementSettings) -> Self {
        Self {
            base_speed: f64::from(settings.base_speed),
            jump_force: f64::from(settings.jump_force),
            swim_jump_force: f64::from(settings.swim_jump_force),
            horizontal_fly_speed: f64::from(settings.horizontal_fly_speed),
            vertical_fly_speed: f64::from(settings.vertical_fly_speed),
            climb_speed: f64::from(settings.climb_speed),
            climb_speed_lateral: f64::from(settings.climb_speed_lateral),
            climb_up_sprint_speed: f64::from(settings.climb_up_sprint_speed),
            climb_down_sprint_speed: f64::from(settings.climb_down_sprint_speed),
            forward_walk: f64::from(settings.forward_walk_speed_multiplier),
            backward_walk: f64::from(settings.backward_walk_speed_multiplier),
            strafe_walk: f64::from(settings.strafe_walk_speed_multiplier),
            forward_run: f64::from(settings.forward_run_speed_multiplier),
            backward_run: f64::from(settings.backward_run_speed_multiplier),
            strafe_run: f64::from(settings.strafe_run_speed_multiplier),
            forward_crouch: f64::from(settings.forward_crouch_speed_multiplier),
            backward_crouch: f64::from(settings.backward_crouch_speed_multiplier),
            strafe_crouch: f64::from(settings.strafe_crouch_speed_multiplier),
            forward_sprint: f64::from(settings.forward_sprint_speed_multiplier),
            air_control: f64::from(settings.air_control_max_multiplier),
            min_slide_entry_speed: f64::from(settings.min_slide_entry_speed),
            slide_exit_speed: f64::from(settings.slide_exit_speed),
            min_fall_speed_to_engage_roll: f64::from(settings.min_fall_speed_to_engage_roll),
            max_fall_speed_to_engage_roll: f64::from(settings.max_fall_speed_to_engage_roll),
            roll_start_speed_modifier: f64::from(settings.roll_start_speed_modifier),
            roll_exit_speed_modifier: f64::from(settings.roll_exit_speed_modifier),
            roll_time_to_complete: f64::from(settings.roll_time_to_complete),
            max_mantle_height: 2.0,
            gravity: GRAVITY_ACCELERATION,
            step_height: MAX_STEP_HEIGHT,
        }
    }

    /// Horizontal target speed for one input and yaw.
    pub fn horizontal_speed(&self, state: MovementState, wish: Vec3, yaw: f64) -> f64 {
        let length_squared = wish.x * wish.x + wish.z * wish.z;
        if length_squared <= f64::EPSILON {
            return 0.0;
        }
        let length = length_squared.sqrt();
        let (dx, dz) = (wish.x / length, wish.z / length);
        let (forward_x, forward_z) = (-yaw.sin(), -yaw.cos());
        let (right_x, right_z) = (-yaw.cos(), yaw.sin());
        let forward = dx * forward_x + dz * forward_z;
        let strafe = dx * right_x + dz * right_z;

        let forward_multiplier = if state.low_profile() {
            if forward >= 0.0 {
                self.forward_crouch
            } else {
                self.backward_crouch
            }
        } else if state.sprinting {
            self.forward_sprint
        } else if state.walking {
            if forward >= 0.0 {
                self.forward_walk
            } else {
                self.backward_walk
            }
        } else if forward >= 0.0 {
            self.forward_run
        } else {
            self.backward_run
        };
        let strafe_multiplier = if state.low_profile() {
            self.strafe_crouch
        } else if state.walking {
            self.strafe_walk
        } else {
            self.strafe_run
        };
        self.base_speed
            * (forward.abs() * forward_multiplier + strafe.abs() * strafe_multiplier)
            * length.min(1.0)
    }
}

/// Server-side traversal state (crouch-slide and safety roll).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TraversalState {
    /// Currently crouch-sliding.
    pub sliding: bool,
    /// Currently safety-rolling.
    pub rolling: bool,
    /// Seconds left in the current roll.
    pub roll_remaining: f64,
}

impl TraversalState {
    /// Update slide/roll transitions from the pre-move speed and input.
    ///
    /// Returns `true` when the state changed.
    pub fn update(
        &mut self,
        tuning: MovementTuning,
        horizontal_speed: f64,
        on_ground: bool,
        landing_speed: f64,
        input: &MovementInput,
        dt: f64,
    ) -> bool {
        let previous = *self;
        if self.rolling {
            self.roll_remaining -= dt.max(0.0);
            if self.roll_remaining <= 0.0 {
                self.rolling = false;
                self.roll_remaining = 0.0;
            }
        }

        if !self.rolling
            && input.roll
            && on_ground
            && !self.sliding
            && (tuning.min_fall_speed_to_engage_roll..=tuning.max_fall_speed_to_engage_roll)
                .contains(&landing_speed)
        {
            self.rolling = true;
            self.roll_remaining = tuning.roll_time_to_complete.max(0.0);
            self.sliding = false;
        }

        if self.sliding {
            if horizontal_speed <= tuning.slide_exit_speed || !on_ground {
                self.sliding = false;
            }
        } else if !self.rolling
            && input.slide
            && on_ground
            && horizontal_speed >= tuning.min_slide_entry_speed
        {
            self.sliding = true;
        }

        *self != previous
    }

    /// Effective movement flags for speed selection and collision height.
    pub fn apply(&self, state: MovementState) -> MovementState {
        MovementState {
            sliding: self.sliding,
            rolling: self.rolling,
            ..state
        }
    }
}

/// A rider's mount/dismount seam.
///
/// Java stores `ClientMovement.mountedTo` and lets the client drive the mount;
/// the server-side model only needs the seat transform until world mount
/// entities land in T58/T59.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MountState {
    /// Mount entity id, or `None` when dismounted.
    pub mount_id: Option<u64>,
    /// Seat offset in mount-local space.
    pub seat_offset: Vec3,
    /// Mount yaw used to rotate the seat offset.
    pub yaw: f64,
}

impl MountState {
    /// Attach the rider to a mount.
    pub fn mount(&mut self, mount_id: u64, seat_offset: Vec3, yaw: f64) {
        self.mount_id = Some(mount_id);
        self.seat_offset = seat_offset;
        self.yaw = yaw;
    }

    /// Detach the rider.
    pub fn dismount(&mut self) -> Option<u64> {
        self.mount_id.take()
    }

    /// Whether the rider is mounted.
    pub fn is_mounted(&self) -> bool {
        self.mount_id.is_some()
    }

    /// Rider feet position for a mount at `mount_position`.
    pub fn rider_position(&self, mount_position: Vec3) -> Vec3 {
        let (sin, cos) = self.yaw.sin_cos();
        mount_position.plus(Vec3::new(
            self.seat_offset.x * cos - self.seat_offset.z * sin,
            self.seat_offset.y,
            self.seat_offset.x * sin + self.seat_offset.z * cos,
        ))
    }
}

/// Kinematic body state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementBody {
    /// Feet position.
    pub position: Vec3,
    /// Velocity in blocks per second.
    pub velocity: Vec3,
    /// Whether the last move ended on the ground.
    pub on_ground: bool,
    /// Slide/roll state.
    pub traversal: TraversalState,
    /// Mount/dismount seam.
    pub mount: MountState,
}

impl MovementBody {
    /// A grounded body at `position`.
    pub fn new(position: Vec3) -> Self {
        Self {
            position,
            velocity: Vec3::default(),
            on_ground: true,
            traversal: TraversalState::default(),
            mount: MountState::default(),
        }
    }

    /// Advance one deterministic step using Java gravity/movement constants.
    pub fn step(
        &mut self,
        world: &dyn CollisionWorld,
        shape: PlayerShape,
        tuning: MovementTuning,
        input: MovementInput,
        dt: f64,
    ) -> MovementStep {
        if !dt.is_finite() || dt < 0.0 {
            return MovementStep {
                position: self.position,
                velocity: self.velocity,
                on_ground: self.on_ground,
                collided: false,
                stepped: false,
                traversal: self.traversal,
            };
        }
        let dt = dt.min(0.25);
        let fall_speed = if self.on_ground {
            0.0
        } else {
            (-self.velocity.y).max(0.0)
        };
        let state = self.traversal.apply(input.state);
        let fly = state.flying;
        let climb = state.climbing;

        let horizontal = if fly {
            Vec3::new(
                input.wish.x.clamp(-1.0, 1.0) * tuning.horizontal_fly_speed,
                0.0,
                input.wish.z.clamp(-1.0, 1.0) * tuning.horizontal_fly_speed,
            )
        } else if climb {
            let length = (input.wish.x * input.wish.x + input.wish.z * input.wish.z)
                .sqrt()
                .max(1.0);
            Vec3::new(
                input.wish.x / length * tuning.climb_speed_lateral,
                0.0,
                input.wish.z / length * tuning.climb_speed_lateral,
            )
        } else {
            let speed = tuning.horizontal_speed(state, input.wish, input.yaw);
            let length = (input.wish.x * input.wish.x + input.wish.z * input.wish.z).sqrt();
            if length > f64::EPSILON {
                Vec3::new(
                    input.wish.x / length * speed,
                    0.0,
                    input.wish.z / length * speed,
                )
            } else {
                Vec3::default()
            }
        };

        if fly || climb {
            let blend = (1.0 - (-12.0 * dt).exp()).clamp(0.0, 1.0);
            self.velocity.x += (horizontal.x - self.velocity.x) * blend;
            self.velocity.z += (horizontal.z - self.velocity.z) * blend;
            self.velocity.y = if climb {
                if input.jump || input.vertical > 0.5 {
                    if state.sprinting {
                        tuning.climb_up_sprint_speed
                    } else {
                        tuning.climb_speed
                    }
                } else if input.vertical < -0.5 {
                    if state.sprinting {
                        tuning.climb_down_sprint_speed
                    } else {
                        tuning.climb_speed
                    }
                } else {
                    0.0
                }
            } else {
                input.vertical.clamp(-1.0, 1.0) * tuning.vertical_fly_speed
            };
        } else {
            if self.on_ground {
                self.velocity.x = horizontal.x;
                self.velocity.z = horizontal.z;
            } else {
                let blend = (tuning.air_control * dt).clamp(0.0, 1.0);
                self.velocity.x += (horizontal.x - self.velocity.x) * blend;
                self.velocity.z += (horizontal.z - self.velocity.z) * blend;
            }

            if input.jump {
                if state.in_fluid {
                    self.velocity.y = tuning.swim_jump_force;
                    self.on_ground = false;
                } else if self.on_ground {
                    self.velocity.y = tuning.jump_force;
                    // The impulse is followed by one gravity step in the same
                    // symplectic-Euler update.
                    self.on_ground = false;
                }
            }
            if state.in_fluid {
                self.velocity.y -= tuning.gravity * 0.25 * dt;
            } else if !self.on_ground {
                self.velocity.y -= tuning.gravity * dt;
            } else if self.velocity.y < 0.0 {
                self.velocity.y = 0.0;
            }
        }

        let target = self.position.plus(self.velocity.scale(dt));
        let resolution = resolve_target(
            world,
            self.position,
            target,
            shape,
            state.low_profile(),
            tuning.step_height,
        );
        self.position = resolution.position;
        if resolution.hit_x {
            self.velocity.x = 0.0;
        }
        if resolution.hit_y {
            self.velocity.y = 0.0;
        }
        if resolution.hit_z {
            self.velocity.z = 0.0;
        }

        let probe = self.position.with_axis(1, self.position.y - GROUND_PROBE);
        let mut scratch = Vec::new();
        self.on_ground =
            aabb_has_collision(world, shape.aabb(probe, state.low_profile()), &mut scratch);
        if self.on_ground && self.velocity.y < 0.0 {
            self.velocity.y = 0.0;
        }

        let horizontal_speed =
            (self.velocity.x * self.velocity.x + self.velocity.z * self.velocity.z).sqrt();
        self.traversal.update(
            tuning,
            horizontal_speed,
            self.on_ground,
            fall_speed,
            &input,
            dt,
        );

        MovementStep {
            position: self.position,
            velocity: self.velocity,
            on_ground: self.on_ground,
            collided: resolution.hit_x || resolution.hit_y || resolution.hit_z,
            stepped: resolution.stepped,
            traversal: self.traversal,
        }
    }

    /// Try to mantle onto a ledge in front of the body.
    ///
    /// Java mantling is client-driven; this is the deterministic server-side
    /// model for NPC/scripted traversal. The body must be grounded, blocked at
    /// its current height in `direction`, and have a free body-sized space on a
    /// support surface above `step_height` (up to `max_mantle_height`).
    pub fn try_mantle(
        &mut self,
        world: &dyn CollisionWorld,
        shape: PlayerShape,
        tuning: MovementTuning,
        direction: Vec3,
    ) -> bool {
        if !self.on_ground {
            return false;
        }
        let length = (direction.x * direction.x + direction.z * direction.z).sqrt();
        if length <= f64::EPSILON {
            return false;
        }
        let direction = Vec3::new(direction.x / length, 0.0, direction.z / length);
        let front = self.position.plus(direction.scale(0.6));
        let mut scratch = Vec::new();
        if !aabb_has_collision(world, shape.aabb(front, true), &mut scratch) {
            return false; // Nothing to mantle; normal movement is free.
        }
        let mut height = tuning.step_height + 0.125;
        let max_height = tuning.max_mantle_height.max(height);
        while height <= max_height + 1e-9 {
            let candidate = front.with_axis(1, self.position.y + height);
            if !aabb_has_collision(world, shape.aabb(candidate, false), &mut scratch)
                && aabb_has_collision(
                    world,
                    shape.aabb(candidate.with_axis(1, candidate.y - 0.05), false),
                    &mut scratch,
                )
            {
                self.position = candidate;
                self.velocity = Vec3::default();
                self.on_ground = true;
                self.traversal.sliding = false;
                self.traversal.rolling = false;
                return true;
            }
            height += 0.125;
        }
        false
    }
}

/// Outcome of one [`MovementBody::step`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementStep {
    /// New feet position.
    pub position: Vec3,
    /// New velocity.
    pub velocity: Vec3,
    /// Ground contact after the move.
    pub on_ground: bool,
    /// A block stopped at least one axis.
    pub collided: bool,
    /// The move auto-stepped onto a block.
    pub stepped: bool,
    /// Slide/roll state after the step.
    pub traversal: TraversalState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_world::collision::Aabb;

    #[derive(Default)]
    struct TestWorld {
        boxes: Vec<Aabb>,
    }

    impl TestWorld {
        fn add_block(&mut self, x: i32, y: i32, z: i32, local: Aabb) {
            self.boxes
                .push(local.translated(Vec3::new(f64::from(x), f64::from(y), f64::from(z))));
        }
    }

    impl CollisionWorld for TestWorld {
        fn collect_block_boxes(&self, x: i32, y: i32, z: i32, out: &mut Vec<Aabb>) {
            let origin = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
            let block = Aabb::unit().translated(origin);
            for collision in &self.boxes {
                if collision.intersects(block) {
                    out.push(*collision);
                }
            }
        }
    }

    fn flat_ground() -> TestWorld {
        let mut world = TestWorld::default();
        for x in -4..8 {
            for z in -4..8 {
                world.add_block(x, -1, z, Aabb::unit());
            }
        }
        world
    }

    #[test]
    fn a_free_target_is_returned_bit_for_bit() {
        let world = flat_ground();
        let from = Vec3::new(0.5, 0.0, 0.5);
        let to = Vec3::new(2.25, 0.0, 2.25);
        let result = resolve_target(
            &world,
            from,
            to,
            PlayerShape::default(),
            false,
            MAX_STEP_HEIGHT,
        );
        assert_eq!(result.position, to);
        assert!(!result.corrected);
    }

    #[test]
    fn a_wall_clamps_the_target_to_the_face() {
        let mut world = flat_ground();
        // Two blocks high: above the one-block auto-step.
        world.add_block(
            2,
            0,
            0,
            Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 1.0)),
        );
        let result = resolve_target(
            &world,
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(3.5, 0.0, 0.5),
            PlayerShape::default(),
            false,
            MAX_STEP_HEIGHT,
        );
        assert!(result.corrected);
        assert!(result.hit_x);
        assert!(
            (result.position.x - (2.0 - 0.325)).abs() < 1e-4,
            "{result:?}"
        );
        assert!((result.position.z - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_half_block_is_stepped_onto() {
        let mut world = flat_ground();
        world.add_block(
            2,
            0,
            0,
            Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.5, 1.0)),
        );
        let result = resolve_target(
            &world,
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(2.5, 0.0, 0.5),
            PlayerShape::default(),
            false,
            MAX_STEP_HEIGHT,
        );
        assert!(result.stepped, "{result:?}");
        assert!((result.position.x - 2.5).abs() < 1e-6, "{result:?}");
        assert!((result.position.y - 0.5).abs() < 1e-4, "{result:?}");
    }

    #[test]
    fn a_one_and_a_half_block_wall_is_not_stepped() {
        let mut world = flat_ground();
        world.add_block(
            2,
            0,
            0,
            Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.5, 1.0)),
        );
        let result = resolve_target(
            &world,
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(3.5, 0.0, 0.5),
            PlayerShape::default(),
            false,
            MAX_STEP_HEIGHT,
        );
        assert!(!result.stepped, "{result:?}");
        assert!((result.position.x - 1.675).abs() < 1e-4, "{result:?}");
    }

    #[test]
    fn gravity_matches_java_symplectic_euler() {
        let world = flat_ground();
        let mut body = MovementBody {
            position: Vec3::new(0.5, 4.0, 0.5),
            velocity: Vec3::default(),
            on_ground: false,
            traversal: TraversalState::default(),
            mount: MountState::default(),
        };
        let dt = 1.0 / 60.0;
        let step = body.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            MovementInput::default(),
            dt,
        );
        assert!(!step.on_ground);
        assert!((body.velocity.y - (-GRAVITY_ACCELERATION * dt)).abs() < 1e-12);
        assert!((body.position.y - (4.0 - GRAVITY_ACCELERATION * dt * dt)).abs() < 1e-12);
    }

    #[test]
    fn jump_uses_the_java_jump_force() {
        let world = flat_ground();
        let mut body = MovementBody::new(Vec3::new(0.5, 0.0, 0.5));
        let dt = 1.0 / 60.0;
        let step = body.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            MovementInput {
                jump: true,
                ..MovementInput::default()
            },
            dt,
        );
        assert!((body.velocity.y - (11.8 - GRAVITY_ACCELERATION * dt)).abs() < 1e-12);
        assert!(body.position.y > 0.0);
        assert!(!step.on_ground);
    }

    #[test]
    fn falling_lands_on_the_floor() {
        let world = flat_ground();
        let mut body = MovementBody {
            position: Vec3::new(0.5, 3.0, 0.5),
            velocity: Vec3::default(),
            on_ground: false,
            traversal: TraversalState::default(),
            mount: MountState::default(),
        };
        for _ in 0..240 {
            body.step(
                &world,
                PlayerShape::default(),
                MovementTuning::default(),
                MovementInput::default(),
                1.0 / 60.0,
            );
        }
        assert!((body.position.y).abs() < 1e-6, "{body:?}");
        assert!(body.on_ground, "{body:?}");
        assert_eq!(body.velocity.y, 0.0);
    }

    #[test]
    fn walking_uses_the_config_speed() {
        let world = flat_ground();
        let mut body = MovementBody::new(Vec3::new(0.5, 0.0, 0.5));
        let tuning = MovementTuning::default();
        let step = body.step(
            &world,
            PlayerShape::default(),
            tuning,
            MovementInput {
                wish: Vec3::new(1.0, 0.0, 0.0),
                yaw: std::f64::consts::FRAC_PI_2,
                state: MovementState {
                    walking: true,
                    ..MovementState::default()
                },
                ..MovementInput::default()
            },
            1.0,
        );
        // yaw = +90deg faces -X, so world +X is a backward walk: the backward
        // multiplier applies but the velocity still points along the wish.
        assert!((step.velocity.x - (5.5 * 0.3)).abs() < 1e-9, "{step:?}");
    }

    #[test]
    fn flying_and_climbing_ignore_gravity() {
        let world = flat_ground();
        let mut flying = MovementBody {
            position: Vec3::new(0.5, 4.0, 0.5),
            velocity: Vec3::default(),
            on_ground: false,
            traversal: TraversalState::default(),
            mount: MountState::default(),
        };
        flying.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            MovementInput {
                vertical: 1.0,
                state: MovementState {
                    flying: true,
                    ..MovementState::default()
                },
                ..MovementInput::default()
            },
            1.0 / 60.0,
        );
        assert!((flying.velocity.y - 10.32).abs() < 1e-12, "{flying:?}");

        let mut climbing = MovementBody {
            position: Vec3::new(0.5, 4.0, 0.5),
            velocity: Vec3::default(),
            on_ground: false,
            traversal: TraversalState::default(),
            mount: MountState::default(),
        };
        climbing.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            MovementInput {
                jump: true,
                state: MovementState {
                    climbing: true,
                    ..MovementState::default()
                },
                ..MovementInput::default()
            },
            1.0 / 60.0,
        );
        assert!((climbing.velocity.y - 0.035).abs() < 1e-12, "{climbing:?}");
        assert!(climbing.position.y > 4.0);
    }

    #[test]
    fn walking_off_a_ledge_falls_on_the_next_step() {
        let mut world = TestWorld::default();
        world.add_block(0, -1, 0, Aabb::unit());
        let mut body = MovementBody::new(Vec3::new(0.5, 0.0, 0.5));
        let input = MovementInput {
            wish: Vec3::new(1.0, 0.0, 0.0),
            yaw: std::f64::consts::FRAC_PI_2,
            ..MovementInput::default()
        };
        let step = body.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            input,
            0.5,
        );
        assert!(!step.on_ground, "{step:?}");
        body.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            input,
            0.5,
        );
        assert!(body.velocity.y < 0.0, "{body:?}");
    }

    #[test]
    fn bounds_reject_non_finite_out_of_world_and_large_steps() {
        let bounds = MovementBounds::default();
        let from = Vec3::new(0.0, 64.0, 0.0);
        assert_eq!(
            bounds.validate(from, Vec3::new(f64::NAN, 64.0, 0.0)),
            Err(MovementRejection::NonFinite)
        );
        assert_eq!(
            bounds.validate(from, Vec3::new(f64::INFINITY, 64.0, 0.0)),
            Err(MovementRejection::NonFinite)
        );
        assert_eq!(
            bounds.validate(from, Vec3::new(0.0, 100_000.0, 0.0)),
            Err(MovementRejection::OutOfBounds)
        );
        assert_eq!(
            bounds.validate(from, Vec3::new(600.0, 64.0, 0.0)),
            Err(MovementRejection::StepTooLarge)
        );
        assert_eq!(bounds.validate(from, Vec3::new(10.0, 64.0, 0.0)), Ok(()));
    }

    #[test]
    fn an_overlapping_start_is_not_corrected_again() {
        let mut world = flat_ground();
        world.add_block(0, 0, 0, Aabb::unit());
        let result = resolve_target(
            &world,
            Vec3::new(0.5, 0.0, 0.5),
            Vec3::new(0.75, 0.0, 0.5),
            PlayerShape::default(),
            false,
            MAX_STEP_HEIGHT,
        );
        assert!(result.overlapping);
        assert!(!result.corrected);
        assert_eq!(result.position, Vec3::new(0.75, 0.0, 0.5));
    }

    #[test]
    fn sliding_enters_at_the_configured_speed_and_exits_low() {
        let world = flat_ground();
        let mut body = MovementBody::new(Vec3::new(0.5, 0.0, 0.5));
        let input = MovementInput {
            wish: Vec3::new(0.0, 0.0, -1.0),
            state: MovementState {
                sprinting: true,
                ..MovementState::default()
            },
            slide: true,
            ..MovementInput::default()
        };
        body.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            input,
            0.05,
        );
        assert!(body.traversal.sliding, "sprint speed starts the slide");
        let stop = MovementInput {
            state: MovementState::default(),
            ..MovementInput::default()
        };
        body.step(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            stop,
            0.05,
        );
        assert!(
            !body.traversal.sliding,
            "dropping below the exit speed ends it"
        );
    }

    #[test]
    fn a_fast_landing_starts_a_timed_safety_roll() {
        let world = flat_ground();
        let mut body = MovementBody::new(Vec3::new(0.5, 5.0, 0.5));
        body.on_ground = false;
        body.velocity.y = -21.0;
        let input = MovementInput {
            roll: true,
            ..MovementInput::default()
        };
        let mut rolled = false;
        for _ in 0..40 {
            let step = body.step(
                &world,
                PlayerShape::default(),
                MovementTuning::default(),
                input,
                0.05,
            );
            if step.traversal.rolling {
                rolled = true;
                assert!(step.traversal.roll_remaining > 0.0);
                break;
            }
        }
        assert!(rolled, "landing at ~25 blocks/s engages the roll");
        for _ in 0..40 {
            body.step(
                &world,
                PlayerShape::default(),
                MovementTuning::default(),
                MovementInput::default(),
                0.05,
            );
        }
        assert!(
            !body.traversal.rolling,
            "the roll completes after its duration"
        );
    }

    #[test]
    fn a_requested_mantle_lifts_the_body_onto_a_two_block_ledge() {
        let mut world = flat_ground();
        world.add_block(0, 0, 0, Aabb::unit());
        world.add_block(0, 1, 0, Aabb::unit());
        let mut body = MovementBody::new(Vec3::new(0.5, 0.0, 1.5));
        assert!(!body.try_mantle(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            Vec3::new(0.0, 0.0, 1.0),
        ));
        assert!(body.try_mantle(
            &world,
            PlayerShape::default(),
            MovementTuning::default(),
            Vec3::new(0.0, 0.0, -1.0),
        ));
        assert!((body.position.y - 2.0).abs() < 1e-9);
        assert!(body.on_ground);
        assert_eq!(body.velocity, Vec3::default());
    }

    #[test]
    fn mount_and_dismount_follow_the_carrier() {
        let mut mount = MountState::default();
        let seat = Vec3::new(0.0, 1.2, -0.4);
        mount.mount(7, seat, std::f64::consts::FRAC_PI_2);
        assert!(mount.is_mounted());
        let rider = mount.rider_position(Vec3::new(10.0, 20.0, 30.0));
        assert!((rider.x - 10.4).abs() < 1e-9);
        assert!((rider.y - 21.2).abs() < 1e-9);
        assert!((rider.z - 30.0).abs() < 1e-9);
        assert_eq!(mount.dismount(), Some(7));
        assert!(!mount.is_mounted());
        assert_eq!(mount.dismount(), None);
    }
}
