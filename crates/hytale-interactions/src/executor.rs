//! Minimal interaction-graph executor for block interactions.
//!
//! The Java graph is large; this walks the structural nodes and block actions
//! needed by doors, state changes and containers. Nodes it does not implement
//! are inert but still follow `Next`, so asset data never crashes a session.

use std::collections::BTreeMap;

use hytale_protocol::generated::common::{GameMode, InteractionType, PlaceMode};
use serde_json::{Map, Value};

use crate::catalog::InteractionCatalog;

/// A world/UI action requested by the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldAction {
    /// Change the target block's state using the `Changes` state map.
    ChangeState {
        /// `current state -> new state` map.
        changes: BTreeMap<String, String>,
    },
    /// Change the target block key using the `Changes` block map.
    ChangeBlock {
        /// `current block key -> new block key` map.
        changes: BTreeMap<String, String>,
    },
    /// Toggle a door (vertical or horizontal).
    Door {
        /// Java `DoorInteraction.Horizontal`.
        horizontal: bool,
    },
    /// Open a block container.
    OpenContainer,
    /// Open a crafting/processing bench.
    OpenBench,
    /// Run the target block's interaction for this type.
    UseBlock {
        /// Interaction type to dispatch on the target block.
        interaction_type: InteractionType,
    },
    /// Break the target block.
    BreakBlock,
    /// Place the held block at the target.
    PlaceBlock,
    /// Change the sender's active hotbar slot.
    ChangeActiveSlot {
        /// Target hotbar slot.
        target_slot: i32,
    },
}

/// Runtime conditions visible to the graph.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Current game mode.
    pub game_mode: GameMode,
    /// Whether the entity is crouching.
    pub crouching: bool,
    /// Whether the entity is jumping.
    pub jumping: bool,
    /// Whether the entity is swimming.
    pub swimming: bool,
    /// Whether the entity is running.
    pub running: bool,
    /// Whether the entity is flying.
    pub flying: bool,
    /// Whether the entity is gliding.
    pub gliding: bool,
    /// Original interaction type.
    pub interaction_type: InteractionType,
    /// Creative placement mode from `SyncPlayerPreferences`.
    pub place_mode: PlaceMode,
    /// Current memories level from the gameplay config.
    pub memories_level: Option<i32>,
    /// Charge duration in seconds for `Charging`/`Wielding`.
    pub charge_value: f32,
    /// Whether the charging interaction was cancelled/failed.
    pub charge_failed: bool,
    /// Client-selected `Chaining.Next` index, when the chain carries one.
    pub chaining_index: Option<usize>,
    /// Client-selected `Chaining.Flags` index, when the chain carries one.
    pub flag_index: Option<usize>,
    /// Current active hotbar slot, if known.
    pub active_slot: Option<i32>,
    /// Interaction variable substitutions.
    pub vars: BTreeMap<String, String>,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            game_mode: GameMode::Adventure,
            crouching: false,
            jumping: false,
            swimming: false,
            running: false,
            flying: false,
            gliding: false,
            interaction_type: InteractionType::Use,
            place_mode: PlaceMode::Default,
            memories_level: None,
            charge_value: 0.0,
            charge_failed: false,
            chaining_index: None,
            flag_index: None,
            active_slot: None,
            vars: BTreeMap::new(),
        }
    }
}

/// Result of walking a graph.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionOutcome {
    /// Actions in execution order.
    pub actions: Vec<WorldAction>,
    /// Whether the chain completed.
    pub success: bool,
}

impl ExecutionOutcome {
    /// Whether the chain produced at least one block/UI action.
    pub fn has_actions(&self) -> bool {
        !self.actions.is_empty()
    }
}

/// Walk one root interaction.
pub fn execute_root(
    catalog: &InteractionCatalog,
    root_id: &str,
    context: &ExecutionContext,
) -> ExecutionOutcome {
    let Some(root) = catalog.root(root_id) else {
        return ExecutionOutcome::default();
    };
    execute_interactions(catalog, root.get("Interactions"), context, 0)
}

/// Walk an `Interactions` value (root, inline node or reference array).
pub fn execute_interactions(
    catalog: &InteractionCatalog,
    interactions: Option<&Value>,
    context: &ExecutionContext,
    depth: usize,
) -> ExecutionOutcome {
    if depth > 64 {
        return ExecutionOutcome::default();
    }
    let Some(interactions) = interactions else {
        return ExecutionOutcome::default();
    };
    match interactions {
        Value::Array(values) => {
            let mut outcome = ExecutionOutcome {
                success: true,
                ..ExecutionOutcome::default()
            };
            for value in values {
                let next = execute_value(catalog, value, context, depth + 1);
                outcome.actions.extend(next.actions);
                outcome.success &= next.success;
                if !next.success {
                    break;
                }
            }
            outcome
        }
        value => execute_value(catalog, value, context, depth + 1),
    }
}

/// Walk one reference/inline node.
pub fn execute_value(
    catalog: &InteractionCatalog,
    value: &Value,
    context: &ExecutionContext,
    depth: usize,
) -> ExecutionOutcome {
    if depth > 64 {
        return ExecutionOutcome::default();
    }
    match value {
        Value::String(id) => {
            if let Some(node) = catalog.interaction(id) {
                execute_node(catalog, node, context, depth + 1)
            } else {
                execute_root(catalog, id, context)
            }
        }
        Value::Object(_) => {
            let Some(node) = catalog.resolve_node(value) else {
                return ExecutionOutcome::default();
            };
            execute_node(catalog, &node, context, depth + 1)
        }
        _ => ExecutionOutcome::default(),
    }
}

fn execute_node(
    catalog: &InteractionCatalog,
    node: &Value,
    context: &ExecutionContext,
    depth: usize,
) -> ExecutionOutcome {
    let node_type = node.get("Type").and_then(Value::as_str).unwrap_or("Simple");
    let mut outcome = ExecutionOutcome {
        success: true,
        ..ExecutionOutcome::default()
    };

    match node_type {
        "Condition" => {
            if condition_matches(node, context) {
                return execute_interactions(catalog, node.get("Next"), context, depth + 1);
            }
            return execute_interactions(catalog, node.get("Failed"), context, depth + 1);
        }
        "Replace" => {
            let variable = node.get("Var").and_then(Value::as_str).unwrap_or_default();
            let replacement = context
                .vars
                .get(variable)
                .map(|value| Value::String(value.clone()))
                .or_else(|| node.get("DefaultValue").cloned());
            return match replacement {
                Some(value) if value.get("Interactions").is_some() => {
                    execute_interactions(catalog, value.get("Interactions"), context, depth + 1)
                }
                Some(value) => execute_value(catalog, &value, context, depth + 1),
                None => ExecutionOutcome::default(),
            };
        }
        "Serial" | "Parallel" => {
            return execute_interactions(
                catalog,
                node.get("Interactions").or_else(|| node.get("Next")),
                context,
                depth + 1,
            );
        }
        "Chaining" => {
            let branch = chaining_branch(node, context);
            let mut outcome = execute_optional(catalog, branch, context, depth + 1);
            outcome.success = branch.is_some();
            return outcome;
        }
        "Charging" | "Wielding" => {
            let branch = if context.charge_failed {
                node.get("Failed")
            } else {
                charging_branch(node, context.charge_value).or_else(|| node.get("Failed"))
            };
            let mut outcome = execute_optional(catalog, branch, context, depth + 1);
            outcome.success = branch.is_some() && !context.charge_failed;
            return outcome;
        }
        "MemoriesCondition" => {
            let branch = memories_branch(node, context.memories_level);
            let mut outcome = execute_optional(catalog, branch, context, depth + 1);
            outcome.success = branch.is_some();
            return outcome;
        }
        "PlaceModeSelect" => {
            let branch = place_mode_branch(node, context);
            let mut outcome = execute_optional(catalog, branch, context, depth + 1);
            if let Some(next) = node.get("Next") {
                let next_outcome = execute_interactions(catalog, Some(next), context, depth + 1);
                outcome.actions.extend(next_outcome.actions);
                outcome.success &= next_outcome.success;
            }
            return outcome;
        }
        "ChangeActiveSlot" => {
            let target = node
                .get("TargetSlot")
                .and_then(Value::as_i64)
                .map(|slot| slot as i32)
                .or(context.active_slot)
                .unwrap_or(i32::MIN);
            if target != i32::MIN && Some(target) != context.active_slot {
                outcome.actions.push(WorldAction::ChangeActiveSlot {
                    target_slot: target,
                });
            }
        }
        "Repeat" => {
            return execute_interactions(
                catalog,
                node.get("ForkInteractions")
                    .and_then(|fork| fork.get("Interactions"))
                    .or_else(|| node.get("Interactions")),
                context,
                depth + 1,
            );
        }
        "Door" => {
            outcome.actions.push(WorldAction::Door {
                horizontal: node
                    .get("Horizontal")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
        "ChangeState" => {
            outcome.actions.push(WorldAction::ChangeState {
                changes: string_map(node.get("Changes")),
            });
        }
        "ChangeBlock" => {
            outcome.actions.push(WorldAction::ChangeBlock {
                changes: string_map(node.get("Changes")),
            });
        }
        "UseBlock" => {
            outcome.actions.push(WorldAction::UseBlock {
                interaction_type: context.interaction_type,
            });
        }
        "OpenContainer" => outcome.actions.push(WorldAction::OpenContainer),
        "OpenBench" => outcome.actions.push(WorldAction::OpenBench),
        "PlaceBlock" => outcome.actions.push(WorldAction::PlaceBlock),
        "BreakBlock" | "DestroyBlock" => outcome.actions.push(WorldAction::BreakBlock),
        _ => {}
    }

    if outcome.success {
        if let Some(next) = node.get("Next") {
            let next_outcome = execute_interactions(catalog, Some(next), context, depth + 1);
            // A node action (Door/ChangeState) is terminal unless the graph
            // explicitly supplies Next.
            outcome.actions.extend(next_outcome.actions);
            outcome.success = next_outcome.success;
        }
    } else if let Some(failed) = node.get("Failed") {
        outcome = execute_interactions(catalog, Some(failed), context, depth + 1);
    }

    outcome
}

fn execute_optional(
    catalog: &InteractionCatalog,
    value: Option<&Value>,
    context: &ExecutionContext,
    depth: usize,
) -> ExecutionOutcome {
    value.map_or_else(ExecutionOutcome::default, |value| {
        execute_value(catalog, value, context, depth + 1)
    })
}

/// Select the `Chaining.Next`/`Flags` branch for this context.
pub(crate) fn chaining_branch<'a>(
    node: &'a Value,
    context: &ExecutionContext,
) -> Option<&'a Value> {
    if let Some(flag_index) = context.flag_index {
        let flags = node.get("Flags").and_then(Value::as_object)?;
        let mut keys: Vec<&String> = flags.keys().collect();
        keys.sort();
        return keys.get(flag_index).and_then(|key| flags.get(*key));
    }
    let next = node.get("Next").and_then(Value::as_array)?;
    next.get(context.chaining_index.unwrap_or(0))
}

/// Select the closest charge branch at or below `charge_value`.
pub(crate) fn charging_branch(node: &Value, charge_value: f32) -> Option<&Value> {
    let next = node.get("Next")?;
    let Some(map) = next.as_object() else {
        // `Wielding.Next` is a single child and always runs at 0 seconds.
        return Some(next);
    };
    if is_interaction_object(map) {
        return Some(next);
    }
    map.iter()
        .filter_map(|(key, value)| {
            key.parse::<f32>()
                .ok()
                .filter(|key| *key <= charge_value)
                .map(|key| (key, value))
        })
        .max_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, value)| value)
}

/// Select the exact memories level branch, or the failed branch.
pub(crate) fn memories_branch(node: &Value, memories_level: Option<i32>) -> Option<&Value> {
    let level = memories_level.unwrap_or(0);
    node.get("Next")
        .and_then(Value::as_object)
        .and_then(|next| next.get(&level.to_string()))
        .or_else(|| node.get("Failed"))
}

/// Select the `PlaceModeSelect` branch for this context.
pub(crate) fn place_mode_branch<'a>(
    node: &'a Value,
    context: &ExecutionContext,
) -> Option<&'a Value> {
    let field = if context.game_mode == GameMode::Creative {
        match context.place_mode {
            PlaceMode::Replace => "ReplaceInteractions",
            PlaceMode::Retype => "RetypeInteractions",
            PlaceMode::Extrude => "ExtrudeInteractions",
            PlaceMode::SurfaceDraw => "SurfaceDrawInteractions",
            PlaceMode::FastPlace => "FastPlaceInteractions",
            PlaceMode::Default => "DefaultInteractions",
        }
    } else {
        "DefaultInteractions"
    };
    node.get(field)
        .filter(|value| !value.is_null())
        .or_else(|| node.get("DefaultInteractions"))
}

fn is_interaction_object(map: &Map<String, Value>) -> bool {
    map.contains_key("Type")
        || map.contains_key("Parent")
        || map.contains_key("Interactions")
        || map.contains_key("Next")
        || map.contains_key("Failed")
}

/// Select the branch a specialized interaction would execute, for tree walks.
pub(crate) fn selected_branch<'a>(
    node: &'a Value,
    context: &ExecutionContext,
) -> Option<Option<&'a Value>> {
    match node.get("Type").and_then(Value::as_str).unwrap_or("Simple") {
        "Chaining" => Some(chaining_branch(node, context)),
        "Charging" | "Wielding" => Some(if context.charge_failed {
            node.get("Failed")
        } else {
            charging_branch(node, context.charge_value).or_else(|| node.get("Failed"))
        }),
        "MemoriesCondition" => Some(memories_branch(node, context.memories_level)),
        "PlaceModeSelect" => Some(place_mode_branch(node, context)),
        "Condition" => Some(if condition_matches(node, context) {
            node.get("Next")
        } else {
            node.get("Failed")
        }),
        _ => None,
    }
}

pub(crate) fn condition_matches(node: &Value, context: &ExecutionContext) -> bool {
    if let Some(required) = node.get("RequiredGameMode").and_then(Value::as_str) {
        let mode = match required {
            "Adventure" => GameMode::Adventure,
            "Creative" => GameMode::Creative,
            _ => context.game_mode,
        };
        if mode != context.game_mode {
            return false;
        }
    }
    for (key, value) in [
        ("Crouching", context.crouching),
        ("Jumping", context.jumping),
        ("Swimming", context.swimming),
        ("Running", context.running),
        ("Flying", context.flying),
        ("Gliding", context.gliding),
    ] {
        if let Some(required) = node.get(key).and_then(Value::as_bool) {
            if required != value {
                return false;
            }
        }
    }
    true
}

fn string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}
