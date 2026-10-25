//! Entity stats: types, values, modifiers and regeneration.
//!
//! Ported from `CoreServer/.../modules/entitystats/{EntityStatMap,
//! EntityStatValue,RegeneratingValue}` and `asset/EntityStatType`. This is the
//! deterministic server-side core; condition evaluation and client update
//! packets are later work.

use std::collections::{BTreeMap, VecDeque};

use serde_json::Value;

/// Java `EntityStatResetBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatResetBehavior {
    /// Reset to `InitialValue`.
    #[default]
    InitialValue,
    /// Reset to `Max`.
    MaxValue,
}

/// Java `EntityStatType.Regenerating.RegenType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegenType {
    /// Add a flat amount.
    Additive,
    /// Add a percentage of `Max - Min`.
    Percentage,
}

/// One `EntityStatType.Regenerating` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Regenerating {
    /// Seconds between regeneration steps.
    pub interval: f32,
    /// Amount added per step.
    pub amount: f32,
    /// Prevent regeneration from taking the value below zero.
    pub clamp_at_zero: bool,
    /// Additive or percentage.
    pub regen_type: RegenType,
    /// Raw condition documents (full condition evaluation is later work).
    pub conditions: Vec<Value>,
}

/// One `EntityStatType` asset (the fields gameplay consumes).
#[derive(Debug, Clone, PartialEq)]
pub struct EntityStatType {
    /// Asset id/key.
    pub id: String,
    /// Numeric asset id.
    pub index: i32,
    /// Initial value.
    pub initial_value: f32,
    /// Minimum.
    pub min: f32,
    /// Maximum.
    pub max: f32,
    /// Shared with other entities.
    pub shared: bool,
    /// Regeneration rules.
    pub regenerating: Vec<Regenerating>,
    /// Reset behavior.
    pub reset_behavior: StatResetBehavior,
    /// Damage bypasses invulnerability.
    pub ignore_invulnerability: bool,
    /// Hidden from tooltips.
    pub hide_from_tooltip: bool,
}

impl EntityStatType {
    /// Decode one merged `Server/Entity/Stats` document.
    pub fn from_value(key: &str, index: i32, value: &Value) -> Result<Self, String> {
        let number = |name: &str| -> f32 {
            value
                .get(name)
                .and_then(Value::as_f64)
                .map(|value| value as f32)
                .unwrap_or(0.0)
        };
        let regenerating = value
            .get("Regenerating")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| Regenerating {
                        interval: entry.get("Interval").and_then(Value::as_f64).unwrap_or(0.0)
                            as f32,
                        amount: entry.get("Amount").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                        clamp_at_zero: entry
                            .get("ClampAtZero")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        regen_type: match entry
                            .get("RegenType")
                            .and_then(Value::as_str)
                            .unwrap_or("Additive")
                        {
                            "Percentage" | "PERCENTAGE" => RegenType::Percentage,
                            _ => RegenType::Additive,
                        },
                        conditions: entry
                            .get("Conditions")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            id: key.to_string(),
            index,
            initial_value: number("InitialValue"),
            min: number("Min"),
            max: number("Max"),
            shared: value
                .get("Shared")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            regenerating,
            reset_behavior: match value.get("ResetType").and_then(Value::as_str) {
                Some("MaxValue") => StatResetBehavior::MaxValue,
                _ => StatResetBehavior::InitialValue,
            },
            ignore_invulnerability: value
                .get("IgnoreInvulnerability")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            hide_from_tooltip: value
                .get("HideFromTooltip")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// Java `Modifier.ModifierTarget`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierTarget {
    /// Minimum.
    Min,
    /// Maximum.
    Max,
}

/// Java `StaticModifier.CalculationType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalculationType {
    /// `value + amount`.
    Additive,
    /// `value * amount`.
    Multiplicative,
}

impl CalculationType {
    /// Java `CalculationType.compute`.
    pub fn compute(self, value: f32, amount: f32) -> f32 {
        match self {
            Self::Additive => value + amount,
            Self::Multiplicative => value * amount,
        }
    }
}

/// One static min/max modifier.
#[derive(Debug, Clone, PartialEq)]
pub struct StaticModifier {
    /// Target min or max.
    pub target: ModifierTarget,
    /// Calculation type.
    pub calculation: CalculationType,
    /// Amount.
    pub amount: f32,
}

/// One stat's live value.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityStatValue {
    /// Stat asset.
    pub stat_type: EntityStatType,
    /// Current value.
    value: f32,
    /// Effective minimum after modifiers.
    min: f32,
    /// Effective maximum after modifiers.
    max: f32,
    /// Keyed modifiers.
    modifiers: BTreeMap<String, StaticModifier>,
    /// Seconds until the next regeneration step (one timer per rule).
    regen_timers: Vec<f32>,
}

impl EntityStatValue {
    /// Create a value at `InitialValue`.
    pub fn new(stat_type: EntityStatType) -> Self {
        let mut value = Self {
            value: stat_type.initial_value,
            min: stat_type.min,
            max: stat_type.max,
            regen_timers: vec![0.0; stat_type.regenerating.len()],
            stat_type,
            modifiers: BTreeMap::new(),
        };
        value.recompute();
        value
    }

    /// Current value.
    pub fn get(&self) -> f32 {
        self.value
    }

    /// Effective minimum.
    pub fn min(&self) -> f32 {
        self.min
    }

    /// Effective maximum.
    pub fn max(&self) -> f32 {
        self.max
    }

    /// Java `asPercentage`.
    pub fn as_percentage(&self) -> f32 {
        if self.max == self.min {
            0.0
        } else {
            (self.value - self.min) / (self.max - self.min)
        }
    }

    /// Java `EntityStatValue.set` (clamped, returns the new value).
    pub fn set(&mut self, value: f32) -> f32 {
        self.value = if value.is_nan() {
            self.value
        } else {
            value.clamp(self.min, self.max)
        };
        self.value
    }

    /// Insert/replace a modifier and recompute min/max.
    pub fn put_modifier(&mut self, key: impl Into<String>, modifier: StaticModifier) {
        self.modifiers.insert(key.into(), modifier);
        self.recompute();
    }

    /// Remove a modifier and recompute min/max.
    pub fn remove_modifier(&mut self, key: &str) -> Option<StaticModifier> {
        let removed = self.modifiers.remove(key);
        if removed.is_some() {
            self.recompute();
        }
        removed
    }

    /// Current modifiers.
    pub fn modifiers(&self) -> &BTreeMap<String, StaticModifier> {
        &self.modifiers
    }

    fn recompute(&mut self) {
        self.min = self.stat_type.min;
        self.max = self.stat_type.max;
        for target in [ModifierTarget::Min, ModifierTarget::Max] {
            let mut additive = 0.0;
            let mut has_additive = false;
            let mut multiplicative = 0.0;
            let mut has_multiplicative = false;
            for modifier in self.modifiers.values() {
                if modifier.target != target {
                    continue;
                }
                match modifier.calculation {
                    CalculationType::Additive => {
                        has_additive = true;
                        additive += modifier.amount;
                    }
                    CalculationType::Multiplicative => {
                        has_multiplicative = true;
                        multiplicative += modifier.amount;
                    }
                }
            }
            let value = match target {
                ModifierTarget::Min => &mut self.min,
                ModifierTarget::Max => &mut self.max,
            };
            if has_additive {
                *value = CalculationType::Additive.compute(*value, additive);
            }
            if has_multiplicative {
                *value = CalculationType::Multiplicative.compute(*value, multiplicative);
            }
        }
        self.value = self.value.clamp(self.min, self.max);
    }

    /// Tick regeneration rules. Returns the total amount restored.
    pub fn tick_regeneration(&mut self, dt: f32) -> f32 {
        let mut total = 0.0;
        for index in 0..self.stat_type.regenerating.len() {
            let (interval, rule_amount, regen_type, clamp_at_zero) = {
                let rule = &self.stat_type.regenerating[index];
                (
                    rule.interval.max(f32::EPSILON),
                    rule.amount,
                    rule.regen_type,
                    rule.clamp_at_zero,
                )
            };
            let timer = &mut self.regen_timers[index];
            *timer -= dt;
            if *timer >= 0.0 {
                continue;
            }
            *timer += interval;
            let mut amount = match regen_type {
                RegenType::Additive => rule_amount,
                RegenType::Percentage => rule_amount * (self.max - self.min),
            };
            // Java `Regenerating.clampAmount`: a negative regeneration may not
            // take the stat below zero while the stat's own minimum is negative.
            if clamp_at_zero && amount < 0.0 && self.min < 0.0 && self.value + amount < 0.0 {
                amount = if self.value > 0.0 { -self.value } else { 0.0 };
            }
            let before = self.value;
            self.set(before + amount);
            total += self.value - before;
        }
        total
    }
}

/// One pending stat update for replication (`EntityStatUpdate` seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatUpdate {
    /// Stat index.
    pub index: i32,
    /// Operation kind.
    pub op: StatOp,
    /// Value before the change.
    pub before: u32,
    /// Delta or new value, depending on `op`.
    pub value: u32,
}

/// Java `EntityStatOp` subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatOp {
    /// Initial snapshot.
    Init,
    /// Add.
    Add,
    /// Set.
    Set,
    /// Minimize.
    Minimize,
    /// Maximize.
    Maximize,
    /// Reset.
    Reset,
}

/// Java `EntityStatMap` deterministic core.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityStatMap {
    values: BTreeMap<i32, EntityStatValue>,
    pending: VecDeque<StatUpdate>,
}

impl EntityStatMap {
    /// Build a map from stat assets.
    pub fn new(stats: impl IntoIterator<Item = EntityStatType>) -> Self {
        Self {
            values: stats
                .into_iter()
                .map(|stat| (stat.index, EntityStatValue::new(stat)))
                .collect(),
            pending: VecDeque::new(),
        }
    }

    /// Number of stats.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// One stat value.
    pub fn get(&self, index: i32) -> Option<&EntityStatValue> {
        self.values.get(&index)
    }

    /// One mutable stat value.
    pub fn get_mut(&mut self, index: i32) -> Option<&mut EntityStatValue> {
        self.values.get_mut(&index)
    }

    /// Java `setStatValue`; returns the clamped value.
    pub fn set(&mut self, index: i32, value: f32) -> Option<f32> {
        let current = self.values.get(&index)?.get();
        let stat = self.values.get_mut(&index)?;
        let new_value = stat.set(value);
        if new_value != current {
            self.push_update(index, StatOp::Set, current, value);
        }
        Some(new_value)
    }

    /// Java `addStatValue`.
    pub fn add(&mut self, index: i32, amount: f32) -> Option<f32> {
        let current = self.values.get(&index)?.get();
        let stat = self.values.get_mut(&index)?;
        let new_value = stat.set(current + amount);
        if new_value != current {
            self.push_update(index, StatOp::Add, current, amount);
        }
        Some(new_value)
    }

    /// Java `subtractStatValue`.
    pub fn subtract(&mut self, index: i32, amount: f32) -> Option<f32> {
        self.add(index, -amount)
    }

    /// Java `minimizeStatValue`.
    pub fn minimize(&mut self, index: i32) -> Option<f32> {
        let current = self.values.get(&index)?.get();
        let stat = self.values.get_mut(&index)?;
        let value = stat.set(stat.min());
        if value != current {
            self.push_update(index, StatOp::Minimize, current, 0.0);
        }
        Some(value)
    }

    /// Java `maximizeStatValue`.
    pub fn maximize(&mut self, index: i32) -> Option<f32> {
        let current = self.values.get(&index)?.get();
        let stat = self.values.get_mut(&index)?;
        let value = stat.set(stat.max());
        if value != current {
            self.push_update(index, StatOp::Maximize, current, 0.0);
        }
        Some(value)
    }

    /// Java reset behavior (`InitialValue` or `MaxValue`).
    pub fn reset(&mut self, index: i32) -> Option<f32> {
        let stat = self.values.get(&index)?;
        let target = match stat.stat_type.reset_behavior {
            StatResetBehavior::InitialValue => stat.stat_type.initial_value,
            StatResetBehavior::MaxValue => stat.max(),
        };
        let current = stat.get();
        let stat = self.values.get_mut(&index)?;
        let value = stat.set(target);
        if value != current {
            self.push_update(index, StatOp::Reset, current, value);
        }
        Some(value)
    }

    /// Add/replace a static modifier.
    pub fn put_modifier(
        &mut self,
        index: i32,
        key: impl Into<String>,
        modifier: StaticModifier,
    ) -> bool {
        let Some(stat) = self.values.get_mut(&index) else {
            return false;
        };
        let before = stat.get();
        stat.put_modifier(key, modifier);
        if stat.get() != before {
            let value = stat.get();
            self.push_update(index, StatOp::Set, before, value);
        }
        true
    }

    /// Remove a static modifier.
    pub fn remove_modifier(&mut self, index: i32, key: &str) -> bool {
        let Some(stat) = self.values.get_mut(&index) else {
            return false;
        };
        let before = stat.get();
        let removed = stat.remove_modifier(key).is_some();
        if removed && stat.get() != before {
            let value = stat.get();
            self.push_update(index, StatOp::Set, before, value);
        }
        removed
    }

    /// Remove every stat modifier whose key starts with `prefix`.
    ///
    /// Used by the effect controller to drop effect-provided modifiers without
    /// touching item/armor modifiers.
    pub fn remove_modifier_prefix(&mut self, prefix: &str) -> usize {
        let mut removed = 0;
        for stat in self.values.values_mut() {
            let keys: Vec<String> = stat
                .modifiers()
                .keys()
                .filter(|key| key.starts_with(prefix))
                .cloned()
                .collect();
            for key in keys {
                if stat.remove_modifier(&key).is_some() {
                    removed += 1;
                }
            }
        }
        removed
    }

    /// Tick every stat's regeneration. Returns `(index, amount)` changes.
    pub fn tick_regeneration(&mut self, dt: f32) -> Vec<(i32, f32)> {
        let mut changed = Vec::new();
        for (&index, stat) in &mut self.values {
            let before = stat.get();
            stat.tick_regeneration(dt);
            let delta = stat.get() - before;
            if delta != 0.0 {
                changed.push((index, delta));
            }
        }
        changed
    }

    /// Drain pending replication updates.
    pub fn take_updates(&mut self) -> Vec<StatUpdate> {
        self.pending.drain(..).collect()
    }

    fn push_update(&mut self, index: i32, op: StatOp, before: f32, value: f32) {
        self.pending.push_back(StatUpdate {
            index,
            op,
            before: before.to_bits(),
            value: value.to_bits(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn health() -> EntityStatType {
        EntityStatType {
            id: "Health".to_string(),
            index: 1,
            initial_value: 100.0,
            min: 0.0,
            max: 100.0,
            shared: true,
            regenerating: vec![Regenerating {
                interval: 0.5,
                amount: 0.05,
                clamp_at_zero: false,
                regen_type: RegenType::Percentage,
                conditions: Vec::new(),
            }],
            reset_behavior: StatResetBehavior::MaxValue,
            ignore_invulnerability: false,
            hide_from_tooltip: false,
        }
    }

    #[test]
    fn add_and_set_clamp_to_the_asset_range() {
        let mut map = EntityStatMap::new([health()]);
        assert_eq!(map.add(1, 25.0), Some(100.0));
        assert_eq!(map.subtract(1, 150.0), Some(0.0));
        assert_eq!(map.set(1, 42.0), Some(42.0));
        assert_eq!(map.get(1).unwrap().as_percentage(), 0.42);
        assert_eq!(map.reset(1), Some(100.0));
    }

    #[test]
    fn static_modifiers_change_min_and_max_then_clamp() {
        let mut map = EntityStatMap::new([health()]);
        assert_eq!(map.maximize(1), Some(100.0));
        map.put_modifier(
            1,
            "armor",
            StaticModifier {
                target: ModifierTarget::Max,
                calculation: CalculationType::Additive,
                amount: 50.0,
            },
        );
        assert_eq!(map.get(1).unwrap().max(), 150.0);
        assert_eq!(map.get(1).unwrap().get(), 100.0);
        map.set(1, 150.0);
        map.put_modifier(
            1,
            "boost",
            StaticModifier {
                target: ModifierTarget::Max,
                calculation: CalculationType::Multiplicative,
                amount: 2.0,
            },
        );
        assert_eq!(map.get(1).unwrap().max(), 300.0);
    }

    #[test]
    fn regeneration_uses_the_interval_and_percentage_amount() {
        let mut map = EntityStatMap::new([health()]);
        map.subtract(1, 50.0);
        let mut changed = Vec::new();
        for _ in 0..4 {
            changed.extend(map.tick_regeneration(0.1));
        }
        assert!(!changed.is_empty());
        // 0.05 * (100 - 0) = 5 health per 0.5 s.
        assert!((map.get(1).unwrap().get() - 55.0).abs() < 1e-4);
    }

    #[test]
    fn clamp_at_zero_caps_negative_regeneration() {
        let mut stat = health();
        stat.min = -10.0;
        stat.max = 10.0;
        stat.initial_value = 1.0;
        stat.regenerating = vec![Regenerating {
            interval: 0.1,
            amount: -0.75,
            clamp_at_zero: true,
            regen_type: RegenType::Additive,
            conditions: Vec::new(),
        }];
        let mut map = EntityStatMap::new([stat]);
        map.tick_regeneration(0.2);
        map.tick_regeneration(0.2);
        assert_eq!(map.get(1).unwrap().get(), 0.0);
        // A further step may not take the value below zero either.
        map.tick_regeneration(0.2);
        assert_eq!(map.get(1).unwrap().get(), 0.0);
    }
}
