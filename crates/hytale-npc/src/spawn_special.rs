//! Beacon and spawn-marker assets.
//!
//! `Server/NPC/Spawn/Beacons` and `Server/NPC/Spawn/Markers` are separate
//! asset stores from the world env spawns. This slice loads both catalogs,
//! resolves their weighted NPC/configuration entries, parses the ISO-8601
//! duration fields used for respawn timers and provides the live marker/beacon
//! controllers used by the world thread.

use std::collections::BTreeMap;
use std::path::Path;

use hytale_assets::AssetPack;
use hytale_world::collision::Vec3;
use hytale_world::world_clock::WorldClock;
use serde_json::Value;
use thiserror::Error;

use crate::flock::{FlockRng, FlockSize};
use crate::spawn::{RoleSpawnParameters, SpawnFlock, SpawnTimeWindow, WorldSpawn};

/// One `BeaconNPCSpawn` document (`NPCSpawn` plus beacon fields).
#[derive(Debug, Clone, PartialEq)]
pub struct BeaconSpawn {
    /// Asset key.
    pub key: String,
    /// Environments this beacon can be dynamically created in.
    pub environments: Vec<String>,
    /// Weighted role entries.
    pub npcs: Vec<RoleSpawnParameters>,
    /// Optional despawn window.
    pub despawn: Option<SpawnTimeWindow>,
    /// Spawn time range in hours.
    pub day_time_range: [f64; 2],
    /// Spawn moon-phase range.
    pub moon_phase_range: [i32; 2],
    /// Optional light ranges.
    pub light_ranges: BTreeMap<String, [f64; 2]>,
    /// Java `ScaleDayTimeRange`.
    pub scale_day_time_range: bool,
    /// Maximum simultaneous NPCs.
    pub max_spawned_npcs: i32,
    /// Concurrent spawn count range.
    pub concurrent_spawns_range: [i32; 2],
    /// Game-time respawn range in seconds.
    pub spawn_after_game_seconds: Option<[f64; 2]>,
    /// Real-time respawn range in seconds.
    pub spawn_after_real_seconds: Option<[f64; 2]>,
    /// Initial spawn delay range in seconds.
    pub initial_spawn_delay_seconds: Option<[f64; 2]>,
    /// NPC spawn radius.
    pub spawn_radius: f64,
    /// Beacon influence radius.
    pub beacon_radius: f64,
    /// The source document.
    pub raw: Value,
}

impl BeaconSpawn {
    /// Parse one beacon document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Result<Self, SpawnSpecialError> {
        let key = key.into();
        let npcs = parse_roles(&key, &raw)?;
        let despawn = raw
            .get("Despawn")
            .map(|value| SpawnTimeWindow::from_json(&key, value))
            .transpose()?;
        Ok(Self {
            key,
            environments: string_array(&raw, "Environments"),
            npcs,
            despawn,
            day_time_range: f64_pair(&raw, "DayTimeRange").unwrap_or([0.0, 24.0]),
            moon_phase_range: i32_pair(&raw, "MoonPhaseRange").unwrap_or([0, i32::MAX]),
            light_ranges: light_range_map(&raw),
            scale_day_time_range: raw
                .get("ScaleDayTimeRange")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            max_spawned_npcs: raw
                .get("MaxSpawnedNPCs")
                .and_then(Value::as_i64)
                .unwrap_or(0) as i32,
            concurrent_spawns_range: i32_pair(&raw, "ConcurrentSpawnsRange").unwrap_or([1, 1]),
            spawn_after_game_seconds: duration_pair(&raw, "SpawnAfterGameTimeRange"),
            spawn_after_real_seconds: duration_pair(&raw, "SpawnAfterRealTimeRange"),
            initial_spawn_delay_seconds: f64_pair(&raw, "InitialSpawnDelayRange"),
            spawn_radius: raw
                .get("SpawnRadius")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            beacon_radius: raw
                .get("BeaconRadius")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            raw,
        })
    }

    /// Pick a weighted role.
    pub fn pick_role<R: FlockRng>(&self, rng: &mut R) -> Option<&RoleSpawnParameters> {
        pick_weighted(&self.npcs, |role| role.weight, rng)
    }

    /// Java `SpawnWrapper.spawnParametersMatch` time/moon gate.
    pub fn is_eligible(&self, clock: &WorldClock) -> bool {
        WorldSpawn {
            key: self.key.clone(),
            environments: self.environments.clone(),
            npcs: self.npcs.clone(),
            despawn: self.despawn,
            day_time_range: self.day_time_range,
            moon_phase_range: self.moon_phase_range,
            light_ranges: self.light_ranges.clone(),
            scale_day_time_range: self.scale_day_time_range,
            moon_phase_weight_modifiers: None,
            raw: Value::Null,
        }
        .is_eligible(clock)
    }
}

/// One marker NPC configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerNpcConfig {
    /// Role key (`Name`).
    pub name: String,
    /// Relative weight.
    pub weight: f64,
    /// Realtime respawn seconds.
    pub realtime_respawn_seconds: f64,
    /// Game-time respawn seconds (`SpawnAfterGameTime`).
    pub game_respawn_seconds: Option<f64>,
    /// Optional flock.
    pub flock: Option<SpawnFlock>,
}

/// One `SpawnMarker` document.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnMarker {
    /// Asset key.
    pub key: String,
    /// Optional marker model.
    pub model: Option<String>,
    /// Weighted NPC configurations.
    pub npcs: Vec<MarkerNpcConfig>,
    /// Player exclusion radius.
    pub exclusion_radius: f64,
    /// Maximum drop height.
    pub max_drop_height: f64,
    /// Whether respawn uses real time.
    pub realtime_respawn: bool,
    /// Manual trigger only.
    pub manual_trigger: bool,
    /// Deactivation distance.
    pub deactivation_distance: f64,
    /// Deactivation delay.
    pub deactivation_time: f64,
    /// The source document.
    pub raw: Value,
}

impl SpawnMarker {
    /// Parse one marker document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Result<Self, SpawnSpecialError> {
        let key = key.into();
        let npcs = raw
            .get("NPCs")
            .and_then(Value::as_array)
            .ok_or_else(|| SpawnSpecialError::InvalidField {
                key: key.clone(),
                field: "NPCs".to_string(),
            })?
            .iter()
            .map(|entry| MarkerNpcConfig {
                name: entry
                    .get("Name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                weight: entry.get("Weight").and_then(Value::as_f64).unwrap_or(0.0),
                realtime_respawn_seconds: entry
                    .get("RealtimeRespawnTime")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                game_respawn_seconds: entry
                    .get("SpawnAfterGameTime")
                    .and_then(Value::as_str)
                    .and_then(parse_iso_duration),
                flock: entry.get("Flock").and_then(|flock| match flock {
                    Value::String(asset) => Some(SpawnFlock::Asset(asset.clone())),
                    value @ Value::Object(_) => FlockSize::from_json(&key, value)
                        .ok()
                        .map(SpawnFlock::Inline),
                    _ => None,
                }),
            })
            .collect();
        Ok(Self {
            key,
            model: raw
                .get("Model")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            npcs,
            exclusion_radius: raw
                .get("ExclusionRadius")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            max_drop_height: raw
                .get("MaxDropHeight")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            realtime_respawn: raw
                .get("RealtimeRespawn")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            manual_trigger: raw
                .get("ManualTrigger")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            deactivation_distance: raw
                .get("DeactivationDistance")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            deactivation_time: raw
                .get("DeactivationTime")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
            raw,
        })
    }

    /// Pick a weighted marker configuration.
    pub fn pick_config<R: FlockRng>(&self, rng: &mut R) -> Option<&MarkerNpcConfig> {
        pick_weighted(&self.npcs, |config| config.weight, rng)
    }

    /// Respawn cooldown for `config` in the marker's clock mode.
    pub fn cooldown_seconds(&self, config: &MarkerNpcConfig) -> Option<f64> {
        if self.realtime_respawn {
            Some(config.realtime_respawn_seconds)
        } else {
            config.game_respawn_seconds
        }
    }
}

/// All beacon definitions.
#[derive(Debug, Clone, Default)]
pub struct BeaconCatalog {
    beacons: BTreeMap<String, BeaconSpawn>,
}

impl BeaconCatalog {
    /// Load `Server/NPC/Spawn/Beacons/**/*.json`.
    pub fn load(pack: &AssetPack) -> Result<Self, SpawnSpecialError> {
        load_catalog(pack, "Server/NPC/Spawn/Beacons/", BeaconSpawn::from_json)
            .map(|beacons| Self { beacons })
    }

    /// Load from an extracted root.
    pub fn load_root(root: &Path) -> Result<Self, SpawnSpecialError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One beacon by key.
    pub fn get(&self, key: &str) -> Option<&BeaconSpawn> {
        self.beacons.get(key)
    }

    /// Number of beacons.
    pub fn len(&self) -> usize {
        self.beacons.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.beacons.is_empty()
    }

    /// Beacons in key order.
    pub fn iter(&self) -> impl Iterator<Item = &BeaconSpawn> {
        self.beacons.values()
    }
}

/// All spawn-marker definitions.
#[derive(Debug, Clone, Default)]
pub struct SpawnMarkerCatalog {
    markers: BTreeMap<String, SpawnMarker>,
}

impl SpawnMarkerCatalog {
    /// Load `Server/NPC/Spawn/Markers/**/*.json`.
    pub fn load(pack: &AssetPack) -> Result<Self, SpawnSpecialError> {
        load_catalog(pack, "Server/NPC/Spawn/Markers/", SpawnMarker::from_json)
            .map(|markers| Self { markers })
    }

    /// Load from an extracted root.
    pub fn load_root(root: &Path) -> Result<Self, SpawnSpecialError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One marker by key.
    pub fn get(&self, key: &str) -> Option<&SpawnMarker> {
        self.markers.get(key)
    }

    /// Number of markers.
    pub fn len(&self) -> usize {
        self.markers.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.markers.is_empty()
    }

    /// Markers in key order.
    pub fn iter(&self) -> impl Iterator<Item = &SpawnMarker> {
        self.markers.values()
    }
}

fn load_catalog<T>(
    pack: &AssetPack,
    prefix: &str,
    parse: impl Fn(String, Value) -> Result<T, SpawnSpecialError>,
) -> Result<BTreeMap<String, T>, SpawnSpecialError> {
    let mut catalog = BTreeMap::new();
    let paths: Vec<String> = pack
        .list()?
        .into_iter()
        .filter(|path| path.starts_with(prefix) && path.ends_with(".json"))
        .collect();
    // One zip open for the whole catalog: `read` reopens the 3.7 GB pack per
    // file and dominates world boot.
    let contents = pack.read_many(&paths)?;
    for (path, bytes) in paths.into_iter().zip(contents) {
        let key = path
            .rsplit('/')
            .next()
            .and_then(|name| name.strip_suffix(".json"))
            .ok_or_else(|| SpawnSpecialError::InvalidPath(path.clone()))?
            .to_string();
        let raw: Value = serde_json::from_slice(&bytes)?;
        catalog.insert(key.clone(), parse(key, raw)?);
    }
    Ok(catalog)
}

fn parse_roles(key: &str, raw: &Value) -> Result<Vec<RoleSpawnParameters>, SpawnSpecialError> {
    raw.get("NPCs")
        .and_then(Value::as_array)
        .ok_or_else(|| SpawnSpecialError::InvalidField {
            key: key.to_string(),
            field: "NPCs".to_string(),
        })?
        .iter()
        .map(|entry| {
            RoleSpawnParameters::from_json(key, entry).map_err(|error| {
                SpawnSpecialError::InvalidField {
                    key: key.to_string(),
                    field: error.to_string(),
                }
            })
        })
        .collect()
}

fn pick_weighted<'a, T, R: FlockRng>(
    entries: &'a [T],
    weight: impl Fn(&T) -> f64,
    rng: &mut R,
) -> Option<&'a T> {
    let sum: f64 = entries.iter().map(&weight).sum();
    if sum <= 0.0 {
        return None;
    }
    let mut remaining = rng.next_f64() * sum;
    let mut result = None;
    for entry in entries {
        result = Some(entry);
        remaining -= weight(entry);
        if remaining < 0.0 {
            break;
        }
    }
    result
}

/// Parse an ISO-8601 duration (`P1D`, `PT5M`, `PT1H30M`, ...).
pub fn parse_iso_duration(text: &str) -> Option<f64> {
    let text = text.trim();
    let chars = text.strip_prefix('P')?.chars();
    let mut total = 0.0;
    let mut in_time = false;
    let mut number = String::new();
    for character in chars {
        if character == 'T' {
            in_time = true;
            continue;
        }
        if character.is_ascii_digit() || character == '.' {
            number.push(character);
            continue;
        }
        let value: f64 = number.parse().ok()?;
        number.clear();
        total += match character {
            'D' if !in_time => value * 86_400.0,
            'H' if in_time => value * 3_600.0,
            'M' if in_time => value * 60.0,
            'S' if in_time => value,
            _ => return None,
        };
    }
    if !number.is_empty() {
        return None;
    }
    Some(total)
}

fn string_array(raw: &Value, key: &str) -> Vec<String> {
    raw.get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn f64_pair(raw: &Value, key: &str) -> Option<[f64; 2]> {
    let values = raw.get(key)?.as_array()?;
    Some([values.first()?.as_f64()?, values.get(1)?.as_f64()?])
}

fn i32_pair(raw: &Value, key: &str) -> Option<[i32; 2]> {
    let values = raw.get(key)?.as_array()?;
    Some([
        i32::try_from(values.first()?.as_i64()?).ok()?,
        i32::try_from(values.get(1)?.as_i64()?).ok()?,
    ])
}

fn duration_pair(raw: &Value, key: &str) -> Option<[f64; 2]> {
    raw.get(key)?
        .as_array()?
        .iter()
        .map(|value| value.as_str().and_then(parse_iso_duration))
        .collect::<Option<Vec<_>>>()?
        .try_into()
        .ok()
}

fn light_range_map(raw: &Value) -> BTreeMap<String, [f64; 2]> {
    raw.get("LightRanges")
        .and_then(Value::as_object)
        .map(|ranges| {
            ranges
                .iter()
                .filter_map(|(name, value)| {
                    let values = value.as_array()?;
                    Some((
                        name.clone(),
                        [values.first()?.as_f64()?, values.get(1)?.as_f64()?],
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One role selected by a live spawn marker.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerSpawnRequest {
    /// Marker asset key.
    pub marker: String,
    /// Selected role id.
    pub role: String,
    /// Respawn cooldown applied after this spawn.
    pub cooldown_seconds: Option<f64>,
    /// Last config key (role) selected.
    pub config: String,
}

/// Live state for one spawn-marker entity.
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnMarkerController {
    /// Marker asset key.
    pub key: String,
    /// Marker entity position.
    pub position: Vec3,
    /// Remaining cooldown in the marker's clock mode.
    pub cooldown_remaining: f64,
    /// Whether the remaining cooldown is game time (otherwise real time).
    pub cooldown_game_time: bool,
    /// Last selected NPC role, for diagnostics/persistence.
    pub last_role: Option<String>,
}

impl SpawnMarkerController {
    /// Create a controller at `position`. `initial_cooldown` restores persisted
    /// state.
    pub fn new(key: impl Into<String>, position: Vec3, initial_cooldown: f64) -> Self {
        Self {
            key: key.into(),
            position,
            cooldown_remaining: initial_cooldown.max(0.0),
            cooldown_game_time: false,
            last_role: None,
        }
    }

    /// Advance the cooldown. `game_dt` and `real_dt` are the elapsed game/real
    /// seconds for this tick.
    pub fn tick_cooldown(&mut self, game_dt: f64, real_dt: f64) {
        if self.cooldown_remaining <= 0.0 {
            return;
        }
        let dt = if self.cooldown_game_time {
            game_dt
        } else {
            real_dt
        };
        self.cooldown_remaining = (self.cooldown_remaining - dt.max(0.0)).max(0.0);
    }

    /// Try an automatic marker trigger. Non-manual markers fire when a player is
    /// inside `DeactivationDistance` (or always when no distance is set).
    pub fn tick<R: FlockRng>(
        &mut self,
        marker: &SpawnMarker,
        players: &[Vec3],
        suppressed: bool,
        game_dt: f64,
        real_dt: f64,
        rng: &mut R,
    ) -> Option<MarkerSpawnRequest> {
        self.tick_cooldown(game_dt, real_dt);
        if suppressed || marker.manual_trigger || self.cooldown_remaining > 0.0 {
            return None;
        }
        if marker.deactivation_distance > 0.0
            && !players.iter().any(|player| {
                distance_squared(*player, self.position)
                    <= marker.deactivation_distance * marker.deactivation_distance
            })
        {
            return None;
        }
        self.trigger(marker, rng)
    }

    /// Explicit/manual trigger. The caller still owns role spawning and should
    /// respect `suppressed` separately.
    pub fn trigger<R: FlockRng>(
        &mut self,
        marker: &SpawnMarker,
        rng: &mut R,
    ) -> Option<MarkerSpawnRequest> {
        if self.cooldown_remaining > 0.0 || marker.npcs.is_empty() {
            return None;
        }
        let config = marker.pick_config(rng)?;
        let cooldown = marker.cooldown_seconds(config);
        self.cooldown_remaining = cooldown.unwrap_or(0.0).max(0.0);
        self.cooldown_game_time = !marker.realtime_respawn;
        self.last_role = Some(config.name.clone());
        Some(MarkerSpawnRequest {
            marker: marker.key.clone(),
            role: config.name.clone(),
            cooldown_seconds: cooldown,
            config: config.name.clone(),
        })
    }

    /// Persisted controller state.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "Key": self.key,
            "Position": [self.position.x, self.position.y, self.position.z],
            "Cooldown": self.cooldown_remaining,
            "CooldownGameTime": self.cooldown_game_time,
            "LastRole": self.last_role,
        })
    }

    /// Restore state written by [`Self::to_json`].
    pub fn apply_json(&mut self, value: &Value) {
        if let Some(position) = value.get("Position").and_then(Value::as_array) {
            if position.len() == 3 {
                self.position = Vec3::new(
                    position[0].as_f64().unwrap_or(self.position.x),
                    position[1].as_f64().unwrap_or(self.position.y),
                    position[2].as_f64().unwrap_or(self.position.z),
                );
            }
        }
        self.cooldown_remaining = value
            .get("Cooldown")
            .and_then(Value::as_f64)
            .unwrap_or(self.cooldown_remaining)
            .max(0.0);
        self.cooldown_game_time = value
            .get("CooldownGameTime")
            .and_then(Value::as_bool)
            .unwrap_or(self.cooldown_game_time);
        self.last_role = value
            .get("LastRole")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
    }
}

/// One batch of roles selected by a live beacon.
#[derive(Debug, Clone, PartialEq)]
pub struct BeaconSpawnRequest {
    /// Beacon asset key.
    pub beacon: String,
    /// Spawn center.
    pub position: Vec3,
    /// Roles to spawn this tick.
    pub roles: Vec<String>,
    /// Respawn cooldown applied after this batch.
    pub cooldown_seconds: Option<f64>,
}

/// Live state for one beacon spawner.
#[derive(Debug, Clone, PartialEq)]
pub struct BeaconController {
    /// Beacon asset key.
    pub key: String,
    /// Beacon world position.
    pub position: Vec3,
    /// Remaining initial/spawn cooldown.
    pub cooldown_remaining: f64,
    /// Whether the cooldown uses game time.
    pub cooldown_game_time: bool,
    /// NPCs currently tracked as spawned by this beacon.
    pub spawned: i32,
}

impl BeaconController {
    /// Create a controller with the Java initial spawn delay.
    pub fn new<R: FlockRng>(
        key: impl Into<String>,
        position: Vec3,
        beacon: &BeaconSpawn,
        rng: &mut R,
    ) -> Self {
        let initial = beacon
            .initial_spawn_delay_seconds
            .map(|range| sample_range(range, rng))
            .unwrap_or(0.0);
        Self {
            key: key.into(),
            position,
            cooldown_remaining: initial.max(0.0),
            cooldown_game_time: beacon.spawn_after_real_seconds.is_none(),
            spawned: 0,
        }
    }

    /// Restore a controller from persisted state.
    pub fn from_state(key: impl Into<String>, position: Vec3, value: &Value) -> Self {
        Self {
            key: key.into(),
            position,
            cooldown_remaining: value
                .get("Cooldown")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .max(0.0),
            cooldown_game_time: value
                .get("CooldownGameTime")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            spawned: value
                .get("Spawned")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0) as i32,
        }
    }

    /// Advance cooldowns and emit a requested batch when eligible.
    #[allow(clippy::too_many_arguments)]
    pub fn tick<R: FlockRng>(
        &mut self,
        beacon: &BeaconSpawn,
        clock: &WorldClock,
        game_dt: f64,
        real_dt: f64,
        rng: &mut R,
    ) -> Option<BeaconSpawnRequest> {
        if self.cooldown_remaining > 0.0 {
            let dt = if self.cooldown_game_time {
                game_dt
            } else {
                real_dt
            };
            self.cooldown_remaining = (self.cooldown_remaining - dt.max(0.0)).max(0.0);
        }
        if self.cooldown_remaining > 0.0
            || !beacon.is_eligible(clock)
            || (beacon.max_spawned_npcs > 0 && self.spawned >= beacon.max_spawned_npcs)
        {
            return None;
        }
        let [min, max] = beacon.concurrent_spawns_range;
        let desired = if max <= min {
            min.max(0) as usize
        } else {
            (min + ((max - min) as f64 * rng.next_f64()).floor() as i32).max(0) as usize
        };
        let capacity = if beacon.max_spawned_npcs > 0 {
            (beacon.max_spawned_npcs - self.spawned).max(0) as usize
        } else {
            desired
        };
        let count = desired.min(capacity);
        if count == 0 {
            return None;
        }
        let mut roles = Vec::new();
        for _ in 0..count {
            if let Some(role) = beacon.pick_role(rng) {
                roles.push(role.id.clone());
            }
        }
        if roles.is_empty() {
            return None;
        }
        let cooldown = beacon
            .spawn_after_game_seconds
            .or(beacon.spawn_after_real_seconds)
            .map(|range| sample_range(range, rng));
        self.cooldown_game_time = beacon.spawn_after_game_seconds.is_some();
        self.cooldown_remaining = cooldown.unwrap_or(0.0).max(0.0);
        self.spawned += roles.len() as i32;
        Some(BeaconSpawnRequest {
            beacon: beacon.key.clone(),
            position: self.position,
            roles,
            cooldown_seconds: cooldown,
        })
    }

    /// Record a spawned NPC count.
    pub fn record_spawn(&mut self, count: usize) {
        self.spawned += count as i32;
    }

    /// Record a despawned NPC.
    pub fn record_despawn(&mut self, count: usize) {
        self.spawned = (self.spawned - count as i32).max(0);
    }

    /// Persisted controller state.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "Key": self.key,
            "Position": [self.position.x, self.position.y, self.position.z],
            "Cooldown": self.cooldown_remaining,
            "CooldownGameTime": self.cooldown_game_time,
            "Spawned": self.spawned,
        })
    }
}

fn sample_range<R: FlockRng>(range: [f64; 2], rng: &mut R) -> f64 {
    let [low, high] = range;
    low + (high - low) * rng.next_f64().clamp(0.0, 1.0)
}

fn distance_squared(left: Vec3, right: Vec3) -> f64 {
    let dx = right.x - left.x;
    let dy = right.y - left.y;
    let dz = right.z - left.z;
    dx * dx + dy * dy + dz * dz
}

/// Beacon/marker loading errors.
#[derive(Debug, Error)]
pub enum SpawnSpecialError {
    /// Asset pack access failed.
    #[error(transparent)]
    Pack(#[from] hytale_assets::AssetError),
    /// A document is not valid JSON.
    #[error("spawn document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A required field is missing or invalid.
    #[error("spawn `{key}` has an invalid `{field}`")]
    InvalidField {
        /// Asset key.
        key: String,
        /// Field name/error.
        field: String,
    },
    /// A nested world-spawn field failed to parse.
    #[error(transparent)]
    Spawn(#[from] crate::spawn::SpawnError),
    /// An asset path had no file name.
    #[error("invalid spawn asset path `{0}`")]
    InvalidPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedRng(f64);

    impl FlockRng for FixedRng {
        fn next_f64(&mut self) -> f64 {
            self.0
        }
    }

    #[test]
    fn iso_durations_cover_the_marker_forms() {
        assert_eq!(parse_iso_duration("P1D"), Some(86_400.0));
        assert_eq!(parse_iso_duration("PT5M"), Some(300.0));
        assert_eq!(parse_iso_duration("PT1H30M"), Some(5_400.0));
        assert_eq!(parse_iso_duration("PT10S"), Some(10.0));
        assert_eq!(parse_iso_duration("nope"), None);
    }

    #[test]
    fn weighted_marker_pick_uses_java_boundaries() {
        let marker = SpawnMarker {
            key: "Test".to_string(),
            model: None,
            npcs: vec![
                MarkerNpcConfig {
                    name: "A".to_string(),
                    weight: 1.0,
                    realtime_respawn_seconds: 5.0,
                    game_respawn_seconds: Some(1.0),
                    flock: None,
                },
                MarkerNpcConfig {
                    name: "B".to_string(),
                    weight: 3.0,
                    realtime_respawn_seconds: 10.0,
                    game_respawn_seconds: Some(2.0),
                    flock: None,
                },
            ],
            exclusion_radius: 0.0,
            max_drop_height: 0.0,
            realtime_respawn: false,
            manual_trigger: false,
            deactivation_distance: 0.0,
            deactivation_time: 0.0,
            raw: Value::Null,
        };
        assert_eq!(
            marker
                .pick_config(&mut FixedRng(0.0))
                .map(|c| c.name.as_str()),
            Some("A")
        );
        assert_eq!(
            marker
                .pick_config(&mut FixedRng(0.25))
                .map(|c| c.name.as_str()),
            Some("B")
        );
        assert_eq!(marker.cooldown_seconds(&marker.npcs[1]), Some(2.0));
    }
}
