//! The entity-gameplay component seam.
//!
//! Java keeps stats, effects and death in separate ECS components
//! (`EntityStatMap`, `EffectControllerComponent`, `DeathComponent`) and drives
//! them from module systems. Rust has no entity ECS yet, so this module bundles
//! the three deterministic cores behind one small type. The world thread can
//! attach it when NPC/combat producers land; T55/T59 consume it directly.

use crate::damage::{apply_health_damage, Damage, DamageCauses, DamageResult, ResistanceModifier};
use crate::effects::{aggregate_resistance_map, EffectController, EntityEffectAsset};
use crate::stats::EntityStatMap;
use std::collections::BTreeMap;

/// Changes produced by one gameplay tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameplayTick {
    /// Stat regeneration changes as `(stat index, delta)`.
    pub regenerated: Vec<(i32, f32)>,
    /// Effects that expired this tick.
    pub expired_effects: Vec<i32>,
}

/// Stats, active effects and the death state of one living entity.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityGameplay {
    /// Stat values.
    pub stats: EntityStatMap,
    /// Active effects.
    pub effects: EffectController,
    /// Health stat index.
    pub health_index: i32,
    dead: bool,
}

impl EntityGameplay {
    /// Build the component seam from a stat map and the health stat index.
    pub fn new(stats: EntityStatMap, health_index: i32) -> Self {
        Self {
            stats,
            effects: EffectController::default(),
            health_index,
            dead: false,
        }
    }

    /// Whether the entity has died (health reached zero).
    pub fn is_dead(&self) -> bool {
        self.dead
    }

    /// Whether any active effect grants invulnerability.
    pub fn is_invulnerable(&self) -> bool {
        self.effects.is_invulnerable()
    }

    /// Current health (zero when the stat is missing).
    pub fn health(&self) -> f32 {
        self.stats
            .get(self.health_index)
            .map_or(0.0, |stat| stat.get())
    }

    /// Apply one effect with the asset's default duration/overlap.
    pub fn apply_effect(&mut self, effect: &EntityEffectAsset) -> bool {
        let changed = self
            .effects
            .apply(effect, effect.duration, effect.overlap_behavior);
        self.effects.apply_stat_modifiers(&mut self.stats);
        changed
    }

    /// Apply one effect with an explicit duration and overlap behavior.
    pub fn apply_effect_with(
        &mut self,
        effect: &EntityEffectAsset,
        duration: f32,
        overlap: crate::effects::OverlapBehavior,
    ) -> bool {
        let changed = self.effects.apply(effect, duration, overlap);
        self.effects.apply_stat_modifiers(&mut self.stats);
        changed
    }

    /// Remove one effect by index.
    pub fn remove_effect(&mut self, index: i32) -> bool {
        let removed = self.effects.remove(index).is_some();
        if removed {
            self.effects.apply_stat_modifiers(&mut self.stats);
        }
        removed
    }

    /// Tick effects and regeneration, then refresh the death state.
    pub fn tick(&mut self, dt: f32) -> GameplayTick {
        let expired = self.effects.tick(dt);
        self.effects.apply_stat_modifiers(&mut self.stats);
        let regenerated = self.stats.tick_regeneration(dt);
        if self
            .stats
            .get(self.health_index)
            .is_some_and(|stat| stat.get() <= 0.0)
        {
            self.dead = true;
        }
        GameplayTick {
            regenerated,
            expired_effects: expired,
        }
    }

    /// Incoming damage modifiers from all active effects.
    pub fn incoming_modifiers(&self) -> BTreeMap<String, Vec<ResistanceModifier>> {
        aggregate_resistance_map(self.effects.active(), false)
    }

    /// Outgoing damage modifiers from all active effects.
    pub fn outgoing_modifiers(&self) -> BTreeMap<String, Vec<ResistanceModifier>> {
        aggregate_resistance_map(self.effects.active(), true)
    }

    /// Apply a damage instance to the health stat through effect resistance,
    /// invulnerability and the damage-cause chain.
    pub fn apply_damage(&mut self, damage: &Damage, causes: &DamageCauses) -> DamageResult {
        let incoming = self.incoming_modifiers();
        let outgoing = self.outgoing_modifiers();
        let result = apply_health_damage(
            &mut self.stats,
            self.health_index,
            damage,
            causes,
            &incoming,
            &outgoing,
            self.effects.is_invulnerable(),
        );
        if result.died {
            self.dead = true;
        }
        result
    }

    /// Reset the health stat and clear the death state (respawn seam).
    pub fn revive(&mut self) -> bool {
        let changed = self.stats.reset(self.health_index).is_some();
        self.dead = false;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage::{DamageCause, DamageSource};
    use crate::effects::{EffectValueType, OverlapBehavior};
    use crate::stats::{EntityStatType, StatResetBehavior};

    fn health_stat() -> EntityStatType {
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

    fn causes() -> DamageCauses {
        DamageCauses::new([DamageCause {
            id: "Physical".to_string(),
            ..DamageCause::default()
        }])
    }

    fn effect(invulnerable: bool, max_bonus: f32) -> EntityEffectAsset {
        EntityEffectAsset {
            id: "Test_Effect".to_string(),
            index: 1,
            name: None,
            duration: 1.0,
            overlap_behavior: OverlapBehavior::Ignore,
            infinite: false,
            debuff: false,
            status_effect_icon: None,
            invulnerable,
            removal_behavior: crate::effects::RemovalBehavior::Complete,
            entity_stats: BTreeMap::new(),
            value_type: EffectValueType::Absolute,
            stat_modifiers: BTreeMap::from([(
                1,
                vec![crate::stats::StaticModifier {
                    target: crate::stats::ModifierTarget::Max,
                    calculation: crate::stats::CalculationType::Additive,
                    amount: max_bonus,
                }],
            )]),
            damage_resistance: BTreeMap::new(),
            outgoing_damage: BTreeMap::new(),
            damage_calculator: None,
            damage_calculator_cooldown: 0.0,
            apply_conditions: Vec::new(),
        }
    }

    #[test]
    fn damage_reaches_zero_and_sets_the_death_state() {
        let mut gameplay = EntityGameplay::new(EntityStatMap::new([health_stat()]), 1);
        let damage = Damage::new(150.0, "Physical", DamageSource::Entity(7));
        let result = gameplay.apply_damage(&damage, &causes());
        assert!(result.died);
        assert!(gameplay.is_dead());
        assert_eq!(gameplay.health(), 0.0);
        assert!(gameplay.revive());
        assert!(!gameplay.is_dead());
        assert_eq!(gameplay.health(), 100.0);
    }

    #[test]
    fn invulnerability_and_stat_modifiers_follow_effect_lifecycle() {
        let mut gameplay = EntityGameplay::new(EntityStatMap::new([health_stat()]), 1);
        assert!(gameplay.apply_effect(&effect(true, 50.0)));
        assert!(gameplay.is_invulnerable());
        assert_eq!(gameplay.stats.get(1).unwrap().max(), 150.0);

        let blocked = gameplay.apply_damage(
            &Damage::new(25.0, "Physical", DamageSource::Environment("test".into())),
            &causes(),
        );
        assert_eq!(blocked.amount, 0.0);
        assert_eq!(gameplay.health(), 100.0);

        let tick = gameplay.tick(1.5);
        assert_eq!(tick.expired_effects, vec![1]);
        assert!(!gameplay.is_invulnerable());
        assert_eq!(gameplay.stats.get(1).unwrap().max(), 100.0);
    }
}
