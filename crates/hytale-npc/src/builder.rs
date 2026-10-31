//! NPC builder instruction graph.
//!
//! Java role documents are trees of `Sensor`/`Instructions`/`Actions` nodes.
//! The runtime subset here parses those trees, evaluates the common sensors
//! (`State`, `Any`, `Not`, `And`, `Or`, timers, targets) and returns the actions
//! of every matching branch in document order. Role `Reference`/`Modify`
//! inheritance is already resolved by [`crate::role`].
//!
//! This is the graph seam the scripted [`crate::combat::NpcActor`] can consume
//! without hard-coding individual role states. Exotic Java sensors/actions are
//! preserved as [`BuilderAction::Unknown`] so a document always parses.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::role::ResolvedRole;

/// One matched or unmatched instruction node.
#[derive(Debug, Clone, PartialEq)]
pub struct InstructionNode {
    /// Optional sensor on this node.
    pub sensor: Option<Sensor>,
    /// Nested instruction nodes.
    pub instructions: Vec<InstructionNode>,
    /// Actions executed when the sensor matches.
    pub actions: Vec<BuilderAction>,
    /// `Reference` resolved into this node.
    pub reference: Option<String>,
    /// Raw `Modify` block, when present.
    pub modify: Option<Value>,
    /// Source comment (`$Comment`), useful for diagnostics.
    pub comment: Option<String>,
}

/// Supported sensors.
#[derive(Debug, Clone, PartialEq)]
pub enum Sensor {
    /// `Any`; `once` fires at most once per graph instance.
    Any {
        /// Fire once.
        once: bool,
    },
    /// Current state name.
    State(String),
    /// Logical negation.
    Not(Box<Sensor>),
    /// All child sensors match.
    And(Vec<Sensor>),
    /// Any child sensor matches.
    Or(Vec<Sensor>),
    /// A player is available.
    Player,
    /// A target is available.
    Target,
    /// An `NPCGroup`/`Mob`/`ViewSector`/`LineOfSight` target filter set.
    Mob {
        /// A matching nearby mob exists.
        present: bool,
        /// Required line of sight.
        line_of_sight: bool,
    },
    /// Named timer state (`Stopped`/`Running`).
    Timer {
        /// Timer name.
        name: String,
        /// Required stopped state.
        stopped: bool,
    },
    /// Encounter/role flag.
    Flag {
        /// Flag name.
        name: String,
        /// Required value.
        value: bool,
    },
    /// `Self` filter sensor (movement/idle/stat/motion filters).
    SelfFilters(Vec<SelfFilter>),
    /// `Stat` sensor relative to health.
    Stat {
        /// Inclusive minimum ratio.
        min: f64,
        /// Inclusive maximum ratio.
        max: f64,
    },
    /// Charge-state sensor.
    ChargeState(Vec<String>),
    /// Nearby-block sensor.
    Block(BTreeSet<String>),
    /// Beacon-message sensor.
    Beacon(String),
    /// Active path sensor.
    Path(String),
    /// `Damage` sensor: the actor was damaged.
    Damage,
    /// `MotionController` sensor: active controller type.
    MotionController(String),
    /// Actor is in water.
    InWater,
    /// Actor is airborne.
    InAir,
    /// In-game time-of-day period (`Time.Period`).
    Time {
        /// Inclusive start hour.
        start: f64,
        /// Inclusive end hour (may wrap past midnight).
        end: f64,
    },
    /// Weather-key sensor (glob patterns from `Weathers`).
    Weather(BTreeSet<String>),
    /// Actor is busy with an interaction.
    IsBusy,
    /// Named alarm state sensor.
    Alarm {
        /// Alarm name.
        name: String,
        /// Required state.
        state: String,
    },
    /// Nearby-target count sensor.
    Count {
        /// Inclusive minimum.
        min: usize,
        /// Inclusive maximum.
        max: usize,
    },
    /// Actor has completed an interaction.
    HasInteracted,
    /// Actor can interact with its target.
    CanInteract,
    /// Actor is backing away.
    IsBackingAway,
    /// Actor inflicted damage this tick.
    InflictedDamage,
    /// Actor is its flock's leader.
    FlockLeader,
    /// Actor moved beyond its leash range.
    Leash {
        /// Leash range.
        range: f64,
    },
    /// Block/light range sensor.
    Light {
        /// Inclusive block-light minimum.
        min: i64,
        /// Inclusive block-light maximum.
        max: i64,
        /// Inclusive sky-light minimum.
        sky_min: i64,
        /// Inclusive sky-light maximum.
        sky_max: i64,
    },
    /// Static builder switch.
    Switch(bool),
    /// A dropped item is nearby.
    DroppedItem,
    /// A watched block changed.
    BlockChange,
    /// The actor can place its block.
    CanPlaceBlock,
    /// The actor killed its target.
    Kill,
    /// Navigation is valid.
    Nav,
    /// An `Eval`/combat-action result.
    Eval,
    /// Reads a named position slot.
    ReadPosition(String),
    /// Wraps a child sensor with a block-set filter.
    BlockType(Box<Sensor>),
    /// Entity-event sensor.
    EntityEvent,
    /// Interaction-context sensor.
    InteractionContext,
    /// Flock combat-damage sensor.
    FlockCombatDamage,
    /// Search-ray sensor.
    SearchRay,
    /// Adjusts the reported position of a child sensor.
    AdjustPosition(Box<Sensor>),
    /// Charge entity-collision sensor.
    ChargeEntityCollisions,
    /// Charge block-collision sensor.
    ChargeBlockCollisions,
    /// Random-chance sensor (`Random`).
    Random {
        /// Chance in `[0, 1]`.
        chance: f64,
    },
    /// Current animation key.
    Animation(String),
    /// Game/role flag (same storage as `Flag`).
    GameFlag {
        /// Flag name.
        name: String,
        /// Required value.
        value: bool,
    },
    /// Unsupported sensor kind, preserved by name.
    Unknown(String),
}

/// One action in a matched branch.
#[derive(Debug, Clone, PartialEq)]
pub struct BuilderAction {
    /// Java action `Type`.
    pub kind: String,
    /// Full action document.
    pub value: Value,
}

impl BuilderAction {
    /// Action type.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// This action plus nested `Actions`/`Action` entries, in document order.
    pub fn flatten(&self) -> Vec<BuilderAction> {
        let mut out = vec![self.clone()];
        for key in ["Actions", "Action"] {
            let Some(value) = self.value.get(key) else {
                continue;
            };
            let nested = value
                .as_array()
                .map(|actions| actions.iter().collect::<Vec<_>>())
                .unwrap_or_else(|| vec![value]);
            for action in nested {
                if let Some(kind) = action.get("Type").and_then(Value::as_str) {
                    out.extend(
                        BuilderAction {
                            kind: kind.to_string(),
                            value: action.clone(),
                        }
                        .flatten(),
                    );
                }
            }
        }
        out
    }

    /// `State`/`ParentState` target.
    pub fn state(&self) -> Option<&str> {
        self.value.get("State").and_then(Value::as_str)
    }

    /// `SetFlag`/`Flag` name and value.
    pub fn flag(&self) -> Option<(&str, bool)> {
        Some((
            self.value.get("Name")?.as_str()?,
            self.value.get("Value").and_then(Value::as_bool)?,
        ))
    }
}

/// Evaluation context for one NPC tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BuilderContext {
    /// Current state name.
    pub state: String,
    /// Boolean flags.
    pub flags: BTreeMap<String, bool>,
    /// Running timers.
    pub timers: BTreeMap<String, bool>,
    /// A player target exists.
    pub player_present: bool,
    /// A generic target exists.
    pub target_present: bool,
    /// A mob target exists.
    pub mob_present: bool,
    /// Line of sight to the selected target.
    pub line_of_sight: bool,
    /// The actor is idle/moving for `Self` filters.
    pub moving: bool,
    /// The actor was damaged this tick.
    pub damaged: bool,
    /// Active motion-controller type (`Walk`/`Fly`/`Dive`).
    pub motion_controller: String,
    /// Health ratio in `[0, 1]`.
    pub health_ratio: f64,
    /// Active charge states.
    pub charge_states: BTreeSet<String>,
    /// Block type keys near the actor.
    pub nearby_blocks: BTreeSet<String>,
    /// Beacon messages currently visible to the actor.
    pub beacons: BTreeSet<String>,
    /// Current path name, when known.
    pub path: Option<String>,
    /// Actor feet position.
    pub position: hytale_world::collision::Vec3,
    /// Leash point, when set.
    pub leash_position: Option<hytale_world::collision::Vec3>,
    /// Actor is in water.
    pub in_water: bool,
    /// Actor is airborne.
    pub in_air: bool,
    /// Current in-game hour (`0..24`).
    pub time_hour: f64,
    /// Current weather key, when known.
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
    /// Named stored positions.
    pub positions: BTreeMap<String, hytale_world::collision::Vec3>,
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
}

/// One `Self` filter.
#[derive(Debug, Clone, PartialEq)]
pub enum SelfFilter {
    /// Health/stat ratio range.
    Stat {
        /// Inclusive minimum ratio.
        min: f64,
        /// Inclusive maximum ratio.
        max: f64,
    },
    /// Moving/idle state.
    MovementState(String),
    /// Motion-controller type.
    MotionController(String),
    /// Unknown filter type.
    Unknown(String),
}

impl BuilderContext {
    /// Set the state.
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = state.into();
        self
    }

    /// Set whether a player target is present.
    pub fn with_player_present(mut self, present: bool) -> Self {
        self.player_present = present;
        self
    }

    /// Set whether a generic target is present.
    pub fn with_target_present(mut self, present: bool) -> Self {
        self.target_present = present;
        self
    }

    /// Set one flag.
    pub fn with_flag(mut self, name: impl Into<String>, value: bool) -> Self {
        self.flags.insert(name.into(), value);
        self
    }

    /// Mark one timer as running/stopped.
    pub fn with_timer(mut self, name: impl Into<String>, running: bool) -> Self {
        self.timers.insert(name.into(), running);
        self
    }
}

/// Parsed role instruction graph.
#[derive(Debug, Clone, Default)]
pub struct BuilderGraph {
    /// Top-level nodes.
    pub nodes: Vec<InstructionNode>,
    fired_once: BTreeSet<Vec<usize>>,
}

impl BuilderGraph {
    /// Parse `Instructions` from a resolved role.
    pub fn from_role(role: &ResolvedRole) -> Self {
        Self::from_value(role.raw.get("Instructions"))
    }

    /// Parse an `Instructions` array.
    pub fn from_value(value: Option<&Value>) -> Self {
        let nodes = value
            .and_then(Value::as_array)
            .map(|array| array.iter().map(parse_node).collect())
            .unwrap_or_default();
        Self {
            nodes,
            fired_once: BTreeSet::new(),
        }
    }

    /// Number of top-level nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every node depth-first.
    pub fn walk(&self) -> Vec<&InstructionNode> {
        let mut out = Vec::new();
        for node in &self.nodes {
            walk_node(node, &mut out);
        }
        out
    }

    /// Evaluate the graph and return matched actions in document order.
    pub fn evaluate(&mut self, context: &BuilderContext) -> Vec<BuilderAction> {
        let Self { nodes, fired_once } = self;
        let mut out = Vec::new();
        let mut path = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            path.push(index);
            eval_node(node, context, &mut path, fired_once, &mut out);
            path.pop();
        }
        out
    }
}

fn eval_node(
    node: &InstructionNode,
    context: &BuilderContext,
    path: &mut Vec<usize>,
    fired_once: &mut BTreeSet<Vec<usize>>,
    out: &mut Vec<BuilderAction>,
) {
    let matches = node
        .sensor
        .as_ref()
        .map(|sensor| sensor_matches(sensor, context, path, fired_once))
        .unwrap_or(true);
    if !matches {
        return;
    }
    out.extend(node.actions.iter().cloned());
    for (index, child) in node.instructions.iter().enumerate() {
        path.push(index);
        eval_node(child, context, path, fired_once, out);
        path.pop();
    }
}

fn sensor_matches(
    sensor: &Sensor,
    context: &BuilderContext,
    path: &[usize],
    fired_once: &mut BTreeSet<Vec<usize>>,
) -> bool {
    match sensor {
        Sensor::Any { once } => {
            if *once {
                if fired_once.contains(path) {
                    return false;
                }
                fired_once.insert(path.to_vec());
            }
            true
        }
        Sensor::State(state) => context.state.eq_ignore_ascii_case(state),
        Sensor::Not(inner) => !sensor_matches(inner, context, path, fired_once),
        Sensor::And(sensors) => sensors
            .iter()
            .all(|sensor| sensor_matches(sensor, context, path, fired_once)),
        Sensor::Or(sensors) => sensors
            .iter()
            .any(|sensor| sensor_matches(sensor, context, path, fired_once)),
        Sensor::Player => context.player_present,
        Sensor::Target => context.target_present,
        Sensor::Mob {
            present,
            line_of_sight,
        } => context.mob_present == *present && (!*line_of_sight || context.line_of_sight),
        Sensor::SelfFilters(filters) => filters
            .iter()
            .all(|filter| self_filter_matches(filter, context)),
        Sensor::Stat { min, max } => context.health_ratio >= *min && context.health_ratio <= *max,
        Sensor::ChargeState(states) => states
            .iter()
            .any(|state| context.charge_states.contains(state)),
        Sensor::Block(blocks) => !context.nearby_blocks.is_disjoint(blocks),
        Sensor::Beacon(message) => context.beacons.contains(message),
        Sensor::Path(path) => context.path.as_deref() == Some(path.as_str()),
        Sensor::Damage => context.damaged,
        Sensor::MotionController(kind) => context.motion_controller.eq_ignore_ascii_case(kind),
        Sensor::InWater => context.in_water,
        Sensor::InAir => context.in_air,
        Sensor::Time { start, end } => {
            if start <= end {
                context.time_hour >= *start && context.time_hour <= *end
            } else {
                context.time_hour >= *start || context.time_hour <= *end
            }
        }
        Sensor::Weather(patterns) => context.weather.as_deref().is_some_and(|weather| {
            patterns
                .iter()
                .any(|pattern| glob_matches(pattern, weather))
        }),
        Sensor::IsBusy => context.busy,
        Sensor::Alarm { name, state } => {
            context
                .alarms
                .get(name)
                .map(String::as_str)
                .unwrap_or("Unset")
                == state
        }
        Sensor::Count { min, max } => context.nearby_count >= *min && context.nearby_count <= *max,
        Sensor::HasInteracted => context.has_interacted,
        Sensor::CanInteract => context.can_interact,
        Sensor::IsBackingAway => context.is_backing_away,
        Sensor::InflictedDamage => context.inflicted_damage,
        Sensor::FlockLeader => context.flock_leader,
        Sensor::Leash { range } => context.leash_position.is_some_and(|leash| {
            let dx = context.position.x - leash.x;
            let dy = context.position.y - leash.y;
            let dz = context.position.z - leash.z;
            dx * dx + dy * dy + dz * dz > range * range
        }),
        Sensor::Light {
            min,
            max,
            sky_min,
            sky_max,
        } => {
            context.light >= *min
                && context.light <= *max
                && context.sky_light >= *sky_min
                && context.sky_light <= *sky_max
        }
        Sensor::Switch(value) => *value,
        Sensor::DroppedItem => context.dropped_item_present,
        Sensor::BlockChange => context.block_changed,
        Sensor::CanPlaceBlock => context.can_place_block,
        Sensor::Kill => context.killed,
        Sensor::Nav => context.nav_ok,
        Sensor::Eval => context.killed || context.inflicted_damage,
        Sensor::ReadPosition(slot) => context.positions.contains_key(slot),
        Sensor::BlockType(sensor) => sensor_matches(sensor, context, path, fired_once),
        Sensor::EntityEvent => context.entity_event,
        Sensor::InteractionContext => context.interaction_context,
        Sensor::FlockCombatDamage => context.flock_combat_damage,
        Sensor::SearchRay => context.search_ray,
        Sensor::AdjustPosition(sensor) => sensor_matches(sensor, context, path, fired_once),
        Sensor::ChargeEntityCollisions => context.charge_entity_collisions,
        Sensor::ChargeBlockCollisions => context.charge_block_collisions,
        Sensor::Random { chance } => context.random_unit < *chance,
        Sensor::Animation(animation) => context
            .animation
            .as_deref()
            .is_some_and(|current| current.eq_ignore_ascii_case(animation)),
        Sensor::GameFlag { name, value } => {
            context.flags.get(name).copied().unwrap_or(false) == *value
        }
        Sensor::Timer { name, stopped } => {
            let running = context.timers.get(name).copied().unwrap_or(false);
            if *stopped {
                !running
            } else {
                running
            }
        }
        Sensor::Flag { name, value } => context.flags.get(name).copied().unwrap_or(false) == *value,
        Sensor::Unknown(_) => false,
    }
}

impl BuilderGraph {
    /// Reset `Once` sensors (for example on a state reset).
    pub fn reset_once(&mut self) {
        self.fired_once.clear();
    }
}

fn walk_node<'a>(node: &'a InstructionNode, out: &mut Vec<&'a InstructionNode>) {
    out.push(node);
    for child in &node.instructions {
        walk_node(child, out);
    }
}

fn parse_self_filter(value: &Value) -> SelfFilter {
    match value
        .get("Type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "Stat" => {
            let range = value
                .get("ValueRange")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
                .unwrap_or_default();
            SelfFilter::Stat {
                min: range.first().copied().unwrap_or(0.0),
                max: range.get(1).copied().unwrap_or(1.0),
            }
        }
        "MovementState" => SelfFilter::MovementState(
            value
                .get("State")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        "MotionController" => SelfFilter::MotionController(
            value
                .get("MotionController")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        other => SelfFilter::Unknown(other.to_string()),
    }
}

fn parse_blocks(value: Option<&Value>) -> BTreeSet<String> {
    match value {
        Some(Value::String(name)) => [name.clone()].into_iter().collect(),
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(first) = parts.next() else {
        return pattern == value;
    };
    if !value.starts_with(first) {
        return false;
    }
    let mut rest = &value[first.len()..];
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let Some(index) = rest.find(part) else {
            return false;
        };
        rest = &rest[index + part.len()..];
    }
    pattern.ends_with('*') || rest.is_empty()
}

fn self_filter_matches(filter: &SelfFilter, context: &BuilderContext) -> bool {
    match filter {
        SelfFilter::Stat { min, max } => {
            context.health_ratio >= *min && context.health_ratio <= *max
        }
        SelfFilter::MovementState(state) => {
            if state == "Idle" {
                !context.moving
            } else if state == "Moving" {
                context.moving
            } else {
                true
            }
        }
        SelfFilter::MotionController(kind) => context.motion_controller.eq_ignore_ascii_case(kind),
        SelfFilter::Unknown(_) => true,
    }
}

fn parse_node(value: &Value) -> InstructionNode {
    let sensor = value.get("Sensor").map(parse_sensor);
    let actions = value
        .get("Actions")
        .and_then(Value::as_array)
        .map(|actions| {
            actions
                .iter()
                .filter_map(|action| {
                    action
                        .get("Type")
                        .and_then(Value::as_str)
                        .map(|kind| BuilderAction {
                            kind: kind.to_string(),
                            value: action.clone(),
                        })
                })
                .collect()
        })
        .unwrap_or_default();
    let instructions = value
        .get("Instructions")
        .and_then(Value::as_array)
        .map(|nodes| nodes.iter().map(parse_node).collect())
        .unwrap_or_default();
    InstructionNode {
        sensor,
        instructions,
        actions,
        reference: value
            .get("Reference")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        modify: value.get("Modify").cloned(),
        comment: value
            .get("$Comment")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
    }
}

fn parse_sensor(value: &Value) -> Sensor {
    let kind = value
        .get("Type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "Any" => Sensor::Any {
            once: value.get("Once").and_then(Value::as_bool).unwrap_or(false),
        },
        "State" => Sensor::State(
            value
                .get("State")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        "Not" => Sensor::Not(Box::new(
            value
                .get("Sensor")
                .map(parse_sensor)
                .unwrap_or(Sensor::Unknown("Not".to_string())),
        )),
        "And" | "Or" => {
            let sensors = value
                .get("Sensors")
                .and_then(Value::as_array)
                .map(|sensors| sensors.iter().map(parse_sensor).collect())
                .unwrap_or_default();
            if kind == "And" {
                Sensor::And(sensors)
            } else {
                Sensor::Or(sensors)
            }
        }
        "Player" => Sensor::Player,
        "Target" => Sensor::Target,
        "Mob" => {
            let filters = value
                .get("Filters")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let line_of_sight = filters
                .iter()
                .any(|filter| filter.get("Type").and_then(Value::as_str) == Some("LineOfSight"))
                || value
                    .get("LineOfSight")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            Sensor::Mob {
                present: true,
                line_of_sight,
            }
        }
        "Timer" => Sensor::Timer {
            name: value
                .get("Name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            stopped: value.get("State").and_then(Value::as_str) == Some("Stopped"),
        },
        "Flag" => Sensor::Flag {
            name: value
                .get("Name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            value: value
                .get("Value")
                .or_else(|| value.get("Set"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
        },
        "Self" => Sensor::SelfFilters(
            value
                .get("Filters")
                .and_then(Value::as_array)
                .map(|filters| filters.iter().map(parse_self_filter).collect())
                .unwrap_or_default(),
        ),
        "Damage" => Sensor::Damage,
        "MotionController" => Sensor::MotionController(
            value
                .get("MotionController")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        "InWater" => Sensor::InWater,
        "InAir" => Sensor::InAir,
        "Time" => {
            let period = value
                .get("Period")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
                .unwrap_or_default();
            Sensor::Time {
                start: period.first().copied().unwrap_or(0.0),
                end: period.get(1).copied().unwrap_or(24.0),
            }
        }
        "Weather" => Sensor::Weather(
            value
                .get("Weathers")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "IsBusy" => Sensor::IsBusy,
        "Alarm" => Sensor::Alarm {
            name: value
                .get("Name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            state: value
                .get("State")
                .and_then(Value::as_str)
                .unwrap_or("Unset")
                .to_string(),
        },
        "Count" => {
            let range = value
                .get("Count")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_u64).collect::<Vec<_>>())
                .unwrap_or_default();
            Sensor::Count {
                min: range.first().copied().unwrap_or(0) as usize,
                max: range.get(1).copied().unwrap_or(usize::MAX as u64) as usize,
            }
        }
        "HasInteracted" => Sensor::HasInteracted,
        "CanInteract" => Sensor::CanInteract,
        "IsBackingAway" => Sensor::IsBackingAway,
        "InflictedDamage" => Sensor::InflictedDamage,
        "FlockLeader" => Sensor::FlockLeader,
        "Leash" => Sensor::Leash {
            range: value.get("Range").and_then(Value::as_f64).unwrap_or(0.0),
        },
        "Light" => {
            let range = |key: &str, default: [i64; 2]| {
                value
                    .get(key)
                    .and_then(Value::as_array)
                    .map(|values| values.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                    .filter(|values| values.len() >= 2)
                    .map(|values| [values[0], values[1]])
                    .unwrap_or(default)
            };
            let [min, max] = range("LightRange", [0, 15]);
            let [sky_min, sky_max] = range("SkyLightRange", [0, 15]);
            Sensor::Light {
                min,
                max,
                sky_min,
                sky_max,
            }
        }
        "Switch" => Sensor::Switch(value.get("Switch").and_then(Value::as_bool).unwrap_or(true)),
        "DroppedItem" => Sensor::DroppedItem,
        "BlockChange" => Sensor::BlockChange,
        "CanPlaceBlock" => Sensor::CanPlaceBlock,
        "Kill" => Sensor::Kill,
        "Nav" => Sensor::Nav,
        "Eval" | "CombatActionEvaluator" => Sensor::Eval,
        "ReadPosition" => Sensor::ReadPosition(
            value
                .get("Slot")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        "BlockType" => Sensor::BlockType(Box::new(
            value
                .get("Sensor")
                .map(parse_sensor)
                .unwrap_or(Sensor::Unknown("BlockType".to_string())),
        )),
        "EntityEvent" => Sensor::EntityEvent,
        "InteractionContext" => Sensor::InteractionContext,
        "FlockCombatDamage" => Sensor::FlockCombatDamage,
        "SearchRay" => Sensor::SearchRay,
        "AdjustPosition" => {
            Sensor::AdjustPosition(Box::new(if let Some(sensor) = value.get("Sensor") {
                parse_sensor(sensor)
            } else {
                Sensor::Any { once: false }
            }))
        }
        "None" => Sensor::Any { once: false },
        "ChargeEntityCollisions" => Sensor::ChargeEntityCollisions,
        "ChargeBlockCollisions" => Sensor::ChargeBlockCollisions,
        "Random" => Sensor::Random {
            chance: value
                .get("Chance")
                .or_else(|| value.get("Probability"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0),
        },
        "Animation" => Sensor::Animation(
            value
                .get("Animation")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        "GameFlag" => Sensor::GameFlag {
            name: value
                .get("Name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            value: value
                .get("Value")
                .or_else(|| value.get("Set"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
        },
        "Stat" => {
            let range = value
                .get("ValueRange")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
                .unwrap_or_default();
            let min = range.first().copied().unwrap_or(0.0);
            let max = range.get(1).copied().unwrap_or(1.0);
            Sensor::Stat { min, max }
        }
        "ChargeState" => Sensor::ChargeState(
            value
                .get("States")
                .and_then(Value::as_array)
                .map(|states| {
                    states
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "Block" => Sensor::Block(parse_blocks(value.get("Blocks"))),
        "Beacon" => Sensor::Beacon(
            value
                .get("Message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        "Path" => Sensor::Path(
            value
                .get("Path")
                .and_then(|path| path.as_str().or_else(|| path.get("Compute")?.as_str()))
                .unwrap_or_default()
                .to_string(),
        ),
        other => Sensor::Unknown(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_sensor<'a>(value: &'a Value, kind: &str) -> Option<&'a Value> {
        match value {
            Value::Object(map) => {
                if map.get("Type").and_then(Value::as_str) == Some(kind) {
                    return Some(value);
                }
                map.values().find_map(|value| find_sensor(value, kind))
            }
            Value::Array(values) => values.iter().find_map(|value| find_sensor(value, kind)),
            _ => None,
        }
    }

    #[test]
    fn sensors_combine_with_and_or_not() {
        let context = BuilderContext::default()
            .with_state("Idle")
            .with_timer("Chase_Ready", false);
        let sensor = Sensor::And(vec![
            Sensor::State("Idle".to_string()),
            Sensor::Not(Box::new(Sensor::State("Combat".to_string()))),
            Sensor::Timer {
                name: "Chase_Ready".to_string(),
                stopped: true,
            },
        ]);
        assert!(sensor_matches(
            &sensor,
            &context,
            &[0],
            &mut BTreeSet::new()
        ));
        let sensor = Sensor::Or(vec![
            Sensor::State("Combat".to_string()),
            Sensor::State("Idle".to_string()),
        ]);
        assert!(sensor_matches(
            &sensor,
            &context,
            &[0],
            &mut BTreeSet::new()
        ));
    }

    #[test]
    fn motion_damage_and_self_sensors_use_context() {
        let context = BuilderContext {
            motion_controller: "Fly".to_string(),
            damaged: true,
            ..BuilderContext::default()
        };
        let mut fired = BTreeSet::new();
        assert!(sensor_matches(
            &Sensor::MotionController("Fly".to_string()),
            &context,
            &[0],
            &mut fired
        ));
        assert!(sensor_matches(&Sensor::Damage, &context, &[0], &mut fired));
        assert!(sensor_matches(
            &Sensor::SelfFilters(Vec::new()),
            &context,
            &[0],
            &mut fired
        ));
        assert!(!sensor_matches(
            &Sensor::MotionController("Walk".to_string()),
            &context,
            &[0],
            &mut fired
        ));
    }

    #[test]
    fn real_feran_role_exposes_damage_sensor_and_beacon_action() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = crate::role::RoleCatalog::load_root(&root).expect("roles");
        let role = catalog
            .resolve("Template_Feran_Cub")
            .expect("resolve")
            .expect("role");
        let mut graph = BuilderGraph::from_role(&role);
        assert!(graph
            .walk()
            .iter()
            .any(|node| matches!(node.sensor, Some(Sensor::Damage))));
        let actions = graph.evaluate(&BuilderContext {
            damaged: true,
            ..BuilderContext::default()
        });
        assert!(actions.iter().any(|action| action.kind == "Beacon"));
        let mut blackboard = crate::blackboard::NpcBlackboard::default();
        blackboard.apply_actions(&actions);
        assert!(blackboard.beacons.contains("Feran_All_Attack"));
    }

    #[test]
    fn real_self_stat_filter_uses_health_ratio() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../Assets/extracted/Server/NPC/Roles/Intelligent/Faction/Feran/Component_Feran_Sensor_Low_HP.json");
        if !path.is_file() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let node = parse_node(&serde_json::json!({ "Sensor": value["Content"] }));
        let Sensor::SelfFilters(filters) = node.sensor.expect("sensor") else {
            panic!("expected Self filters");
        };
        assert!(filters
            .iter()
            .any(|filter| matches!(filter, SelfFilter::Stat { .. })));
        let mut fired = BTreeSet::new();
        let low = BuilderContext {
            health_ratio: 0.3,
            ..BuilderContext::default()
        };
        let high = BuilderContext {
            health_ratio: 0.8,
            ..BuilderContext::default()
        };
        assert!(sensor_matches(
            &Sensor::SelfFilters(filters.clone()),
            &low,
            &[0],
            &mut fired
        ));
        assert!(!sensor_matches(
            &Sensor::SelfFilters(filters),
            &high,
            &[0],
            &mut fired
        ));
    }

    #[test]
    fn real_charge_block_and_path_sensors_parse() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = crate::role::RoleCatalog::load_root(&root).expect("roles");
        for (key, predicate) in [
            (
                "Template_Dungeon_Feastmaster_Boss",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::ChargeState(_)))
                    as Box<dyn Fn(&Sensor) -> bool>,
            ),
            (
                "Template_Feran_Civilian",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Block(_))),
            ),
            (
                "Template_Dungeon_Goblin_Burner",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Path(_))),
            ),
        ] {
            let role = catalog.resolve(key).expect("resolve").expect("role");
            let graph = BuilderGraph::from_role(&role);
            assert!(
                graph
                    .walk()
                    .iter()
                    .filter_map(|node| node.sensor.as_ref())
                    .any(predicate),
                "{key} exposes the expected sensor"
            );
        }
    }

    #[test]
    fn real_water_time_weather_alarm_and_count_sensors_parse_and_evaluate() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        // Real role documents are parsed directly so a role whose expression
        // parameters need runtime RNG (for example `randomInRange`) does not
        // block the sensor-shape assertions.
        let role_files = [
            (
                "Server/NPC/Roles/_Core/Tests/Test_In_Water.json",
                "InWater",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::InWater))
                    as Box<dyn Fn(&Sensor) -> bool>,
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Feran/Component_Sensor_Feran_Day.json",
                "Time",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Time { .. })),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Trork/Components/Component_Trork_Sensor_Can_Sleep.json",
                "Weather",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Weather(_))),
            ),
            (
                "Server/NPC/Roles/_Core/Templates/Template_Animal_Neutral.json",
                "Alarm",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Alarm { .. })),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Goblin/Components/Component_Instruction_Dungeon_Goblin_Scale_Health.json",
                "Count",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Count { .. })),
            ),
        ];
        for (path, kind, predicate) in role_files {
            let value: Value = serde_json::from_slice(
                &std::fs::read(root.join(path)).expect("real role document"),
            )
            .expect("role json");
            let sensor = find_sensor(&value, kind).unwrap_or_else(|| panic!("{path} has `{kind}`"));
            let sensor = parse_sensor(sensor);
            assert!(predicate(&sensor), "{path} parses `{kind}`");
        }

        let context = BuilderContext {
            in_water: true,
            time_hour: 18.0,
            weather: Some("Rain_Light".to_string()),
            busy: true,
            alarms: [("Produce_Ready".to_string(), "Set".to_string())].into(),
            nearby_count: 2,
            ..BuilderContext::default()
        };
        let mut fired = BTreeSet::new();
        assert!(sensor_matches(&Sensor::InWater, &context, &[0], &mut fired));
        assert!(sensor_matches(
            &Sensor::Time {
                start: 16.99,
                end: 23.99
            },
            &context,
            &[0],
            &mut fired
        ));
        assert!(sensor_matches(
            &Sensor::Weather(["*Rain*".to_string()].into_iter().collect()),
            &context,
            &[0],
            &mut fired
        ));
        assert!(sensor_matches(&Sensor::IsBusy, &context, &[0], &mut fired));
        assert!(sensor_matches(
            &Sensor::Alarm {
                name: "Produce_Ready".to_string(),
                state: "Set".to_string()
            },
            &context,
            &[0],
            &mut fired
        ));
        assert!(sensor_matches(
            &Sensor::Count { min: 2, max: 2 },
            &context,
            &[0],
            &mut fired
        ));
        assert!(glob_matches("*Rain*", "Rain_Light"));
        assert!(!glob_matches("*Rain*", "Clear"));
    }

    #[test]
    fn real_interaction_animation_flock_and_game_flag_sensors_parse() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        for (path, kind, predicate) in [
            (
                "Server/NPC/Roles/Intelligent/Faction/Goblin/Goblin_Launcher.json",
                "HasInteracted",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::HasInteracted))
                    as Box<dyn Fn(&Sensor) -> bool>,
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Goblin/Goblin_Launcher.json",
                "CanInteract",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::CanInteract)),
            ),
            (
                "Server/NPC/Roles/_Core/Templates/Template_Boss_Proto.json",
                "IsBackingAway",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::IsBackingAway)),
            ),
            (
                "Server/NPC/Roles/_Core/Templates/Template_Summoned_Ally.json",
                "InflictedDamage",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::InflictedDamage)),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Feran/Templates/Template_Feran_Cub.json",
                "FlockLeader",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::FlockLeader)),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Trork/Components/Component_Trork_Instruction_Greeting.json",
                "Animation",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Animation(_))),
            ),
            (
                "Server/NPC/Roles/Intelligent/Temple/Templates/Template_Goblin_Scientist.json",
                "GameFlag",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::GameFlag { .. })),
            ),
            (
                "Server/NPC/Roles/Boss/Dungeon_Feastmaster/Templates/Template_Dungeon_Feastmaster_Boss.json",
                "Leash",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Leash { .. })),
            ),
            (
                "Server/NPC/Roles/_Core/Components/Sensors/Component_Sensor_Is_Indoors.json",
                "Light",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Light { .. })),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Trork/Templates/Template_Trork_Companion.json",
                "Switch",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Switch(_))),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Goblin/Components/Component_Instruction_Goblin_Eat.json",
                "DroppedItem",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::DroppedItem)),
            ),
            (
                "Server/NPC/Roles/Creature/Vermin/Templates/Template_Spider.json",
                "BlockChange",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::BlockChange)),
            ),
            (
                "Server/NPC/Roles/_Core/Tests/Test_Place_Block.json",
                "CanPlaceBlock",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::CanPlaceBlock)),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Goblin/Goblin_Spectator.json",
                "Kill",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Kill)),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Trork/Components/Component_Trork_Instruction_Return_Home.json",
                "Nav",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Nav)),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Trork/Components/Component_Trork_Instruction_Converge.json",
                "Eval",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Eval)),
            ),
            (
                "Server/NPC/Roles/Boss/Dungeon_Feastmaster/Templates/Template_Dungeon_Feastmaster_Boss.json",
                "CombatActionEvaluator",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Eval)),
            ),
            (
                "Server/NPC/Roles/Creature/Vermin/Templates/Template_Spider.json",
                "ReadPosition",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::ReadPosition(_))),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Goblin/Templates/Template_Goblin_Miner.json",
                "BlockType",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::BlockType(_))),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Kweebec/Templates/Template_Kweebec_Razorleaf.json",
                "EntityEvent",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::EntityEvent)),
            ),
            (
                "Server/NPC/Roles/_Core/Templates/Template_Livestock.json",
                "InteractionContext",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::InteractionContext)),
            ),
            (
                "Server/NPC/Roles/_Core/Templates/Template_Summoned_Ally.json",
                "FlockCombatDamage",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::FlockCombatDamage)),
            ),
            (
                "Server/NPC/Roles/Intelligent/Faction/Goblin/Templates/Template_Goblin_Miner.json",
                "SearchRay",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::SearchRay)),
            ),
            (
                "Server/NPC/Roles/Boss/Cinematic_Dragon_Void.json",
                "AdjustPosition",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::AdjustPosition(_))),
            ),
            (
                "Server/NPC/Roles/Boss/Dungeon_Feastmaster/Templates/Template_Dungeon_Feastmaster_Boss.json",
                "ChargeEntityCollisions",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::ChargeEntityCollisions)),
            ),
            (
                "Server/NPC/Roles/Boss/Dungeon_Feastmaster/Templates/Template_Dungeon_Feastmaster_Boss.json",
                "ChargeBlockCollisions",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::ChargeBlockCollisions)),
            ),
            (
                "Server/NPC/Roles/_Core/Tests/Test_Random_Sensor.json",
                "Random",
                Box::new(|sensor: &Sensor| matches!(sensor, Sensor::Random { .. })),
            ),
        ] {
            let value: Value = serde_json::from_slice(
                &std::fs::read(root.join(path)).expect("real role document"),
            )
            .expect("role json");
            let sensor = find_sensor(&value, kind).unwrap_or_else(|| panic!("{path} has `{kind}`"));
            assert!(predicate(&parse_sensor(sensor)), "{path} parses `{kind}`");
        }

        let context = BuilderContext {
            has_interacted: true,
            can_interact: true,
            is_backing_away: true,
            inflicted_damage: true,
            flock_leader: true,
            animation: Some("Greeting".to_string()),
            flags: [("Quest_Done".to_string(), true)].into(),
            position: hytale_world::collision::Vec3::new(4.0, 0.0, 4.0),
            leash_position: Some(hytale_world::collision::Vec3::new(0.0, 0.0, 0.0)),
            light: 3,
            sky_light: 10,
            dropped_item_present: true,
            block_changed: true,
            can_place_block: true,
            killed: true,
            nav_ok: true,
            entity_event: true,
            interaction_context: true,
            flock_combat_damage: true,
            search_ray: true,
            charge_entity_collisions: true,
            charge_block_collisions: true,
            random_unit: 0.0,
            positions: [
                (
                    "SteppedBlock".to_string(),
                    hytale_world::collision::Vec3::new(1.0, 1.0, 1.0),
                ),
                (
                    "MineBlock".to_string(),
                    hytale_world::collision::Vec3::new(2.0, 1.0, 2.0),
                ),
            ]
            .into(),
            ..BuilderContext::default()
        };
        let mut fired = BTreeSet::new();
        for sensor in [
            Sensor::HasInteracted,
            Sensor::CanInteract,
            Sensor::IsBackingAway,
            Sensor::InflictedDamage,
            Sensor::FlockLeader,
            Sensor::Animation("Greeting".to_string()),
            Sensor::GameFlag {
                name: "Quest_Done".to_string(),
                value: true,
            },
            Sensor::Leash { range: 2.0 },
            Sensor::Light {
                min: 0,
                max: 5,
                sky_min: 5,
                sky_max: 15,
            },
            Sensor::Switch(true),
            Sensor::DroppedItem,
            Sensor::BlockChange,
            Sensor::CanPlaceBlock,
            Sensor::Kill,
            Sensor::Nav,
            Sensor::Eval,
            Sensor::ReadPosition("SteppedBlock".to_string()),
            Sensor::BlockType(Box::new(Sensor::ReadPosition("MineBlock".to_string()))),
            Sensor::EntityEvent,
            Sensor::InteractionContext,
            Sensor::FlockCombatDamage,
            Sensor::SearchRay,
            Sensor::ChargeEntityCollisions,
            Sensor::ChargeBlockCollisions,
            Sensor::Random { chance: 0.5 },
        ] {
            assert!(sensor_matches(&sensor, &context, &[0], &mut fired));
        }
        assert!(!sensor_matches(
            &Sensor::Leash { range: 8.0 },
            &context,
            &[0],
            &mut fired
        ));
        assert!(matches!(
            parse_sensor(&serde_json::json!({"Type": "None"})),
            Sensor::Any { .. }
        ));
        assert!(!sensor_matches(
            &Sensor::Random { chance: 0.0 },
            &context,
            &[0],
            &mut fired
        ));
    }

    #[test]
    fn referenced_sensor_components_expand_into_concrete_sensors() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = crate::role::RoleCatalog::load_root(&root).expect("roles");
        let role = catalog
            .resolve("Template_Boss_Goblin_Gobliterator")
            .expect("resolve")
            .expect("role");
        let graph = BuilderGraph::from_role(&role);
        let sensors = graph
            .walk()
            .iter()
            .filter_map(|node| node.sensor.as_ref())
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            sensors.iter().any(|sensor| matches!(
                sensor,
                Sensor::Mob { .. } | Sensor::Player | Sensor::Beacon(_) | Sensor::Target
            )),
            "the referenced Standard_Detection sensor expands"
        );
        assert!(
            !sensors
                .iter()
                .any(|sensor| matches!(sensor, Sensor::Unknown(kind) if kind.is_empty())),
            "no empty-type sensor remains after reference resolution"
        );
    }

    #[test]
    fn once_sensor_fires_a_single_time() {
        let node = InstructionNode {
            sensor: Some(Sensor::Any { once: true }),
            instructions: Vec::new(),
            actions: vec![BuilderAction {
                kind: "Log".to_string(),
                value: serde_json::json!({"Type": "Log", "Message": "hi"}),
            }],
            reference: None,
            modify: None,
            comment: None,
        };
        let mut graph = BuilderGraph {
            nodes: vec![node],
            fired_once: BTreeSet::new(),
        };
        let context = BuilderContext::default();
        assert_eq!(graph.evaluate(&context).len(), 1);
        assert!(graph.evaluate(&context).is_empty());
        graph.reset_once();
        assert_eq!(graph.evaluate(&context).len(), 1);
    }

    #[test]
    fn parses_a_real_role_instruction_tree() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/NPC/Roles").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = crate::role::RoleCatalog::load_root(&root).expect("roles");
        let role = catalog
            .resolve("Template_Goblin_Scrapper")
            .expect("resolve")
            .expect("role exists");
        let mut graph = BuilderGraph::from_role(&role);
        assert!(!graph.is_empty());
        let all = graph.walk();
        assert!(
            all.iter()
                .any(|node| matches!(&node.sensor, Some(Sensor::State(state)) if state == "Idle")),
            "the Idle state branch is parsed"
        );
        assert!(
            all.iter().any(|node| node
                .actions
                .iter()
                .any(|action| action.kind == "TimerStart")),
            "timer actions are parsed"
        );
        assert!(
            !all.iter()
                .filter_map(|node| node.sensor.as_ref())
                .any(|sensor| matches!(sensor, Sensor::Unknown(kind) if kind.is_empty())),
            "sensor references resolve into concrete sensors"
        );
        assert!(
            all.iter()
                .filter_map(|node| node.sensor.as_ref())
                .any(|sensor| matches!(
                    sensor,
                    Sensor::Mob { .. }
                        | Sensor::Player
                        | Sensor::Beacon(_)
                        | Sensor::Damage
                        | Sensor::Target
                )),
            "referenced detection sensors are expanded"
        );
        let actions = graph.evaluate(&BuilderContext::default().with_state("Idle"));
        assert!(!actions.is_empty());
    }
}
