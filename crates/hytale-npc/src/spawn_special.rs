//! Beacon and spawn-marker assets.
//!
//! `Server/NPC/Spawn/Beacons` and `Server/NPC/Spawn/Markers` are separate
//! asset stores from the world env spawns. This slice loads both catalogs,
//! resolves their weighted NPC/configuration entries and parses the ISO-8601
//! duration fields used for respawn timers. Live beacon/marker controllers and
//! their persistent state are a later T59b slice.

use std::collections::BTreeMap;
use std::path::Path;

use hytale_assets::AssetPack;
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
    for path in pack.list()? {
        if !path.starts_with(prefix) || !path.ends_with(".json") {
            continue;
        }
        let key = path
            .rsplit('/')
            .next()
            .and_then(|name| name.strip_suffix(".json"))
            .ok_or_else(|| SpawnSpecialError::InvalidPath(path.clone()))?
            .to_string();
        let raw: Value = serde_json::from_slice(&pack.read(&path)?)?;
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
