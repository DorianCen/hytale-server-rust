//! Legacy projectile launch, flight and hit resolution.
//!
//! Ported from `CoreServer/.../entity/entities/ProjectileComponent`,
//! `modules/projectile/config/{Projectile,ProjectileConfig,StandardPhysicsConfig}`
//! and `modules/physics/{SimplePhysicsProvider,PhysicsMath}`.
//!
//! The core is deterministic and collision-oriented: a projectile is a small
//! AABB moving through [`hytale_world::collision::CollisionWorld`] plus a
//! caller-supplied list of [`ProjectileTarget`] boxes. Gravity, the terminal
//! velocity cap, bounce damping, stick/rest, hit/miss/death timers and TTL are
//! modelled; Java's density/drag/buoyancy integration, rotation damping,
//! particle/sound playback and entity spawning stay later work.

use hytale_world::collision::{Aabb, CollisionWorld, Vec3};
use serde_json::Value;

use crate::damage::{Damage, DamageSource};

/// Java `SimplePhysicsProvider.ROTATION_MODE` subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RotationMode {
    /// Do not update rotation.
    None,
    /// Snap yaw/pitch to the velocity.
    #[default]
    Velocity,
    /// Ease yaw/pitch toward the velocity (the legacy default).
    VelocityDamped,
    /// Velocity rotation plus visual roll.
    VelocityRoll,
}

impl RotationMode {
    /// Decode the enum name used by both projectile asset stores.
    pub fn from_java(value: &str) -> Option<Self> {
        match value {
            "None" | "NONE" => Some(Self::None),
            "Velocity" | "VELOCITY" => Some(Self::Velocity),
            "VelocityDamped" | "VELOCITY_DAMPED" => Some(Self::VelocityDamped),
            "VelocityRoll" | "VELOCITY_ROLL" => Some(Self::VelocityRoll),
            _ => None,
        }
    }
}

/// One legacy `Server/Projectiles` asset (`projectile/config/Projectile`).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileAsset {
    /// Asset id.
    pub id: String,
    /// Model appearance id.
    pub appearance: Option<String>,
    /// Collision box width/depth (`Radius`).
    pub radius: f64,
    /// Collision height.
    pub height: f64,
    /// Eye-to-spawn vertical offset.
    pub vertical_center_shot: f64,
    /// Eye-to-spawn horizontal offset.
    pub horizontal_center_shot: f64,
    /// Eye-to-spawn depth offset.
    pub depth_shot: f64,
    /// Rotate the depth offset by pitch.
    pub pitch_adjust_shot: bool,
    /// Launch speed.
    pub muzzle_velocity: f64,
    /// Speed cap (0 = no cap).
    pub terminal_velocity: f64,
    /// Gravity in blocks/second².
    pub gravity: f64,
    /// Restitution multiplier.
    pub bounciness: f64,
    /// Extra energy loss per impact.
    pub impact_slowdown: f64,
    /// Stick to the ground/wall on a slow impact.
    pub sticks_vertically: bool,
    /// Update yaw from velocity.
    pub compute_yaw: bool,
    /// Update pitch from velocity.
    pub compute_pitch: bool,
    /// Update roll from velocity (kept for parity; the core does not roll).
    pub compute_roll: bool,
    /// Rotation update mode.
    pub rotation_mode: RotationMode,
    /// Lifetime in seconds (0 = no TTL).
    pub time_to_live: f64,
    /// Damage on a living-entity hit.
    pub damage: i32,
    /// Delay between a hit and removal.
    pub dead_time: f64,
    /// Delay between a miss and removal.
    pub dead_time_miss: f64,
    /// Run death effects after a hit.
    pub death_effects_on_hit: bool,
    /// Density (new-physics seam).
    pub density: f64,
    /// Water terminal-velocity multiplier.
    pub water_terminal_velocity_multiplier: f64,
    /// Water impulse loss.
    pub water_hit_impulse_loss: f64,
    /// Rotation damping.
    pub damping_rotation: f64,
    /// Rotation speed / velocity ratio.
    pub rotation_speed_velocity_ratio: f64,
    /// Swimming damping.
    pub swimming_damping_factor: f64,
    /// Bounce sound id.
    pub bounce_sound_event_id: Option<String>,
    /// Hit sound id.
    pub hit_sound_event_id: Option<String>,
    /// Miss sound id.
    pub miss_sound_event_id: Option<String>,
    /// Death sound id.
    pub death_sound_event_id: Option<String>,
    /// Has bounce particles.
    pub has_bounce_particles: bool,
    /// Has hit particles.
    pub has_hit_particles: bool,
    /// Has miss particles.
    pub has_miss_particles: bool,
    /// Has death particles.
    pub has_death_particles: bool,
    /// Has an explosion config.
    pub has_explosion_config: bool,
}

impl ProjectileAsset {
    /// Decode one merged `Server/Projectiles` document.
    pub fn from_value(key: &str, value: &Value) -> Result<Self, String> {
        let number = |name: &str| value.get(name).and_then(Value::as_f64).unwrap_or(0.0);
        let boolean = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
        let string = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_string);
        let rotation_mode = match string("RotationMode").as_deref() {
            None => RotationMode::Velocity,
            Some(value) => RotationMode::from_java(value)
                .ok_or_else(|| format!("Projectile `{key}` has unknown RotationMode `{value}`"))?,
        };
        Ok(Self {
            id: key.to_string(),
            appearance: string("Appearance"),
            radius: number("Radius"),
            height: number("Height"),
            vertical_center_shot: number("VerticalCenterShot"),
            horizontal_center_shot: number("HorizontalCenterShot"),
            depth_shot: number("DepthShot"),
            pitch_adjust_shot: boolean("PitchAdjustShot"),
            muzzle_velocity: number("MuzzleVelocity"),
            terminal_velocity: number("TerminalVelocity"),
            gravity: number("Gravity"),
            bounciness: number("Bounciness"),
            impact_slowdown: number("ImpactSlowdown"),
            sticks_vertically: boolean("SticksVertically"),
            compute_yaw: value
                .get("ComputeYaw")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            compute_pitch: value
                .get("ComputePitch")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            compute_roll: value
                .get("ComputeRoll")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            rotation_mode,
            time_to_live: number("TimeToLive"),
            damage: value.get("Damage").and_then(Value::as_i64).unwrap_or(0) as i32,
            dead_time: number("DeadTime"),
            dead_time_miss: value
                .get("DeadTimeMiss")
                .and_then(Value::as_f64)
                .unwrap_or(10.0),
            death_effects_on_hit: boolean("DeathEffectsOnHit"),
            density: value
                .get("Density")
                .and_then(Value::as_f64)
                .unwrap_or(2000.0),
            water_terminal_velocity_multiplier: value
                .get("WaterTerminalVelocityMultiplier")
                .and_then(Value::as_f64)
                .unwrap_or(1.0),
            water_hit_impulse_loss: number("WaterHitImpulseLoss"),
            damping_rotation: number("DampingRotation"),
            rotation_speed_velocity_ratio: value
                .get("RotationSpeedVelocityRatio")
                .and_then(Value::as_f64)
                .unwrap_or(2.0),
            swimming_damping_factor: value
                .get("SwimmingDampingFactor")
                .and_then(Value::as_f64)
                .unwrap_or(1.0),
            bounce_sound_event_id: string("BounceSoundEventId"),
            hit_sound_event_id: string("HitSoundEventId"),
            miss_sound_event_id: string("MissSoundEventId"),
            death_sound_event_id: string("DeathSoundEventId"),
            has_bounce_particles: value.get("BounceParticles").is_some(),
            has_hit_particles: value.get("HitParticles").is_some(),
            has_miss_particles: value.get("MissParticles").is_some(),
            has_death_particles: value.get("DeathParticles").is_some(),
            has_explosion_config: value.get("ExplosionConfig").is_some(),
        })
    }

    /// Collision box with the feet at `position`.
    pub fn aabb(&self, position: Vec3) -> Aabb {
        // Java `Box.horizontallyCentered(radius, height, radius)` treats
        // `Radius` as the full width/depth.
        Aabb::player(
            position,
            (self.radius * 0.5).max(0.001),
            self.height.max(0.001),
        )
    }

    /// Java `ProjectileComponent.computeStartOffset`.
    pub fn start_offset(&self, yaw: f64, pitch: f64) -> Vec3 {
        let mut offset = Vec3::default();
        if self.depth_shot != 0.0 {
            let direction =
                direction_from_angles(yaw, if self.pitch_adjust_shot { pitch } else { 0.0 });
            let length = direction.length_squared().sqrt();
            if length > 0.0 {
                offset = direction.scale(self.depth_shot / length);
            }
        }
        // Java: -headingZ(yaw) = cos(yaw); +headingX(yaw) = -sin(yaw).
        offset.plus(Vec3::new(
            yaw.cos() * self.horizontal_center_shot,
            -self.vertical_center_shot,
            -yaw.sin() * self.horizontal_center_shot,
        ))
    }

    /// Initial velocity from the game yaw/pitch angles.
    pub fn launch_velocity(&self, yaw: f64, pitch: f64) -> Vec3 {
        direction_from_angles(yaw, pitch).scale(self.muzzle_velocity)
    }

    /// Java `setBounciness(bounciness * (1 - impactSlowdown))`.
    pub fn effective_bounciness(&self) -> f64 {
        (self.bounciness * (1.0 - self.impact_slowdown)).clamp(0.0, 1.0)
    }
}

/// New-style `StandardPhysicsConfig` fields (`ProjectileConfig.Physics`).
#[derive(Debug, Clone, PartialEq)]
pub struct StandardPhysics {
    /// `Standard` or `GroundFollow`.
    pub ground_follow: bool,
    /// Density.
    pub density: f64,
    /// Gravity.
    pub gravity: f64,
    /// Restitution.
    pub bounciness: f64,
    /// Bounce count (-1 = unlimited).
    pub bounce_count: i32,
    /// Minimum bounce speed.
    pub bounce_limit: f64,
    /// Stick on slow impact.
    pub sticks_vertically: bool,
    /// Update yaw.
    pub compute_yaw: bool,
    /// Update pitch.
    pub compute_pitch: bool,
    /// Rotation mode.
    pub rotation_mode: RotationMode,
    /// Push-out speed from solid blocks.
    pub move_out_of_solid_speed: f64,
    /// Air terminal velocity.
    pub terminal_velocity_air: f64,
    /// Air density.
    pub density_air: f64,
    /// Water terminal velocity.
    pub terminal_velocity_water: f64,
    /// Water density.
    pub density_water: f64,
    /// Water impulse loss.
    pub hit_water_impulse_loss: f64,
    /// Rotation damping force.
    pub rotation_force: f64,
    /// Speed/rotation ratio.
    pub speed_rotation_factor: f64,
    /// Swimming damping.
    pub swimming_damping_factor: f64,
    /// Allow rolling.
    pub allow_rolling: bool,
    /// Rolling friction.
    pub rolling_friction_factor: f64,
    /// Rolling speed.
    pub rolling_speed: f64,
    /// `GroundFollow.MaxClimb`.
    pub max_climb: i32,
    /// `GroundFollow.MaxDrop`.
    pub max_drop: i32,
}

impl Default for StandardPhysics {
    fn default() -> Self {
        Self {
            ground_follow: false,
            density: 700.0,
            gravity: 0.0,
            bounciness: 0.0,
            bounce_count: -1,
            bounce_limit: 0.4,
            sticks_vertically: false,
            compute_yaw: true,
            compute_pitch: true,
            rotation_mode: RotationMode::VelocityDamped,
            move_out_of_solid_speed: 0.0,
            terminal_velocity_air: 1.0,
            density_air: 1.2,
            terminal_velocity_water: 1.0,
            density_water: 998.0,
            hit_water_impulse_loss: 0.2,
            rotation_force: 3.0,
            speed_rotation_factor: 2.0,
            swimming_damping_factor: 1.0,
            allow_rolling: false,
            rolling_friction_factor: 0.99,
            rolling_speed: 0.1,
            max_climb: 1,
            max_drop: 1,
        }
    }
}

/// One `Server/ProjectileConfigs` asset (new projectile system seam).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileConfigAsset {
    /// Asset id.
    pub id: String,
    /// Model id.
    pub model: Option<String>,
    /// Derive scale from the model range.
    pub use_model_scale: bool,
    /// Launch force.
    pub launch_force: f64,
    /// Spawn offset.
    pub spawn_offset: Vec3,
    /// Rotate the spawn offset by pitch.
    pub rotate_spawn_offset_by_pitch: bool,
    /// Rotate the spawn offset by yaw.
    pub rotate_spawn_offset_by_yaw: bool,
    /// Physics block.
    pub physics: StandardPhysics,
}

impl ProjectileConfigAsset {
    /// Decode one merged `Server/ProjectileConfigs` document.
    pub fn from_value(key: &str, value: &Value) -> Result<Self, String> {
        let number = |value: &Value, name: &str, default: f64| {
            value.get(name).and_then(Value::as_f64).unwrap_or(default)
        };
        let boolean = |value: &Value, name: &str, default: bool| {
            value.get(name).and_then(Value::as_bool).unwrap_or(default)
        };
        let mut physics = StandardPhysics::default();
        if let Some(config) = value.get("Physics") {
            let kind = config
                .get("Type")
                .and_then(Value::as_str)
                .unwrap_or("Standard");
            physics.ground_follow = match kind {
                "Standard" => false,
                "GroundFollow" => true,
                other => {
                    return Err(format!(
                        "ProjectileConfig `{key}` has unknown Physics.Type `{other}`"
                    ))
                }
            };
            physics.density = number(config, "Density", physics.density);
            physics.gravity = number(config, "Gravity", physics.gravity);
            physics.bounciness = number(config, "Bounciness", physics.bounciness);
            physics.bounce_count = config
                .get("BounceCount")
                .and_then(Value::as_i64)
                .unwrap_or(i64::from(physics.bounce_count))
                as i32;
            physics.bounce_limit = number(config, "BounceLimit", physics.bounce_limit);
            physics.sticks_vertically =
                boolean(config, "SticksVertically", physics.sticks_vertically);
            physics.compute_yaw = boolean(config, "ComputeYaw", physics.compute_yaw);
            physics.compute_pitch = boolean(config, "ComputePitch", physics.compute_pitch);
            if let Some(mode) = config.get("RotationMode").and_then(Value::as_str) {
                physics.rotation_mode = RotationMode::from_java(mode).ok_or_else(|| {
                    format!("ProjectileConfig `{key}` has unknown RotationMode `{mode}`")
                })?;
            }
            physics.move_out_of_solid_speed = number(
                config,
                "MoveOutOfSolidSpeed",
                physics.move_out_of_solid_speed,
            );
            physics.terminal_velocity_air =
                number(config, "TerminalVelocityAir", physics.terminal_velocity_air);
            physics.density_air = number(config, "DensityAir", physics.density_air);
            physics.terminal_velocity_water = number(
                config,
                "TerminalVelocityWater",
                physics.terminal_velocity_water,
            );
            physics.density_water = number(config, "DensityWater", physics.density_water);
            physics.hit_water_impulse_loss = number(
                config,
                "HitWaterImpulseLoss",
                physics.hit_water_impulse_loss,
            );
            physics.rotation_force = number(config, "RotationForce", physics.rotation_force);
            physics.speed_rotation_factor =
                number(config, "SpeedRotationFactor", physics.speed_rotation_factor);
            physics.swimming_damping_factor = number(
                config,
                "SwimmingDampingFactor",
                physics.swimming_damping_factor,
            );
            physics.allow_rolling = boolean(config, "AllowRolling", physics.allow_rolling);
            physics.rolling_friction_factor = number(
                config,
                "RollingFrictionFactor",
                physics.rolling_friction_factor,
            );
            physics.rolling_speed = number(config, "RollingSpeed", physics.rolling_speed);
            physics.max_climb = config
                .get("MaxClimb")
                .and_then(Value::as_i64)
                .unwrap_or(i64::from(physics.max_climb)) as i32;
            physics.max_drop = config
                .get("MaxDrop")
                .and_then(Value::as_i64)
                .unwrap_or(i64::from(physics.max_drop)) as i32;
        }
        let spawn_offset = value.get("SpawnOffset").cloned().unwrap_or(Value::Null);
        Ok(Self {
            id: key.to_string(),
            model: value
                .get("Model")
                .and_then(Value::as_str)
                .map(str::to_string),
            use_model_scale: boolean(value, "UseModelScale", false),
            launch_force: value
                .get("LaunchForce")
                .and_then(Value::as_f64)
                .unwrap_or(1.0),
            spawn_offset: Vec3::new(
                number(&spawn_offset, "X", 0.0),
                number(&spawn_offset, "Y", 0.0),
                number(&spawn_offset, "Z", 0.0),
            ),
            rotate_spawn_offset_by_pitch: boolean(value, "RotateSpawnOffsetByPitch", true),
            rotate_spawn_offset_by_yaw: boolean(value, "RotateSpawnOffsetByYaw", true),
            physics,
        })
    }
}

/// One AABB target supplied by the world/NPC layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProjectileTarget {
    /// Network/entity id reported on a hit.
    pub id: u64,
    /// World-space box.
    pub aabb: Aabb,
}

/// Yaw/pitch/roll orientation of a flying projectile.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProjectileRotation {
    /// Game yaw.
    pub yaw: f64,
    /// Game pitch.
    pub pitch: f64,
    /// Roll (not simulated).
    pub roll: f64,
}

/// Lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProjectileState {
    /// Flying and colliding.
    #[default]
    Flying,
    /// Stuck/resting after a slow impact.
    Resting,
    /// Removed (death timer or TTL).
    Dead,
}

/// One event produced by a step.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectileEvent {
    /// Hit an entity target.
    Hit {
        /// Target id.
        target: u64,
        /// Impact position.
        position: Vec3,
    },
    /// Hit a block without sticking; the projectile bounced/continued.
    Bounce {
        /// Impact position.
        position: Vec3,
    },
    /// Hit a block and came to rest (or missed a target).
    Miss {
        /// Impact position.
        position: Vec3,
    },
    /// Lifetime or dead timer expired.
    Expired {
        /// Final position.
        position: Vec3,
    },
}

/// Result of one [`ProjectileBody::step`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileStep {
    /// Events in resolution order.
    pub events: Vec<ProjectileEvent>,
    /// Current state.
    pub state: ProjectileState,
    /// Current position.
    pub position: Vec3,
    /// Current velocity.
    pub velocity: Vec3,
    /// Current rotation.
    pub rotation: ProjectileRotation,
}

/// A deterministic flying projectile.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectileBody {
    /// Asset values.
    pub asset: ProjectileAsset,
    /// Creator entity id (damage attribution).
    pub creator: u64,
    /// Feet position.
    pub position: Vec3,
    /// Velocity.
    pub velocity: Vec3,
    /// Orientation.
    pub rotation: ProjectileRotation,
    /// Lifecycle state.
    pub state: ProjectileState,
    /// Seconds since launch.
    pub age: f64,
    /// Remaining death timer, when set.
    pub dead_timer: Option<f64>,
    /// Bounces so far.
    pub bounces: i32,
    /// Target hit by the last hit event.
    pub last_hit: Option<u64>,
}

impl ProjectileBody {
    /// Launch a projectile from `position` with Java spawn-offset/velocity math.
    pub fn launch(
        asset: ProjectileAsset,
        creator: u64,
        position: Vec3,
        yaw: f64,
        pitch: f64,
    ) -> Self {
        let velocity = asset.launch_velocity(yaw, pitch);
        Self {
            position: position.plus(asset.start_offset(yaw, pitch)),
            velocity,
            rotation: ProjectileRotation {
                yaw,
                pitch,
                roll: 0.0,
            },
            asset,
            creator,
            state: ProjectileState::Flying,
            age: 0.0,
            dead_timer: None,
            bounces: 0,
            last_hit: None,
        }
    }

    /// Whether the projectile is still in the world.
    pub fn is_alive(&self) -> bool {
        self.state != ProjectileState::Dead
    }

    /// Damage produced by a hit on a living entity.
    pub fn damage_instance(&self) -> Damage {
        Damage::new(
            self.asset.damage.max(0) as f32,
            "Projectile",
            DamageSource::Projectile(self.creator),
        )
    }

    /// Advance one simulation step.
    pub fn step(
        &mut self,
        world: &dyn CollisionWorld,
        targets: &[ProjectileTarget],
        dt: f64,
    ) -> ProjectileStep {
        let mut events = Vec::new();
        if self.state == ProjectileState::Dead || !dt.is_finite() || dt <= 0.0 {
            return self.snapshot(events);
        }

        if let Some(remaining) = &mut self.dead_timer {
            *remaining -= dt;
            if *remaining <= 0.0 {
                self.state = ProjectileState::Dead;
                events.push(ProjectileEvent::Expired {
                    position: self.position,
                });
                return self.snapshot(events);
            }
        }

        self.age += dt;
        if self.asset.time_to_live > 0.0 && self.age >= self.asset.time_to_live {
            self.state = ProjectileState::Dead;
            events.push(ProjectileEvent::Expired {
                position: self.position,
            });
            return self.snapshot(events);
        }

        if self.state == ProjectileState::Resting {
            return self.snapshot(events);
        }

        // Gravity and the terminal-velocity cap (Java's legacy comment says the
        // ballistic path is a speed cap, not a full drag solve).
        self.velocity.y -= self.asset.gravity * dt;
        let speed_squared = self.velocity.length_squared();
        if self.asset.terminal_velocity > 0.0
            && speed_squared > self.asset.terminal_velocity * self.asset.terminal_velocity
        {
            let scale = self.asset.terminal_velocity / speed_squared.sqrt();
            self.velocity = self.velocity.scale(scale);
        }

        let start = self.position;
        let delta = self.velocity.scale(dt);
        let start_box = self.asset.aabb(start);
        let block_hit = sweep_aabb(world, start_box, delta);
        let entity_hit = targets
            .iter()
            .filter_map(|target| {
                sweep_aabb_box(start_box, delta, target.aabb).map(|hit| (target, hit))
            })
            .min_by(|(_, left), (_, right)| left.t.total_cmp(&right.t));

        let (hit_t, normal, target) = match (entity_hit, block_hit) {
            (Some((target, hit)), Some(block)) if hit.t <= block.t => {
                (hit.t, hit.normal, Some(target.id))
            }
            (Some((target, hit)), None) => (hit.t, hit.normal, Some(target.id)),
            (_, Some(block)) => (block.t, block.normal, None),
            (None, None) => {
                self.position = start.plus(delta);
                self.update_rotation();
                return self.snapshot(events);
            }
        };

        let point = start.plus(delta.scale(hit_t.clamp(0.0, 1.0)));
        self.position = point;

        if let Some(target) = target {
            self.velocity = Vec3::default();
            self.state = ProjectileState::Resting;
            self.last_hit = Some(target);
            events.push(ProjectileEvent::Hit {
                target,
                position: point,
            });
            self.dead_timer = Some(self.asset.dead_time.max(0.0));
            return self.snapshot(events);
        }

        // Reflect around the block contact normal, then damp by Java's
        // `bounciness * (1 - impactSlowdown)`.
        let speed = self.velocity.length_squared().sqrt();
        let reflected = if speed > 0.0 {
            self.velocity
                .minus(normal.scale(2.0 * dot(self.velocity, normal)))
                .scale(self.asset.effective_bounciness())
        } else {
            Vec3::default()
        };
        let bounce_speed = reflected.length_squared().sqrt();
        let min_bounce = 0.4;
        if bounce_speed * dt < min_bounce {
            self.velocity = Vec3::default();
            let ground = normal.y > 0.5;
            if self.asset.sticks_vertically || ground {
                self.state = ProjectileState::Resting;
                events.push(ProjectileEvent::Miss { position: point });
                self.dead_timer = Some(self.asset.dead_time_miss.max(0.0));
            } else {
                events.push(ProjectileEvent::Bounce { position: point });
            }
        } else {
            self.velocity = reflected;
            self.bounces += 1;
            events.push(ProjectileEvent::Bounce { position: point });
            // Move the remaining fraction of the step so the projectile does not
            // lose distance at a glancing impact.
            if hit_t < 1.0 {
                let remaining = delta.scale(1.0 - hit_t);
                if let Some(next) = sweep_aabb(world, self.asset.aabb(self.position), remaining) {
                    self.position = self.position.plus(remaining.scale(next.t.clamp(0.0, 1.0)));
                } else {
                    self.position = self.position.plus(remaining);
                }
            }
        }
        self.update_rotation();
        self.snapshot(events)
    }

    fn snapshot(&self, events: Vec<ProjectileEvent>) -> ProjectileStep {
        ProjectileStep {
            events,
            state: self.state,
            position: self.position,
            velocity: self.velocity,
            rotation: self.rotation,
        }
    }

    fn update_rotation(&mut self) {
        if !(self.asset.compute_yaw || self.asset.compute_pitch) {
            return;
        }
        let speed_squared = self.velocity.length_squared();
        if speed_squared <= 1e-8 {
            return;
        }
        if self.asset.compute_yaw {
            self.rotation.yaw = (-self.velocity.x).atan2(-self.velocity.z);
        }
        if self.asset.compute_pitch {
            self.rotation.pitch = self.velocity.y.atan2(
                (self.velocity.x * self.velocity.x + self.velocity.z * self.velocity.z).sqrt(),
            );
        }
    }
}

fn dot(left: Vec3, right: Vec3) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

/// Java `PhysicsMath.vectorFromAngles`.
pub fn direction_from_angles(yaw: f64, pitch: f64) -> Vec3 {
    let cos_pitch = pitch.cos();
    Vec3::new(-yaw.sin() * cos_pitch, pitch.sin(), -yaw.cos() * cos_pitch)
}

/// One swept-box contact.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepHit {
    /// Entry fraction in `[0, 1]`.
    pub t: f64,
    /// Unit contact normal.
    pub normal: Vec3,
}

/// Sweep `start_box` through `delta` against the world's block boxes.
pub fn sweep_aabb(world: &dyn CollisionWorld, start_box: Aabb, delta: Vec3) -> Option<SweepHit> {
    let end_box = start_box.translated(delta);
    let min = Vec3::new(
        start_box.min.x.min(end_box.min.x),
        start_box.min.y.min(end_box.min.y),
        start_box.min.z.min(end_box.min.z),
    );
    let max = Vec3::new(
        start_box.max.x.max(end_box.max.x),
        start_box.max.y.max(end_box.max.y),
        start_box.max.z.max(end_box.max.z),
    );
    let mut boxes = Vec::new();
    for x in (min.x.floor() as i32 - 1)..=(max.x.floor() as i32 + 1) {
        for y in (min.y.floor() as i32 - 1)..=(max.y.floor() as i32 + 1) {
            for z in (min.z.floor() as i32 - 1)..=(max.z.floor() as i32 + 1) {
                world.collect_block_boxes(x, y, z, &mut boxes);
            }
        }
    }
    boxes
        .into_iter()
        .filter_map(|candidate| sweep_aabb_box(start_box, delta, candidate))
        .min_by(|left, right| left.t.total_cmp(&right.t))
}

fn sweep_aabb_box(start_box: Aabb, delta: Vec3, obstacle: Aabb) -> Option<SweepHit> {
    let half = Vec3::new(
        (start_box.max.x - start_box.min.x) * 0.5,
        (start_box.max.y - start_box.min.y) * 0.5,
        (start_box.max.z - start_box.min.z) * 0.5,
    );
    let center = Vec3::new(
        (start_box.min.x + start_box.max.x) * 0.5,
        (start_box.min.y + start_box.max.y) * 0.5,
        (start_box.min.z + start_box.max.z) * 0.5,
    );
    let expanded_min = obstacle.min.minus(half);
    let expanded_max = obstacle.max.plus(half);

    let mut enter = 0.0_f64;
    let mut exit = f64::INFINITY;
    let mut normal = Vec3::default();
    for axis in 0..3 {
        let origin = center.axis(axis);
        let direction = delta.axis(axis);
        let low = expanded_min.axis(axis);
        let high = expanded_max.axis(axis);
        if direction.abs() <= f64::EPSILON {
            if origin < low || origin > high {
                return None;
            }
            continue;
        }
        let mut near = (low - origin) / direction;
        let mut far = (high - origin) / direction;
        let mut axis_normal = Vec3::default().with_axis(axis, -direction.signum());
        if near > far {
            std::mem::swap(&mut near, &mut far);
            axis_normal = Vec3::default().with_axis(axis, direction.signum());
        }
        if near > enter {
            enter = near;
            normal = axis_normal;
        }
        exit = exit.min(far);
        if enter > exit {
            return None;
        }
    }
    if exit < 0.0 || enter > 1.0 {
        return None;
    }
    Some(SweepHit {
        t: enter.max(0.0),
        normal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestWorld {
        boxes: Vec<Aabb>,
    }

    impl TestWorld {
        fn add_block(&mut self, x: i32, y: i32, z: i32) {
            self.boxes.push(Aabb::unit().translated(Vec3::new(
                f64::from(x),
                f64::from(y),
                f64::from(z),
            )));
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

    fn asset() -> ProjectileAsset {
        ProjectileAsset {
            id: "Test".to_string(),
            appearance: None,
            radius: 0.1,
            height: 0.2,
            vertical_center_shot: 0.0,
            horizontal_center_shot: 0.0,
            depth_shot: 0.0,
            pitch_adjust_shot: false,
            muzzle_velocity: 20.0,
            terminal_velocity: 50.0,
            gravity: 10.0,
            bounciness: 0.0,
            impact_slowdown: 0.0,
            sticks_vertically: true,
            compute_yaw: true,
            compute_pitch: true,
            compute_roll: true,
            rotation_mode: RotationMode::Velocity,
            time_to_live: 10.0,
            damage: 15,
            dead_time: 0.0,
            dead_time_miss: 0.0,
            death_effects_on_hit: false,
            density: 2000.0,
            water_terminal_velocity_multiplier: 1.0,
            water_hit_impulse_loss: 0.0,
            damping_rotation: 0.0,
            rotation_speed_velocity_ratio: 2.0,
            swimming_damping_factor: 1.0,
            bounce_sound_event_id: None,
            hit_sound_event_id: None,
            miss_sound_event_id: None,
            death_sound_event_id: None,
            has_bounce_particles: false,
            has_hit_particles: false,
            has_miss_particles: false,
            has_death_particles: false,
            has_explosion_config: false,
        }
    }

    fn flat_wall() -> TestWorld {
        let mut world = TestWorld::default();
        for x in -4..=4 {
            for y in 0..4 {
                for z in 0..=4 {
                    if z == 4 {
                        world.add_block(x, y, z);
                    }
                }
            }
        }
        world
    }

    #[test]
    fn launch_offset_and_velocity_use_java_angles() {
        let mut asset = asset();
        asset.muzzle_velocity = 10.0;
        asset.depth_shot = 1.0;
        asset.horizontal_center_shot = 0.2;
        asset.vertical_center_shot = 0.1;
        asset.pitch_adjust_shot = true;
        let body = ProjectileBody::launch(asset, 7, Vec3::new(1.0, 2.0, 3.0), 0.0, 0.5);
        let offset = Vec3::new(0.2, -0.1 + 0.5f64.sin(), -0.5f64.cos());
        assert!((body.position.x - (1.0 + offset.x)).abs() < 1e-9);
        assert!((body.position.y - (2.0 + offset.y)).abs() < 1e-9);
        assert!((body.position.z - (3.0 + offset.z)).abs() < 1e-9);
        assert!((body.velocity.x - 0.0).abs() < 1e-9);
        assert!((body.velocity.y - 10.0 * 0.5f64.sin()).abs() < 1e-9);
        assert!((body.velocity.z + 10.0 * 0.5f64.cos()).abs() < 1e-9);
    }

    #[test]
    fn gravity_caps_speed_and_flight_moves_forward() {
        let mut asset = asset();
        asset.gravity = 100.0;
        asset.terminal_velocity = 5.0;
        asset.time_to_live = 10.0;
        let mut body = ProjectileBody::launch(asset, 1, Vec3::new(0.5, 5.0, 0.5), 0.0, 0.0);
        let world = TestWorld::default();
        for _ in 0..10 {
            body.step(&world, &[], 0.1);
        }
        assert!(body.velocity.length_squared().sqrt() <= 5.0 + 1e-9);
        assert!(body.position.z < 0.5);
    }

    #[test]
    fn a_wall_hit_comes_to_rest_and_a_target_hit_reports_damage() {
        let world = flat_wall();
        let yaw = std::f64::consts::PI;
        let mut body = ProjectileBody::launch(asset(), 9, Vec3::new(0.5, 1.0, 0.5), yaw, 0.0);
        let mut events = Vec::new();
        for _ in 0..20 {
            events.extend(body.step(&world, &[], 0.05).events);
            if !body.is_alive() {
                break;
            }
        }
        assert!(events
            .iter()
            .any(|event| matches!(event, ProjectileEvent::Miss { .. })));
        assert_eq!(body.state, ProjectileState::Dead);
        assert_eq!(body.damage_instance().amount, 15.0);
        assert_eq!(body.damage_instance().cause, "Projectile");

        let mut body = ProjectileBody::launch(asset(), 9, Vec3::new(0.5, 1.0, 0.5), yaw, 0.0);
        let target = ProjectileTarget {
            id: 42,
            aabb: Aabb::unit().translated(Vec3::new(0.0, 0.0, 2.0)),
        };
        let mut hit = None;
        for _ in 0..20 {
            for event in body.step(&world, &[target], 0.05).events {
                if let ProjectileEvent::Hit { target, position } = event {
                    hit = Some((target, position));
                }
            }
        }
        let (target, _) = hit.expect("entity hit");
        assert_eq!(target, 42);
        assert_eq!(body.last_hit, Some(42));
    }

    #[test]
    fn ttl_and_dead_timer_expire() {
        let world = TestWorld::default();
        let mut asset = asset();
        asset.time_to_live = 0.2;
        let mut body = ProjectileBody::launch(asset, 1, Vec3::new(0.5, 5.0, 0.5), 0.0, 0.0);
        let step = body.step(&world, &[], 0.3);
        assert_eq!(step.state, ProjectileState::Dead);
        assert!(matches!(
            step.events.as_slice(),
            [ProjectileEvent::Expired { .. }]
        ));
    }
}
