//! Active entity effects.
//!
//! Ported from `CoreServer/.../asset/type/entityeffect/config/EntityEffect`,
//! `entity/effect/ActiveEntityEffect` and `EffectControllerComponent`. The
//! controller owns duration/overlap handling and aggregates the invulnerability
//! and damage modifiers used by the combat core.

use std::collections::BTreeMap;

use crate::damage::ResistanceModifier;
use crate::stats::{EntityStatMap, StaticModifier};

/// Java `OverlapBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlapBehavior {
    /// Keep the existing effect.
    Ignore,
    /// Replace the remaining duration.
    #[default]
    Overwrite,
    /// Add the new duration to the remaining duration.
    Extend,
}

/// Java `RemovalBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemovalBehavior {
    /// Remove immediately.
    #[default]
    Remove,
    /// Let the effect run out.
    Complete,
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
    /// Stat modifiers to apply while active.
    pub stat_modifiers: Vec<(i32, StaticModifier)>,
    /// Incoming damage modifiers keyed by damage-cause id.
    pub damage_resistance: BTreeMap<String, Vec<ResistanceModifier>>,
    /// Outgoing damage modifiers keyed by damage-cause id.
    pub outgoing_damage: BTreeMap<String, Vec<ResistanceModifier>>,
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
    /// Stat modifiers copied from the effect asset.
    pub stat_modifiers: Vec<(i32, StaticModifier)>,
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
    /// the effect can be mirrored later without scanning all effects.
    pub fn apply_stat_modifiers(&self, stats: &mut EntityStatMap) {
        for effect in self.active.values() {
            for (position, (stat_index, modifier)) in effect.stat_modifiers.iter().enumerate() {
                stats.put_modifier(
                    *stat_index,
                    format!("effect:{}:{position}", effect.effect_index),
                    modifier.clone(),
                );
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
            removal_behavior: RemovalBehavior::Remove,
            stat_modifiers: Vec::new(),
            damage_resistance: BTreeMap::from([(
                "Physical".to_string(),
                vec![ResistanceModifier {
                    calculation_type: ResistanceCalculationType::Percent,
                    amount: -0.2,
                }],
            )]),
            outgoing_damage: BTreeMap::new(),
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
}
