//! Block-type interaction metadata.
//!
//! Java `BlockType` merges the item's `BlockType` object; the `Interactions`
//! map selects a root interaction per `InteractionType`. Benches and containers
//! also have dedicated `Bench`/`Container` objects that are UI consumers rather
//! than interaction nodes.

use std::collections::BTreeMap;

use hytale_protocol::generated::common::InteractionType;
use serde_json::Value;

/// Interactions and UI capabilities declared by one block type.
#[derive(Debug, Clone, Default)]
pub struct BlockInteractions {
    /// Root interaction id per interaction type.
    pub roots: BTreeMap<InteractionType, String>,
    /// The block declares a `Bench` object.
    pub bench: bool,
    /// The block declares a `Container` object.
    pub container: bool,
    /// Declared state definition keys.
    pub states: Vec<String>,
    /// Current state derived from the block asset key.
    pub state: Option<String>,
}

impl BlockInteractions {
    /// Extract metadata from a merged `BlockType` JSON document.
    pub fn from_json(block_key: &str, json: &Value) -> Self {
        let mut roots = BTreeMap::new();
        if let Some(interactions) = json.get("Interactions").and_then(Value::as_object) {
            for (name, value) in interactions {
                let Some(interaction_type) = interaction_type_from_name(name) else {
                    continue;
                };
                if let Some(root) = value.as_str() {
                    roots.insert(interaction_type, root.to_string());
                }
            }
        }
        let states = json
            .get("State")
            .and_then(|state| state.get("Definitions"))
            .and_then(Value::as_object)
            .map(|definitions| definitions.keys().cloned().collect())
            .unwrap_or_default();
        Self {
            roots,
            bench: json.get("Bench").is_some_and(Value::is_object),
            container: json.get("Container").is_some_and(Value::is_object),
            states,
            state: state_from_block_key(block_key),
        }
    }

    /// Root id for one interaction type.
    pub fn root(&self, interaction_type: InteractionType) -> Option<&str> {
        self.roots.get(&interaction_type).map(String::as_str)
    }

    /// Fallback root for right-click use: `Use`, then `Secondary`, then
    /// `Primary`.
    pub fn use_root(&self) -> Option<(InteractionType, &str)> {
        for interaction_type in [
            InteractionType::Use,
            InteractionType::Secondary,
            InteractionType::Primary,
        ] {
            if let Some(root) = self.root(interaction_type) {
                return Some((interaction_type, root));
            }
        }
        None
    }

    /// Whether the current state is one of the open-door variants.
    pub fn is_door_open(&self) -> bool {
        self.state
            .as_deref()
            .is_some_and(|state| state.starts_with("OpenDoor"))
    }

    /// State map lookup with Java's `default` fallback for the base state.
    pub fn mapped_state<'a>(&self, changes: &'a BTreeMap<String, String>) -> Option<&'a String> {
        changes
            .get(self.state.as_deref().unwrap_or("default"))
            .or_else(|| changes.get("default"))
    }
}

/// Parse a Java interaction type name.
pub fn interaction_type_from_name(name: &str) -> Option<InteractionType> {
    Some(match name {
        "Primary" => InteractionType::Primary,
        "Secondary" => InteractionType::Secondary,
        "Ability1" => InteractionType::Ability1,
        "Ability2" => InteractionType::Ability2,
        "Ability3" => InteractionType::Ability3,
        "Ability4" => InteractionType::Ability4,
        "Use" => InteractionType::Use,
        "Pick" => InteractionType::Pick,
        "Pickup" => InteractionType::Pickup,
        "CollisionEnter" => InteractionType::CollisionEnter,
        "CollisionLeave" => InteractionType::CollisionLeave,
        "Collision" => InteractionType::Collision,
        "EntityStatEffect" => InteractionType::EntityStatEffect,
        "SwapTo" => InteractionType::SwapTo,
        "SwapFrom" => InteractionType::SwapFrom,
        "Death" => InteractionType::Death,
        "Wielding" => InteractionType::Wielding,
        "ProjectileSpawn" => InteractionType::ProjectileSpawn,
        "ProjectileHit" => InteractionType::ProjectileHit,
        "ProjectileMiss" => InteractionType::ProjectileMiss,
        "ProjectileBounce" => InteractionType::ProjectileBounce,
        _ => return None,
    })
}

/// Derive the contained `State.Definitions.<name>` suffix from a block key.
pub fn state_from_block_key(key: &str) -> Option<String> {
    let suffix = key.split("_State_Definitions_").nth(1)?;
    Some(suffix.to_string())
}
