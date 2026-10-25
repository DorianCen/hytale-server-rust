//! Per-section block health and fragility storage.
//!
//! Ported from `CoreServer/.../modules/blockhealth/BlockHealthSection.java`
//! (version 0). The component is a BSON document with two short-keyed maps:
//!
//! ```text
//! {
//!   "Version": 0,
//!   "Health":    {"<section block index>": {"Health": f32, "LastDamage": epoch millis i64}},
//!   "Fragility": {"<section block index>": remaining seconds f32}
//! }
//! ```
//!
//! Both maps are omitted when empty so ordinary undamaged chunks keep the same
//! byte layout as Java.

use std::collections::BTreeMap;

use bson::{Bson, Document};

use super::{Result, SectionError};

/// Java `BlockHealth`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockHealth {
    /// Remaining health (`0.0` destroyed, `1.0` undamaged).
    pub health: f32,
    /// Last damage game time as epoch milliseconds.
    pub last_damage_millis: i64,
}

/// Java `BlockHealthSection` (version 0).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockHealthSection {
    health: BTreeMap<u16, BlockHealth>,
    fragility: BTreeMap<u16, f32>,
    pending_deltas: BTreeMap<u16, f32>,
}

impl BlockHealthSection {
    /// Java `BlockHealth.FULL_HEALTH`.
    pub const FULL_HEALTH: f32 = 1.0;
    /// Java `BlockHealthSection.VERSION`.
    pub const VERSION: i32 = 0;

    /// An empty section.
    pub fn new() -> Self {
        Self::default()
    }

    /// True when both persistent maps are empty (the component must be omitted).
    pub fn is_empty(&self) -> bool {
        self.health.is_empty() && self.fragility.is_empty()
    }

    /// Java `getHealth(int index)`.
    pub fn health(&self, index: u16) -> f32 {
        self.health
            .get(&index)
            .map_or(Self::FULL_HEALTH, |entry| entry.health)
    }

    /// Java `isDamaged`.
    pub fn is_damaged(&self, index: u16) -> bool {
        self.health.contains_key(&index)
    }

    /// Java `getFragilitySeconds`.
    pub fn fragility(&self, index: u16) -> f32 {
        self.fragility.get(&index).copied().unwrap_or(0.0)
    }

    /// Java `isFragile`.
    pub fn is_fragile(&self, index: u16) -> bool {
        self.fragility.contains_key(&index)
    }

    /// Java `damage(int index, float amount, Instant now)`.
    pub fn damage(&mut self, index: u16, amount: f32, now_millis: i64) -> f32 {
        let current = self.health(index);
        let new_health = current - amount;
        if !is_damaged(new_health) {
            self.clear_health(index);
            return new_health;
        }
        self.health.insert(
            index,
            BlockHealth {
                health: new_health,
                last_damage_millis: now_millis,
            },
        );
        *self.pending_deltas.entry(index).or_insert(0.0) += -amount;
        new_health
    }

    /// The damaged entries, in section index order.
    pub fn entries(&self) -> impl Iterator<Item = (u16, BlockHealth)> + '_ {
        self.health.iter().map(|(&index, entry)| (index, *entry))
    }

    /// Java `clearHealth`.
    pub fn clear_health(&mut self, index: u16) -> bool {
        self.pending_deltas.remove(&index);
        self.health.remove(&index).is_some()
    }

    /// Clear every health, fragility and pending-delta entry for one block.
    ///
    /// Java drops the block's component state when a block is replaced; the
    /// damage of a removed block must not outlive the new block.
    pub fn clear(&mut self, index: u16) {
        self.pending_deltas.remove(&index);
        self.health.remove(&index);
        self.fragility.remove(&index);
    }

    /// Java `makeFragile`.
    pub fn make_fragile(&mut self, index: u16, seconds: f32) {
        if is_expired(seconds) {
            self.clear_fragility(index);
        } else {
            self.fragility.insert(index, seconds);
        }
    }

    /// Java `clearFragility`.
    pub fn clear_fragility(&mut self, index: u16) {
        self.fragility.remove(&index);
    }

    /// Java `tickFragility`.
    pub fn tick_fragility(&mut self, delta_seconds: f32) -> bool {
        let before = self.fragility.len();
        self.fragility.retain(|_, remaining| {
            *remaining -= delta_seconds;
            *remaining > 0.0
        });
        self.fragility.len() != before
    }

    /// Java `tickRegeneration`.
    pub fn tick_regeneration(
        &mut self,
        now_millis: i64,
        delta_seconds: f32,
        seconds_until_regeneration: i64,
        healing_per_second: f32,
    ) -> bool {
        let mut removed = false;
        let delay = seconds_until_regeneration * 1000;
        let mut finished = Vec::new();
        for (&index, entry) in &mut self.health {
            if now_millis - entry.last_damage_millis < delay {
                continue;
            }
            let delta = healing_per_second * delta_seconds;
            let new_health = entry.health + delta;
            if new_health < Self::FULL_HEALTH {
                entry.health = new_health;
                *self.pending_deltas.entry(index).or_insert(0.0) += delta;
            } else {
                *self.pending_deltas.entry(index).or_insert(0.0) +=
                    Self::FULL_HEALTH - entry.health;
                finished.push(index);
                removed = true;
            }
        }
        for index in finished {
            self.health.remove(&index);
        }
        removed
    }

    /// Drain replication deltas (`index`, accumulated health delta).
    pub fn take_pending_deltas(&mut self) -> Vec<(u16, f32)> {
        std::mem::take(&mut self.pending_deltas)
            .into_iter()
            .collect()
    }

    /// Insert a migrated health entry, keeping any section entry that already exists.
    pub fn put_health_if_absent(&mut self, index: u16, entry: BlockHealth) {
        if is_damaged(entry.health) {
            self.health.entry(index).or_insert(entry);
        }
    }

    /// Insert a migrated fragility entry, keeping any section entry that already exists.
    pub fn put_fragility_if_absent(&mut self, index: u16, seconds: f32) {
        if !is_expired(seconds) {
            self.fragility.entry(index).or_insert(seconds);
        }
    }

    /// Encode the Java `BlockHealthSection` BSON document.
    pub fn encode_bson(&self) -> Document {
        let mut document = Document::new();
        document.insert("Version", Bson::Int32(Self::VERSION));
        if !self.health.is_empty() {
            let mut health = Document::new();
            for (&index, entry) in &self.health {
                health.insert(
                    index.to_string(),
                    Bson::Document(bson::doc! {
                        "Health": f64::from(entry.health),
                        "LastDamage": entry.last_damage_millis,
                    }),
                );
            }
            document.insert("Health", Bson::Document(health));
        }
        if !self.fragility.is_empty() {
            let mut fragility = Document::new();
            for (&index, seconds) in &self.fragility {
                fragility.insert(index.to_string(), Bson::Double(f64::from(*seconds)));
            }
            document.insert("Fragility", Bson::Document(fragility));
        }
        document
    }

    /// Decode the Java `BlockHealthSection` BSON document.
    pub fn decode_bson(document: &Document) -> Result<Self> {
        let mut section = Self::new();
        if let Some(Bson::Document(health)) = document.get("Health") {
            for (key, value) in health {
                let index = parse_index(key)?;
                let Bson::Document(entry) = value else {
                    return Err(SectionError::InvalidData(
                        "BlockHealth entry is not a document",
                    ));
                };
                let health = match entry.get("Health") {
                    Some(Bson::Double(value)) => *value as f32,
                    Some(_) => {
                        return Err(SectionError::InvalidData(
                            "BlockHealth.Health is not a double",
                        ))
                    }
                    None => return Err(SectionError::InvalidData("BlockHealth.Health is missing")),
                };
                let last_damage_millis = match entry.get("LastDamage") {
                    Some(Bson::Int64(value)) => *value,
                    Some(Bson::Int32(value)) => i64::from(*value),
                    _ => 0,
                };
                section.put_health_if_absent(
                    index,
                    BlockHealth {
                        health,
                        last_damage_millis,
                    },
                );
            }
        }
        if let Some(Bson::Document(fragility)) = document.get("Fragility") {
            for (key, value) in fragility {
                let index = parse_index(key)?;
                let seconds = match value {
                    Bson::Double(value) => *value as f32,
                    Bson::Int32(value) => *value as f32,
                    Bson::Int64(value) => *value as f32,
                    _ => {
                        return Err(SectionError::InvalidData(
                            "BlockHealthSection.Fragility value is not numeric",
                        ))
                    }
                };
                section.put_fragility_if_absent(index, seconds);
            }
        }
        Ok(section)
    }
}

fn parse_index(key: &str) -> Result<u16> {
    key.parse::<u16>()
        .map_err(|_| SectionError::InvalidData("BlockHealthSection key is not a section index"))
}

fn is_damaged(health: f32) -> bool {
    health > 0.0 && health < BlockHealthSection::FULL_HEALTH && !health.is_nan()
}

fn is_expired(seconds: f32) -> bool {
    seconds.is_nan() || seconds <= 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_and_fragility_round_trip_the_java_layout() {
        let mut section = BlockHealthSection::new();
        section.damage(7, 0.25, 1_000);
        section.make_fragile(7, 3.5);
        section.make_fragile(8, 0.0);
        assert!((section.health(7) - 0.75).abs() < 1e-6);
        assert!((section.fragility(7) - 3.5).abs() < 1e-6);

        let document = section.encode_bson();
        let decoded = BlockHealthSection::decode_bson(&document).expect("decode");
        let mut expected = section.clone();
        expected.take_pending_deltas();
        assert_eq!(decoded, expected);
        let health = document.get_document("Health").expect("health map");
        assert!(matches!(health.get("7"), Some(Bson::Document(_))));
        assert!(document.contains_key("Fragility"));
        assert!(!BlockHealthSection::new()
            .encode_bson()
            .contains_key("Health"));
    }

    #[test]
    fn invalid_and_expired_entries_are_dropped() {
        let mut section = BlockHealthSection::new();
        section.put_health_if_absent(
            1,
            BlockHealth {
                health: 0.0,
                last_damage_millis: 0,
            },
        );
        section.put_health_if_absent(
            2,
            BlockHealth {
                health: f32::NAN,
                last_damage_millis: 0,
            },
        );
        section.put_fragility_if_absent(1, -1.0);
        assert!(section.is_empty());
    }

    #[test]
    fn regeneration_closes_the_entry_and_reports_deltas() {
        let mut section = BlockHealthSection::new();
        section.damage(3, 0.95, 0);
        let removed = section.tick_regeneration(6_000, 1.0, 5, 10.0);
        assert!(removed);
        assert!(!section.is_damaged(3));
        let deltas = section.take_pending_deltas();
        // Damage (-0.95) and full regeneration (+0.95) cancel out.
        assert!(deltas
            .iter()
            .any(|(index, delta)| *index == 3 && delta.abs() < 1e-4));
    }
}
