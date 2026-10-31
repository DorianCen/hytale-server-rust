//! NPC blackboard state.
//!
//! Role builder instructions read/write a small blackboard: current state,
//! boolean flags, named timers and the selected target. The graph runner in
//! [`crate::builder`] evaluates sensors against this state; matched actions
//! update it here.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::builder::{BuilderAction, BuilderContext};
use crate::role::ResolvedRole;

/// One NPC's instruction/state blackboard.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcBlackboard {
    /// Current state name.
    pub state: String,
    /// Boolean flags.
    pub flags: BTreeMap<String, bool>,
    /// Remaining timer seconds.
    pub timers: BTreeMap<String, f64>,
    /// Selected target slot/id for diagnostics.
    pub target: Option<u64>,
    /// Current target feet position.
    pub target_position: Option<hytale_world::collision::Vec3>,
    /// Visible beacon messages.
    pub beacons: std::collections::BTreeSet<String>,
    /// Current path name, when known.
    pub path: Option<String>,
    /// Builder-driven invulnerability flag.
    pub invulnerable: bool,
    /// Actor is in water.
    pub in_water: bool,
    /// Actor is airborne.
    pub in_air: bool,
    /// Current in-game hour.
    pub time_hour: f64,
    /// Current weather key.
    pub weather: Option<String>,
    /// Actor is busy with an interaction.
    pub busy: bool,
    /// Named alarm states.
    pub alarms: BTreeMap<String, String>,
    /// Nearby matching target count.
    pub nearby_count: usize,
    /// Actor completed an interaction.
    pub has_interacted: bool,
    /// Actor can interact with its target.
    pub can_interact: bool,
    /// Actor is backing away.
    pub is_backing_away: bool,
    /// Actor inflicted damage this tick.
    pub inflicted_damage: bool,
    /// Actor is its flock leader.
    pub flock_leader: bool,
    /// Current animation key.
    pub animation: Option<String>,
    /// Last `Notify` message.
    pub notification: Option<String>,
    /// `OverrideAttitude` value.
    pub attitude_override: Option<String>,
    /// Named stored positions (`StorePosition` slots).
    pub positions: BTreeMap<String, hytale_world::collision::Vec3>,
    /// Current leash position.
    pub leash_position: Option<hytale_world::collision::Vec3>,
    /// Actor feet position used by position-dependent sensors.
    pub position: hytale_world::collision::Vec3,
    /// Current block light.
    pub light: i64,
    /// Current sky light.
    pub sky_light: i64,
    /// A dropped item is nearby.
    pub dropped_item_present: bool,
    /// A watched block changed.
    pub block_changed: bool,
    /// The actor can place its block.
    pub can_place_block: bool,
    /// The actor killed its target.
    pub killed: bool,
    /// Navigation is valid.
    pub nav_ok: bool,
    /// Display name/nameplate override.
    pub display_name: Option<String>,
    /// Interactable flag.
    pub interactable: bool,
    /// Crouch flag.
    pub crouching: bool,
    /// Block key to place.
    pub block_to_place: Option<String>,
    /// Flock state/target.
    pub flock_state: Option<String>,
    /// Flock target id.
    pub flock_target: Option<u64>,
    /// Hostile-target memory ids.
    pub hostile_memory: BTreeSet<u64>,
    /// Current combat ability key.
    pub ability: Option<String>,
    /// Locked-on target id.
    pub locked_target: Option<u64>,
    /// Flock membership key/state.
    pub flock: Option<String>,
    /// Overridden altitude.
    pub altitude: Option<f64>,
    /// Model attachment key.
    pub model_attachment: Option<String>,
    /// Pending pickup item selector.
    pub pickup_item: Option<String>,
    /// Inventory operation (`EquipHotbar`, ...).
    pub inventory_operation: Option<String>,
    /// Mount movement config/key.
    pub mount: Option<String>,
    /// Hit interaction key.
    pub hit_interaction: Option<String>,
    /// Entity-event sensor.
    pub entity_event: bool,
    /// Interaction-context sensor.
    pub interaction_context: bool,
    /// Flock combat-damage sensor.
    pub flock_combat_damage: bool,
    /// Search-ray sensor.
    pub search_ray: bool,
    /// Charge entity-collision sensor.
    pub charge_entity_collisions: bool,
    /// Charge block-collision sensor.
    pub charge_block_collisions: bool,
    /// Random unit value for `Random` sensors.
    pub random_unit: f64,
    /// Minimal NPC-carried items (`PickUpItem`/`DropItem` bookkeeping).
    pub items: BTreeMap<String, i32>,
    /// Currently equipped item (minimal `Inventory` seam).
    pub equipped: Option<String>,
    /// Pending barter shop key.
    pub barter_shop: Option<String>,
    /// Current task/objective keys.
    pub task: Option<String>,
    /// Current objective key.
    pub objective: Option<String>,
    /// State-evaluator toggle.
    pub state_evaluator: bool,
    /// Signaled tagged-volume keys.
    pub signaled_volumes: BTreeSet<String>,
}

impl Default for NpcBlackboard {
    fn default() -> Self {
        Self {
            state: "Idle".to_string(),
            flags: BTreeMap::new(),
            timers: BTreeMap::new(),
            target: None,
            target_position: None,
            beacons: std::collections::BTreeSet::new(),
            path: None,
            invulnerable: false,
            in_water: false,
            in_air: false,
            time_hour: 0.0,
            weather: None,
            busy: false,
            alarms: BTreeMap::new(),
            nearby_count: 0,
            has_interacted: false,
            can_interact: false,
            is_backing_away: false,
            inflicted_damage: false,
            flock_leader: false,
            animation: None,
            notification: None,
            attitude_override: None,
            positions: BTreeMap::new(),
            leash_position: None,
            position: hytale_world::collision::Vec3::default(),
            light: 0,
            sky_light: 0,
            dropped_item_present: false,
            block_changed: false,
            can_place_block: false,
            killed: false,
            nav_ok: false,
            display_name: None,
            interactable: false,
            crouching: false,
            block_to_place: None,
            flock_state: None,
            flock_target: None,
            hostile_memory: BTreeSet::new(),
            ability: None,
            locked_target: None,
            flock: None,
            altitude: None,
            model_attachment: None,
            pickup_item: None,
            inventory_operation: None,
            mount: None,
            hit_interaction: None,
            entity_event: false,
            interaction_context: false,
            flock_combat_damage: false,
            search_ray: false,
            charge_entity_collisions: false,
            charge_block_collisions: false,
            random_unit: 0.0,
            items: BTreeMap::new(),
            equipped: None,
            barter_shop: None,
            task: None,
            objective: None,
            state_evaluator: false,
            signaled_volumes: BTreeSet::new(),
        }
    }
}

impl NpcBlackboard {
    /// Build from a resolved role's `StartState`.
    pub fn from_role(role: &ResolvedRole) -> Self {
        Self {
            state: role
                .raw
                .get("StartState")
                .and_then(Value::as_str)
                .unwrap_or("Idle")
                .to_string(),
            ..Self::default()
        }
    }

    /// Sensor context for the current state.
    pub fn context(&self, target: Option<u64>, player_present: bool) -> BuilderContext {
        BuilderContext {
            state: self.state.clone(),
            flags: self.flags.clone(),
            timers: self
                .timers
                .iter()
                .map(|(name, remaining)| (name.clone(), *remaining > 0.0))
                .collect(),
            player_present,
            target_present: target.is_some(),
            mob_present: target.is_some(),
            line_of_sight: true,
            moving: false,
            damaged: false,
            motion_controller: String::new(),
            health_ratio: 1.0,
            charge_states: Default::default(),
            nearby_blocks: Default::default(),
            beacons: self.beacons.clone(),
            path: self.path.clone(),
            position: self.position,
            leash_position: self.leash_position,
            in_water: self.in_water,
            in_air: self.in_air,
            time_hour: self.time_hour,
            weather: self.weather.clone(),
            busy: self.busy,
            alarms: self.alarms.clone(),
            nearby_count: self.nearby_count,
            has_interacted: self.has_interacted,
            can_interact: self.can_interact,
            is_backing_away: self.is_backing_away,
            inflicted_damage: self.inflicted_damage,
            flock_leader: self.flock_leader,
            animation: self.animation.clone(),
            light: self.light,
            sky_light: self.sky_light,
            dropped_item_present: self.dropped_item_present,
            block_changed: self.block_changed,
            can_place_block: self.can_place_block,
            killed: self.killed,
            nav_ok: self.nav_ok,
            positions: self.positions.clone(),
            entity_event: self.entity_event,
            interaction_context: self.interaction_context,
            flock_combat_damage: self.flock_combat_damage,
            search_ray: self.search_ray,
            charge_entity_collisions: self.charge_entity_collisions,
            charge_block_collisions: self.charge_block_collisions,
            random_unit: self.random_unit,
        }
    }

    /// Advance named timers.
    pub fn tick(&mut self, dt: f64) {
        for remaining in self.timers.values_mut() {
            *remaining = (*remaining - dt.max(0.0)).max(0.0);
        }
    }

    /// Apply matched builder actions to the blackboard.
    pub fn apply_actions(&mut self, actions: &[BuilderAction]) {
        for action in actions {
            match action.kind() {
                "State" | "ParentState" => {
                    if let Some(state) = action.value.get("State").and_then(Value::as_str) {
                        self.state = state.to_string();
                    }
                }
                "SetFlag" | "Flag" => {
                    if let Some((name, value)) = action.flag() {
                        self.flags.insert(name.to_string(), value);
                    }
                }
                "TimerStart" => {
                    if let Some(name) = action.value.get("Name").and_then(Value::as_str) {
                        let value = range_mid(action.value.get("StartValueRange"))
                            .or_else(|| action.value.get("Seconds").and_then(Value::as_f64))
                            .unwrap_or(0.0);
                        self.timers.insert(name.to_string(), value.max(0.0));
                    }
                }
                "TimerRestart" => {
                    if let Some(name) = action.value.get("Name").and_then(Value::as_str) {
                        let value = range_mid(action.value.get("RestartValueRange"))
                            .or_else(|| action.value.get("Seconds").and_then(Value::as_f64))
                            .unwrap_or(0.0);
                        self.timers.insert(name.to_string(), value.max(0.0));
                    }
                }
                "TimerModify" => {
                    if let Some(name) = action.value.get("Name").and_then(Value::as_str) {
                        let value = range_mid(action.value.get("Value"))
                            .or_else(|| action.value.get("Seconds").and_then(Value::as_f64))
                            .unwrap_or(0.0);
                        self.timers.insert(name.to_string(), value.max(0.0));
                    }
                }
                "TimerStop" => {
                    if let Some(name) = action.value.get("Name").and_then(Value::as_str) {
                        self.timers.remove(name);
                    }
                }
                "Beacon" => {
                    if let Some(message) = action.value.get("Message").and_then(Value::as_str) {
                        self.beacons.insert(message.to_string());
                    }
                }
                "ResetPath" => self.path = None,
                "RecomputePath" => self.path = None,
                "ReleaseTarget" => {
                    self.target = None;
                    self.target_position = None;
                }
                "SetMarkedTarget" => {
                    self.target = action
                        .value
                        .get("TargetId")
                        .or_else(|| action.value.get("EntityId"))
                        .or_else(|| action.value.get("Target"))
                        .and_then(Value::as_u64);
                }
                "SetInvulnerable" => {
                    self.invulnerable = action
                        .value
                        .get("Invulnerable")
                        .or_else(|| action.value.get("Value"))
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                }
                "Log" | "Nothing" => {}
                "Sequence" | "ForEach" | "Timeout" => {
                    if let Some(actions) = action.value.get("Actions").and_then(Value::as_array) {
                        let nested = actions
                            .iter()
                            .filter_map(|action| {
                                action.get("Type").and_then(Value::as_str).map(|kind| {
                                    BuilderAction {
                                        kind: kind.to_string(),
                                        value: action.clone(),
                                    }
                                })
                            })
                            .collect::<Vec<_>>();
                        self.apply_actions(&nested);
                    }
                    if let Some(action) = action.value.get("Action") {
                        if let Some(kind) = action.get("Type").and_then(Value::as_str) {
                            self.apply_actions(&[BuilderAction {
                                kind: kind.to_string(),
                                value: action.clone(),
                            }]);
                        }
                    }
                }
                "Random" => {
                    if let Some(actions) = action.value.get("Actions").and_then(Value::as_array) {
                        if let Some(action) = actions.first() {
                            if let Some(kind) = action.get("Type").and_then(Value::as_str) {
                                self.apply_actions(&[BuilderAction {
                                    kind: kind.to_string(),
                                    value: action.clone(),
                                }]);
                            }
                        }
                    }
                }
                "SetAlarm" => {
                    if let Some(name) = action.value.get("Name").and_then(Value::as_str) {
                        let state = action
                            .value
                            .get("State")
                            .and_then(Value::as_str)
                            .unwrap_or("Set");
                        self.alarms.insert(name.to_string(), state.to_string());
                    }
                }
                "TimerContinue" => {
                    if let Some(name) = action.value.get("Name").and_then(Value::as_str) {
                        let value = range_mid(action.value.get("Value"))
                            .or_else(|| action.value.get("Seconds").and_then(Value::as_f64))
                            .unwrap_or(0.0);
                        *self.timers.entry(name.to_string()).or_insert(0.0) += value;
                    }
                }
                "Notify" => {
                    self.notification = action
                        .value
                        .get("Message")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "OverrideAttitude" => {
                    self.attitude_override = action
                        .value
                        .get("Attitude")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "StorePosition" => {
                    if let Some(slot) = action.value.get("Slot").and_then(Value::as_str) {
                        if let Some(position) = self.target_position {
                            self.positions.insert(slot.to_string(), position);
                        }
                    }
                }
                "SetLeashPosition" => {
                    if action
                        .value
                        .get("ToCurrent")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    {
                        self.leash_position = self.target_position;
                    } else if let Some(position) = parse_position(&action.value) {
                        self.leash_position = Some(position);
                    }
                }
                "RefreshPath" => self.path = None,
                "MakePath" => {
                    self.path = action
                        .value
                        .get("Path")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "PlayAnimation" => {
                    self.animation = action
                        .value
                        .get("Animation")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "DisplayName" => {
                    self.display_name = action
                        .value
                        .get("Name")
                        .or_else(|| action.value.get("Text"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "SetInteractable" => {
                    self.interactable = action
                        .value
                        .get("Interactable")
                        .or_else(|| action.value.get("Value"))
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                }
                "Crouch" => {
                    self.crouching = action
                        .value
                        .get("Crouch")
                        .or_else(|| action.value.get("Value"))
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                }
                "SetBlockToPlace" => {
                    self.block_to_place = action
                        .value
                        .get("Block")
                        .or_else(|| action.value.get("BlockId"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "FlockState" => {
                    self.flock_state = action
                        .value
                        .get("State")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "FlockTarget" => {
                    self.flock_target = action
                        .value
                        .get("TargetId")
                        .or_else(|| action.value.get("EntityId"))
                        .and_then(Value::as_u64);
                }
                "AddToHostileTargetMemory" => {
                    if let Some(id) = action
                        .value
                        .get("TargetId")
                        .or_else(|| action.value.get("EntityId"))
                        .or_else(|| action.value.get("Target"))
                        .and_then(Value::as_u64)
                    {
                        self.hostile_memory.insert(id);
                    }
                }
                "FlockBeacon" => {
                    if let Some(message) = action.value.get("Message").and_then(Value::as_str) {
                        self.beacons.insert(message.to_string());
                    }
                }
                "CombatAbility" => {
                    self.ability = action
                        .value
                        .get("Ability")
                        .or_else(|| action.value.get("Name"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "LockOnInteractionTarget" => {
                    self.locked_target = action
                        .value
                        .get("TargetId")
                        .or_else(|| action.value.get("EntityId"))
                        .and_then(Value::as_u64);
                }
                "JoinFlock" | "LeaveFlock" => {
                    self.flock = action
                        .value
                        .get("Flock")
                        .or_else(|| action.value.get("Name"))
                        .or_else(|| action.value.get("Group"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "OverrideAltitude" => {
                    self.altitude = action
                        .value
                        .get("Altitude")
                        .or_else(|| action.value.get("Value"))
                        .and_then(Value::as_f64);
                }
                "ModelAttachment" => {
                    self.model_attachment = action
                        .value
                        .get("Model")
                        .or_else(|| action.value.get("Attachment"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "PickUpItem" => {
                    self.pickup_item = action
                        .value
                        .get("Items")
                        .or_else(|| action.value.get("Item"))
                        .or_else(|| action.value.get("ItemId"))
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned);
                }
                "Inventory" => {
                    let operation = action
                        .value
                        .get("Operation")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    if operation.as_deref() == Some("EquipHotbar") {
                        if let Some(item) = self.items.keys().next().cloned() {
                            self.equipped = Some(item);
                        }
                    }
                    self.inventory_operation = operation;
                }
                "Mount" => {
                    self.mount = action
                        .value
                        .get("MovementConfig")
                        .or_else(|| action.value.get("Mount"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                "EntityHitInteraction" | "BlockHitInteraction" => {
                    self.hit_interaction = action
                        .value
                        .get("Interaction")
                        .and_then(|value| {
                            value
                                .as_str()
                                .or_else(|| value.get("Compute").and_then(Value::as_str))
                        })
                        .map(ToOwned::to_owned);
                }
                "ResetBlockSensors"
                | "ResetSearchRays"
                | "SignalTaggedVolumes"
                | "ToggleStateEvaluator"
                | "OpenBarterShop"
                | "CompleteTask"
                | "StartObjective"
                | "Test"
                | "None" => match action.kind() {
                    "ResetBlockSensors" => {
                        self.block_changed = false;
                        self.can_place_block = false;
                        self.search_ray = false;
                    }
                    "ResetSearchRays" => self.search_ray = false,
                    "ToggleStateEvaluator" => {
                        self.state_evaluator = !self.state_evaluator;
                    }
                    "SignalTaggedVolumes" => {
                        if let Some(key) = action
                            .value
                            .get("Tag")
                            .or_else(|| action.value.get("Signal"))
                            .or_else(|| action.value.get("Key"))
                            .and_then(Value::as_str)
                        {
                            self.signaled_volumes.insert(key.to_string());
                        }
                    }
                    "OpenBarterShop" => {
                        self.barter_shop = action
                            .value
                            .get("Shop")
                            .or_else(|| action.value.get("Name"))
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned);
                    }
                    "CompleteTask" => {
                        self.task = action
                            .value
                            .get("Task")
                            .or_else(|| action.value.get("Name"))
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned);
                    }
                    "StartObjective" => {
                        self.objective = action
                            .value
                            .get("Objective")
                            .or_else(|| action.value.get("Name"))
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned);
                    }
                    _ => {}
                },
                "Path" => {
                    self.path = action
                        .value
                        .get("Path")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                }
                _ => {}
            }
        }
    }
}

fn parse_position(value: &Value) -> Option<hytale_world::collision::Vec3> {
    let position = value.get("Position")?.as_array()?;
    if position.len() != 3 {
        return None;
    }
    Some(hytale_world::collision::Vec3::new(
        position[0].as_f64()?,
        position[1].as_f64()?,
        position[2].as_f64()?,
    ))
}

fn range_mid(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    if let Some(array) = value.as_array() {
        let values: Vec<f64> = array.iter().filter_map(Value::as_f64).collect();
        return match values.as_slice() {
            [] => None,
            [single] => Some(*single),
            [low, high, ..] => Some((low + high) * 0.5),
        };
    }
    value
        .get("Min")
        .and_then(Value::as_f64)
        .zip(value.get("Max").and_then(Value::as_f64))
        .map(|(low, high)| (low + high) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::BuilderAction;

    fn action(kind: &str, value: Value) -> BuilderAction {
        BuilderAction {
            kind: kind.to_string(),
            value,
        }
    }

    #[test]
    fn actions_update_state_flags_and_timers() {
        let mut blackboard = NpcBlackboard::default();
        blackboard.apply_actions(&[
            action("State", serde_json::json!({"State": "Combat"})),
            action(
                "SetFlag",
                serde_json::json!({"Name": "Alerted", "Value": true}),
            ),
            action(
                "TimerStart",
                serde_json::json!({"Name": "Chase_Ready", "StartValueRange": [1, 3]}),
            ),
        ]);
        assert_eq!(blackboard.state, "Combat");
        assert_eq!(blackboard.flags.get("Alerted"), Some(&true));
        assert_eq!(blackboard.timers.get("Chase_Ready"), Some(&2.0));
        blackboard.tick(1.0);
        assert_eq!(blackboard.timers.get("Chase_Ready"), Some(&1.0));
        blackboard.apply_actions(&[action(
            "TimerStop",
            serde_json::json!({"Name": "Chase_Ready"}),
        )]);
        assert!(!blackboard.timers.contains_key("Chase_Ready"));
    }

    #[test]
    fn target_release_invulnerability_and_marked_target_actions_apply() {
        let mut blackboard = NpcBlackboard {
            target: Some(7),
            ..NpcBlackboard::default()
        };
        blackboard.apply_actions(&[
            action("SetMarkedTarget", serde_json::json!({"TargetId": 42})),
            action("SetInvulnerable", serde_json::json!({"Invulnerable": true})),
        ]);
        assert_eq!(blackboard.target, Some(42));
        assert!(blackboard.invulnerable);
        blackboard.apply_actions(&[action("ReleaseTarget", serde_json::json!({}))]);
        assert_eq!(blackboard.target, None);
        assert_eq!(blackboard.target_position, None);
        blackboard.apply_actions(&[action(
            "SetInvulnerable",
            serde_json::json!({"Value": false}),
        )]);
        assert!(!blackboard.invulnerable);
    }

    #[test]
    fn notify_attitude_position_and_leash_actions_apply() {
        let mut blackboard = NpcBlackboard {
            target_position: Some(hytale_world::collision::Vec3::new(1.0, 2.0, 3.0)),
            ..NpcBlackboard::default()
        };
        blackboard.apply_actions(&[
            action("Notify", serde_json::json!({"Message": "Goblin_Attack"})),
            action(
                "OverrideAttitude",
                serde_json::json!({"Attitude": "Hostile"}),
            ),
            action("StorePosition", serde_json::json!({"Slot": "SteppedBlock"})),
            action("SetLeashPosition", serde_json::json!({"ToCurrent": true})),
        ]);
        assert_eq!(blackboard.notification.as_deref(), Some("Goblin_Attack"));
        assert_eq!(blackboard.attitude_override.as_deref(), Some("Hostile"));
        assert_eq!(
            blackboard.positions.get("SteppedBlock").copied(),
            Some(hytale_world::collision::Vec3::new(1.0, 2.0, 3.0))
        );
        assert_eq!(
            blackboard.leash_position,
            Some(hytale_world::collision::Vec3::new(1.0, 2.0, 3.0))
        );
    }

    #[test]
    fn presentation_flock_path_and_place_actions_apply() {
        let mut blackboard = NpcBlackboard::default();
        blackboard.items.insert("Food_Apple".to_string(), 1);
        blackboard.apply_actions(&[
            action(
                "PlayAnimation",
                serde_json::json!({"Animation": "Greeting"}),
            ),
            action("DisplayName", serde_json::json!({"Name": "Guard"})),
            action("SetInteractable", serde_json::json!({"Interactable": true})),
            action("Crouch", serde_json::json!({"Crouch": true})),
            action(
                "SetBlockToPlace",
                serde_json::json!({"Block": "Rock_Stone"}),
            ),
            action("FlockState", serde_json::json!({"State": "Moving"})),
            action("FlockTarget", serde_json::json!({"TargetId": 9})),
            action(
                "AddToHostileTargetMemory",
                serde_json::json!({"TargetId": 11}),
            ),
            action("FlockBeacon", serde_json::json!({"Message": "Flock_Alert"})),
            action("CombatAbility", serde_json::json!({"Ability": "Charge"})),
            action(
                "LockOnInteractionTarget",
                serde_json::json!({"TargetId": 12}),
            ),
            action("JoinFlock", serde_json::json!({"Flock": "Wolves"})),
            action("OverrideAltitude", serde_json::json!({"Altitude": 12.5})),
            action("ModelAttachment", serde_json::json!({"Model": "Saddle"})),
            action("PickUpItem", serde_json::json!({"Items": "Food_Apple"})),
            action("Inventory", serde_json::json!({"Operation": "EquipHotbar"})),
            action(
                "Mount",
                serde_json::json!({"MovementConfig": "Goblin_Launcher_Immobile"}),
            ),
            action(
                "EntityHitInteraction",
                serde_json::json!({"Interaction": "Combat_Charge_Hit"}),
            ),
            action("MakePath", serde_json::json!({"Path": "Path_Home"})),
        ]);
        assert_eq!(blackboard.animation.as_deref(), Some("Greeting"));
        assert_eq!(blackboard.display_name.as_deref(), Some("Guard"));
        assert!(blackboard.interactable);
        assert!(blackboard.crouching);
        assert_eq!(blackboard.block_to_place.as_deref(), Some("Rock_Stone"));
        assert_eq!(blackboard.flock_state.as_deref(), Some("Moving"));
        assert_eq!(blackboard.flock_target, Some(9));
        assert!(blackboard.hostile_memory.contains(&11));
        assert!(blackboard.beacons.contains("Flock_Alert"));
        assert_eq!(blackboard.ability.as_deref(), Some("Charge"));
        assert_eq!(blackboard.locked_target, Some(12));
        assert_eq!(blackboard.flock.as_deref(), Some("Wolves"));
        assert_eq!(blackboard.altitude, Some(12.5));
        assert_eq!(blackboard.model_attachment.as_deref(), Some("Saddle"));
        assert_eq!(blackboard.pickup_item.as_deref(), Some("Food_Apple"));
        assert_eq!(
            blackboard.inventory_operation.as_deref(),
            Some("EquipHotbar")
        );
        assert_eq!(blackboard.equipped.as_deref(), Some("Food_Apple"));
        assert_eq!(
            blackboard.mount.as_deref(),
            Some("Goblin_Launcher_Immobile")
        );
        assert_eq!(
            blackboard.hit_interaction.as_deref(),
            Some("Combat_Charge_Hit")
        );
        assert_eq!(blackboard.path.as_deref(), Some("Path_Home"));
        blackboard.apply_actions(&[action("ResetPath", serde_json::json!({}))]);
        assert!(blackboard.path.is_none());
    }

    #[test]
    fn nested_timeout_action_flattens_and_applies() {
        let action = BuilderAction {
            kind: "Timeout".to_string(),
            value: serde_json::json!({
                "Type": "Timeout",
                "Delay": [1, 1],
                "Action": {
                    "Type": "Sequence",
                    "Actions": [
                        {"Type": "SetBlockToPlace", "Block": "Rock_Marble_Cobble"},
                        {"Type": "SetFlag", "Name": "Nested", "Value": true}
                    ]
                }
            }),
        };
        let flat = action.flatten();
        assert!(flat.iter().any(|action| action.kind == "SetBlockToPlace"));
        assert!(flat.iter().any(|action| action.kind == "SetFlag"));

        let mut blackboard = NpcBlackboard::default();
        blackboard.apply_actions(&[action]);
        assert_eq!(
            blackboard.block_to_place.as_deref(),
            Some("Rock_Marble_Cobble")
        );
        assert_eq!(blackboard.flags.get("Nested"), Some(&true));
    }

    #[test]
    fn reset_signal_task_and_shop_actions_apply() {
        let mut blackboard = NpcBlackboard {
            block_changed: true,
            can_place_block: true,
            search_ray: true,
            ..NpcBlackboard::default()
        };
        blackboard.apply_actions(&[
            action("ResetBlockSensors", serde_json::json!({})),
            action("SignalTaggedVolumes", serde_json::json!({"Tag": "Power"})),
            action("ToggleStateEvaluator", serde_json::json!({})),
            action("OpenBarterShop", serde_json::json!({"Shop": "Shop_Test"})),
            action("CompleteTask", serde_json::json!({"Task": "Task_A"})),
            action("StartObjective", serde_json::json!({"Objective": "Obj_B"})),
        ]);
        assert!(!blackboard.block_changed);
        assert!(!blackboard.can_place_block);
        assert!(!blackboard.search_ray);
        assert!(blackboard.signaled_volumes.contains("Power"));
        assert!(blackboard.state_evaluator);
        assert_eq!(blackboard.barter_shop.as_deref(), Some("Shop_Test"));
        assert_eq!(blackboard.task.as_deref(), Some("Task_A"));
        assert_eq!(blackboard.objective.as_deref(), Some("Obj_B"));
    }

    #[test]
    fn nested_alarm_and_timer_continue_actions_apply() {
        let mut blackboard = NpcBlackboard::default();
        blackboard.apply_actions(&[action(
            "Sequence",
            serde_json::json!({"Actions": [
                {"Type": "SetFlag", "Name": "Sequence", "Value": true},
                {"Type": "SetAlarm", "Name": "Produce_Ready", "State": "Set"}
            ]}),
        )]);
        assert_eq!(blackboard.flags.get("Sequence"), Some(&true));
        assert_eq!(
            blackboard.alarms.get("Produce_Ready").map(String::as_str),
            Some("Set")
        );

        blackboard.apply_actions(&[action(
            "TimerStart",
            serde_json::json!({"Name": "Chase", "Seconds": 1.0}),
        )]);
        blackboard.apply_actions(&[action(
            "TimerContinue",
            serde_json::json!({"Name": "Chase", "Seconds": 2.0}),
        )]);
        assert_eq!(blackboard.timers.get("Chase"), Some(&3.0));

        blackboard.apply_actions(&[action(
            "Random",
            serde_json::json!({"Actions": [
                {"Type": "SetFlag", "Name": "Random", "Value": true}
            ]}),
        )]);
        assert_eq!(blackboard.flags.get("Random"), Some(&true));
    }
}
