//! Active entity effects.
//!
//! Ported from `CoreServer/.../asset/type/entityeffect/config/EntityEffect`,
//! `entity/effect/ActiveEntityEffect` and `EffectControllerComponent`. The
//! controller owns duration/overlap handling and aggregates the invulnerability
//! and damage modifiers used by the combat core.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::damage::{ResistanceCalculationType, ResistanceModifier};
use crate::stats::{CalculationType, EntityStatMap, ModifierTarget, StaticModifier};

/// Java `OverlapBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlapBehavior {
    /// Keep the existing effect (Java's default).
    #[default]
    Ignore,
    /// Replace the remaining duration.
    Overwrite,
    /// Add the new duration to the remaining duration.
    Extend,
}

impl OverlapBehavior {
    /// Decode Java's CamelCase enum value.
    pub fn from_java(value: &str) -> Option<Self> {
        match value {
            "Ignore" | "IGNORE" => Some(Self::Ignore),
            "Overwrite" | "OVERWRITE" => Some(Self::Overwrite),
            "Extend" | "EXTEND" => Some(Self::Extend),
            _ => None,
        }
    }
}

/// Java `RemovalBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemovalBehavior {
    /// Remove immediately (Java's default).
    #[default]
    Complete,
    /// Clear the infinite flag.
    Infinite,
    /// Let the effect run out.
    Duration,
}

impl RemovalBehavior {
    /// Decode Java's CamelCase enum value.
    pub fn from_java(value: &str) -> Option<Self> {
        match value {
            "Complete" | "COMPLETE" => Some(Self::Complete),
            "Infinite" | "INFINITE" => Some(Self::Infinite),
            "Duration" | "DURATION" => Some(Self::Duration),
            _ => None,
        }
    }
}

/// Java protocol `ValueType` for effect stat changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectValueType {
    /// Amount is a fraction of the stat range.
    Percent,
    /// Amount is an absolute value (Java default).
    #[default]
    Absolute,
}

impl EffectValueType {
    /// Decode Java's enum value.
    pub fn from_java(value: &str) -> Option<Self> {
        match value {
            "Percent" | "PERCENT" => Some(Self::Percent),
            "Absolute" | "ABSOLUTE" => Some(Self::Absolute),
            _ => None,
        }
    }
}

/// One `Server/Entity/Effects` asset (core gameplay fields).
#[derive(Debug, Clone, PartialEq)]
pub struct EntityEffectAsset {
    /// Asset id.
    pub id: String,
    /// Numeric asset id.
    pub index: i32,
    /// Display/localization name.
    pub name: Option<String>,
    /// Default duration in seconds.
    pub duration: f32,
    /// Overlap behavior.
    pub overlap_behavior: OverlapBehavior,
    /// Infinite effects do not tick down.
    pub infinite: bool,
    /// Debuff flag for the UI.
    pub debuff: bool,
    /// Status icon path.
    pub status_effect_icon: Option<String>,
    /// Invulnerable while active.
    pub invulnerable: bool,
    /// Removal behavior.
    pub removal_behavior: RemovalBehavior,
    /// Stat values applied while active (`StatModifiers`).
    pub entity_stats: BTreeMap<i32, f32>,
    /// How `entity_stats` values are interpreted.
    pub value_type: EffectValueType,
    /// Min/max stat modifiers to apply while active (`RawStatModifiers`).
    pub stat_modifiers: BTreeMap<i32, Vec<StaticModifier>>,
    /// Incoming damage modifiers keyed by damage-cause id.
    pub damage_resistance: BTreeMap<String, Vec<ResistanceModifier>>,
    /// Outgoing damage modifiers keyed by damage-cause id.
    pub outgoing_damage: BTreeMap<String, Vec<ResistanceModifier>>,
    /// Damage-over-time calculator (raw asset document; consumed by T57/T59).
    pub damage_calculator: Option<Value>,
    /// Seconds between damage-calculator ticks.
    pub damage_calculator_cooldown: f32,
    /// Conditions that must all hold for the effect to stay active (raw).
    pub apply_conditions: Vec<Value>,
}

impl EntityEffectAsset {
    /// Decode one merged `Server/Entity/Effects` document.
    ///
    /// `stat_id` resolves a stat name to the `EntityStatType` index; unknown
    /// names are rejected so asset drift fails the load instead of silently
    /// dropping effects.
    pub fn from_value(
        key: &str,
        index: i32,
        value: &Value,
        stat_id: impl Fn(&str) -> Option<i32>,
    ) -> Result<Self, String> {
        let string = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_string);
        let number = |name: &str| value.get(name).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        let boolean = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
        let mut entity_stats = BTreeMap::new();
        if let Some(entries) = value.get("StatModifiers").and_then(Value::as_object) {
            for (name, amount) in entries {
                let stat = stat_id(name).ok_or_else(|| {
                    format!("EntityEffect `{key}` references unknown stat `{name}`")
                })?;
                entity_stats.insert(stat, amount.as_f64().unwrap_or(0.0) as f32);
            }
        }
        let mut stat_modifiers = BTreeMap::new();
        if let Some(entries) = value.get("RawStatModifiers").and_then(Value::as_object) {
            for (name, modifiers) in entries {
                let stat = stat_id(name).ok_or_else(|| {
                    format!("EntityEffect `{key}` references unknown stat `{name}`")
                })?;
                let mut parsed = Vec::new();
                for modifier in modifiers.as_array().into_iter().flatten() {
                    parsed.push(static_modifier(key, modifier)?);
                }
                stat_modifiers.insert(stat, parsed);
            }
        }
        let overlap_behavior = string("OverlapBehavior")
            .as_deref()
            .map(|value| {
                OverlapBehavior::from_java(value).ok_or_else(|| {
                    format!("EntityEffect `{key}` has unknown OverlapBehavior `{value}`")
                })
            })
            .transpose()?
            .unwrap_or_default();
        let removal_behavior = string("RemovalBehavior")
            .as_deref()
            .map(|value| {
                RemovalBehavior::from_java(value).ok_or_else(|| {
                    format!("EntityEffect `{key}` has unknown RemovalBehavior `{value}`")
                })
            })
            .transpose()?
            .unwrap_or_default();
        let value_type = string("ValueType")
            .as_deref()
            .map(|value| {
                EffectValueType::from_java(value)
                    .ok_or_else(|| format!("EntityEffect `{key}` has unknown ValueType `{value}`"))
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            id: key.to_string(),
            index,
            name: string("Name"),
            duration: number("Duration"),
            overlap_behavior,
            infinite: boolean("Infinite"),
            debuff: boolean("Debuff"),
            status_effect_icon: string("StatusEffectIcon"),
            invulnerable: boolean("Invulnerable"),
            removal_behavior,
            entity_stats,
            value_type,
            stat_modifiers,
            damage_resistance: resistance_map(key, "DamageResistance", value)?,
            outgoing_damage: resistance_map(key, "OutgoingDamage", value)?,
            damage_calculator: value.get("DamageCalculator").cloned(),
            damage_calculator_cooldown: number("DamageCalculatorCooldown"),
            apply_conditions: value
                .get("ApplyConditions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        })
    }
}

fn static_modifier(key: &str, value: &Value) -> Result<StaticModifier, String> {
    let target = match value.get("Target").and_then(Value::as_str).unwrap_or("Max") {
        "Min" | "MIN" => ModifierTarget::Min,
        "Max" | "MAX" => ModifierTarget::Max,
        other => {
            return Err(format!(
                "EntityEffect `{key}` has unknown modifier target `{other}`"
            ))
        }
    };
    let calculation = match value
        .get("CalculationType")
        .and_then(Value::as_str)
        .unwrap_or("Additive")
    {
        "Additive" | "ADDITIVE" => CalculationType::Additive,
        "Multiplicative" | "MULTIPLICATIVE" => CalculationType::Multiplicative,
        other => {
            return Err(format!(
                "EntityEffect `{key}` has unknown modifier calculation `{other}`"
            ))
        }
    };
    Ok(StaticModifier {
        target,
        calculation,
        amount: value.get("Amount").and_then(Value::as_f64).unwrap_or(0.0) as f32,
    })
}

fn resistance_map(
    key: &str,
    field: &str,
    value: &Value,
) -> Result<BTreeMap<String, Vec<ResistanceModifier>>, String> {
    let mut out = BTreeMap::new();
    let Some(entries) = value.get(field).and_then(Value::as_object) else {
        return Ok(out);
    };
    for (cause, modifiers) in entries {
        let mut parsed = Vec::new();
        for modifier in modifiers.as_array().into_iter().flatten() {
            let calculation_type = match modifier
                .get("CalculationType")
                .and_then(Value::as_str)
                .unwrap_or("Flat")
            {
                "Flat" | "FLAT" => ResistanceCalculationType::Flat,
                "Percent" | "PERCENT" => ResistanceCalculationType::Percent,
                other => {
                    return Err(format!(
                        "EntityEffect `{key}` has unknown resistance calculation `{other}`"
                    ))
                }
            };
            parsed.push(ResistanceModifier {
                calculation_type,
                amount: modifier
                    .get("Amount")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
            });
        }
        out.insert(cause.clone(), parsed);
    }
    Ok(out)
}

/// One active effect instance (`ActiveEntityEffect`).
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveEntityEffect {
    /// Effect asset id.
    pub effect_id: String,
    /// Effect asset index.
    pub effect_index: i32,
    /// Initial duration.
    pub initial_duration: f32,
    /// Remaining duration.
    pub remaining_duration: f32,
    /// Infinite.
    pub infinite: bool,
    /// Debuff.
    pub debuff: bool,
    /// Status icon.
    pub status_effect_icon: Option<String>,
    /// Invulnerable while active.
    pub invulnerable: bool,
    /// Damage-tick multiplier (used by later combat/ability work).
    pub damage_multiplier: f32,
    /// Stat values applied while active.
    pub entity_stats: BTreeMap<i32, f32>,
    /// How `entity_stats` values are interpreted.
    pub value_type: EffectValueType,
    /// Min/max stat modifiers copied from the effect asset.
    pub stat_modifiers: BTreeMap<i32, Vec<StaticModifier>>,
    /// Incoming damage modifiers copied from the effect asset.
    pub damage_resistance: BTreeMap<String, Vec<ResistanceModifier>>,
    /// Outgoing damage modifiers copied from the effect asset.
    pub outgoing_damage: BTreeMap<String, Vec<ResistanceModifier>>,
}

/// Java `EffectControllerComponent` deterministic core.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectController {
    active: BTreeMap<i32, ActiveEntityEffect>,
}

impl EffectController {
    /// Add/update an effect. Returns `true` when the controller changed.
    pub fn apply(
        &mut self,
        effect: &EntityEffectAsset,
        duration: f32,
        overlap: OverlapBehavior,
    ) -> bool {
        if let Some(current) = self.active.get_mut(&effect.index) {
            if current.infinite {
                return false;
            }
            match overlap {
                OverlapBehavior::Ignore => return false,
                OverlapBehavior::Overwrite => current.remaining_duration = duration,
                OverlapBehavior::Extend => current.remaining_duration += duration,
            }
            current.initial_duration = current.remaining_duration;
            return true;
        }
        self.active.insert(
            effect.index,
            ActiveEntityEffect {
                effect_id: effect.id.clone(),
                effect_index: effect.index,
                initial_duration: duration,
                remaining_duration: duration,
                infinite: effect.infinite || duration.is_infinite(),
                debuff: effect.debuff,
                status_effect_icon: effect.status_effect_icon.clone(),
                invulnerable: effect.invulnerable,
                damage_multiplier: 1.0,
                entity_stats: effect.entity_stats.clone(),
                value_type: effect.value_type,
                stat_modifiers: effect.stat_modifiers.clone(),
                damage_resistance: effect.damage_resistance.clone(),
                outgoing_damage: effect.outgoing_damage.clone(),
            },
        );
        true
    }

    /// Remove one effect by index.
    pub fn remove(&mut self, index: i32) -> Option<ActiveEntityEffect> {
        self.active.remove(&index)
    }

    /// Remove every effect.
    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// One active effect.
    pub fn get(&self, index: i32) -> Option<&ActiveEntityEffect> {
        self.active.get(&index)
    }

    /// Active effects in index order.
    pub fn active(&self) -> impl Iterator<Item = &ActiveEntityEffect> {
        self.active.values()
    }

    /// Number of active effects.
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// Whether no effects are active.
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Java `isInvulnerable`: any active effect grants invulnerability.
    pub fn is_invulnerable(&self) -> bool {
        self.active.values().any(|effect| effect.invulnerable)
    }

    /// Tick durations and drop expired effects. Returns removed effect ids.
    pub fn tick(&mut self, dt: f32) -> Vec<i32> {
        let mut removed = Vec::new();
        for (&index, effect) in &mut self.active {
            if effect.infinite {
                continue;
            }
            effect.remaining_duration -= dt;
            if effect.remaining_duration <= 0.0 {
                removed.push(index);
            }
        }
        for index in &removed {
            self.active.remove(index);
        }
        removed
    }

    /// Apply every active stat modifier to a stat map.
    ///
    /// Modifier keys follow Java's effect-index convention so removing/expiring
    /// the effect can be mirrored later without scanning all effects. The
    /// previous effect modifiers are removed first, so this is a full sync.
    pub fn apply_stat_modifiers(&self, stats: &mut EntityStatMap) {
        stats.remove_modifier_prefix("effect:");
        for effect in self.active.values() {
            for (stat_index, modifiers) in &effect.stat_modifiers {
                for (position, modifier) in modifiers.iter().enumerate() {
                    stats.put_modifier(
                        *stat_index,
                        format!("effect:{}:{stat_index}:{position}", effect.effect_index),
                        modifier.clone(),
                    );
                }
            }
        }
    }
}

/// Aggregate damage modifiers from active effects and the effect assets.
pub fn aggregate_resistance<'a>(
    effects: impl IntoIterator<Item = &'a ActiveEntityEffect>,
    cause: &str,
    outgoing: bool,
) -> Vec<ResistanceModifier> {
    let mut out = Vec::new();
    for effect in effects {
        let map = if outgoing {
            &effect.outgoing_damage
        } else {
            &effect.damage_resistance
        };
        if let Some(modifiers) = map.get(cause) {
            out.extend(modifiers.iter().cloned());
        }
    }
    out
}

/// Aggregate every effect's damage modifiers into the cause-keyed map expected
/// by `apply_resistance`/`apply_outgoing`.
pub fn aggregate_resistance_map<'a>(
    effects: impl IntoIterator<Item = &'a ActiveEntityEffect>,
    outgoing: bool,
) -> BTreeMap<String, Vec<ResistanceModifier>> {
    let mut out: BTreeMap<String, Vec<ResistanceModifier>> = BTreeMap::new();
    for effect in effects {
        let map = if outgoing {
            &effect.outgoing_damage
        } else {
            &effect.damage_resistance
        };
        for (cause, modifiers) in map {
            out.entry(cause.clone())
                .or_default()
                .extend(modifiers.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage::ResistanceCalculationType;

    fn effect(index: i32) -> EntityEffectAsset {
        EntityEffectAsset {
            id: format!("Effect_{index}"),
            index,
            name: None,
            duration: 2.0,
            overlap_behavior: OverlapBehavior::Overwrite,
            infinite: false,
            debuff: true,
            status_effect_icon: None,
            invulnerable: index == 2,
            removal_behavior: RemovalBehavior::Complete,
            entity_stats: BTreeMap::new(),
            value_type: EffectValueType::Absolute,
            stat_modifiers: BTreeMap::new(),
            damage_resistance: BTreeMap::from([(
                "Physical".to_string(),
                vec![ResistanceModifier {
                    calculation_type: ResistanceCalculationType::Percent,
                    amount: -0.2,
                }],
            )]),
            outgoing_damage: BTreeMap::new(),
            damage_calculator: None,
            damage_calculator_cooldown: 0.0,
            apply_conditions: Vec::new(),
        }
    }

    #[test]
    fn overlap_ignore_overwrite_and_extend_follow_java() {
        let effect = effect(1);
        let mut controller = EffectController::default();
        assert!(controller.apply(&effect, 2.0, OverlapBehavior::Overwrite));
        assert!(!controller.apply(&effect, 5.0, OverlapBehavior::Ignore));
        assert_eq!(controller.get(1).unwrap().remaining_duration, 2.0);
        assert!(controller.apply(&effect, 3.0, OverlapBehavior::Extend));
        assert_eq!(controller.get(1).unwrap().remaining_duration, 5.0);
        assert!(controller.apply(&effect, 1.0, OverlapBehavior::Overwrite));
        assert_eq!(controller.get(1).unwrap().remaining_duration, 1.0);
    }

    #[test]
    fn expiry_and_invulnerability_aggregate() {
        let mut controller = EffectController::default();
        controller.apply(&effect(1), 1.0, OverlapBehavior::Overwrite);
        controller.apply(&effect(2), 5.0, OverlapBehavior::Overwrite);
        assert!(controller.is_invulnerable());
        assert_eq!(controller.tick(1.5), vec![1]);
        assert_eq!(controller.len(), 1);
        assert!(controller.is_invulnerable());
        controller.tick(4.0);
        assert!(controller.is_empty());
        assert!(!controller.is_invulnerable());
    }

    #[test]
    fn resistance_aggregation_uses_the_effect_assets() {
        let effect = effect(1);
        let mut controller = EffectController::default();
        controller.apply(&effect, 2.0, OverlapBehavior::Overwrite);
        let modifiers = aggregate_resistance(controller.active(), "Physical", false);
        assert_eq!(modifiers.len(), 1);
        assert!((modifiers[0].amount + 0.2).abs() < 1e-6);
    }

    #[test]
    fn asset_decoding_defaults_and_resolves_named_stats() {
        let value = serde_json::json!({
            "Duration": 3.5,
            "OverlapBehavior": "Extend",
            "RemovalBehavior": "DURATION",
            "Infinite": true,
            "StatModifiers": {"Health": 2.0},
            "RawStatModifiers": {
                "Health": [{"Target": "Max", "CalculationType": "Multiplicative", "Amount": 1.5}]
            },
            "DamageResistance": {
                "Physical": [{"CalculationType": "Percent", "Amount": 0.25}]
            },
            "ApplyConditions": [{"Id": "Alive"}]
        });
        let effect = EntityEffectAsset::from_value("Test", 4, &value, |name| match name {
            "Health" => Some(1),
            _ => None,
        })
        .expect("decode");
        assert_eq!(effect.duration, 3.5);
        assert_eq!(effect.overlap_behavior, OverlapBehavior::Extend);
        assert_eq!(effect.removal_behavior, RemovalBehavior::Duration);
        assert!(effect.infinite);
        assert_eq!(effect.entity_stats.get(&1), Some(&2.0));
        assert_eq!(
            effect.stat_modifiers[&1][0].calculation,
            CalculationType::Multiplicative
        );
        assert_eq!(effect.damage_resistance["Physical"][0].amount, 0.25);
        assert_eq!(effect.apply_conditions.len(), 1);
        assert!(effect.damage_calculator.is_none());

        let defaults =
            EntityEffectAsset::from_value("Defaults", 5, &serde_json::json!({}), |_| None)
                .expect("decode defaults");
        assert_eq!(defaults.overlap_behavior, OverlapBehavior::Ignore);
        assert_eq!(defaults.removal_behavior, RemovalBehavior::Complete);
        assert_eq!(defaults.value_type, EffectValueType::Absolute);
        assert!(defaults.stat_modifiers.is_empty());

        let error = EntityEffectAsset::from_value(
            "Broken",
            6,
            &serde_json::json!({"StatModifiers": {"Missing": 1.0}}),
            |_| None,
        )
        .unwrap_err();
        assert!(error.contains("unknown stat `Missing`"));
    }
}
