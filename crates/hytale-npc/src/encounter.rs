//! `Server/EncounterManager` catalog and the deterministic evaluator.
//!
//! Java encounters are instruction trees shared with the NPC builder. This
//! module keeps that shape: the catalog resolves builder variants, macros and
//! forwarded parameter slots, while [`EncounterRuntime`] evaluates the sensor
//! and action subset used by the base-game encounters. Instance/portal actions
//! that need world services are emitted as deferred events instead of silently
//! doing nothing.
//!
//! The evaluator intentionally models the server-authoritative pieces (state,
//! timers, flags, target slots, boss bar/audio/weather intentions, portal timer)
//! and leaves particle/sound packet playback to the presentation layer.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use hytale_assets::{AssetError, AssetPack};
use hytale_world::collision::Vec3;
use serde_json::{Map, Value};
use thiserror::Error;

use crate::expr::{evaluate, EvalContext, Value as ExprValue};

/// One encounter asset document (`Class` = `EncounterManager` or `Instruction`).
#[derive(Debug, Clone, PartialEq)]
pub struct EncounterDocument {
    /// Asset key (file stem).
    pub key: String,
    /// `Class` value.
    pub class: String,
    /// `Type` value (`Generic`, `Variant`, `Abstract`, `Component`, ...).
    pub kind: String,
    /// `Reference` template, when present.
    pub reference: Option<String>,
    /// `Interface` for reusable component macros.
    pub interface: Option<String>,
    /// Declared parameters after inheritance/`Modify` resolution.
    pub parameters: BTreeMap<String, Value>,
    /// Raw source document.
    pub raw: Value,
    /// Resolved document (parameters/computes/macros expanded).
    pub resolved: Value,
    /// Resolved instruction nodes.
    pub instructions: Vec<Value>,
    /// Resolved state transitions.
    pub state_transitions: Vec<StateTransition>,
    /// Initial state (`StartState`, Java default `start`).
    pub start_state: String,
    /// `CleanupOnRemove`.
    pub cleanup_on_remove: bool,
}

impl EncounterDocument {
    /// Whether this document can be spawned as an encounter.
    pub fn is_spawnable(&self) -> bool {
        self.class == "EncounterManager" && self.kind != "Abstract"
    }

    /// Resolve a fresh runtime for this document.
    pub fn runtime(&self) -> EncounterRuntime {
        EncounterRuntime::new(self.clone())
    }
}

/// One `StateTransitions` entry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateTransition {
    /// Source states; an empty list matches any state.
    pub from: Vec<String>,
    /// Destination states.
    pub to: Vec<String>,
    /// Actions run during the transition.
    pub actions: Vec<Value>,
    /// Raw entry (for diagnostics).
    pub raw: Value,
}

impl StateTransition {
    /// Whether this transition matches an `old -> new` state change.
    pub fn matches(&self, from: &str, to: &str) -> bool {
        (self.from.is_empty() || self.from.iter().any(|state| state == from))
            && (self.to.is_empty() || self.to.iter().any(|state| state == to))
    }
}

/// All `Server/EncounterManager/**` documents keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct EncounterCatalog {
    documents: BTreeMap<String, EncounterDocument>,
}

impl EncounterCatalog {
    /// Load every encounter/instruction document from a base-game pack.
    pub fn load(pack: &AssetPack) -> Result<Self, EncounterError> {
        let paths: Vec<String> = pack
            .list()?
            .into_iter()
            .filter(|path| path.starts_with("Server/EncounterManager/") && path.ends_with(".json"))
            .collect();
        let mut raw = BTreeMap::new();
        for (path, bytes) in paths.iter().zip(pack.read_many(&paths)?) {
            let key = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| EncounterError::InvalidPath(path.clone()))?
                .to_string();
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|source| EncounterError::Json {
                    path: path.clone(),
                    source,
                })?;
            raw.insert(key, value);
        }
        Self::from_raw_documents(raw)
    }

    /// Load from an extracted asset root (`<root>/Server/EncounterManager`).
    pub fn load_root(root: &Path) -> Result<Self, EncounterError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// Build a catalog from already-loaded documents.
    pub fn from_raw_documents(raw: BTreeMap<String, Value>) -> Result<Self, EncounterError> {
        let mut documents = BTreeMap::new();
        for (key, value) in &raw {
            let resolved = resolve_document(key, &raw, &BTreeMap::new(), &mut BTreeSet::new())?;
            let class = resolved
                .get("Class")
                .and_then(Value::as_str)
                .unwrap_or("EncounterManager")
                .to_string();
            let kind = resolved
                .get("Type")
                .and_then(Value::as_str)
                .unwrap_or("Generic")
                .to_string();
            let reference = resolved
                .get("Reference")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let interface = resolved
                .get("Interface")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let parameters = resolved
                .get("Parameters")
                .and_then(Value::as_object)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|(name, entry)| {
                            (
                                name.clone(),
                                entry.get("Value").cloned().unwrap_or(Value::Null),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            let instructions = resolved
                .get("Instructions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let state_transitions = parse_transitions(&resolved);
            let start_state = resolved
                .get("StartState")
                .and_then(Value::as_str)
                .unwrap_or("start")
                .to_string();
            let cleanup_on_remove = resolved
                .get("CleanupOnRemove")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            documents.insert(
                key.clone(),
                EncounterDocument {
                    key: key.clone(),
                    class,
                    kind,
                    reference,
                    interface,
                    parameters,
                    raw: value.clone(),
                    resolved,
                    instructions,
                    state_transitions,
                    start_state,
                    cleanup_on_remove,
                },
            );
        }
        Ok(Self { documents })
    }

    /// One document by key.
    pub fn get(&self, key: &str) -> Option<&EncounterDocument> {
        self.documents.get(key)
    }

    /// Resolve one document (catalog documents are already resolved).
    pub fn resolve(&self, key: &str) -> Option<&EncounterDocument> {
        self.documents.get(key)
    }

    /// Number of documents.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Documents in key order.
    pub fn iter(&self) -> impl Iterator<Item = &EncounterDocument> {
        self.documents.values()
    }

    /// Spawnable encounter documents only.
    pub fn spawnable(&self) -> impl Iterator<Item = &EncounterDocument> {
        self.documents
            .values()
            .filter(|document| document.is_spawnable())
    }
}

/// Encounter catalog/resolution errors.
#[derive(Debug, Error)]
pub enum EncounterError {
    /// Pack access failed.
    #[error(transparent)]
    Pack(#[from] AssetError),
    /// A document is invalid JSON.
    #[error("{path}: invalid JSON: {source}")]
    Json {
        /// Pack-relative path.
        path: String,
        /// Serde error.
        #[source]
        source: serde_json::Error,
    },
    /// A path had no file stem.
    #[error("invalid encounter asset path `{0}`")]
    InvalidPath(String),
    /// A `Reference` chain is cyclic.
    #[error("encounter reference cycle at `{0}`")]
    ReferenceCycle(String),
}

/// One target slot visible to encounter sensors/actions.
#[derive(Debug, Clone, PartialEq)]
pub struct EncounterTarget {
    /// Stable entity id.
    pub id: u64,
    /// Feet position.
    pub position: Vec3,
    /// Current health.
    pub health: f32,
    /// Maximum health.
    pub max_health: f32,
    /// Whether this target is a player.
    pub is_player: bool,
    /// Attitude group(s) such as `Friendly`.
    pub attitude: Vec<String>,
    /// NPC/role group names.
    pub groups: Vec<String>,
    /// Current role asset key.
    pub role: Option<String>,
    /// Whether the target is invulnerable.
    pub invulnerable: bool,
}

impl Default for EncounterTarget {
    fn default() -> Self {
        Self {
            id: 0,
            position: Vec3::default(),
            health: 1.0,
            max_health: 1.0,
            is_player: false,
            attitude: Vec::new(),
            groups: Vec::new(),
            role: None,
            invulnerable: false,
        }
    }
}

impl EncounterTarget {
    /// Health ratio (`health / max_health`), clamped to `[0, 1]`.
    pub fn health_fraction(&self) -> f32 {
        if self.max_health <= 0.0 {
            0.0
        } else {
            (self.health / self.max_health).clamp(0.0, 1.0)
        }
    }
}

/// One beacon message visible to the encounter.
#[derive(Debug, Clone, PartialEq)]
pub struct BeaconMessage {
    /// Message key.
    pub message: String,
    /// Source position, when known.
    pub position: Option<Vec3>,
    /// Range limit, when provided.
    pub range: Option<f64>,
    /// Target slot the sender wants bound.
    pub target_slot: Option<String>,
    /// Target groups.
    pub target_groups: Vec<String>,
}

impl BeaconMessage {
    /// Construct a message with only its key.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            position: None,
            range: None,
            target_slot: None,
            target_groups: Vec::new(),
        }
    }
}

/// Inputs for one encounter tick.
#[derive(Debug, Clone, Default)]
pub struct EncounterContext {
    /// Encounter entity/world position used by range sensors. When omitted,
    /// the first target (or player) position is used.
    pub position: Option<Vec3>,
    /// Player feet positions.
    pub players: Vec<Vec3>,
    /// Target slots (`Boss`, `EncounterTarget`, ...).
    pub targets: BTreeMap<String, EncounterTarget>,
    /// Pending beacon messages.
    pub beacons: Vec<BeaconMessage>,
    /// Per-target line-of-sight results supplied by the world layer. Missing
    /// entries are treated as visible.
    pub line_of_sight: BTreeMap<u64, bool>,
}

impl EncounterContext {
    /// Add/replace a target slot.
    pub fn with_target(mut self, slot: impl Into<String>, target: EncounterTarget) -> Self {
        self.targets.insert(slot.into(), target);
        self
    }

    /// Add a player position.
    pub fn with_player(mut self, position: Vec3) -> Self {
        self.players.push(position);
        self
    }

    /// Set the encounter entity position used by range sensors.
    pub fn at(mut self, position: Vec3) -> Self {
        self.position = Some(position);
        self
    }
}

/// A boss-bar action requested by an encounter.
#[derive(Debug, Clone, PartialEq)]
pub struct BossBarState {
    /// Localisation key/name.
    pub name: String,
    /// Hard-mode variant.
    pub hard_mode: bool,
    /// Target slot the bar follows.
    pub target_slot: Option<String>,
}

/// A portal/instance timer change requested by an encounter.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PortalState {
    /// Remaining portal seconds, when the encounter runs in a portal world.
    pub remaining_seconds: Option<f64>,
}

/// An action that needs a world service (spawner, particle, sound, instance).
#[derive(Debug, Clone, PartialEq)]
pub struct DeferredEncounterAction {
    /// Java action type.
    pub kind: String,
    /// Decoded action document.
    pub value: Value,
    /// Target slot/sensor position that selected the action, when known.
    pub target_slot: Option<String>,
}

/// A deterministic encounter action/output, in execution order.
#[derive(Debug, Clone, PartialEq)]
pub enum EncounterAction {
    /// `Log`.
    Log {
        /// Message text.
        message: String,
    },
    /// `State`/`ParentState` changed the current state.
    State {
        /// New state name.
        state: String,
    },
    /// `TimerStart`.
    TimerStart {
        /// Timer name.
        name: String,
        /// Start value in seconds.
        seconds: f64,
        /// Restart value in seconds, when supplied.
        restart_seconds: Option<f64>,
    },
    /// `TimerRestart`.
    TimerRestart {
        /// Timer name.
        name: String,
        /// Restart value in seconds.
        seconds: f64,
    },
    /// `TimerModify`.
    TimerModify {
        /// Timer name.
        name: String,
        /// Absolute new value in seconds.
        seconds: f64,
    },
    /// `TimerStop`.
    TimerStop {
        /// Timer name.
        name: String,
    },
    /// `SetFlag`.
    Flag {
        /// Flag name.
        name: String,
        /// New value.
        value: bool,
    },
    /// `SetEncounterBossBar`.
    BossBar {
        /// Boss bar name/localisation key.
        name: String,
        /// Hard-mode bar.
        hard_mode: bool,
        /// Bound target slot.
        target_slot: Option<String>,
    },
    /// `ClearEncounterBossBar`.
    ClearBossBar,
    /// `StartEncounterMusic`.
    StartMusic {
        /// Music container key.
        music_container: String,
    },
    /// `StopEncounterMusic`.
    StopMusic,
    /// `SetEncounterAudioState`.
    AudioState {
        /// Audio state asset key.
        audio_state: String,
        /// State value.
        value: String,
        /// Fade duration in milliseconds.
        fade_ms: f64,
    },
    /// `SetEncounterWeather`.
    Weather {
        /// Weather asset key.
        weather: String,
        /// Transition seconds.
        transition_seconds: f64,
    },
    /// `ClearEncounterWeather`.
    ClearWeather,
    /// `SetTargetNPCInvulnerable`.
    TargetInvulnerable {
        /// Target slot.
        target_slot: String,
        /// New invulnerability.
        invulnerable: bool,
    },
    /// `ChangeTargetRole`.
    TargetRole {
        /// Target slot.
        target_slot: String,
        /// New role asset key.
        role: String,
        /// Change appearance with the role.
        change_appearance: bool,
        /// Optional NPC state (`Combat.Revive`).
        state: Option<String>,
    },
    /// `ReleaseTarget`.
    ReleaseTarget {
        /// Target slot.
        target_slot: String,
    },
    /// `TriggerSpawners`.
    TriggerSpawners {
        /// Search range.
        range: f64,
        /// Spawn count.
        count: usize,
        /// Spawn marker key.
        spawn_marker: Option<String>,
        /// Target slot to bind.
        target_slot: Option<String>,
        /// Rebind existing spawns.
        rebind: bool,
    },
    /// `AdjustPortalTimer`.
    PortalTimer {
        /// Seconds added (negative drains).
        seconds: f64,
        /// Remaining seconds after the change.
        remaining_seconds: Option<f64>,
    },
    /// `Spawn`.
    Spawn {
        /// Role/kind key.
        kind: String,
        /// Requested count range.
        count: [i64; 2],
        /// Distance range.
        distance: [f64; 2],
    },
    /// `SpawnInteraction`.
    SpawnInteraction {
        /// Interaction key.
        interaction: String,
        /// Sensor position source.
        position_source: String,
        /// Optional delay.
        delay: Option<[f64; 2]>,
    },
    /// `SpawnParticles`.
    Particles {
        /// Particle system key.
        particle_system: String,
        /// Sensor position source.
        position_source: String,
    },
    /// `PlaySound`.
    PlaySound {
        /// Sound event key.
        sound_event_id: String,
    },
    /// `ApplyEntityEffect`.
    EntityEffect {
        /// Effect key.
        effect_id: String,
        /// Target (`Target`, `User`, ...).
        target: String,
    },
    /// An action with no dedicated evaluator, preserved for the world layer.
    Deferred(DeferredEncounterAction),
}

/// Result of one encounter tick.
#[derive(Debug, Clone, PartialEq)]
pub struct EncounterTickReport {
    /// Current state after the tick.
    pub state: String,
    /// Whether the state changed this tick (including transition actions).
    pub state_changed: bool,
    /// Actions in execution order.
    pub actions: Vec<EncounterAction>,
    /// Deferred actions also appear in `actions`; this list is convenient for
    /// world producers.
    pub deferred: Vec<DeferredEncounterAction>,
}

impl EncounterTickReport {
    /// Messages logged this tick.
    pub fn logs(&self) -> impl Iterator<Item = &str> {
        self.actions.iter().filter_map(|action| match action {
            EncounterAction::Log { message } => Some(message.as_str()),
            _ => None,
        })
    }
}

/// One live encounter instance.
#[derive(Debug, Clone)]
pub struct EncounterRuntime {
    document: EncounterDocument,
    state: String,
    pending_transition: Option<(String, String)>,
    timers: BTreeMap<String, Timer>,
    flags: BTreeMap<String, bool>,
    once: BTreeSet<String>,
    blocking: BTreeMap<String, BlockingState>,
    targets: BTreeMap<String, EncounterTarget>,
    position: Option<Vec3>,
    players: Vec<Vec3>,
    beacons: VecDeque<BeaconMessage>,
    line_of_sight: BTreeMap<u64, bool>,
    pub boss_bar: Option<BossBarState>,
    pub audio_state: BTreeMap<String, String>,
    pub music: Option<String>,
    pub weather: Option<String>,
    pub portal: PortalState,
    random_unit: f64,
    completed: bool,
}

#[derive(Debug, Clone, Default)]
struct Timer {
    remaining: f64,
    restart_value: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct BlockingState {
    index: usize,
    delay: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Progress {
    Done,
    Delay(f64),
}

impl EncounterRuntime {
    /// Build a runtime from a resolved document.
    pub fn new(document: EncounterDocument) -> Self {
        Self {
            state: document.start_state.clone(),
            document,
            pending_transition: None,
            timers: BTreeMap::new(),
            flags: BTreeMap::new(),
            once: BTreeSet::new(),
            blocking: BTreeMap::new(),
            targets: BTreeMap::new(),
            position: None,
            players: Vec::new(),
            beacons: VecDeque::new(),
            line_of_sight: BTreeMap::new(),
            boss_bar: None,
            audio_state: BTreeMap::new(),
            music: None,
            weather: None,
            portal: PortalState::default(),
            random_unit: 0.5,
            completed: false,
        }
    }

    /// The document backing this runtime.
    pub fn document(&self) -> &EncounterDocument {
        &self.document
    }

    /// Current state.
    pub fn state(&self) -> &str {
        &self.state
    }

    /// Whether the encounter reached its `Complete`/`Completed` state.
    pub fn is_completed(&self) -> bool {
        self.completed
            || self.state.eq_ignore_ascii_case("complete")
            || self.state.eq_ignore_ascii_case("completed")
    }

    /// Deterministic RNG unit used for ranges (default `0.5`).
    pub fn set_random_unit(&mut self, unit: f64) {
        self.random_unit = unit.clamp(0.0, 1.0);
    }

    /// Current timers as `(name, remaining seconds)`.
    pub fn timers(&self) -> impl Iterator<Item = (&str, f64)> {
        self.timers
            .iter()
            .map(|(name, timer)| (name.as_str(), timer.remaining))
    }

    /// Current flags.
    pub fn flags(&self) -> &BTreeMap<String, bool> {
        &self.flags
    }

    /// Current target slots.
    pub fn targets(&self) -> &BTreeMap<String, EncounterTarget> {
        &self.targets
    }

    /// Mutable target slots (world producer/death seam).
    pub fn targets_mut(&mut self) -> &mut BTreeMap<String, EncounterTarget> {
        &mut self.targets
    }

    /// Set the encounter entity/world position used by range sensors.
    pub fn set_position(&mut self, position: Vec3) {
        self.position = Some(position);
    }

    /// Set a portal timer (world seam).
    pub fn set_portal_timer(&mut self, seconds: Option<f64>) {
        self.portal.remaining_seconds = seconds.map(|seconds| seconds.max(0.0));
    }

    /// Add a target slot (world seam/tests).
    pub fn set_target(&mut self, slot: impl Into<String>, target: EncounterTarget) {
        self.targets.insert(slot.into(), target);
    }

    /// Queue a beacon message (world seam/tests).
    pub fn send_beacon(&mut self, message: BeaconMessage) {
        self.beacons.push_back(message);
    }

    /// Advance the encounter by `dt` seconds.
    pub fn tick(&mut self, dt: f64, context: &EncounterContext) -> EncounterTickReport {
        let mut report = EncounterTickReport {
            state: self.state.clone(),
            state_changed: false,
            actions: Vec::new(),
            deferred: Vec::new(),
        };
        if !dt.is_finite() || dt <= 0.0 {
            return report;
        }

        self.position = context.position;
        self.players = context.players.clone();
        self.line_of_sight = context.line_of_sight.clone();
        for (slot, target) in &context.targets {
            self.targets.insert(slot.clone(), target.clone());
        }
        for beacon in &context.beacons {
            self.beacons.push_back(beacon.clone());
        }

        self.tick_timers(dt);
        self.run_pending_transitions(dt, &mut report);
        let instructions = self.document.instructions.clone();
        self.execute_instructions(&instructions, "root", dt, &mut report, 0);
        report.state = self.state.clone();
        report.deferred = report
            .actions
            .iter()
            .filter_map(|action| match action {
                EncounterAction::Deferred(deferred) => Some(deferred.clone()),
                _ => None,
            })
            .collect();
        report
    }

    fn tick_timers(&mut self, dt: f64) {
        for timer in self.timers.values_mut() {
            if timer.remaining > 0.0 {
                timer.remaining = (timer.remaining - dt).max(0.0);
            }
        }
    }

    fn run_pending_transitions(&mut self, dt: f64, report: &mut EncounterTickReport) {
        // A transition action may change state again. Cap the cascade so a
        // malformed asset cannot spin forever.
        for _ in 0..16 {
            let Some((from, to)) = self.pending_transition.take() else {
                return;
            };
            let transitions = self.document.state_transitions.clone();
            for (index, transition) in transitions.iter().enumerate() {
                if !transition.matches(&from, &to) {
                    continue;
                }
                let path = format!("transition/{index}");
                let actions = transition.actions.clone();
                self.execute_action_list(&actions, true, &path, dt, report, 0);
            }
            if self.pending_transition.is_none() {
                return;
            }
        }
    }

    fn execute_instructions(
        &mut self,
        instructions: &[Value],
        path: &str,
        dt: f64,
        report: &mut EncounterTickReport,
        depth: usize,
    ) {
        if depth > 64 {
            return;
        }
        for (index, instruction) in instructions.iter().enumerate() {
            if !self.enabled(instruction) {
                continue;
            }
            let instruction_path = format!("{path}/{index}");
            if !self.sensor_matches(
                instruction.get("Sensor"),
                &format!("{instruction_path}/sensor"),
            ) {
                continue;
            }
            self.execute_instruction(instruction, &instruction_path, dt, report, depth + 1);
            if !instruction
                .get("Continue")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                break;
            }
        }
    }

    fn execute_instruction(
        &mut self,
        instruction: &Value,
        path: &str,
        dt: f64,
        report: &mut EncounterTickReport,
        depth: usize,
    ) {
        if depth > 64 {
            return;
        }
        if let Some(children) = instruction.get("Instructions").and_then(Value::as_array) {
            self.execute_instructions(children, path, dt, report, depth + 1);
            return;
        }
        // `ActionsBlocking` is a boolean flag in the Java instruction builder;
        // some documents also carry an explicit blocking action list.
        let blocking_actions = instruction.get("ActionsBlocking").and_then(Value::as_array);
        if let Some(actions) = blocking_actions {
            self.execute_action_list(actions, true, path, dt, report, depth + 1);
        }
        if let Some(actions) = instruction.get("Actions").and_then(Value::as_array) {
            let blocking = instruction
                .get("ActionsBlocking")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            self.execute_action_list(actions, blocking, path, dt, report, depth + 1);
        }
    }

    fn execute_action_list(
        &mut self,
        actions: &[Value],
        blocking: bool,
        path: &str,
        dt: f64,
        report: &mut EncounterTickReport,
        depth: usize,
    ) {
        if depth > 64 {
            return;
        }
        if blocking {
            let mut state = self.blocking.get(path).cloned().unwrap_or_default();
            if state.delay > 0.0 {
                state.delay = (state.delay - dt).max(0.0);
                if state.delay > 0.0 {
                    self.blocking.insert(path.to_string(), state);
                    return;
                }
            }
            while state.index < actions.len() {
                let action = &actions[state.index];
                let action_path = format!("{path}/a{}", state.index);
                match self.execute_action(action, &action_path, dt, report, depth + 1) {
                    Progress::Delay(delay) => {
                        state.index += 1;
                        state.delay = delay;
                        self.blocking.insert(path.to_string(), state);
                        return;
                    }
                    Progress::Done => {
                        state.index += 1;
                    }
                }
            }
            self.blocking.remove(path);
            return;
        }

        for (index, action) in actions.iter().enumerate() {
            let action_path = format!("{path}/a{index}");
            let _ = self.execute_action(action, &action_path, dt, report, depth + 1);
        }
    }

    fn execute_action(
        &mut self,
        action: &Value,
        path: &str,
        dt: f64,
        report: &mut EncounterTickReport,
        depth: usize,
    ) -> Progress {
        if depth > 64 {
            return Progress::Done;
        }
        if !self.enabled(action) {
            return Progress::Done;
        }
        let kind = action
            .get("Type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "Log" => {
                let message = text(action, "Message");
                if !message.is_empty() {
                    report.actions.push(EncounterAction::Log { message });
                }
            }
            "State" => {
                let state = text(action, "State");
                self.set_state(state, report);
            }
            "ParentState" => {
                let state = text(action, "State");
                self.set_state(state, report);
            }
            "TimerStart" => {
                let name = text(action, "Name");
                let seconds = range_value(action.get("StartValueRange")).unwrap_or(0.0);
                let restart_seconds = range_value(action.get("RestartValueRange"));
                self.timers.insert(
                    name.clone(),
                    Timer {
                        remaining: seconds,
                        restart_value: restart_seconds,
                    },
                );
                report.actions.push(EncounterAction::TimerStart {
                    name,
                    seconds,
                    restart_seconds,
                });
            }
            "TimerRestart" => {
                let name = text(action, "Name");
                let timer = self.timers.entry(name.clone()).or_default();
                let seconds = timer
                    .restart_value
                    .or_else(|| range_value(action.get("RestartValueRange")))
                    .unwrap_or(0.0);
                timer.remaining = seconds;
                timer.restart_value = Some(seconds);
                report
                    .actions
                    .push(EncounterAction::TimerRestart { name, seconds });
            }
            "TimerModify" => {
                let name = text(action, "Name");
                let seconds = action
                    .get("SetValue")
                    .and_then(Value::as_f64)
                    .or_else(|| action.get("Value").and_then(Value::as_f64))
                    .unwrap_or(0.0);
                let timer = self.timers.entry(name.clone()).or_default();
                timer.remaining = seconds.max(0.0);
                report
                    .actions
                    .push(EncounterAction::TimerModify { name, seconds });
            }
            "TimerStop" => {
                let name = text(action, "Name");
                let timer = self.timers.entry(name.clone()).or_default();
                timer.remaining = 0.0;
                report.actions.push(EncounterAction::TimerStop { name });
            }
            "SetFlag" => {
                let name = text(action, "Name");
                let value = action.get("SetTo").and_then(Value::as_bool).unwrap_or(true);
                self.flags.insert(name.clone(), value);
                report.actions.push(EncounterAction::Flag { name, value });
            }
            "SetEncounterBossBar" => {
                let state = BossBarState {
                    name: text(action, "Name"),
                    hard_mode: action
                        .get("HardMode")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    target_slot: optional_text(action, "TargetSlot"),
                };
                self.boss_bar = Some(state.clone());
                report.actions.push(EncounterAction::BossBar {
                    name: state.name,
                    hard_mode: state.hard_mode,
                    target_slot: state.target_slot,
                });
            }
            "ClearEncounterBossBar" => {
                self.boss_bar = None;
                report.actions.push(EncounterAction::ClearBossBar);
            }
            "StartEncounterMusic" => {
                let music_container = text(action, "MusicContainer");
                self.music = Some(music_container.clone());
                report
                    .actions
                    .push(EncounterAction::StartMusic { music_container });
            }
            "StopEncounterMusic" => {
                self.music = None;
                report.actions.push(EncounterAction::StopMusic);
            }
            "SetEncounterAudioState" => {
                let audio_state = text(action, "AudioState");
                let value = text(action, "Value");
                self.audio_state.insert(audio_state.clone(), value.clone());
                report.actions.push(EncounterAction::AudioState {
                    audio_state,
                    value,
                    fade_ms: action.get("FadeMs").and_then(Value::as_f64).unwrap_or(0.0),
                });
            }
            "SetEncounterWeather" => {
                let weather = text(action, "Weather");
                let transition_seconds = action
                    .get("TransitionSeconds")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                self.weather = Some(weather.clone());
                report.actions.push(EncounterAction::Weather {
                    weather,
                    transition_seconds,
                });
            }
            "ClearEncounterWeather" => {
                self.weather = None;
                report.actions.push(EncounterAction::ClearWeather);
            }
            "SetTargetNPCInvulnerable" => {
                let target_slot = self.target_slot(action, "TargetSlot");
                let invulnerable = action
                    .get("Invulnerable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(target) = self.targets.get_mut(&target_slot) {
                    target.invulnerable = invulnerable;
                }
                report.actions.push(EncounterAction::TargetInvulnerable {
                    target_slot,
                    invulnerable,
                });
            }
            "ChangeTargetRole" => {
                let target_slot = self.target_slot(action, "TargetSlot");
                let role = text(action, "Role");
                let change_appearance = action
                    .get("ChangeAppearance")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let state = optional_text(action, "State");
                if let Some(target) = self.targets.get_mut(&target_slot) {
                    target.role = Some(role.clone());
                }
                report.actions.push(EncounterAction::TargetRole {
                    target_slot,
                    role,
                    change_appearance,
                    state,
                });
            }
            "ReleaseTarget" => {
                let target_slot = self.target_slot(action, "TargetSlot");
                self.targets.remove(&target_slot);
                report
                    .actions
                    .push(EncounterAction::ReleaseTarget { target_slot });
            }
            "TriggerSpawners" => {
                let state = EncounterAction::TriggerSpawners {
                    range: action.get("Range").and_then(Value::as_f64).unwrap_or(0.0),
                    count: action.get("Count").and_then(Value::as_u64).unwrap_or(1) as usize,
                    spawn_marker: optional_text(action, "SpawnMarker"),
                    target_slot: optional_text(action, "TargetSlot"),
                    rebind: action
                        .get("Rebind")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                };
                report.actions.push(state);
            }
            "AdjustPortalTimer" => {
                let seconds = action.get("Seconds").and_then(Value::as_f64).unwrap_or(0.0);
                let remaining_seconds = self
                    .portal
                    .remaining_seconds
                    .map(|current| (current + seconds).max(0.0));
                self.portal.remaining_seconds = remaining_seconds;
                report.actions.push(EncounterAction::PortalTimer {
                    seconds,
                    remaining_seconds,
                });
            }
            "Spawn" => {
                report.actions.push(EncounterAction::Spawn {
                    kind: text(action, "Kind"),
                    count: integer_range(action.get("CountRange")).unwrap_or([1, 1]),
                    distance: range_value(action.get("DistanceRange")).map_or([0.0, 0.0], |_| {
                        number_range(action.get("DistanceRange")).unwrap_or([0.0, 0.0])
                    }),
                });
            }
            "SpawnInteraction" => {
                report.actions.push(EncounterAction::SpawnInteraction {
                    interaction: text(action, "Interaction"),
                    position_source: text(action, "PositionSource"),
                    delay: number_range(action.get("Delay")),
                });
            }
            "SpawnParticles" => {
                report.actions.push(EncounterAction::Particles {
                    particle_system: text(action, "ParticleSystem"),
                    position_source: text(action, "PositionSource"),
                });
            }
            "PlaySound" => {
                report.actions.push(EncounterAction::PlaySound {
                    sound_event_id: text(action, "SoundEventId"),
                });
            }
            "ApplyEntityEffect" => {
                report.actions.push(EncounterAction::EntityEffect {
                    effect_id: text(action, "EffectId"),
                    target: text(action, "Entity"),
                });
            }
            "Timeout" => {
                let delay = range_value(action.get("Delay")).unwrap_or(0.0);
                if delay > 0.0 {
                    return Progress::Delay(delay);
                }
            }
            "Sequence" => {
                let blocking = action
                    .get("Blocking")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if let Some(actions) = action.get("Actions").and_then(Value::as_array) {
                    self.execute_action_list(actions, blocking, path, dt, report, depth + 1);
                }
            }
            "ForEach" | "ProjectToGround" | "AdjustPosition" => {
                if let Some(actions) = action.get("Actions").and_then(Value::as_array) {
                    self.execute_action_list(actions, false, path, dt, report, depth + 1);
                }
            }
            "" => {}
            _ => {
                let deferred = DeferredEncounterAction {
                    kind: kind.to_string(),
                    value: action.clone(),
                    target_slot: optional_text(action, "TargetSlot"),
                };
                report.actions.push(EncounterAction::Deferred(deferred));
            }
        }
        Progress::Done
    }

    fn set_state(&mut self, state: String, report: &mut EncounterTickReport) {
        if state.is_empty() {
            return;
        }
        let state = if let Some(substate) = state.strip_prefix('.') {
            format!(
                "{}.{}",
                self.state.split('.').next().unwrap_or(&self.state),
                substate
            )
        } else {
            state
        };
        if state == self.state {
            return;
        }
        let old = std::mem::replace(&mut self.state, state.clone());
        self.pending_transition = Some((old, state.clone()));
        self.once.clear();
        self.blocking.clear();
        self.completed =
            state.eq_ignore_ascii_case("complete") || state.eq_ignore_ascii_case("completed");
        report.state_changed = true;
        report.state = state.clone();
        report.actions.push(EncounterAction::State { state });
    }

    fn enabled(&self, node: &Value) -> bool {
        node.get("Enabled").and_then(Value::as_bool).unwrap_or(true)
    }

    fn sensor_matches(&mut self, sensor: Option<&Value>, path: &str) -> bool {
        let Some(sensor) = sensor else {
            return true;
        };
        let once = sensor.get("Once").and_then(Value::as_bool).unwrap_or(false);
        if once && self.once.contains(path) {
            return false;
        }
        let matched = self.sensor_value(sensor);
        if matched && once {
            self.once.insert(path.to_string());
        }
        matched
    }

    fn sensor_value(&mut self, sensor: &Value) -> bool {
        let kind = sensor
            .get("Type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "Any" => true,
            "State" => {
                let required = sensor
                    .get("State")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.state == required
                    || self.state.ends_with(&format!(".{required}"))
                    || required == ".Default"
                    || required == "Default"
            }
            "Player" => {
                let range = sensor.get("Range").and_then(Value::as_f64).unwrap_or(0.0);
                range <= 0.0
                    || self
                        .players
                        .iter()
                        .any(|position| distance(*position, self.center()) <= range)
                    || (!self.players.is_empty() && sensor.get("Range").is_none())
            }
            "Timer" => {
                let name = text(sensor, "Name");
                let required = sensor
                    .get("State")
                    .and_then(Value::as_str)
                    .unwrap_or("Stopped");
                let remaining = self.timers.get(&name).map_or(0.0, |timer| timer.remaining);
                match required {
                    "Running" | "Started" => remaining > 0.0,
                    _ => remaining <= 0.0,
                }
            }
            "And" => sensor
                .get("Sensors")
                .and_then(Value::as_array)
                .is_some_and(|sensors| sensors.iter().all(|sensor| self.sensor_value(sensor))),
            "Or" => sensor
                .get("Sensors")
                .and_then(Value::as_array)
                .is_some_and(|sensors| sensors.iter().any(|sensor| self.sensor_value(sensor))),
            "Not" => !sensor
                .get("Sensor")
                .is_some_and(|sensor| self.sensor_value(sensor)),
            "Target" | "HasTarget" | "TargetDistance" => self.target_matches(sensor),
            "Mob" => self.target_matches(sensor),
            "Count" => self.count_matches(sensor),
            "Flag" => {
                let name = text(sensor, "Name");
                let expected = sensor
                    .get("Set")
                    .or_else(|| sensor.get("SetTo"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                self.flags.get(&name).copied().unwrap_or(false) == expected
            }
            "Beacon" => self.beacon_matches(sensor),
            "Points" | "Block" | "BlockType" => false,
            _ => false,
        }
    }

    fn target_matches(&self, sensor: &Value) -> bool {
        let Some(target) = self.sensor_target(sensor) else {
            return false;
        };
        if !self.target_attitude_matches(target, sensor) {
            return false;
        }
        let range = sensor.get("Range").and_then(Value::as_f64);
        if let Some(range) = range {
            if range > 0.0 && distance(target.position, self.center()) > range {
                return false;
            }
        }
        if let Some(range) = sensor.get("DistanceRange").and_then(Value::as_array) {
            let low = range.first().and_then(Value::as_f64).unwrap_or(0.0);
            let high = range.get(1).and_then(Value::as_f64).unwrap_or(f64::MAX);
            let distance = distance(target.position, self.center());
            if distance < low || distance > high {
                return false;
            }
        }
        self.filters_match(target, sensor.get("Filters"), Some(&self.line_of_sight))
    }

    fn count_matches(&self, sensor: &Value) -> bool {
        let range = number_range(sensor.get("Range")).unwrap_or([0.0, f64::MAX]);
        let count_range = integer_range(sensor.get("Count")).unwrap_or([1, i64::MAX]);
        let count = self
            .targets
            .values()
            .filter(|target| {
                let distance = distance(target.position, self.center());
                distance >= range[0]
                    && distance <= range[1]
                    && self.group_filter_matches(target, sensor)
            })
            .count() as i64;
        count >= count_range[0] && count <= count_range[1]
    }

    fn beacon_matches(&mut self, sensor: &Value) -> bool {
        let message = sensor
            .get("Message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(index) = self
            .beacons
            .iter()
            .position(|beacon| beacon.message == message)
        else {
            return false;
        };
        let consume = sensor
            .get("ConsumeMessage")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if consume {
            self.beacons.remove(index);
        }
        true
    }

    fn sensor_target(&self, sensor: &Value) -> Option<&EncounterTarget> {
        let slot = sensor.get("TargetSlot").and_then(Value::as_str);
        if let Some(slot) = slot {
            return self.targets.get(slot);
        }
        self.targets
            .get("Target")
            .or_else(|| self.targets.get("EncounterTarget"))
            .or_else(|| self.targets.values().next())
    }

    fn target_slot(&self, action: &Value, key: &str) -> String {
        optional_text(action, key)
            .or_else(|| self.targets.keys().next().cloned())
            .unwrap_or_else(|| "Target".to_string())
    }

    fn target_attitude_matches(&self, target: &EncounterTarget, sensor: &Value) -> bool {
        let Some(filters) = sensor.get("Filters").and_then(Value::as_array) else {
            return true;
        };
        filters.iter().all(|filter| {
            if filter.get("Type").and_then(Value::as_str) == Some("Attitude") {
                let Some(attitudes) = filter.get("Attitudes").and_then(Value::as_array) else {
                    return true;
                };
                attitudes.iter().any(|attitude| {
                    attitude.as_str().is_some_and(|attitude| {
                        target.attitude.iter().any(|entry| entry == attitude)
                    })
                })
            } else {
                true
            }
        })
    }

    fn filters_match(
        &self,
        target: &EncounterTarget,
        filters: Option<&Value>,
        context: Option<&BTreeMap<u64, bool>>,
    ) -> bool {
        let Some(filters) = filters.and_then(Value::as_array) else {
            return true;
        };
        filters.iter().all(|filter| {
            if !filter
                .get("Enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            {
                return true;
            }
            let kind = filter
                .get("Type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            match kind {
                "Attitude" => filter
                    .get("Attitudes")
                    .and_then(Value::as_array)
                    .is_none_or(|attitudes| {
                        attitudes.iter().any(|attitude| {
                            attitude.as_str().is_some_and(|attitude| {
                                target.attitude.iter().any(|entry| entry == attitude)
                            })
                        })
                    }),
                "NPCGroup" => filter
                    .get("IncludeGroups")
                    .and_then(Value::as_array)
                    .is_none_or(|groups| {
                        groups.iter().any(|group| {
                            group.as_str().is_some_and(|group| {
                                target.groups.iter().any(|entry| entry == group)
                            })
                        })
                    }),
                "Stat" => {
                    let value = target.health_fraction();
                    number_range(filter.get("ValueRange"))
                        .is_none_or(|range| value >= range[0] as f32 && value <= range[1] as f32)
                }
                "LineOfSight" => context
                    .and_then(|sight| sight.get(&target.id))
                    .copied()
                    .unwrap_or(true),
                "HeightDifference" => true,
                "Not" => !filter.get("Filter").is_some_and(|filter| {
                    self.filters_match(target, Some(&Value::Array(vec![filter.clone()])), context)
                }),
                _ => true,
            }
        })
    }

    fn group_filter_matches(&self, target: &EncounterTarget, sensor: &Value) -> bool {
        let included = sensor
            .get("IncludeGroups")
            .and_then(Value::as_array)
            .is_none_or(|groups| {
                groups.iter().any(|group| {
                    group
                        .as_str()
                        .is_some_and(|group| target.groups.iter().any(|entry| entry == group))
                })
            });
        let excluded = sensor
            .get("ExcludeGroups")
            .and_then(Value::as_array)
            .is_some_and(|groups| {
                groups.iter().any(|group| {
                    group.as_str().is_some_and(|group| {
                        target.groups.iter().any(|entry| entry == group)
                            || (group == "Player" && target.is_player)
                    })
                })
            });
        included && !excluded
    }

    fn center(&self) -> Vec3 {
        self.position
            .or_else(|| self.targets.values().next().map(|target| target.position))
            .or_else(|| self.players.first().copied())
            .unwrap_or_default()
    }
}

fn distance(left: Vec3, right: Vec3) -> f64 {
    let dx = right.x - left.x;
    let dy = right.y - left.y;
    let dz = right.z - left.z;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn parse_transitions(document: &Value) -> Vec<StateTransition> {
    let Some(entries) = document.get("StateTransitions").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .map(|entry| {
            let mut from = Vec::new();
            let mut to = Vec::new();
            if let Some(states) = entry.get("States").and_then(Value::as_array) {
                for state in states {
                    if let Some(values) = state.get("From").and_then(Value::as_array) {
                        from.extend(
                            values
                                .iter()
                                .filter_map(|value| value.as_str().map(ToOwned::to_owned)),
                        );
                    }
                    if let Some(values) = state.get("To").and_then(Value::as_array) {
                        to.extend(
                            values
                                .iter()
                                .filter_map(|value| value.as_str().map(ToOwned::to_owned)),
                        );
                    }
                }
            }
            let actions = match entry.get("Actions") {
                Some(Value::Array(actions)) => actions.clone(),
                Some(action) => vec![action.clone()],
                None => Vec::new(),
            };
            StateTransition {
                from,
                to,
                actions,
                raw: entry.clone(),
            }
        })
        .collect()
}

/// Merge a document's raw `Reference` chain without evaluating expressions.
fn merged_raw_document(
    key: &str,
    raw: &BTreeMap<String, Value>,
    stack: &mut BTreeSet<String>,
) -> Result<Value, EncounterError> {
    if !stack.insert(format!("raw:{key}")) {
        return Err(EncounterError::ReferenceCycle(key.to_string()));
    }
    let document = raw
        .get(key)
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let mut merged = if document.get("Type").and_then(Value::as_str) == Some("Variant") {
        if let Some(reference) = document.get("Reference").and_then(Value::as_str) {
            merged_raw_document(reference, raw, stack)?
        } else {
            Value::Object(Map::new())
        }
    } else {
        Value::Object(Map::new())
    };
    merge_document(&mut merged, &document);
    stack.remove(&format!("raw:{key}"));
    Ok(merged)
}

/// Resolve one document and its `Reference` chain.
fn resolve_document(
    key: &str,
    raw: &BTreeMap<String, Value>,
    forwards: &BTreeMap<String, Value>,
    stack: &mut BTreeSet<String>,
) -> Result<Value, EncounterError> {
    if !stack.insert(key.to_string()) {
        return Err(EncounterError::ReferenceCycle(key.to_string()));
    }
    let document = raw
        .get(key)
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let mut merged = merged_raw_document(key, raw, stack)?;

    let mut parameters = extract_parameters(&merged);
    if let Some(modify) = document.get("Modify").and_then(Value::as_object) {
        for (name, value) in modify {
            if !name.starts_with('_') {
                parameters.insert(name.clone(), value.clone());
            }
        }
    }
    let vars = evaluate_parameters(&parameters);
    let mut resolved = evaluate_compute(&mut merged, &vars);

    let nested_forwards = document
        .get("Modify")
        .and_then(|modify| modify.get("_ForwardedModifiers"))
        .and_then(Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_else(|| forwards.clone());

    if let Some(instructions) = resolved.get("Instructions").cloned() {
        let instructions = expand_value(&instructions, &vars, &nested_forwards, raw, stack, 0)?;
        resolved["Instructions"] = instructions;
    }
    if let Some(transitions) = resolved.get("StateTransitions").cloned() {
        let transitions = expand_value(&transitions, &vars, &nested_forwards, raw, stack, 0)?;
        resolved["StateTransitions"] = transitions;
    }
    // Expose the final parameter values (including variant `Modify` overrides)
    // through `Parameters` so callers see the same scope the tree used.
    if let Some(parameters) = resolved
        .get_mut("Parameters")
        .and_then(Value::as_object_mut)
    {
        for (name, value) in &vars {
            let entry = parameters
                .entry(name.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(entry) = entry.as_object_mut() {
                entry.insert("Value".to_string(), value.to_json());
            }
        }
    }
    stack.remove(key);
    Ok(resolved)
}

/// Merge an encounter variant over its template.
fn merge_document(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                match key.as_str() {
                    "Reference" | "Type" | "Class" => {
                        target.insert(key.clone(), value.clone());
                    }
                    "Parameters" => {
                        let entry = target
                            .entry("Parameters".to_string())
                            .or_insert_with(|| Value::Object(Map::new()));
                        if let (Some(entry), Some(source)) =
                            (entry.as_object_mut(), value.as_object())
                        {
                            for (name, value) in source {
                                entry.insert(name.clone(), value.clone());
                            }
                        }
                    }
                    "Modify" => {
                        let modify = target
                            .entry("Modify".to_string())
                            .or_insert_with(|| Value::Object(Map::new()));
                        if let (Some(modify), Some(source)) =
                            (modify.as_object_mut(), value.as_object())
                        {
                            for (name, value) in source {
                                modify.insert(name.clone(), value.clone());
                            }
                        }
                    }
                    "_ForwardedModifiers" => {
                        let forwards = target
                            .entry("_ForwardedModifiers".to_string())
                            .or_insert_with(|| Value::Object(Map::new()));
                        if let (Some(forwards), Some(source)) =
                            (forwards.as_object_mut(), value.as_object())
                        {
                            for (name, value) in source {
                                forwards.insert(name.clone(), value.clone());
                            }
                        }
                    }
                    _ => {
                        if let Some(existing) = target.get_mut(key) {
                            merge_document(existing, value);
                        } else {
                            target.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        }
        (target, source) => *target = source.clone(),
    }
}

fn extract_parameters(document: &Value) -> BTreeMap<String, Value> {
    document
        .get("Parameters")
        .and_then(Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .map(|(name, entry)| {
                    (
                        name.clone(),
                        entry.get("Value").cloned().unwrap_or(Value::Null),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn evaluate_parameters(parameters: &BTreeMap<String, Value>) -> BTreeMap<String, ExprValue> {
    let mut vars = BTreeMap::new();
    let mut pending = parameters.clone();
    for _ in 0..8 {
        let mut progressed = false;
        let mut remaining = BTreeMap::new();
        for (name, value) in pending {
            let mut context = EvalContext::new(&vars);
            match evaluate_value(&value, &mut context) {
                Some(resolved) => {
                    vars.insert(name, resolved);
                    progressed = true;
                }
                None => {
                    remaining.insert(name, value);
                }
            }
        }
        if !progressed || remaining.is_empty() {
            break;
        }
        pending = remaining;
    }
    vars
}

/// Recursively evaluate `{"Compute": "..."}` values. Unknown variables are
/// left untouched so asset-specific extension scopes remain visible.
fn evaluate_compute(value: &mut Value, vars: &BTreeMap<String, ExprValue>) -> Value {
    match value {
        Value::Object(object) => {
            // Preserve the parameter name used by a slot reference; the
            // evaluated string alone loses the `_ForwardedModifiers` key.
            if let Some(reference) = object.get("Reference") {
                if let Some(expression) = reference.get("Compute").and_then(Value::as_str) {
                    object.insert(
                        "_ReferenceSlot".to_string(),
                        Value::String(expression.to_string()),
                    );
                }
            }
            if let Some(expression) = object.get("Compute").and_then(Value::as_str) {
                let expression = expression.to_string();
                let mut context = EvalContext::new(vars);
                if let Ok(resolved) = evaluate(&expression, &mut context) {
                    return resolved.to_json();
                }
                return value.clone();
            }
            for value in object.values_mut() {
                *value = evaluate_compute(value, vars);
            }
            value.clone()
        }
        Value::Array(values) => {
            for value in values.iter_mut() {
                *value = evaluate_compute(value, vars);
            }
            value.clone()
        }
        _ => value.clone(),
    }
}

fn evaluate_value(value: &Value, context: &mut EvalContext<'_>) -> Option<ExprValue> {
    if let Some(expression) = value.get("Compute").and_then(Value::as_str) {
        evaluate(expression, context).ok()
    } else {
        ExprValue::from_json(value)
    }
}

fn expand_value(
    value: &Value,
    vars: &BTreeMap<String, ExprValue>,
    forwards: &BTreeMap<String, Value>,
    raw: &BTreeMap<String, Value>,
    stack: &mut BTreeSet<String>,
    depth: usize,
) -> Result<Value, EncounterError> {
    if depth > 64 {
        return Ok(value.clone());
    }
    match value {
        Value::Array(values) => {
            let mut expanded = Vec::new();
            for value in values {
                expanded.push(expand_value(value, vars, forwards, raw, stack, depth + 1)?);
            }
            Ok(Value::Array(expanded))
        }
        Value::Object(object) => {
            if let Some(reference_value) = object.get("Reference") {
                let reference_name = object
                    .get("_ReferenceSlot")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        reference_value
                            .get("Compute")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    })
                    .or_else(|| reference_value.as_str().map(ToOwned::to_owned));
                let mut context = EvalContext::new(vars);
                let reference = evaluate_value(reference_value, &mut context)
                    .and_then(|value| match value {
                        ExprValue::String(value) => Some(value),
                        _ => None,
                    })
                    .unwrap_or_default();
                if reference.is_empty() || reference == "$Null" {
                    return Ok(Value::Null);
                }
                if let Some(document) = raw.get(&reference) {
                    if document.get("Class").and_then(Value::as_str) == Some("Instruction") {
                        let forwarded = reference_name
                            .as_ref()
                            .and_then(|name| forwards.get(name))
                            .cloned();
                        return expand_macro(
                            &reference,
                            document,
                            object,
                            forwarded.as_ref(),
                            vars,
                            forwards,
                            raw,
                            stack,
                            depth + 1,
                        );
                    }
                }
            }

            let mut expanded = Map::new();
            for (key, value) in object {
                if key == "_ReferenceSlot" {
                    continue;
                }
                expanded.insert(
                    key.clone(),
                    expand_value(value, vars, forwards, raw, stack, depth + 1)?,
                );
            }
            Ok(Value::Object(expanded))
        }
        _ => Ok(value.clone()),
    }
}

#[allow(clippy::too_many_arguments)]
fn expand_macro(
    reference: &str,
    document: &Value,
    reference_node: &Map<String, Value>,
    forwarded: Option<&Value>,
    parent_vars: &BTreeMap<String, ExprValue>,
    parent_forwards: &BTreeMap<String, Value>,
    raw: &BTreeMap<String, Value>,
    stack: &mut BTreeSet<String>,
    depth: usize,
) -> Result<Value, EncounterError> {
    if depth > 64 || !stack.insert(reference.to_string()) {
        return Err(EncounterError::ReferenceCycle(reference.to_string()));
    }
    let mut parameters = extract_parameters(document);
    if let Some(modify) = reference_node.get("Modify").and_then(Value::as_object) {
        for (name, value) in modify {
            if name == "_InterfaceParameters" {
                if let Some(interfaces) = value.as_object() {
                    for interface in interfaces.values().filter_map(Value::as_object) {
                        for (name, value) in interface {
                            parameters.insert(name.clone(), value.clone());
                        }
                    }
                }
                continue;
            }
            if !name.starts_with('_') {
                parameters.insert(name.clone(), value.clone());
            }
        }
    }
    if let Some(forwarded) = forwarded.and_then(Value::as_object) {
        for (name, value) in forwarded {
            parameters.insert(name.clone(), value.clone());
        }
    }
    // Values not supplied by the slot fall back to the parent scope.
    for (name, value) in parent_vars {
        parameters
            .entry(name.clone())
            .or_insert_with(|| value.to_json());
    }
    let vars = evaluate_parameters(&parameters);
    let mut body = document
        .get("Content")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    body = evaluate_compute(&mut body, &vars);
    body = expand_value(&body, &vars, parent_forwards, raw, stack, depth + 1)?;

    // Copy outer node fields (sensor/continue/once) over the macro body.
    if let (Some(body), Some(reference_node)) = (body.as_object_mut(), Some(reference_node)) {
        for (key, value) in reference_node {
            if matches!(
                key.as_str(),
                "Reference" | "Modify" | "Interfaces" | "AcceptsForward" | "Nullable"
            ) || key.starts_with('_')
            {
                continue;
            }
            body.insert(key.clone(), value.clone());
        }
    }

    // Java state import/export: a slot's `_ExportStates` supplies the state
    // consumed by the macro's `ParentState: "Next"`.
    if let Some(exports) = reference_node
        .get("Modify")
        .and_then(|modify| modify.get("_ExportStates"))
        .and_then(Value::as_array)
    {
        if let Some(export) = exports.first().and_then(Value::as_str) {
            rewrite_parent_state(&mut body, export);
        }
    }
    stack.remove(reference);
    Ok(body)
}

fn rewrite_parent_state(value: &mut Value, state: &str) {
    match value {
        Value::Object(object) => {
            if object.get("Type").and_then(Value::as_str) == Some("ParentState") {
                object.insert("State".to_string(), Value::String(state.to_string()));
            }
            for value in object.values_mut() {
                rewrite_parent_state(value, state);
            }
        }
        Value::Array(values) => {
            for value in values {
                rewrite_parent_state(value, state);
            }
        }
        _ => {}
    }
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn optional_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn range_value(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::Array(values) => {
            let low = values.first().and_then(Value::as_f64)?;
            let high = values.get(1).and_then(Value::as_f64).unwrap_or(low);
            Some((low + high) * 0.5)
        }
        _ => None,
    }
}

fn number_range(value: Option<&Value>) -> Option<[f64; 2]> {
    match value? {
        Value::Number(number) => number.as_f64().map(|value| [value, value]),
        Value::Array(values) => {
            let low = values.first().and_then(Value::as_f64)?;
            let high = values.get(1).and_then(Value::as_f64).unwrap_or(low);
            Some([low, high])
        }
        _ => None,
    }
}

fn integer_range(value: Option<&Value>) -> Option<[i64; 2]> {
    match value? {
        Value::Number(number) => number.as_i64().map(|value| [value, value]),
        Value::Array(values) => {
            let low = values.first().and_then(Value::as_i64)?;
            let high = values.get(1).and_then(Value::as_i64).unwrap_or(low);
            Some([low, high])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> EncounterCatalog {
        let raw = BTreeMap::from([
            (
                "Test_Encounter".to_string(),
                serde_json::json!({
                    "Class": "EncounterManager",
                    "Type": "Generic",
                    "StartState": "Idle",
                    "StateTransitions": [{
                        "States": [{"From": [], "To": ["Active"]}],
                        "Actions": [
                            {"Type": "Log", "Message": "active"},
                            {"Type": "TimerStart", "Name": "Round", "StartValueRange": [5, 5], "RestartValueRange": [5, 5]}
                        ]
                    }],
                    "Instructions": [{
                        "Sensor": {"Type": "State", "State": "Idle"},
                        "Instructions": [{
                            "Sensor": {"Type": "Player", "Range": 8},
                            "Actions": [{"Type": "State", "State": "Active"}]
                        }]
                    }]
                }),
            ),
            (
                "Test_Macro".to_string(),
                serde_json::json!({
                    "Class": "Instruction",
                    "Type": "Component",
                    "Interface": "Hytale.Instruction.Encounter.Phase.Logic",
                    "Parameters": {"Label": {"Value": "default"}},
                    "Content": {
                        "Actions": [{"Type": "Log", "Message": {"Compute": "Label"}}]
                    }
                }),
            ),
            (
                "Test_Host".to_string(),
                serde_json::json!({
                    "Class": "EncounterManager",
                    "Type": "Generic",
                    "Instructions": [{
                        "Reference": "Test_Macro",
                        "Modify": {"Label": "hello"}
                    }]
                }),
            ),
        ]);
        EncounterCatalog::from_raw_documents(raw).expect("catalog")
    }

    #[test]
    fn state_transitions_and_actions_run_in_order() {
        let catalog = catalog();
        let document = catalog.resolve("Test_Encounter").unwrap();
        let mut runtime = document.runtime();
        let context = EncounterContext::default().with_player(Vec3::new(0.0, 0.0, 0.0));
        let first = runtime.tick(0.1, &context);
        assert_eq!(first.state, "Active");
        assert_eq!(
            first.actions,
            vec![EncounterAction::State {
                state: "Active".to_string()
            }]
        );
        assert!(first.state_changed);
        let second = runtime.tick(0.1, &context);
        assert_eq!(second.actions.len(), 2);
        assert_eq!(
            second.actions[0],
            EncounterAction::Log {
                message: "active".to_string()
            }
        );
        assert!(matches!(
            second.actions[1],
            EncounterAction::TimerStart { ref name, seconds, .. }
                if name == "Round" && (seconds - 5.0).abs() < 1e-9
        ));
        assert_eq!(runtime.timers().collect::<Vec<_>>(), vec![("Round", 5.0)]);
    }

    #[test]
    fn macros_expand_with_modified_parameters() {
        let catalog = catalog();
        let document = catalog.resolve("Test_Host").unwrap();
        assert_eq!(document.instructions.len(), 1);
        let mut runtime = document.runtime();
        let report = runtime.tick(0.1, &EncounterContext::default());
        assert_eq!(
            report.actions,
            vec![EncounterAction::Log {
                message: "hello".to_string()
            }]
        );
    }

    #[test]
    fn boss_bar_audio_weather_and_portal_actions_emit() {
        let raw = BTreeMap::from([(
            "Boss".to_string(),
            serde_json::json!({
                "Class": "EncounterManager",
                "Type": "Generic",
                "Instructions": [{
                    "Actions": [
                        {"Type": "SetEncounterBossBar", "Name": "BossBar"},
                        {"Type": "StartEncounterMusic", "MusicContainer": "Track"},
                        {"Type": "SetEncounterAudioState", "AudioState": "Intensity", "Value": "Climax", "FadeMs": 1500},
                        {"Type": "SetEncounterWeather", "Weather": "Blood_Moon"},
                        {"Type": "AdjustPortalTimer", "Seconds": -30}
                    ]
                }]
            }),
        )]);
        let catalog = EncounterCatalog::from_raw_documents(raw).unwrap();
        let document = catalog.resolve("Boss").unwrap();
        let mut runtime = document.runtime();
        runtime.set_portal_timer(Some(60.0));
        let report = runtime.tick(0.1, &EncounterContext::default());
        assert!(matches!(
            report.actions[0],
            EncounterAction::BossBar { ref name, .. } if name == "BossBar"
        ));
        assert!(matches!(
            report.actions[1],
            EncounterAction::StartMusic { ref music_container } if music_container == "Track"
        ));
        assert!(matches!(
            &report.actions[2],
            EncounterAction::AudioState { audio_state, value, fade_ms }
                if audio_state == "Intensity" && value == "Climax" && (*fade_ms - 1500.0).abs() < 1e-9
        ));
        assert!(matches!(
            report.actions[3],
            EncounterAction::Weather { ref weather, .. } if weather == "Blood_Moon"
        ));
        assert!(matches!(
            report.actions[4],
            EncounterAction::PortalTimer {
                remaining_seconds: Some(30.0),
                ..
            }
        ));
    }

    #[test]
    fn blockings_timeout_delays_following_actions() {
        let raw = BTreeMap::from([(
            "Timed".to_string(),
            serde_json::json!({
                "Class": "EncounterManager",
                "Type": "Generic",
                "Instructions": [{
                    "ActionsBlocking": true,
                    "Actions": [
                        {"Type": "Log", "Message": "before"},
                        {"Type": "Timeout", "Delay": [2, 2]},
                        {"Type": "Log", "Message": "after"}
                    ]
                }]
            }),
        )]);
        let catalog = EncounterCatalog::from_raw_documents(raw).unwrap();
        let doc = catalog.resolve("Timed").unwrap();
        let mut runtime = doc.runtime();
        let first = runtime.tick(1.0, &EncounterContext::default());
        assert_eq!(first.logs().collect::<Vec<_>>(), vec!["before"]);
        let second = runtime.tick(0.5, &EncounterContext::default());
        assert_eq!(second.logs().count(), 0);
        let third = runtime.tick(1.0, &EncounterContext::default());
        assert_eq!(third.logs().count(), 0);
        let fourth = runtime.tick(0.6, &EncounterContext::default());
        assert_eq!(fourth.logs().collect::<Vec<_>>(), vec!["after"]);
    }
}
