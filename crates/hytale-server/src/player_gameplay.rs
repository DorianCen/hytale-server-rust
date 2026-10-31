//! Per-player gameplay stats.
//!
//! Java attaches `EntityStatMap` (`EntityStats`) to every player entity. The
//! Rust world thread keeps the same map for connected players, applies NPC
//! melee damage through the T53 health/resistance core, and queues
//! `EntityStatUpdate` replication packets for the session layer.

use std::collections::BTreeMap;

use hytale_assets::AssetRegistry;
use hytale_entities::components::EntityGameplay;
use hytale_entities::damage::{Damage, DamageCauses, DamageResult, DamageSource};
use hytale_entities::effects::EntityEffectAsset;
use hytale_entities::stats::{StatOp, StatUpdate};
use hytale_interactions::combat::DamageCalculator;
use hytale_npc::combat::npc_stat_map;
use hytale_protocol::generated::common::{
    ComponentUpdate, EffectOp, EntityEffectUpdate, EntityEffectsUpdate, EntityStatOp,
    EntityStatUpdate, EntityStatsUpdate, EntityUpdate,
};
use hytale_protocol::generated::entities::EntityUpdates;

/// One connected player's live stat/effect component.
#[derive(Debug, Clone)]
pub struct PlayerGameplay {
    gameplay: EntityGameplay,
    damage_over_time: BTreeMap<i32, DotState>,
    known_effects: BTreeMap<i32, EntityEffectUpdate>,
}

#[derive(Debug, Clone)]
struct DotState {
    effect_id: String,
    calculator: DamageCalculator,
    cooldown: f32,
    elapsed: f32,
}

impl PlayerGameplay {
    /// Build the Java-default stat map from the real `EntityStatType` store,
    /// restoring a saved health value when present.
    pub fn from_assets(assets: &AssetRegistry, saved_health: Option<f32>) -> Result<Self, String> {
        let max_health = assets
            .entity_stat_types
            .iter()
            .find(|record| record.key.eq_ignore_ascii_case("Health"))
            .and_then(|record| record.value.get("InitialValue"))
            .and_then(serde_json::Value::as_f64)
            .map(|value| value as f32)
            .filter(|value| *value > 0.0)
            .unwrap_or(100.0);
        let (mut stats, health_index) =
            npc_stat_map(assets, max_health).map_err(|error| error.to_string())?;
        if let Some(health) = saved_health {
            stats.set(health_index, health);
        }
        // The first packet is a full `Init` snapshot, not a delta.
        stats.take_updates();
        Ok(Self {
            gameplay: EntityGameplay::new(stats, health_index),
            damage_over_time: BTreeMap::new(),
            known_effects: BTreeMap::new(),
        })
    }

    /// Current health.
    pub fn health(&self) -> f32 {
        self.gameplay.health()
    }

    /// Whether health reached zero.
    pub fn is_dead(&self) -> bool {
        self.gameplay.is_dead()
    }

    /// Health stat index.
    pub fn health_index(&self) -> i32 {
        self.gameplay.health_index
    }

    /// Apply one damage instance through the stat/effect/resistance core.
    pub fn apply_damage(&mut self, damage: &Damage, causes: &DamageCauses) -> DamageResult {
        self.gameplay.apply_damage(damage, causes)
    }

    /// Reset health and clear the death state.
    pub fn revive(&mut self) -> bool {
        self.gameplay.revive()
    }

    /// Death/respawn: reset health, clear effects/DoT and queue removals.
    pub fn respawn(&mut self) -> bool {
        let changed = self.gameplay.revive();
        self.gameplay.effects.clear();
        self.gameplay
            .effects
            .apply_stat_modifiers(&mut self.gameplay.stats);
        self.damage_over_time.clear();
        changed | !self.known_effects.is_empty()
    }

    /// Apply one effect asset with an optional duration.
    pub fn apply_effect(&mut self, effect: &EntityEffectAsset, duration: Option<f32>) -> bool {
        let duration = duration.unwrap_or(effect.duration);
        let changed = self
            .gameplay
            .apply_effect_with(effect, duration, effect.overlap_behavior);
        if let Some(calculator) = effect
            .damage_calculator
            .as_ref()
            .and_then(DamageCalculator::from_value)
        {
            self.damage_over_time.insert(
                effect.index,
                DotState {
                    effect_id: effect.id.clone(),
                    calculator,
                    cooldown: effect.damage_calculator_cooldown.max(0.01),
                    elapsed: 0.0,
                },
            );
        }
        changed
    }

    /// Advance regeneration/effect timers and apply damage-over-time ticks.
    pub fn tick(&mut self, dt: f32, causes: &DamageCauses) {
        self.gameplay.tick(dt);
        let mut expired = Vec::new();
        for (&index, dot) in &mut self.damage_over_time {
            dot.elapsed += dt.max(0.0);
            while dot.elapsed >= dot.cooldown {
                dot.elapsed -= dot.cooldown;
                for (cause, amount) in dot.calculator.roll(dot.cooldown as f64, 0.5) {
                    if amount <= 0.0 {
                        continue;
                    }
                    self.gameplay.apply_damage(
                        &Damage::new(
                            amount,
                            cause,
                            DamageSource::EntityEffect(dot.effect_id.clone()),
                        ),
                        causes,
                    );
                }
            }
            if self.gameplay.effects.get(index).is_none() {
                expired.push(index);
            }
        }
        for index in expired {
            self.damage_over_time.remove(&index);
        }
    }

    /// Drain effect add/update/remove packets.
    pub fn take_effect_updates(&mut self) -> Vec<EntityEffectUpdate> {
        let active: BTreeMap<i32, EntityEffectUpdate> = self
            .gameplay
            .effects
            .active()
            .map(|effect| {
                (
                    effect.effect_index,
                    EntityEffectUpdate {
                        r#type: EffectOp::Add,
                        id: effect.effect_index,
                        remaining_time: effect.remaining_duration,
                        infinite: effect.infinite,
                        debuff: effect.debuff,
                        status_effect_icon: effect.status_effect_icon.clone(),
                    },
                )
            })
            .collect();
        let mut updates = Vec::new();
        for (index, update) in &active {
            if self.known_effects.get(index) != Some(update) {
                updates.push(update.clone());
            }
        }
        for index in self.known_effects.keys() {
            if !active.contains_key(index) {
                updates.push(EntityEffectUpdate {
                    r#type: EffectOp::Remove,
                    id: *index,
                    remaining_time: 0.0,
                    infinite: false,
                    debuff: false,
                    status_effect_icon: None,
                });
            }
        }
        self.known_effects = active;
        updates
    }

    /// Build one `EntityUpdates` packet carrying effect updates.
    pub fn effect_entity_updates(
        network_id: i32,
        updates: Vec<EntityEffectUpdate>,
    ) -> EntityUpdates {
        EntityUpdates {
            removed: None,
            updates: Some(vec![EntityUpdate {
                network_id,
                removed: None,
                updates: Some(vec![ComponentUpdate::EntityEffectsUpdate(
                    EntityEffectsUpdate {
                        entity_effect_updates: updates,
                    },
                )]),
            }]),
        }
    }

    /// Full `Init` snapshot for the joining client, keyed by stat index.
    pub fn init_updates(&self) -> Vec<(i32, EntityStatUpdate)> {
        self.gameplay
            .stats
            .iter()
            .map(|(index, stat)| {
                (
                    index,
                    EntityStatUpdate {
                        op: EntityStatOp::Init,
                        predictable: false,
                        value: stat.get(),
                        modifiers: None,
                        modifier_key: None,
                        modifier: None,
                    },
                )
            })
            .collect()
    }

    /// Drain pending stat deltas for replication, keyed by stat index.
    pub fn take_updates(&mut self) -> Vec<(i32, EntityStatUpdate)> {
        self.gameplay
            .stats
            .take_updates()
            .into_iter()
            .map(|update| (update.index, stat_update_packet(update)))
            .collect()
    }

    /// Build one `EntityUpdates` packet for `network_id` from indexed updates.
    ///
    /// Java's `EntityStatsUpdate.entityStatUpdates` map is keyed by stat index
    /// (`EntityStatMap.createInitUpdate`), not by entity id; the entity id lives
    /// on the enclosing `EntityUpdate`.
    pub fn entity_updates(network_id: i32, updates: Vec<(i32, EntityStatUpdate)>) -> EntityUpdates {
        let mut entity_stat_updates = BTreeMap::new();
        for (index, update) in updates {
            entity_stat_updates
                .entry(index)
                .or_insert_with(Vec::new)
                .push(update);
        }
        EntityUpdates {
            removed: None,
            updates: Some(vec![EntityUpdate {
                network_id,
                removed: None,
                updates: Some(vec![ComponentUpdate::EntityStatsUpdate(
                    EntityStatsUpdate {
                        entity_stat_updates,
                    },
                )]),
            }]),
        }
    }
}

/// Convert one internal replication delta into the wire packet.
pub fn stat_update_packet(update: StatUpdate) -> EntityStatUpdate {
    EntityStatUpdate {
        op: match update.op {
            StatOp::Init => EntityStatOp::Init,
            StatOp::Add => EntityStatOp::Add,
            StatOp::Set => EntityStatOp::Set,
            StatOp::Minimize => EntityStatOp::Minimize,
            StatOp::Maximize => EntityStatOp::Maximize,
            StatOp::Reset => EntityStatOp::Reset,
        },
        predictable: false,
        value: f32::from_bits(update.value),
        modifiers: None,
        modifier_key: None,
        modifier: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_entities::damage::DamageSource;
    use hytale_entities::stats::{EntityStatMap, EntityStatType};

    #[test]
    fn stat_updates_keep_values_and_operations() {
        let update = StatUpdate {
            index: 1,
            op: StatOp::Add,
            before: 100.0f32.to_bits(),
            value: (-15.0f32).to_bits(),
        };
        let packet = stat_update_packet(update);
        assert_eq!(packet.op, EntityStatOp::Add);
        assert_eq!(packet.value, -15.0);
    }

    #[test]
    fn stat_packet_targets_the_entity_and_keys_stats_by_index() {
        let packet = PlayerGameplay::entity_updates(
            7,
            vec![(
                1,
                EntityStatUpdate {
                    op: EntityStatOp::Set,
                    value: 50.0,
                    ..EntityStatUpdate::default()
                },
            )],
        );
        let update = &packet.updates.unwrap()[0];
        assert_eq!(update.network_id, 7);
        let component = &update.updates.as_ref().unwrap()[0];
        let ComponentUpdate::EntityStatsUpdate(stats) = component else {
            panic!("expected EntityStatsUpdate");
        };
        assert_eq!(stats.entity_stat_updates[&1][0].value, 50.0);
        assert!(!stats.entity_stat_updates.contains_key(&7));
    }

    #[test]
    fn revive_resets_health_and_clears_death() {
        let stat = EntityStatType {
            id: "Health".to_string(),
            index: 1,
            initial_value: 100.0,
            min: 0.0,
            max: 100.0,
            shared: true,
            regenerating: Vec::new(),
            reset_behavior: Default::default(),
            ignore_invulnerability: false,
            hide_from_tooltip: false,
        };
        let mut gameplay = EntityGameplay::new(EntityStatMap::new([stat]), 1);
        let causes = DamageCauses::default();
        let result = gameplay.apply_damage(
            &Damage::new(150.0, "Physical", DamageSource::Command),
            &causes,
        );
        assert!(result.died);
        assert!(gameplay.is_dead());
        assert!(gameplay.revive());
        assert_eq!(gameplay.health(), 100.0);
    }
}
