//! NPC combat profile and runtime actor.
//!
//! Role documents carry the combat-relevant parameters (`MaxHealth`,
//! `ViewRange`, `AttackDistance`, `TargetRange`, speeds and `_InteractionVars`
//! damage calculators). This module resolves those into a
//! [`CombatProfile`], builds an [`EntityGameplay`] stat map from the real
//! `EntityStatType` store, and drives a small deterministic [`NpcActor`]:
//! target acquisition, path following, melee attacks, damage and death.
//!
//! The `ActionAttack` selection/aiming/charge/LoS fields and the
//! `DamageEntity` damage graph are resolved from the real interaction catalog.
//! Java's `MotionController`/blackboard instruction graph is still not ported:
//! the actor is the scripted ECS seam the server attaches to runtime entities.

use std::collections::BTreeMap;

use hytale_assets::AssetRegistry;
use hytale_entities::components::EntityGameplay;
use hytale_entities::damage::{Damage, DamageCauses, DamageResult};
use hytale_entities::stats::{EntityStatMap, EntityStatType};
use hytale_interactions::catalog::InteractionCatalog;
use hytale_interactions::combat::{collect_attack_hits, AttackHit};
use hytale_world::collision::Vec3;
use hytale_world::runtime::RuntimeEntity;
use hytale_world::world::World;
use serde_json::Value;
use thiserror::Error;

use crate::navigation::BlockPos;
use crate::path::PathFollower;
use crate::role::{ResolvedRole, RoleCatalog, RoleError};
use crate::worldgrid::WorldGrid;

/// Seconds between live path replans.
pub const PATH_REPLAN_INTERVAL: f64 = 0.5;
/// Horizontal tolerance before a target is considered reached.
pub const TARGET_EPSILON: f64 = 1e-4;

pub use hytale_interactions::combat::{DamageCalculator, DamageEffects, DamageType};

/// Java `ActionAttack.AttackType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackType {
    /// Primary interaction.
    Primary,
    /// Secondary interaction.
    Secondary,
    /// Ability slot 1.
    Ability1,
    /// Ability slot 2.
    Ability2,
    /// Ability slot 3.
    Ability3,
    /// Ability slot 4.
    Ability4,
}

impl AttackType {
    /// Parse the Java enum spelling.
    pub fn from_name(name: &str) -> Self {
        match name {
            "Secondary" => Self::Secondary,
            "Ability1" => Self::Ability1,
            "Ability2" => Self::Ability2,
            "Ability3" => Self::Ability3,
            "Ability4" => Self::Ability4,
            _ => Self::Primary,
        }
    }

    /// Java enum spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Secondary => "Secondary",
            Self::Ability1 => "Ability1",
            Self::Ability2 => "Ability2",
            Self::Ability3 => "Ability3",
            Self::Ability4 => "Ability4",
        }
    }
}

/// Java `ActionAttack.BallisticMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BallisticMode {
    /// Shorter flight curve.
    Short,
    /// Longer flight curve.
    Long,
    /// Alternate between short and long.
    Alternate,
    /// Random short or long.
    Random,
}

impl BallisticMode {
    /// Parse the Java enum spelling.
    pub fn from_name(name: &str) -> Self {
        match name {
            "Long" => Self::Long,
            "Alternate" => Self::Alternate,
            "Random" => Self::Random,
            _ => Self::Short,
        }
    }
}

/// One resolved Java `ActionAttack` node (`"Type": "Attack"`).
///
/// A role usually has one attack action in its instruction tree, but bosses use
/// several (for example intro/transition attacks). The scripted actor picks the
/// first action with an attack root, which matches the primary melee path the
/// T59 runtime needs.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackAction {
    /// Root interaction key. `None` means "use the item/explicit override".
    pub attack: Option<String>,
    /// Interaction slot.
    pub attack_type: AttackType,
    /// Charge time in seconds.
    pub charge_for: f32,
    /// Minimum pause range after the attack.
    pub attack_pause_range: [f64; 2],
    /// Range from which the max aiming time is sampled.
    pub aiming_time_range: [f64; 2],
    /// Half-angle used for the on-target check (radians).
    pub melee_cone_angle: f32,
    /// Ballistic trajectory preference.
    pub ballistic_mode: BallisticMode,
    /// Require line of sight before firing.
    pub check_line_of_sight: bool,
    /// Avoid friendly entities blocking the line of sight.
    pub avoid_friendly_fire: bool,
    /// Allow damage to friendlies.
    pub damage_friendlies: bool,
    /// Skip the aiming phase.
    pub skip_aiming: bool,
    /// Distance required before a charge attack may fire.
    pub charge_distance: f64,
    /// Per-action interaction-var overrides.
    pub interaction_vars: BTreeMap<String, Value>,
}

impl AttackAction {
    /// Decode an `"Type": "Attack"` action. `fallback_root` is the role's
    /// top-level `Attack` parameter.
    pub fn from_value(value: &Value, fallback_root: Option<&str>) -> Self {
        let attack = value
            .get("Attack")
            .and_then(Value::as_str)
            .filter(|attack| !attack.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| fallback_root.map(ToOwned::to_owned));
        Self {
            attack,
            attack_type: value
                .get("AttackType")
                .and_then(Value::as_str)
                .map(AttackType::from_name)
                .unwrap_or(AttackType::Primary),
            charge_for: value
                .get("ChargeFor")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            attack_pause_range: f64_pair(value.get("AttackPauseRange")).unwrap_or([0.0, 0.0]),
            aiming_time_range: f64_pair(value.get("AimingTimeRange")).unwrap_or([0.0, 0.0]),
            melee_cone_angle: value
                .get("MeleeConeAngle")
                .and_then(Value::as_f64)
                .unwrap_or(30.0)
                .to_radians() as f32
                / 2.0,
            ballistic_mode: value
                .get("BallisticMode")
                .and_then(Value::as_str)
                .map(BallisticMode::from_name)
                .unwrap_or(BallisticMode::Short),
            check_line_of_sight: value
                .get("LineOfSight")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            avoid_friendly_fire: value
                .get("AvoidFriendlyFire")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            damage_friendlies: value
                .get("DamageFriendlies")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            skip_aiming: value
                .get("SkipAiming")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            charge_distance: value
                .get("ChargeDistance")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            interaction_vars: value
                .get("InteractionVars")
                .and_then(Value::as_object)
                .map(|vars| {
                    vars.iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Java `newAimingTime`: sample the max aiming time range.
    pub fn aiming_time(&self, random_unit: f64) -> f64 {
        let [low, high] = self.aiming_time_range;
        if high <= 0.0 {
            0.0
        } else {
            low + (high - low) * random_unit.clamp(0.0, 1.0)
        }
    }

    /// Java `newAttackPause`: sample the pause range.
    pub fn attack_pause(&self, random_unit: f64) -> f64 {
        let [low, high] = self.attack_pause_range;
        (low + (high - low) * random_unit.clamp(0.0, 1.0)).max(0.0)
    }
}

/// The combat-relevant slice of a resolved role.
#[derive(Debug, Clone, PartialEq)]
pub struct CombatProfile {
    /// Java `MaxHealth`.
    pub max_health: f32,
    /// Java `ViewRange`.
    pub view_range: f64,
    /// Java `HearingRange`.
    pub hearing_range: f64,
    /// Java `AttackDistance`.
    pub attack_distance: f64,
    /// Java `TargetRange`.
    pub target_range: f64,
    /// Java `MaxSpeed`.
    pub max_speed: f64,
    /// Java `ChaseRelativeSpeed`.
    pub chase_relative_speed: f64,
    /// Java `LeashDistance`.
    pub leash_distance: f64,
    /// Java `HardLeashDistance`.
    pub hard_leash_distance: f64,
    /// Java `AttackPauseRange`, default `[1.0, 1.0]`.
    pub attack_pause_range: [f64; 2],
    /// Root interaction named by the role's `Attack` parameter.
    pub attack_root: Option<String>,
    /// Resolved `"Type": "Attack"` actions from the role instruction tree.
    pub attack_actions: Vec<AttackAction>,
    /// Role-level interaction variables used by the attack graph.
    pub interaction_vars: BTreeMap<String, Value>,
    /// Reachable interaction damage calculators (role-template fallback).
    pub attacks: Vec<DamageCalculator>,
}

impl CombatProfile {
    /// Resolve a combat profile from a T58 resolved role.
    pub fn from_role(role: &ResolvedRole) -> Result<Self, CombatError> {
        let max_health = role.max_health().map(|value| value as f32).ok_or_else(|| {
            CombatError::MissingField {
                role: role.key.clone(),
                field: "MaxHealth",
            }
        })?;
        let number = |name: &str| -> Option<f64> {
            role.raw
                .get(name)
                .and_then(Value::as_f64)
                .or_else(|| role.parameter(name).and_then(Value::as_f64))
        };
        let attacks = collect_damage_calculators(&role.raw);
        let attack_root = role
            .raw
            .get("Attack")
            .and_then(Value::as_str)
            .filter(|attack| !attack.is_empty())
            .map(ToOwned::to_owned);
        let attack_actions = collect_attack_actions(&role.raw, attack_root.as_deref());
        let interaction_vars = collect_interaction_vars(&role.raw);
        Ok(Self {
            max_health,
            view_range: number("ViewRange").unwrap_or(10.0),
            hearing_range: number("HearingRange").unwrap_or(5.0),
            attack_distance: number("AttackDistance").unwrap_or(2.0),
            target_range: number("TargetRange").unwrap_or(2.0),
            max_speed: number("MaxSpeed").unwrap_or(1.0),
            chase_relative_speed: number("ChaseRelativeSpeed").unwrap_or(1.0),
            leash_distance: number("LeashDistance").unwrap_or(50.0),
            hard_leash_distance: number("HardLeashDistance").unwrap_or(80.0),
            attack_pause_range: f64_pair(role.raw.get("AttackPauseRange")).unwrap_or([1.0, 1.0]),
            attack_root,
            attack_actions,
            interaction_vars,
            attacks,
        })
    }

    /// The primary `ActionAttack` for scripted/runtime combat.
    ///
    /// The first action with an attack root is the primary melee path used by
    /// ordinary creatures. Roles without an instruction-tree action fall back
    /// to the top-level `Attack` root.
    pub fn attack_action(&self) -> Option<AttackAction> {
        // The role's top-level `Attack` parameter is authoritative for the
        // primary combat path. Instruction-tree actions can be intro/phase
        // attacks; for those, use the role root when they don't name one.
        if let Some(root) = &self.attack_root {
            if let Some(action) = self
                .attack_actions
                .iter()
                .find(|action| action.attack.as_deref() == Some(root.as_str()))
            {
                return Some(action.clone());
            }
            if let Some(mut action) = self.attack_actions.first().cloned() {
                action.attack = Some(root.clone());
                return Some(action);
            }
            return Some(AttackAction {
                attack: Some(root.clone()),
                attack_type: AttackType::Primary,
                charge_for: 0.0,
                attack_pause_range: self.attack_pause_range,
                aiming_time_range: [0.0, 0.0],
                melee_cone_angle: 15.0_f32.to_radians(),
                ballistic_mode: BallisticMode::Short,
                check_line_of_sight: false,
                avoid_friendly_fire: false,
                damage_friendlies: false,
                skip_aiming: true,
                charge_distance: 0.0,
                interaction_vars: BTreeMap::new(),
            });
        }
        self.attack_actions
            .iter()
            .find(|action| action.attack.is_some())
            .cloned()
    }

    /// Resolve the attack graph for one action. Role-level vars are overlaid by
    /// action-level vars, matching Java `ActionAttack.getInteractionVars`.
    pub fn attack_hits(
        &self,
        interactions: &InteractionCatalog,
        action: &AttackAction,
    ) -> Vec<AttackHit> {
        let Some(root) = action.attack.as_deref() else {
            return Vec::new();
        };
        let mut vars = self.interaction_vars.clone();
        vars.extend(action.interaction_vars.clone());
        collect_attack_hits(interactions, root, &vars)
    }

    /// The highest-damage interaction. Scripted combat uses this as the
    /// primary melee attack; the full `ActionAttack` selector is T59b.
    ///
    /// Templates define a generic `Melee_Damage` fallback that many variants
    /// override with named attack vars. When a role has any non-generic
    /// calculator, the generic one is ignored.
    pub fn primary_attack(&self) -> Option<&DamageCalculator> {
        let specific = || {
            self.attacks
                .iter()
                .filter(|attack| attack.source.as_deref() != Some("Melee_Damage"))
        };
        let mut candidates = specific().peekable();
        let pool: Vec<&DamageCalculator> = if candidates.peek().is_some() {
            candidates.collect()
        } else {
            self.attacks.iter().collect()
        };
        pool.into_iter().max_by(|left, right| {
            left.maximum_damage(1.0)
                .partial_cmp(&right.maximum_damage(1.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// Java `ChaseRelativeSpeed * MaxSpeed`.
    pub fn chase_speed(&self) -> f64 {
        (self.max_speed * self.chase_relative_speed).max(0.0)
    }

    /// Deterministic attack delay (midpoint of `AttackPauseRange`).
    pub fn attack_pause(&self) -> f64 {
        let [low, high] = self.attack_pause_range;
        ((low + high) * 0.5).max(0.0)
    }
}

/// A live NPC actor owned by the world runtime.
#[derive(Debug, Clone)]
pub struct NpcActor {
    /// Role asset key.
    pub role: String,
    /// Appearance/model key.
    pub appearance: Option<String>,
    /// Display name/nameplate text.
    pub name: Option<String>,
    /// World position (feet).
    pub position: Vec3,
    /// Yaw in radians.
    pub yaw: f32,
    /// Stats/effects/death state.
    pub gameplay: EntityGameplay,
    /// Combat profile.
    pub profile: CombatProfile,
    /// Current target id.
    pub target: Option<u64>,
    /// Path follower.
    pub path: PathFollower,
    /// Attack cooldown in seconds.
    pub attack_cooldown: f64,
    /// Aiming countdown before the next attack.
    pub aiming_remaining: f64,
    /// Charge countdown before the next attack.
    pub charge_remaining: f64,
    /// Path replan countdown.
    pub replan_cooldown: f64,
    /// Optional interaction catalog used to resolve `ActionAttack` roots.
    #[doc(hidden)]
    pub interactions: Option<InteractionCatalog>,
    last_target_position: Option<Vec3>,
}

/// One target the actor may pursue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NpcTarget {
    /// Stable entity id.
    pub id: u64,
    /// Feet position.
    pub position: Vec3,
}

/// One melee attack produced by [`NpcActor::tick`].
#[derive(Debug, Clone, PartialEq)]
pub struct NpcAttack {
    /// Target id.
    pub target: u64,
    /// Rolled damage by cause.
    pub damages: Vec<(String, f32)>,
    /// Root interaction that produced the damage.
    pub attack_root: Option<String>,
    /// `InteractionVars` key that selected the damage branch (for example
    /// `Bite_Damage`).
    pub source: Option<String>,
    /// Interaction runtime used to scale DPS calculators.
    pub runtime: f64,
    /// Knockback/sound/particle metadata from `DamageEffects`.
    pub effects: Option<DamageEffects>,
}

/// Result of one actor tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NpcTickReport {
    /// Actor position changed.
    pub moved: bool,
    /// Distance moved this tick.
    pub moved_distance: f64,
    /// Attack produced this tick.
    pub attack: Option<NpcAttack>,
    /// Current target id.
    pub target: Option<u64>,
    /// Actor is dead.
    pub died: bool,
}

impl NpcActor {
    /// Build an actor from a resolved role and the real asset stat/damage
    /// stores.
    pub fn from_assets(
        assets: &AssetRegistry,
        role: &ResolvedRole,
        position: Vec3,
        yaw: f32,
    ) -> Result<Self, CombatError> {
        Self::from_assets_with_interactions(assets, role, position, yaw, None)
    }

    /// Build an actor with the interaction catalog used by `ActionAttack`.
    pub fn from_assets_with_interactions(
        assets: &AssetRegistry,
        role: &ResolvedRole,
        position: Vec3,
        yaw: f32,
        interactions: Option<&InteractionCatalog>,
    ) -> Result<Self, CombatError> {
        let profile = CombatProfile::from_role(role)?;
        let (stats, health_index) = npc_stat_map(assets, profile.max_health)?;
        Ok(Self {
            role: role.key.clone(),
            appearance: role.appearance().map(ToOwned::to_owned),
            name: role
                .raw
                .get("NameTranslationKey")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            position,
            yaw,
            gameplay: EntityGameplay::new(stats, health_index),
            profile,
            target: None,
            path: PathFollower::default(),
            attack_cooldown: 0.0,
            aiming_remaining: 0.0,
            charge_remaining: 0.0,
            replan_cooldown: 0.0,
            interactions: interactions.cloned(),
            last_target_position: None,
        })
    }

    /// Attach/replace the attack interaction catalog.
    pub fn with_interactions(mut self, interactions: &InteractionCatalog) -> Self {
        self.interactions = Some(interactions.clone());
        self
    }

    /// Attach/replace the attack interaction catalog in place.
    pub fn set_interactions(&mut self, interactions: &InteractionCatalog) {
        self.interactions = Some(interactions.clone());
    }

    /// Rebuild an actor from a persisted runtime entity. Returns `None` when
    /// the entity has no `Npc` component or the role no longer resolves.
    pub fn from_runtime_entity(
        assets: &AssetRegistry,
        roles: &RoleCatalog,
        entity: &RuntimeEntity,
    ) -> Result<Option<Self>, CombatError> {
        Self::from_runtime_entity_with_interactions(assets, roles, entity, None)
    }

    /// Rebuild an actor and attach the attack interaction catalog.
    pub fn from_runtime_entity_with_interactions(
        assets: &AssetRegistry,
        roles: &RoleCatalog,
        entity: &RuntimeEntity,
        interactions: Option<&InteractionCatalog>,
    ) -> Result<Option<Self>, CombatError> {
        let Some(role_key) = entity
            .components
            .get("Npc")
            .and_then(|npc| npc.get("Role"))
            .and_then(Value::as_str)
        else {
            return Ok(None);
        };
        let mut rng = |low: f64, high: f64| (low + high) * 0.5;
        let Some(resolved) = roles.resolve_with_rng(role_key, &mut rng)? else {
            return Ok(None);
        };
        let mut actor = Self::from_assets_with_interactions(
            assets,
            &resolved,
            Vec3::new(
                entity.transform.position.x,
                entity.transform.position.y,
                entity.transform.position.z,
            ),
            entity.transform.rotation.yaw,
            interactions,
        )?;
        if let Some(health) = entity
            .components
            .get("Npc")
            .and_then(|npc| npc.get("Health"))
            .and_then(Value::as_f64)
        {
            if health <= 0.0 {
                return Ok(None);
            }
            let health = (health as f32).min(actor.profile.max_health);
            actor
                .gameplay
                .stats
                .set(actor.gameplay.health_index, health);
        }
        Ok(Some(actor))
    }

    /// Write the current health back into `entity.components` for persistence.
    pub fn sync_runtime_entity(&self, entity: &mut RuntimeEntity) {
        let health = self.health();
        if let Some(npc) = entity.components.get_mut("Npc") {
            npc["Health"] = Value::from(f64::from(health));
        }
    }

    /// Whether the actor has died.
    pub fn is_dead(&self) -> bool {
        self.gameplay.is_dead()
    }

    /// Current health.
    pub fn health(&self) -> f32 {
        self.gameplay.health()
    }

    /// Acquire the nearest target inside `ViewRange`.
    pub fn acquire_target(&mut self, targets: &[NpcTarget]) -> Option<u64> {
        let range_squared = self.profile.view_range * self.profile.view_range;
        let nearest = targets
            .iter()
            .filter(|target| target.id != 0 && !self.is_dead())
            .filter(|target| {
                let dx = target.position.x - self.position.x;
                let dz = target.position.z - self.position.z;
                dx * dx + dz * dz <= range_squared
            })
            .min_by(|left, right| {
                let left_distance = horizontal_distance_squared(self.position, left.position);
                let right_distance = horizontal_distance_squared(self.position, right.position);
                left_distance
                    .partial_cmp(&right_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|target| target.id);
        if nearest != self.target {
            self.target = nearest;
            self.path.clear_path();
            self.replan_cooldown = 0.0;
        }
        nearest
    }

    /// Tick targeting, path movement and attack cooldown.
    #[allow(clippy::too_many_arguments)]
    pub fn tick<F>(
        &mut self,
        dt: f64,
        world: &World,
        is_solid: F,
        targets: &[NpcTarget],
        random_unit: f64,
    ) -> NpcTickReport
    where
        F: Fn(i32) -> bool + Copy,
    {
        let mut report = NpcTickReport {
            target: self.target,
            died: self.is_dead(),
            ..NpcTickReport::default()
        };
        if self.is_dead() || !dt.is_finite() || dt <= 0.0 {
            return report;
        }

        if self.attack_cooldown > 0.0 {
            self.attack_cooldown = (self.attack_cooldown - dt).max(0.0);
        }

        if self
            .target
            .is_none_or(|id| !targets.iter().any(|target| target.id == id))
        {
            self.acquire_target(targets);
        }
        report.target = self.target;
        let Some(target) = self
            .target
            .and_then(|id| targets.iter().find(|target| target.id == id))
            .copied()
        else {
            return report;
        };

        let target_distance = distance(self.position, target.position);
        if target_distance <= self.profile.attack_distance {
            self.path.clear_path();
            if self.attack_cooldown > 0.0 {
                return report;
            }
            let Some(action) = self.profile.attack_action() else {
                if let Some(attack) = self.profile.primary_attack() {
                    report.attack = Some(NpcAttack {
                        target: target.id,
                        damages: attack.roll(1.0, random_unit),
                        attack_root: None,
                        source: attack.source.clone(),
                        runtime: 1.0,
                        effects: None,
                    });
                    self.attack_cooldown = self.profile.attack_pause();
                }
                return report;
            };
            if action.check_line_of_sight
                && !self.has_line_of_sight(world, target.position, is_solid)
            {
                self.aiming_remaining = 0.0;
                self.charge_remaining = 0.0;
                return report;
            }
            if action.charge_distance > 0.0 && target_distance > action.charge_distance {
                self.charge_remaining = 0.0;
                return report;
            }

            // Java `ActionAttack`: aim before launching. The scripted actor
            // consumes the sampled aiming time across ticks.
            if !action.skip_aiming {
                if self.aiming_remaining > 0.0 {
                    self.aiming_remaining = (self.aiming_remaining - dt).max(0.0);
                    if self.aiming_remaining > 0.0 {
                        return report;
                    }
                } else {
                    self.aiming_remaining = action.aiming_time(random_unit);
                    if self.aiming_remaining > 0.0 {
                        return report;
                    }
                }
            }
            if action.charge_for > 0.0 {
                if self.charge_remaining <= 0.0 {
                    self.charge_remaining = f64::from(action.charge_for);
                    return report;
                }
                self.charge_remaining = (self.charge_remaining - dt).max(0.0);
                if self.charge_remaining > 0.0 {
                    return report;
                }
            }

            if let Some(attack) = self.resolve_attack(&action, target, random_unit) {
                report.attack = Some(attack);
                let pause = if action.attack_pause_range == [0.0, 0.0] {
                    self.profile.attack_pause()
                } else {
                    action.attack_pause(random_unit)
                };
                self.attack_cooldown = pause;
                self.aiming_remaining = 0.0;
                self.charge_remaining = 0.0;
            }
            return report;
        }

        // Leaving attack range cancels the in-progress aim/charge.
        self.aiming_remaining = 0.0;
        self.charge_remaining = 0.0;

        self.replan_cooldown -= dt;
        let target_moved = self
            .last_target_position
            .is_none_or(|last| distance_squared(last, target.position) > TARGET_EPSILON);
        if self.path.is_empty() || self.replan_cooldown < 0.0 || target_moved {
            self.replan_path(world, is_solid, target.position);
            self.last_target_position = Some(target.position);
            self.replan_cooldown = PATH_REPLAN_INTERVAL;
        }

        let speed = self.profile.chase_speed();
        if speed <= 0.0 {
            return report;
        }
        let step = speed * dt;
        let before = self.position;
        let goal = self
            .path
            .current_waypoint_position()
            .unwrap_or(target.position);
        let offset = Vec3::new(
            goal.x - self.position.x,
            goal.y - self.position.y,
            goal.z - self.position.z,
        );
        let horizontal = (offset.x * offset.x + offset.z * offset.z).sqrt();
        if horizontal > TARGET_EPSILON {
            let scale = (step / horizontal).min(1.0);
            self.position = Vec3::new(
                self.position.x + offset.x * scale,
                self.position.y,
                self.position.z + offset.z * scale,
            );
            self.yaw = offset.x.atan2(offset.z) as f32;
        }
        // Advance waypoints whose centre we reached, including the final one.
        for _ in 0..8 {
            let Some(waypoint) = self.path.current_waypoint_position() else {
                break;
            };
            if horizontal_distance_squared(self.position, waypoint) > 0.25 {
                break;
            }
            self.path.update_current_target(self.position);
        }
        report.moved = distance_squared(before, self.position) > 0.0;
        report.moved_distance = distance(before, self.position);
        report
    }

    /// Resolve the damage for one attack action. The interaction graph is used
    /// when a catalog is attached; otherwise the T59 profile fallback fires.
    fn resolve_attack(
        &self,
        action: &AttackAction,
        target: NpcTarget,
        random_unit: f64,
    ) -> Option<NpcAttack> {
        let root = action
            .attack
            .clone()
            .or_else(|| self.profile.attack_root.clone());
        if let Some(interactions) = &self.interactions {
            let mut hits = self.profile.attack_hits(interactions, action);
            // Prefer role-specific vars over the generic template fallback,
            // then the strongest reachable attack. This mirrors the T59
            // primary-attack rule while still using the real damage node.
            hits.sort_by(|left, right| {
                let left_specific = left.source.as_deref() != Some("Melee_Damage");
                let right_specific = right.source.as_deref() != Some("Melee_Damage");
                right_specific.cmp(&left_specific).then_with(|| {
                    let left_max = left
                        .calculator()
                        .map_or(0.0, |calculator| calculator.maximum_damage(left.runtime));
                    let right_max = right
                        .calculator()
                        .map_or(0.0, |calculator| calculator.maximum_damage(right.runtime));
                    right_max
                        .partial_cmp(&left_max)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });
            for hit in hits {
                let (calculator, effects) = hit.select(0.0, None);
                let Some(calculator) = calculator.cloned() else {
                    continue;
                };
                let effects = effects.cloned();
                let damage = self.attack_damage(
                    target.id,
                    root.clone(),
                    hit,
                    &calculator,
                    effects.as_ref(),
                    random_unit,
                );
                return Some(damage);
            }
        }

        let calculator = self.profile.primary_attack()?;
        Some(NpcAttack {
            target: target.id,
            damages: calculator.roll(1.0, random_unit),
            attack_root: root,
            source: calculator.source.clone(),
            runtime: 1.0,
            effects: None,
        })
    }

    fn attack_damage(
        &self,
        target: u64,
        root: Option<String>,
        hit: AttackHit,
        calculator: &DamageCalculator,
        effects: Option<&DamageEffects>,
        random_unit: f64,
    ) -> NpcAttack {
        NpcAttack {
            target,
            damages: calculator.roll(hit.runtime.max(f64::EPSILON), random_unit),
            attack_root: root.or_else(|| Some(hit.interaction.clone())),
            source: hit.source,
            runtime: hit.runtime,
            effects: effects.cloned(),
        }
    }

    /// Whether the straight eye→target segment is clear. Java uses the
    /// position cache's line-of-sight test; this is the same block-solid
    /// approximation the T59 pathfinder uses.
    fn has_line_of_sight<F>(&self, world: &World, target: Vec3, is_solid: F) -> bool
    where
        F: Fn(i32) -> bool + Copy,
    {
        let start = Vec3::new(self.position.x, self.position.y + 1.6, self.position.z);
        let end = Vec3::new(target.x, target.y + 1.0, target.z);
        let offset = end.minus(start);
        let distance = offset.length_squared().sqrt();
        if distance <= TARGET_EPSILON {
            return true;
        }
        let steps = (distance / 0.25).ceil().max(1.0) as usize;
        for step in 1..steps {
            let factor = step as f64 / steps as f64;
            let point = start.plus(offset.scale(factor));
            let block = block_position(point);
            if is_solid(world.get_block(block.x, block.y, block.z)) {
                return false;
            }
        }
        true
    }

    fn replan_path<F>(&mut self, world: &World, is_solid: F, target: Vec3)
    where
        F: Fn(i32) -> bool + Copy,
    {
        let grid = WorldGrid::new(world, is_solid);
        let start = block_position(self.position);
        let goal = block_position(target);
        if let Some(path) = grid.plan_walk_path(start, goal) {
            self.path.set_path(path.waypoint_positions(), self.position);
        } else {
            self.path.clear_path();
        }
    }

    /// Apply a damage instance through the stat/effect/resistance core.
    pub fn take_damage(&mut self, damage: &Damage, causes: &DamageCauses) -> DamageResult {
        let result = self.gameplay.apply_damage(damage, causes);
        if result.died {
            self.path.clear_path();
            self.target = None;
        }
        result
    }

    /// Tick effects and regeneration.
    pub fn tick_gameplay(&mut self, dt: f32) {
        self.gameplay.tick(dt);
        if self.is_dead() {
            self.path.clear_path();
            self.target = None;
        }
    }

    /// Runtime entity component document (model/nameplate plus `Npc` state).
    pub fn runtime_components(&self) -> Value {
        let mut components = serde_json::Map::new();
        if let Some(appearance) = &self.appearance {
            components.insert(
                "Model".to_string(),
                serde_json::json!({"Model": {"Id": appearance, "Scale": 1.0}}),
            );
        }
        if let Some(name) = &self.name {
            components.insert("Nameplate".to_string(), serde_json::json!({"Text": name}));
        }
        components.insert(
            "Npc".to_string(),
            serde_json::json!({
                "Role": self.role,
                "Health": self.health(),
                "MaxHealth": self.profile.max_health,
            }),
        );
        Value::Object(components)
    }
}

/// Build an `EntityGameplay` stat map from the real `EntityStatType` store,
/// with the role's health maximum applied to the `Health` stat.
pub fn npc_stat_map(
    assets: &AssetRegistry,
    max_health: f32,
) -> Result<(EntityStatMap, i32), CombatError> {
    let mut stats = Vec::new();
    let mut health_index = None;
    for record in assets.entity_stat_types.iter() {
        let Some(index) = record.id else {
            continue;
        };
        let mut stat =
            EntityStatType::from_value(&record.key, index, &record.value).map_err(|message| {
                CombatError::InvalidStat {
                    stat: record.key.clone(),
                    message,
                }
            })?;
        if record.key.eq_ignore_ascii_case("Health") {
            stat.initial_value = max_health;
            stat.max = max_health;
            health_index = Some(index);
        }
        stats.push(stat);
    }
    let health_index = health_index.ok_or(CombatError::MissingHealthStat)?;
    Ok((EntityStatMap::new(stats), health_index))
}

fn collect_attack_actions(root: &Value, fallback_root: Option<&str>) -> Vec<AttackAction> {
    let mut actions = Vec::new();
    let mut stack = vec![root];
    let mut visited = 0usize;
    while let Some(value) = stack.pop() {
        visited += 1;
        if visited > 50_000 {
            break;
        }
        match value {
            Value::Array(values) => stack.extend(values),
            Value::Object(object) => {
                if object.get("Type").and_then(Value::as_str) == Some("Attack") {
                    let action = AttackAction::from_value(value, fallback_root);
                    if !actions.contains(&action) {
                        actions.push(action);
                    }
                }
                stack.extend(object.values());
            }
            _ => {}
        }
    }
    actions
}

fn collect_interaction_vars(root: &Value) -> BTreeMap<String, Value> {
    let mut vars = BTreeMap::new();
    for key in ["InteractionVars", "_InteractionVars"] {
        if let Some(entries) = root.get(key).and_then(Value::as_object) {
            for (name, value) in entries {
                vars.insert(name.clone(), value.clone());
            }
        }
    }
    vars
}

fn collect_damage_calculators(root: &Value) -> Vec<DamageCalculator> {
    let mut calculators = Vec::new();
    for key in ["InteractionVars", "_InteractionVars"] {
        let Some(vars) = root.get(key).and_then(Value::as_object) else {
            continue;
        };
        for (source, value) in vars {
            collect_from_value(value, Some(source), &mut calculators);
        }
    }
    if calculators.is_empty() {
        collect_from_value(root, None, &mut calculators);
    }
    calculators
}

fn collect_from_value(value: &Value, source: Option<&str>, out: &mut Vec<DamageCalculator>) {
    let mut stack = vec![value];
    let mut visited = 0usize;
    while let Some(value) = stack.pop() {
        visited += 1;
        if visited > 20_000 {
            break;
        }
        match value {
            Value::Array(values) => stack.extend(values),
            Value::Object(object) => {
                if let Some(mut calculator) = object
                    .get("DamageCalculator")
                    .and_then(DamageCalculator::from_value)
                {
                    calculator.source = source.map(ToOwned::to_owned);
                    if !out.contains(&calculator) {
                        out.push(calculator);
                    }
                }
                stack.extend(object.values());
            }
            _ => {}
        }
    }
}

fn block_position(position: Vec3) -> BlockPos {
    BlockPos::new(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

fn distance_squared(left: Vec3, right: Vec3) -> f64 {
    let dx = right.x - left.x;
    let dy = right.y - left.y;
    let dz = right.z - left.z;
    dx * dx + dy * dy + dz * dz
}

fn horizontal_distance_squared(left: Vec3, right: Vec3) -> f64 {
    let dx = right.x - left.x;
    let dz = right.z - left.z;
    dx * dx + dz * dz
}

fn distance(left: Vec3, right: Vec3) -> f64 {
    distance_squared(left, right).sqrt()
}

fn f64_pair(value: Option<&Value>) -> Option<[f64; 2]> {
    let values = value?.as_array()?;
    if values.len() != 2 {
        return None;
    }
    Some([values[0].as_f64()?, values[1].as_f64()?])
}

/// Combat profile/actor errors.
#[derive(Debug, Error)]
pub enum CombatError {
    /// A required role field is missing.
    #[error("role `{role}` has no `{field}`")]
    MissingField {
        /// Role key.
        role: String,
        /// Field name.
        field: &'static str,
    },
    /// The `Health` stat asset is missing.
    #[error("the EntityStatType store has no `Health` stat")]
    MissingHealthStat,
    /// One stat asset failed to decode.
    #[error("EntityStatType `{stat}`: {message}")]
    InvalidStat {
        /// Stat key.
        stat: String,
        /// Decode error.
        message: String,
    },
    /// A persisted role failed to resolve.
    #[error(transparent)]
    Role(#[from] RoleError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn damage(value: Value) -> DamageCalculator {
        DamageCalculator::from_value(&value).expect("damage calculator")
    }

    #[test]
    fn damage_ranges_and_rolls_match_java() {
        let calculator = damage(serde_json::json!({
            "Type": "Absolute",
            "BaseDamage": {"Physical": 10},
            "RandomPercentageModifier": 0.1
        }));
        assert_eq!(calculator.compute_range(1.0), [9.0, 11.0]);
        assert_eq!(
            calculator.roll(1.0, 0.0),
            vec![("Physical".to_string(), 9.0)]
        );
        assert_eq!(
            calculator.roll(1.0, 0.5),
            vec![("Physical".to_string(), 10.0)]
        );

        let dps = damage(serde_json::json!({
            "Type": "DPS",
            "BaseDamage": {"Fire": 2, "Physical": 1}
        }));
        assert_eq!(dps.compute_range(2.0), [6.0, 6.0]);
    }

    #[test]
    fn profile_uses_role_combat_fields_and_primary_damage() {
        let role = ResolvedRole {
            key: "Rat".to_string(),
            role_type: crate::role::RoleType::Variant,
            reference: None,
            parameters: BTreeMap::new(),
            raw: serde_json::json!({
                "MaxHealth": 10,
                "ViewRange": 8,
                "HearingRange": 10,
                "AttackDistance": 2,
                "TargetRange": 2,
                "MaxSpeed": 8,
                "ChaseRelativeSpeed": 0.6,
                "_InteractionVars": {
                    "Bite_Damage": {
                        "Interactions": [{
                            "Parent": "Rat_Bite_Damage",
                            "DamageCalculator": {
                                "Type": "Absolute",
                                "BaseDamage": {"Physical": 9},
                                "RandomPercentageModifier": 0.1
                            }
                        }]
                    }
                }
            }),
            unresolved_expressions: Vec::new(),
        };
        let profile = CombatProfile::from_role(&role).expect("profile");
        assert_eq!(profile.max_health, 10.0);
        assert_eq!(profile.view_range, 8.0);
        assert_eq!(profile.attack_distance, 2.0);
        assert!((profile.chase_speed() - 4.8).abs() < 1e-9);
        let range = profile.primary_attack().unwrap().compute_range(1.0);
        assert!((range[0] - 8.1).abs() < 1e-5);
        assert!((range[1] - 9.9).abs() < 1e-5);
    }

    #[test]
    fn actor_acquires_pursues_and_attacks_on_a_flat_world() {
        use hytale_world::chunk::column::ChunkColumn;
        use hytale_world::world::WorldStorage;

        let dir = std::env::temp_dir().join(format!(
            "hytale-npc-combat-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut world = World::new(WorldStorage::new(dir.clone()));
        let mut column = ChunkColumn::new(0, 0);
        for x in 0..=15 {
            for z in 0..=15 {
                column.set_block(x, 0, z, 1, 0, 0).unwrap();
            }
        }
        world.insert_chunk(column);

        let role = ResolvedRole {
            key: "Rat".to_string(),
            role_type: crate::role::RoleType::Variant,
            reference: None,
            parameters: BTreeMap::new(),
            raw: serde_json::json!({
                "MaxHealth": 10,
                "ViewRange": 20,
                "AttackDistance": 2,
                "MaxSpeed": 4,
                "ChaseRelativeSpeed": 1,
                "_InteractionVars": {
                    "Bite_Damage": {
                        "DamageCalculator": {
                            "BaseDamage": {"Physical": 3}
                        }
                    }
                }
            }),
            unresolved_expressions: Vec::new(),
        };
        let stats = EntityStatMap::new([EntityStatType {
            id: "Health".to_string(),
            index: 1,
            initial_value: 10.0,
            min: 0.0,
            max: 10.0,
            shared: true,
            regenerating: Vec::new(),
            reset_behavior: hytale_entities::stats::StatResetBehavior::MaxValue,
            ignore_invulnerability: false,
            hide_from_tooltip: false,
        }]);
        let mut actor = NpcActor {
            role: role.key.clone(),
            appearance: None,
            name: None,
            position: Vec3::new(0.5, 1.0, 0.5),
            yaw: 0.0,
            gameplay: EntityGameplay::new(stats, 1),
            profile: CombatProfile::from_role(&role).unwrap(),
            target: None,
            path: PathFollower::default(),
            attack_cooldown: 0.0,
            aiming_remaining: 0.0,
            charge_remaining: 0.0,
            replan_cooldown: 0.0,
            interactions: None,
            last_target_position: None,
        };
        let targets = [NpcTarget {
            id: 7,
            position: Vec3::new(8.5, 1.0, 0.5),
        }];
        let mut attacked = None;
        for _ in 0..20 {
            let report = actor.tick(0.5, &world, |block| block != 0, &targets, 0.5);
            if report.attack.is_some() {
                attacked = report.attack;
                break;
            }
        }
        let attack = attacked.expect("the actor reaches and attacks its target");
        assert_eq!(attack.target, 7);
        assert_eq!(attack.damages, vec![("Physical".to_string(), 3.0)]);
        std::fs::remove_dir_all(dir).ok();
    }
}
