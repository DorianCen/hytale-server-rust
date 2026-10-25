//! Damage causes, resistance/outgoing modifiers and health/death basics.
//!
//! Ported from `CoreServer/.../modules/entity/damage/{Damage,DamageCause,
//! DamageSystems,ResistanceModifier,DeathComponent}`. The full damage-calculator
//! graph, knockback, animations and death loot are later combat work; this is
//! the deterministic server-side resolution used by scripted combat tests.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::stats::EntityStatMap;

/// Java `ResistanceModifier.ResistanceCalculationType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResistanceCalculationType {
    /// Flat subtraction.
    Flat,
    /// Percentage reduction/increase (`0.25` = 25%).
    Percent,
}

/// One `ResistanceModifier`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResistanceModifier {
    /// Flat or percent.
    pub calculation_type: ResistanceCalculationType,
    /// Amount.
    pub amount: f32,
}

/// Java `DamageCause` (core fields).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DamageCause {
    /// Asset id.
    pub id: String,
    /// Parent cause id.
    pub inherits: Option<String>,
    /// Causes durability loss.
    pub durability_loss: bool,
    /// Causes stamina loss.
    pub stamina_loss: bool,
    /// Bypasses resistances.
    pub bypass_resistances: bool,
    /// Damage text color.
    pub damage_text_color: Option<String>,
    /// Hurt animation.
    pub animation_id: String,
    /// Death animation.
    pub death_animation_id: String,
    /// Death-message localization key.
    pub death_message_key: Option<String>,
}

impl DamageCause {
    /// Decode a merged `Server/Entity/Damage` document.
    pub fn from_value(key: &str, value: &Value) -> Self {
        Self {
            id: key.to_string(),
            inherits: value
                .get("Inherits")
                .and_then(Value::as_str)
                .map(str::to_string),
            durability_loss: value
                .get("DurabilityLoss")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            stamina_loss: value
                .get("StaminaLoss")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            bypass_resistances: value
                .get("BypassResistances")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            damage_text_color: value
                .get("DamageTextColor")
                .and_then(Value::as_str)
                .map(str::to_string),
            animation_id: value
                .get("AnimationId")
                .and_then(Value::as_str)
                .unwrap_or("Hurt")
                .to_string(),
            death_animation_id: value
                .get("DeathAnimationId")
                .and_then(Value::as_str)
                .unwrap_or("Death")
                .to_string(),
            death_message_key: value
                .get("DeathMessageKey")
                .and_then(Value::as_str)
                .map(str::to_string),
        }
    }
}

/// The `Server/Entity/Damage` asset set, used for inheritance matching.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DamageCauses {
    /// Causes by id.
    pub causes: BTreeMap<String, DamageCause>,
}

impl DamageCauses {
    /// Build from decoded causes.
    pub fn new(causes: impl IntoIterator<Item = DamageCause>) -> Self {
        Self {
            causes: causes
                .into_iter()
                .map(|cause| (cause.id.clone(), cause))
                .collect(),
        }
    }

    /// One cause.
    pub fn get(&self, id: &str) -> Option<&DamageCause> {
        self.causes.get(id)
    }

    /// Java `DamageCause.isOrInherits(cause)`: this cause is `target` or a
    /// descendant of it.
    pub fn is_or_inherits(&self, cause: &str, target: &str) -> bool {
        let mut current = Some(cause);
        let mut depth = 0;
        while let Some(id) = current {
            if id == target {
                return true;
            }
            if depth >= 8 {
                return false;
            }
            depth += 1;
            current = self
                .causes
                .get(id)
                .and_then(|cause| cause.inherits.as_deref());
        }
        false
    }

    /// The cause chain from `cause` toward its root.
    pub fn chain<'a>(&'a self, cause: &'a str) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut current = Some(cause);
        let mut depth = 0;
        while let Some(id) = current {
            if depth >= 8 {
                break;
            }
            out.push(id);
            depth += 1;
            current = self
                .causes
                .get(id)
                .and_then(|cause| cause.inherits.as_deref());
        }
        out
    }
}

/// Where a damage instance came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DamageSource {
    /// Environment/system damage.
    Environment(String),
    /// Entity melee.
    Entity(u64),
    /// Projectile.
    Projectile(u64),
    /// Entity effect damage-over-time.
    EntityEffect(String),
    /// Command/admin.
    Command,
}

/// One damage instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Damage {
    /// Raw amount before resistances.
    pub amount: f32,
    /// Cause asset id.
    pub cause: String,
    /// Source.
    pub source: DamageSource,
    /// Ignores invulnerability (from the health stat/effect flags).
    pub ignore_invulnerability: bool,
}

impl Damage {
    /// Construct a damage instance.
    pub fn new(amount: f32, cause: impl Into<String>, source: DamageSource) -> Self {
        Self {
            amount,
            cause: cause.into(),
            source,
            ignore_invulnerability: false,
        }
    }
}

/// Result of applying damage to a health stat.
#[derive(Debug, Clone, PartialEq)]
pub struct DamageResult {
    /// Damage actually applied (after resistances).
    pub amount: f32,
    /// Health after the hit.
    pub remaining_health: f32,
    /// True when the entity crossed to zero.
    pub died: bool,
    /// Cause id.
    pub cause: String,
    /// Source.
    pub source: DamageSource,
}

/// Java outgoing-damage modifier application.
pub fn apply_outgoing(
    raw: f32,
    cause: &str,
    causes: &DamageCauses,
    modifiers: &BTreeMap<String, Vec<ResistanceModifier>>,
) -> f32 {
    let mut flat = 0.0;
    let mut percent = 0.0;
    for (parent, entries) in modifiers {
        if !causes.is_or_inherits(cause, parent) {
            continue;
        }
        for modifier in entries {
            match modifier.calculation_type {
                ResistanceCalculationType::Flat => flat += modifier.amount,
                ResistanceCalculationType::Percent => percent += modifier.amount,
            }
        }
    }
    ((raw + flat) * (1.0 + percent)).max(0.0)
}

/// Java `DamageSystems.Resistance` application (flat, then percentage), walking
/// the cause's inheritance chain.
pub fn apply_resistance(
    raw: f32,
    cause: &str,
    causes: &DamageCauses,
    modifiers: &BTreeMap<String, Vec<ResistanceModifier>>,
) -> f32 {
    let mut amount = raw;
    for id in causes.chain(cause) {
        let Some(entries) = modifiers.get(id) else {
            continue;
        };
        let mut flat = 0.0;
        let mut percent = 0.0;
        for modifier in entries {
            match modifier.calculation_type {
                ResistanceCalculationType::Flat => flat += modifier.amount,
                ResistanceCalculationType::Percent => percent += modifier.amount,
            }
        }
        amount = (amount - flat).max(0.0);
        amount *= (1.0 - percent).max(0.0);
    }
    amount.max(0.0)
}

/// Apply a damage instance to one health stat.
#[allow(clippy::too_many_arguments)]
pub fn apply_health_damage(
    stats: &mut EntityStatMap,
    health_index: i32,
    damage: &Damage,
    causes: &DamageCauses,
    incoming: &BTreeMap<String, Vec<ResistanceModifier>>,
    outgoing: &BTreeMap<String, Vec<ResistanceModifier>>,
    invulnerable: bool,
) -> DamageResult {
    let cause = causes.get(&damage.cause);
    let ignore_invulnerability = damage.ignore_invulnerability
        || stats
            .get(health_index)
            .is_some_and(|stat| stat.stat_type.ignore_invulnerability);
    if invulnerable && !ignore_invulnerability {
        return DamageResult {
            amount: 0.0,
            remaining_health: stats.get(health_index).map_or(0.0, |stat| stat.get()),
            died: false,
            cause: damage.cause.clone(),
            source: damage.source.clone(),
        };
    }
    let outgoing_amount = apply_outgoing(damage.amount.max(0.0), &damage.cause, causes, outgoing);
    let amount = if cause.is_some_and(|cause| cause.bypass_resistances) {
        outgoing_amount
    } else {
        apply_resistance(outgoing_amount, &damage.cause, causes, incoming)
    };
    let remaining = stats
        .subtract(health_index, amount)
        .unwrap_or_default()
        .max(0.0);
    DamageResult {
        amount,
        remaining_health: remaining,
        died: remaining <= 0.0,
        cause: damage.cause.clone(),
        source: damage.source.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{EntityStatType, StatResetBehavior};

    fn causes() -> DamageCauses {
        DamageCauses::new([
            DamageCause {
                id: "Physical".to_string(),
                ..DamageCause::default()
            },
            DamageCause {
                id: "Slashing".to_string(),
                inherits: Some("Physical".to_string()),
                ..DamageCause::default()
            },
        ])
    }

    fn health() -> EntityStatType {
        EntityStatType {
            id: "Health".to_string(),
            index: 1,
            initial_value: 100.0,
            min: 0.0,
            max: 100.0,
            shared: true,
            regenerating: Vec::new(),
            reset_behavior: StatResetBehavior::MaxValue,
            ignore_invulnerability: false,
            hide_from_tooltip: false,
        }
    }

    #[test]
    fn resistance_applies_flat_then_percent_and_walks_parents() {
        let causes = causes();
        let modifiers = BTreeMap::from([
            (
                "Slashing".to_string(),
                vec![ResistanceModifier {
                    calculation_type: ResistanceCalculationType::Flat,
                    amount: 5.0,
                }],
            ),
            (
                "Physical".to_string(),
                vec![ResistanceModifier {
                    calculation_type: ResistanceCalculationType::Percent,
                    amount: 0.5,
                }],
            ),
        ]);
        // (20 - 5) * (1 - 0.5) = 7.5
        assert!((apply_resistance(20.0, "Slashing", &causes, &modifiers) - 7.5).abs() < 1e-6);
    }

    #[test]
    fn outgoing_modifiers_match_parents_and_stack() {
        let causes = causes();
        let modifiers = BTreeMap::from([(
            "Physical".to_string(),
            vec![
                ResistanceModifier {
                    calculation_type: ResistanceCalculationType::Flat,
                    amount: 2.0,
                },
                ResistanceModifier {
                    calculation_type: ResistanceCalculationType::Percent,
                    amount: 0.25,
                },
            ],
        )]);
        assert!((apply_outgoing(10.0, "Slashing", &causes, &modifiers) - 15.0).abs() < 1e-6);
    }

    #[test]
    fn damage_clamps_at_zero_and_reports_death() {
        let mut stats = EntityStatMap::new([health()]);
        let causes = causes();
        let damage = Damage::new(120.0, "Slashing", DamageSource::Entity(7));
        let result = apply_health_damage(
            &mut stats,
            1,
            &damage,
            &causes,
            &BTreeMap::new(),
            &BTreeMap::new(),
            false,
        );
        assert!(result.died);
        assert_eq!(result.remaining_health, 0.0);
        assert_eq!(stats.get(1).unwrap().get(), 0.0);
        assert_eq!(result.cause, "Slashing");
    }

    #[test]
    fn invulnerability_blocks_damage_unless_the_cause_ignores_it() {
        let mut stats = EntityStatMap::new([health()]);
        let causes = causes();
        let mut damage = Damage::new(10.0, "Physical", DamageSource::Environment("test".into()));
        let result = apply_health_damage(
            &mut stats,
            1,
            &damage,
            &causes,
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
        );
        assert_eq!(result.amount, 0.0);
        assert_eq!(result.remaining_health, 100.0);
        damage.ignore_invulnerability = true;
        let result = apply_health_damage(
            &mut stats,
            1,
            &damage,
            &causes,
            &BTreeMap::new(),
            &BTreeMap::new(),
            true,
        );
        assert_eq!(result.amount, 10.0);
        assert_eq!(result.remaining_health, 90.0);
    }
}
