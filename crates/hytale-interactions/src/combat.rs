//! Damage interactions and the `ActionAttack` damage path.
//!
//! Java NPC attacks are root interactions that chain through selectors and
//! `Replace` nodes until a `DamageEntity` interaction runs. This module ports
//! the deterministic data layer: the `DamageCalculator`, `DamageEffects`
//! (knockback/sound/particle metadata), angled/targeted overrides and a graph
//! walker that resolves the damage nodes a root interaction can reach.
//!
//! The graph walker is intentionally conservative: unknown interaction types
//! are traversed through their common child slots (`Next`, `Interactions`,
//! `HitEntity`, ...) so a new asset node does not hide the damage path. It does
//! not execute animations or packet chains; those remain presentation work.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::catalog::InteractionCatalog;

/// Java `DamageCalculator.Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DamageType {
    /// `Absolute`.
    Absolute,
    /// `DPS` (base damage is per second and scaled by the runtime).
    Dps,
}

impl DamageType {
    /// Decode the Java enum spelling.
    pub fn from_name(name: &str) -> Self {
        match name {
            "DPS" | "Dps" => Self::Dps,
            _ => Self::Absolute,
        }
    }
}

/// One `DamageCalculator` document.
#[derive(Debug, Clone, PartialEq)]
pub struct DamageCalculator {
    /// Absolute or DPS.
    pub damage_type: DamageType,
    /// Per-cause base damage.
    pub base_damage: BTreeMap<String, f32>,
    /// Java `RandomPercentageModifier` (`0.1` = -10%..+10%).
    pub random_percentage_modifier: f32,
    /// Optional `Class` (`Melee`, ...).
    pub damage_class: Option<String>,
    /// Java `SequentialModifierStep` (multiplier lost per extra hit).
    pub sequential_modifier_step: f32,
    /// Java `SequentialModifierMinimum` (floor for the sequential modifier).
    pub sequential_modifier_minimum: f32,
    /// `InteractionVars` key this calculator was reached from, when known.
    /// Used to prefer role-specific attacks over the generic template
    /// `Melee_Damage` fallback.
    pub source: Option<String>,
}

impl DamageCalculator {
    /// Decode one `DamageCalculator` document.
    pub fn from_value(value: &Value) -> Option<Self> {
        let damage_type = value
            .get("Type")
            .and_then(Value::as_str)
            .map(DamageType::from_name)
            .unwrap_or(DamageType::Absolute);
        let base_damage = value
            .get("BaseDamage")
            .and_then(Value::as_object)?
            .iter()
            .filter_map(|(cause, amount)| {
                amount.as_f64().map(|amount| (cause.clone(), amount as f32))
            })
            .collect::<BTreeMap<_, _>>();
        if base_damage.is_empty() {
            return None;
        }
        Some(Self {
            damage_type,
            base_damage,
            random_percentage_modifier: value
                .get("RandomPercentageModifier")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            damage_class: value
                .get("Class")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            sequential_modifier_step: value
                .get("SequentialModifierStep")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            sequential_modifier_minimum: value
                .get("SequentialModifierMinimum")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            source: None,
        })
    }

    /// Java `DamageCalculator.computeDamageRange`: total minimum/maximum
    /// damage across every cause for an interaction of `duration_seconds`.
    pub fn compute_range(&self, duration_seconds: f64) -> [f32; 2] {
        let scaled: f32 = self
            .base_damage
            .values()
            .map(|damage| self.scale(duration_seconds, *damage))
            .sum();
        [
            scaled * (1.0 - self.random_percentage_modifier),
            scaled * (1.0 + self.random_percentage_modifier),
        ]
    }

    /// Java `scaleDamage`.
    pub fn scale(&self, duration_seconds: f64, damage: f32) -> f32 {
        match self.damage_type {
            DamageType::Absolute => damage,
            DamageType::Dps => duration_seconds as f32 * damage,
        }
    }

    /// Java `DamageCalculator.calculateDamage` without the asset-map lookup.
    /// `random_unit` is in `[0, 1)`; each cause is rolled with the same
    /// percentage offset.
    pub fn roll(&self, duration_seconds: f64, random_unit: f64) -> Vec<(String, f32)> {
        let random_percentage = -self.random_percentage_modifier
            + 2.0 * self.random_percentage_modifier * random_unit as f32;
        self.base_damage
            .iter()
            .map(|(cause, base)| {
                let scaled = self.scale(duration_seconds, *base);
                (cause.clone(), scaled + scaled * random_percentage)
            })
            .collect()
    }

    /// Java `DamageCalculatorSystems.SequenceModifier` multiplier for a hit
    /// sequence. The first hit is unmodified (`hits == 0`).
    pub fn sequential_multiplier(&self, sequential_hits: u32) -> f32 {
        if sequential_hits == 0 {
            return 1.0;
        }
        (1.0 - self.sequential_modifier_step * sequential_hits as f32)
            .max(self.sequential_modifier_minimum)
    }

    /// Maximum possible total damage (`compute_range()[1]`).
    pub fn maximum_damage(&self, duration_seconds: f64) -> f32 {
        self.compute_range(duration_seconds)[1]
    }
}

/// One knockback definition (`Directional`, `Force` or `Point`).
#[derive(Debug, Clone, PartialEq)]
pub enum Knockback {
    /// No explicit `Type`; relative offsets around the attacker→target axis.
    Directional {
        /// Base force.
        force: f32,
        /// Right/left offset rotated by yaw.
        relative_x: f32,
        /// Vertical velocity.
        velocity_y: f32,
        /// Forward/back offset rotated by yaw.
        relative_z: f32,
        /// Continuous force duration.
        duration: f32,
        /// Java `ChangeVelocityType` (`Add`/`Set`).
        velocity_type: String,
    },
    /// Fixed direction rotated by yaw.
    Force {
        /// Base force.
        force: f32,
        /// Normalised local direction.
        direction: [f32; 3],
        /// Continuous force duration.
        duration: f32,
        /// Java `ChangeVelocityType`.
        velocity_type: String,
    },
    /// Push away from a point, with optional rotation/offset.
    Point {
        /// Base force.
        force: f32,
        /// Vertical velocity.
        velocity_y: f32,
        /// Rotation applied to the radial direction (degrees).
        rotate_y: i32,
        /// Attacker-space X offset of the source point.
        offset_x: i32,
        /// Attacker-space Z offset of the source point.
        offset_z: i32,
        /// Continuous force duration.
        duration: f32,
        /// Java `ChangeVelocityType`.
        velocity_type: String,
    },
}

impl Knockback {
    /// Decode one `DamageEffects.Knockback` document.
    pub fn from_value(value: &Value) -> Option<Self> {
        let force = value.get("Force").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let duration = value.get("Duration").and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let velocity_type = value
            .get("VelocityType")
            .and_then(Value::as_str)
            .unwrap_or("Add")
            .to_string();
        match value.get("Type").and_then(Value::as_str) {
            Some("Force") => {
                let direction = value
                    .get("Direction")
                    .and_then(Value::as_array)
                    .filter(|values| values.len() == 3)
                    .map(|values| {
                        [
                            values[0].as_f64().unwrap_or(0.0) as f32,
                            values[1].as_f64().unwrap_or(0.0) as f32,
                            values[2].as_f64().unwrap_or(0.0) as f32,
                        ]
                    })
                    .unwrap_or([0.0, 1.0, 0.0]);
                Some(Self::Force {
                    force,
                    direction: normalize3(direction),
                    duration,
                    velocity_type,
                })
            }
            Some("Point") => Some(Self::Point {
                force,
                velocity_y: value
                    .get("VelocityY")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
                rotate_y: value.get("RotateY").and_then(Value::as_i64).unwrap_or(0) as i32,
                offset_x: value.get("OffsetX").and_then(Value::as_i64).unwrap_or(0) as i32,
                offset_z: value.get("OffsetZ").and_then(Value::as_i64).unwrap_or(0) as i32,
                duration,
                velocity_type,
            }),
            _ => Some(Self::Directional {
                force,
                relative_x: value
                    .get("RelativeX")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
                velocity_y: value
                    .get("VelocityY")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
                relative_z: value
                    .get("RelativeZ")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
                duration,
                velocity_type,
            }),
        }
    }

    /// Java `Knockback.calculateVector(source, yaw, target)`.
    pub fn calculate_vector(&self, source: [f64; 3], yaw: f32, target: [f64; 3]) -> [f64; 3] {
        match self {
            Self::Directional {
                force,
                relative_x,
                velocity_y,
                relative_z,
                ..
            } => {
                let mut vector = [source[0] - target[0], 0.0, source[2] - target[2]];
                let length = (vector[0] * vector[0] + vector[2] * vector[2]).sqrt();
                if length <= 1e-4 {
                    vector = rotate_y([0.0, 0.0, -1.0], yaw);
                } else {
                    vector[0] /= length;
                    vector[2] /= length;
                }
                if *relative_x != 0.0 || *relative_z != 0.0 {
                    let rotation = rotate_y([*relative_x as f64, 0.0, *relative_z as f64], yaw);
                    vector[0] += rotation[0];
                    vector[2] += rotation[2];
                }
                [
                    vector[0] * f64::from(*force),
                    f64::from(*velocity_y),
                    vector[2] * f64::from(*force),
                ]
            }
            Self::Force {
                force, direction, ..
            } => {
                let rotated = rotate_y(
                    [
                        f64::from(direction[0]),
                        f64::from(direction[1]),
                        f64::from(direction[2]),
                    ],
                    yaw,
                );
                [
                    rotated[0] * f64::from(*force),
                    rotated[1] * f64::from(*force),
                    rotated[2] * f64::from(*force),
                ]
            }
            Self::Point {
                force,
                velocity_y,
                rotate_y: rotation_degrees,
                offset_x,
                offset_z,
                ..
            } => {
                let mut from = source;
                if *offset_x != 0 || *offset_z != 0 {
                    let offset = rotate_y([f64::from(*offset_x), 0.0, f64::from(*offset_z)], yaw);
                    from[0] += offset[0];
                    from[2] += offset[2];
                }
                let mut vector = [target[0] - from[0], 0.0, target[2] - from[2]];
                let length = (vector[0] * vector[0] + vector[2] * vector[2]).sqrt();
                if length > 1e-4 {
                    vector[0] /= length;
                    vector[2] /= length;
                } else {
                    vector = rotate_y([0.0, 0.0, -1.0], yaw);
                }
                if *rotation_degrees != 0 {
                    vector = rotate_y(vector, (*rotation_degrees as f64).to_radians() as f32);
                }
                [
                    vector[0] * f64::from(*force),
                    f64::from(*velocity_y),
                    vector[2] * f64::from(*force),
                ]
            }
        }
    }
}

/// Java `DamageEffects`: the non-damage metadata attached to a hit.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamageEffects {
    /// `ModelParticles` raw documents.
    pub model_particles: Vec<Value>,
    /// `WorldParticles` raw documents.
    pub world_particles: Vec<Value>,
    /// Local impact sound asset key.
    pub local_sound_event_id: Option<String>,
    /// World impact sound asset key.
    pub world_sound_event_id: Option<String>,
    /// Sound played to the damaged player.
    pub player_sound_event_id: Option<String>,
    /// Particle broadcast distance (`ParticleUtil.DEFAULT_PARTICLE_DISTANCE`).
    pub view_distance: f64,
    /// Optional knockback.
    pub knockback: Option<Knockback>,
    /// Camera effect asset key.
    pub camera_effect_id: Option<String>,
    /// Stamina drain multiplier.
    pub stamina_drain_multiplier: f32,
}

impl DamageEffects {
    /// Decode one `DamageEffects` document.
    pub fn from_value(value: &Value) -> Self {
        Self {
            model_particles: array_values(value.get("ModelParticles")),
            world_particles: array_values(value.get("WorldParticles")),
            local_sound_event_id: string_value(value.get("LocalSoundEventId")),
            world_sound_event_id: string_value(value.get("WorldSoundEventId")),
            player_sound_event_id: string_value(value.get("PlayerSoundEventId")),
            view_distance: value
                .get("ViewDistance")
                .and_then(Value::as_f64)
                .unwrap_or(10.0),
            knockback: value.get("Knockback").and_then(Knockback::from_value),
            camera_effect_id: string_value(value.get("CameraEffect")),
            stamina_drain_multiplier: value
                .get("StaminaDrainMultiplier")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
        }
    }

    /// Whether this document carries no presentation data.
    pub fn is_empty(&self) -> bool {
        self.model_particles.is_empty()
            && self.world_particles.is_empty()
            && self.local_sound_event_id.is_none()
            && self.world_sound_event_id.is_none()
            && self.player_sound_event_id.is_none()
            && self.knockback.is_none()
            && self.camera_effect_id.is_none()
            && (self.stamina_drain_multiplier - 1.0).abs() <= f32::EPSILON
    }
}

/// One `TargetedDamage` entry keyed by the interaction hit detail.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TargetedDamage {
    /// Optional damage calculator override.
    pub damage_calculator: Option<DamageCalculator>,
    /// Optional effects override.
    pub damage_effects: Option<DamageEffects>,
    /// Optional `Next` interaction.
    pub next: Option<String>,
    /// Sorted-hit-detail index assigned by Java `afterDecode`.
    pub index: usize,
}

/// One `AngledDamage` entry selected by the relative target angle.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AngledDamage {
    /// Optional damage calculator override.
    pub damage_calculator: Option<DamageCalculator>,
    /// Optional effects override.
    pub damage_effects: Option<DamageEffects>,
    /// Optional `Next` interaction.
    pub next: Option<String>,
    /// Java `Angle` in radians.
    pub angle_rad: f32,
    /// Java `AngleDistance` in radians.
    pub angle_distance_rad: f32,
}

/// One resolved `DamageEntity` interaction.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamageEntity {
    /// Base calculator (`None` means the node only selects an override).
    pub damage_calculator: Option<DamageCalculator>,
    /// Base effects.
    pub damage_effects: Option<DamageEffects>,
    /// Optional `DamageAttribute` scale key.
    pub damage_attribute: Option<String>,
    /// Angled overrides in document order.
    pub angled_damage: Vec<AngledDamage>,
    /// Targeted overrides sorted by key like Java `afterDecode`.
    pub targeted_damage: BTreeMap<String, TargetedDamage>,
    /// Success chain.
    pub next: Option<String>,
    /// Failure chain.
    pub failed: Option<String>,
    /// Blocked chain.
    pub blocked: Option<String>,
}

impl DamageEntity {
    /// Resolve a damage node. `catalog` supplies parent inheritance.
    pub fn from_node(catalog: &InteractionCatalog, node: &Value) -> Option<Self> {
        let node = catalog.resolve_node(node)?;
        let node_type = node.get("Type").and_then(Value::as_str)?;
        if !matches!(node_type, "DamageEntity" | "DamageEntityInteraction") {
            return None;
        }
        let mut angled_damage = Vec::new();
        if let Some(entries) = node.get("AngledDamage").and_then(Value::as_array) {
            for entry in entries {
                angled_damage.push(AngledDamage {
                    damage_calculator: entry
                        .get("DamageCalculator")
                        .and_then(DamageCalculator::from_value),
                    damage_effects: entry.get("DamageEffects").map(DamageEffects::from_value),
                    next: entry
                        .get("Next")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    angle_rad: entry
                        .get("Angle")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        .to_radians() as f32,
                    angle_distance_rad: entry
                        .get("AngleDistance")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0)
                        .to_radians() as f32,
                });
            }
        }
        let mut targeted_damage = BTreeMap::new();
        if let Some(entries) = node.get("TargetedDamage").and_then(Value::as_object) {
            for (index, (key, entry)) in entries.iter().enumerate() {
                targeted_damage.insert(
                    key.clone(),
                    TargetedDamage {
                        damage_calculator: entry
                            .get("DamageCalculator")
                            .and_then(DamageCalculator::from_value),
                        damage_effects: entry.get("DamageEffects").map(DamageEffects::from_value),
                        next: entry
                            .get("Next")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned),
                        index,
                    },
                );
            }
        }
        Some(Self {
            damage_calculator: node
                .get("DamageCalculator")
                .and_then(DamageCalculator::from_value),
            damage_effects: node.get("DamageEffects").map(DamageEffects::from_value),
            damage_attribute: string_value(node.get("DamageAttribute")),
            angled_damage,
            targeted_damage,
            next: string_value(node.get("Next")),
            failed: string_value(node.get("Failed")),
            blocked: string_value(node.get("Blocked")),
        })
    }

    /// Select the calculator/effects for one hit. `target_yaw_relative` is the
    /// Java `angleBetween + PI - targetYaw`; `hit_detail` matches
    /// `TargetedDamage`.
    pub fn select(
        &self,
        target_yaw_relative: f32,
        hit_detail: Option<&str>,
    ) -> (Option<&DamageCalculator>, Option<&DamageEffects>) {
        let mut calculator = self.damage_calculator.as_ref();
        let mut effects = self.damage_effects.as_ref();
        for angled in &self.angled_damage {
            if angle_difference(target_yaw_relative, angled.angle_rad).abs()
                < angled.angle_distance_rad
            {
                if angled.damage_calculator.is_some() {
                    calculator = angled.damage_calculator.as_ref();
                }
                if angled.damage_effects.is_some() {
                    effects = angled.damage_effects.as_ref();
                }
                break;
            }
        }
        if let Some(hit_detail) = hit_detail {
            if let Some(targeted) = self.targeted_damage.get(hit_detail) {
                if targeted.damage_calculator.is_some() {
                    calculator = targeted.damage_calculator.as_ref();
                }
                if targeted.damage_effects.is_some() {
                    effects = targeted.damage_effects.as_ref();
                }
            }
        }
        (calculator, effects)
    }
}

/// One damage hit reachable from an attack root.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackHit {
    /// Root/interaction that owns the damage node.
    pub interaction: String,
    /// `InteractionVars` key that selected this branch, when known.
    pub source: Option<String>,
    /// Resolved damage node.
    pub damage: DamageEntity,
    /// Cumulative `RunTime` from the root to the damage node.
    pub runtime: f64,
    /// Selector yaw start offset (degrees) for the branch, when a selector is
    /// part of the path.
    pub selector_yaw_offset: Option<f64>,
}

impl AttackHit {
    /// The best calculator in this hit, including angled/targeted overrides.
    pub fn calculator(&self) -> Option<&DamageCalculator> {
        self.damage
            .damage_calculator
            .as_ref()
            .or_else(|| {
                self.damage
                    .angled_damage
                    .iter()
                    .find_map(|entry| entry.damage_calculator.as_ref())
            })
            .or_else(|| {
                self.damage
                    .targeted_damage
                    .values()
                    .find_map(|entry| entry.damage_calculator.as_ref())
            })
    }

    /// The selected calculator for this hit.
    pub fn select(
        &self,
        target_yaw_relative: f32,
        hit_detail: Option<&str>,
    ) -> (Option<&DamageCalculator>, Option<&DamageEffects>) {
        self.damage.select(target_yaw_relative, hit_detail)
    }
}

/// Resolve every damage hit reachable from a root interaction.
pub fn collect_attack_hits(
    catalog: &InteractionCatalog,
    root_id: &str,
    vars: &BTreeMap<String, Value>,
) -> Vec<AttackHit> {
    let Some(root) = catalog.root(root_id) else {
        return Vec::new();
    };
    let mut walker = AttackWalker {
        catalog,
        vars,
        hits: Vec::new(),
        visited: BTreeSet::new(),
        runtime: 0.0,
        selector_yaw_offset: None,
    };
    if let Some(interactions) = root.get("Interactions") {
        walker.walk(interactions, None, 0);
    } else {
        walker.walk(&Value::String(root_id.to_string()), None, 0);
    }
    walker.hits
}

/// Resolve damage hits from an inline interaction value (used by tests and
/// role fragments that embed a whole root).
pub fn collect_value_hits(
    catalog: &InteractionCatalog,
    value: &Value,
    vars: &BTreeMap<String, Value>,
) -> Vec<AttackHit> {
    let mut walker = AttackWalker {
        catalog,
        vars,
        hits: Vec::new(),
        visited: BTreeSet::new(),
        runtime: 0.0,
        selector_yaw_offset: None,
    };
    walker.walk(value, None, 0);
    walker.hits
}

struct AttackWalker<'a> {
    catalog: &'a InteractionCatalog,
    vars: &'a BTreeMap<String, Value>,
    hits: Vec<AttackHit>,
    visited: BTreeSet<String>,
    runtime: f64,
    selector_yaw_offset: Option<f64>,
}

impl AttackWalker<'_> {
    fn walk(&mut self, value: &Value, source: Option<&str>, depth: usize) {
        if depth > 96 {
            return;
        }
        match value {
            Value::String(id) => {
                if !self.visited.insert(id.to_ascii_lowercase()) {
                    return;
                }
                if let Some(node) = self.catalog.interaction(id) {
                    self.walk_node(node, source, depth + 1);
                } else if let Some(root) = self.catalog.root(id) {
                    if let Some(interactions) = root.get("Interactions") {
                        self.walk(interactions, source, depth + 1);
                    } else {
                        self.walk_node(root, source, depth + 1);
                    }
                }
            }
            Value::Object(_) => {
                let Some(node) = self.catalog.resolve_node(value) else {
                    return;
                };
                self.walk_node(&node, source, depth + 1);
            }
            Value::Array(values) => {
                // Parallel/array branches all share the same start time.
                for value in values {
                    self.walk(value, source, depth + 1);
                }
            }
            _ => {}
        }
    }

    fn walk_node(&mut self, node: &Value, source: Option<&str>, depth: usize) {
        if depth > 96 {
            return;
        }
        let node_type = node.get("Type").and_then(Value::as_str).unwrap_or("Simple");
        if let Some(damage) = DamageEntity::from_node(self.catalog, node) {
            let interaction = node
                .get("Id")
                .and_then(Value::as_str)
                .unwrap_or("DamageEntity")
                .to_string();
            self.hits.push(AttackHit {
                interaction,
                source: source.map(ToOwned::to_owned),
                damage,
                runtime: self.runtime + runtime(node),
                selector_yaw_offset: self.selector_yaw_offset,
            });
        }

        let run_before = self.runtime;
        self.runtime += runtime(node);

        match node_type {
            "Replace" => {
                let variable = node.get("Var").and_then(Value::as_str).unwrap_or_default();
                let replacement = self
                    .vars
                    .get(variable)
                    .cloned()
                    .or_else(|| node.get("DefaultValue").cloned());
                if let Some(replacement) = replacement {
                    let source = if self.vars.contains_key(variable) {
                        Some(variable)
                    } else {
                        source
                    };
                    if let Some(interactions) = replacement.get("Interactions") {
                        self.walk(interactions, source, depth + 1);
                    } else {
                        self.walk(&replacement, source, depth + 1);
                    }
                }
            }
            "Selector" | "ComboInteraction" => {
                let previous_offset = self.selector_yaw_offset;
                if let Some(selector) = node.get("Selector") {
                    self.selector_yaw_offset = selector
                        .get("YawStartOffset")
                        .and_then(Value::as_f64)
                        .or(self.selector_yaw_offset);
                }
                if let Some(hit_entity) = node.get("HitEntity") {
                    let interactions = hit_entity.get("Interactions").unwrap_or(hit_entity);
                    self.walk(interactions, source, depth + 1);
                }
                self.selector_yaw_offset = previous_offset;
            }
            "Chaining" => {
                if let Some(next) = node.get("Next") {
                    self.walk(next, source, depth + 1);
                }
            }
            "Serial" | "Parallel" | "Sequence" | "Random" | "Weighted" => {
                if let Some(interactions) = node.get("Interactions") {
                    self.walk(interactions, source, depth + 1);
                }
            }
            "Repeat" => {
                if let Some(fork) = node.get("ForkInteractions") {
                    let interactions = fork.get("Interactions").unwrap_or(fork);
                    self.walk(interactions, source, depth + 1);
                }
            }
            _ => {}
        }

        // `Next` is the normal chain for Simple/Condition and most custom
        // nodes. Chaining/Replace/Selector already consumed theirs above.
        if !matches!(
            node_type,
            "Replace" | "Selector" | "ComboInteraction" | "Chaining"
        ) {
            if let Some(next) = node.get("Next") {
                self.walk(next, source, depth + 1);
            }
        }

        self.runtime = run_before;
    }
}

fn runtime(node: &Value) -> f64 {
    node.get("RunTime")
        .and_then(Value::as_f64)
        .or_else(|| node.get("Time").and_then(Value::as_f64))
        .unwrap_or(0.0)
}

fn angle_difference(left: f32, right: f32) -> f32 {
    let mut difference = (left - right) % std::f32::consts::TAU;
    if difference > std::f32::consts::PI {
        difference -= std::f32::consts::TAU;
    } else if difference < -std::f32::consts::PI {
        difference += std::f32::consts::TAU;
    }
    difference
}

fn rotate_y(vector: [f64; 3], yaw: f32) -> [f64; 3] {
    let sin = f64::from(yaw.sin());
    let cos = f64::from(yaw.cos());
    [
        vector[0] * cos + vector[2] * sin,
        vector[1],
        -vector[0] * sin + vector[2] * cos,
    ]
}

fn normalize3(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length <= f32::EPSILON {
        [0.0, 1.0, 0.0]
    } else {
        [vector[0] / length, vector[1] / length, vector[2] / length]
    }
}

fn array_values(value: Option<&Value>) -> Vec<Value> {
    value.and_then(Value::as_array).cloned().unwrap_or_default()
}

fn string_value(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> InteractionCatalog {
        let mut interactions = BTreeMap::new();
        interactions.insert(
            "rat_bite_damage".to_string(),
            serde_json::json!({
                "Parent": "DamageEntityParent",
                "DamageCalculator": {
                    "Type": "Absolute",
                    "BaseDamage": {"Physical": 5},
                    "RandomPercentageModifier": 0.1
                },
                "DamageEffects": {
                    "Knockback": {
                        "Force": 1,
                        "RelativeX": 0,
                        "RelativeZ": 5,
                        "VelocityY": 5
                    }
                }
            }),
        );
        interactions.insert(
            "damageentityparent".to_string(),
            serde_json::json!({"Type": "DamageEntity", "Next": "DamageEntityParent_On_Hit"}),
        );
        interactions.insert(
            "rat_bite".to_string(),
            serde_json::json!({
                "Type": "Simple",
                "RunTime": 0.117,
                "Next": {
                    "Type": "Selector",
                    "RunTime": 0.133,
                    "Selector": {"YawStartOffset": -15},
                    "HitEntity": {
                        "Interactions": [{
                            "Type": "Replace",
                            "DefaultValue": {"Interactions": ["Rat_Bite_Damage"]},
                            "Var": "Bite_Damage"
                        }]
                    }
                }
            }),
        );
        let mut roots = BTreeMap::new();
        roots.insert(
            "root_npc_rat_attack".to_string(),
            serde_json::json!({
                "Interactions": [{
                    "Type": "Serial",
                    "Interactions": [{
                        "Type": "Replace",
                        "DefaultValue": {"Interactions": ["Rat_Bite"]},
                        "Var": "Melee_Start"
                    }]
                }]
            }),
        );
        InteractionCatalog::from_parts(interactions, roots)
    }

    #[test]
    fn graph_resolves_role_damage_override_and_effects() {
        let catalog = catalog();
        let vars = BTreeMap::from([(
            "Bite_Damage".to_string(),
            serde_json::json!({
                "Interactions": [{
                    "Parent": "Rat_Bite_Damage",
                    "DamageCalculator": {
                        "Type": "Absolute",
                        "BaseDamage": {"Physical": 9},
                        "RandomPercentageModifier": 0.1
                    }
                }]
            }),
        )]);
        let hits = collect_attack_hits(&catalog, "Root_NPC_Rat_Attack", &vars);
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        assert_eq!(hit.source.as_deref(), Some("Bite_Damage"));
        assert!((hit.runtime - 0.25).abs() < 1e-9);
        let calculator = hit.calculator().expect("calculator");
        let range = calculator.compute_range(1.0);
        assert!((range[0] - 8.1).abs() < 1e-5);
        assert!((range[1] - 9.9).abs() < 1e-5);
        assert_eq!(
            calculator.roll(1.0, 0.5),
            vec![("Physical".to_string(), 9.0)]
        );
        assert!(hit.damage.damage_effects.is_some());
    }

    #[test]
    fn default_branch_is_used_when_the_role_does_not_override() {
        let catalog = catalog();
        let hits = collect_attack_hits(&catalog, "Root_NPC_Rat_Attack", &BTreeMap::new());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source, None);
        assert!((hits[0].calculator().unwrap().maximum_damage(1.0) - 5.5).abs() < 1e-5);
    }

    #[test]
    fn damage_effects_and_knockback_decode() {
        let effects = DamageEffects::from_value(&serde_json::json!({
            "WorldSoundEventId": "SFX_Unarmed_Impact",
            "Knockback": {"Force": 1, "RelativeZ": -1, "VelocityY": 3, "VelocityType": "Set"}
        }));
        assert_eq!(
            effects.world_sound_event_id.as_deref(),
            Some("SFX_Unarmed_Impact")
        );
        assert_eq!(effects.stamina_drain_multiplier, 1.0);
        let knockback = effects.knockback.expect("knockback");
        let vector = knockback.calculate_vector([0.0, 0.0, 0.0], 0.0, [0.0, 0.0, 1.0]);
        assert!((vector[0] - 0.0).abs() < 1e-6);
        assert!((vector[2] + 2.0).abs() < 1e-6);
        assert!((vector[1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn angled_and_targeted_damage_override_the_base_hit() {
        let catalog = InteractionCatalog::from_parts(BTreeMap::new(), BTreeMap::new());
        let node = serde_json::json!({
            "Type": "DamageEntity",
            "DamageCalculator": {"BaseDamage": {"Physical": 5}},
            "AngledDamage": [{
                "Angle": 90.0,
                "AngleDistance": 20.0,
                "DamageCalculator": {"BaseDamage": {"Fire": 8}}
            }],
            "TargetedDamage": {
                "Head": {
                    "DamageCalculator": {"BaseDamage": {"Physical": 12}}
                }
            }
        });
        let damage = DamageEntity::from_node(&catalog, &node).expect("damage node");
        let (calculator, _) = damage.select((90.0_f32).to_radians(), None);
        assert_eq!(
            calculator.unwrap().base_damage,
            BTreeMap::from([("Fire".to_string(), 8.0)])
        );
        let (calculator, _) = damage.select(0.0, Some("Head"));
        assert_eq!(
            calculator.unwrap().base_damage,
            BTreeMap::from([("Physical".to_string(), 12.0)])
        );
        let (calculator, _) = damage.select(0.0, None);
        assert_eq!(
            calculator.unwrap().base_damage,
            BTreeMap::from([("Physical".to_string(), 5.0)])
        );
    }

    #[test]
    fn sequential_modifier_matches_java() {
        let calculator = DamageCalculator::from_value(&serde_json::json!({
            "BaseDamage": {"Physical": 10},
            "SequentialModifierStep": 0.2,
            "SequentialModifierMinimum": 0.3
        }))
        .unwrap();
        assert_eq!(calculator.sequential_multiplier(0), 1.0);
        assert!((calculator.sequential_multiplier(2) - 0.6).abs() < 1e-6);
        assert!((calculator.sequential_multiplier(9) - 0.3).abs() < 1e-6);
    }
}
