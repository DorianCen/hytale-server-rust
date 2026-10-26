//! `Server/NPC/Spawn/World` spawning configurations.
//!
//! The Java spawning module has a large runtime (chunk tracking, suppression,
//! candidate searches, beacons and markers). This slice ports the deterministic
//! configuration and selection core:
//!
//! * [`WorldSpawnCatalog`] loads the world spawn documents.
//! * [`WorldSpawn`] keeps the day/moon/light gates and the per-role weighted
//!   entries.
//! * [`RoleSpawnParameters`] keeps role weight, block/fluid restrictions, flock
//!   and movement-mode data.
//! * Eligibility and moon weighting mirror `WorldSpawnWrapper`/
//!   `WorldNPCSpawnStat`; weighted selection mirrors
//!   `RandomExtra.randomWeightedElement`.
//!
//! Position search, beacon/marker controllers and the ECS spawn producer remain
//! T59b.

use std::collections::BTreeMap;
use std::path::Path;

use hytale_assets::AssetPack;
use hytale_world::world_clock::WorldClock;
use serde_json::Value;
use thiserror::Error;

use crate::flock::{FlockCatalog, FlockError, FlockRng, FlockSize};

/// Java `NPCSpawn.HOURS_PER_DAY`.
pub const HOURS_PER_DAY: f64 = 24.0;
/// Java `NPCSpawn.DEFAULT_DAY_TIME_RANGE` normalized to hours.
pub const DEFAULT_DAY_TIME_RANGE: [f64; 2] = [0.0, HOURS_PER_DAY];
/// Java `NPCSpawn.DEFAULT_MOON_PHASE_RANGE`.
pub const DEFAULT_MOON_PHASE_RANGE: [i32; 2] = [0, i32::MAX];

/// One spawning configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSpawn {
    /// Asset key (file stem).
    pub key: String,
    /// Environment ids this configuration applies to.
    pub environments: Vec<String>,
    /// Weighted role entries.
    pub npcs: Vec<RoleSpawnParameters>,
    /// Optional despawn time/moon window.
    pub despawn: Option<SpawnTimeWindow>,
    /// Spawn time range in in-game hours.
    pub day_time_range: [f64; 2],
    /// Spawn moon-phase range.
    pub moon_phase_range: [i32; 2],
    /// Optional per-light-type ranges (`0..100`).
    pub light_ranges: BTreeMap<String, [f64; 2]>,
    /// Java `ScaleDayTimeRange` (defaults to `true`).
    pub scale_day_time_range: bool,
    /// Per-moon-phase weight multipliers (`None` = all phases at `1`).
    pub moon_phase_weight_modifiers: Option<Vec<f64>>,
    /// The source document.
    pub raw: Value,
}

impl WorldSpawn {
    /// Parse one `WorldNPCSpawn` document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Result<Self, SpawnError> {
        let key = key.into();
        let environments = raw
            .get("Environments")
            .and_then(Value::as_array)
            .ok_or_else(|| SpawnError::InvalidField {
                key: key.clone(),
                field: "Environments",
            })?
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let npcs = raw
            .get("NPCs")
            .and_then(Value::as_array)
            .ok_or_else(|| SpawnError::InvalidField {
                key: key.clone(),
                field: "NPCs",
            })?
            .iter()
            .map(|entry| RoleSpawnParameters::from_json(&key, entry))
            .collect::<Result<Vec<_>, _>>()?;

        let day_time_range = f64_pair(raw.get("DayTimeRange")).unwrap_or(DEFAULT_DAY_TIME_RANGE);
        let moon_phase_range =
            i32_pair(raw.get("MoonPhaseRange")).unwrap_or(DEFAULT_MOON_PHASE_RANGE);
        let light_ranges = raw
            .get("LightRanges")
            .and_then(Value::as_object)
            .map(|ranges| {
                ranges
                    .iter()
                    .filter_map(|(name, value)| {
                        f64_pair(Some(value)).map(|range| (name.clone(), range))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let despawn = raw
            .get("Despawn")
            .map(|value| SpawnTimeWindow::from_json(&key, value))
            .transpose()?;
        let moon_phase_weight_modifiers = raw
            .get("MoonPhaseWeightModifiers")
            .and_then(Value::as_array)
            .map(|modifiers| {
                modifiers
                    .iter()
                    .map(|value| value.as_f64().unwrap_or(0.0))
                    .collect()
            });

        Ok(Self {
            key,
            environments,
            npcs,
            despawn,
            day_time_range,
            moon_phase_range,
            light_ranges,
            scale_day_time_range: raw
                .get("ScaleDayTimeRange")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            moon_phase_weight_modifiers,
            raw,
        })
    }

    /// Java `SpawnWrapper.spawnParametersMatch` time/moon portion.
    pub fn is_within_time_and_moon(&self, clock: &WorldClock) -> bool {
        let day_range = normalize_hours(self.day_time_range);
        let time_matches = if self.scale_day_time_range {
            clock.is_scaled_day_time_within_range(day_range[0], day_range[1])
        } else {
            clock.is_day_time_within_range(day_range[0], day_range[1])
        };
        time_matches
            && clock.is_moon_phase_within_range(self.moon_phase_range[0], self.moon_phase_range[1])
    }

    /// Alias matching the spawning runtime's term.
    pub fn is_eligible(&self, clock: &WorldClock) -> bool {
        self.is_within_time_and_moon(clock)
    }

    /// Java `SpawnWrapper.shouldDespawn` time/moon portion.
    pub fn is_despawn_due(&self, clock: &WorldClock) -> bool {
        let Some(despawn) = &self.despawn else {
            return false;
        };
        let day_range = normalize_hours(despawn.day_time_range);
        let time_matches = if self.scale_day_time_range {
            clock.is_scaled_day_time_within_range(day_range[0], day_range[1])
        } else {
            clock.is_day_time_within_range(day_range[0], day_range[1])
        };
        time_matches
            && clock.is_moon_phase_within_range(
                despawn.moon_phase_range[0],
                despawn.moon_phase_range[1],
            )
    }

    /// Java `WorldSpawnWrapper.getMoonPhaseWeightModifier`.
    pub fn moon_phase_weight_modifier(&self, moon_phase: i32) -> f64 {
        let Some(modifiers) = &self.moon_phase_weight_modifiers else {
            return 1.0;
        };
        if moon_phase < 0 || moon_phase as usize >= modifiers.len() {
            return 0.0;
        }
        modifiers[moon_phase as usize]
    }

    /// Java `WorldNPCSpawnStat.getWeight`.
    pub fn effective_weight(&self, role: &RoleSpawnParameters, moon_phase: i32) -> f64 {
        role.weight * self.moon_phase_weight_modifier(moon_phase)
    }

    /// Pick one role with Java's weighted-selection algorithm.
    pub fn pick_role<R: FlockRng>(
        &self,
        rng: &mut R,
        moon_phase: i32,
    ) -> Option<&RoleSpawnParameters> {
        let sum: f64 = self
            .npcs
            .iter()
            .map(|role| self.effective_weight(role, moon_phase))
            .sum();
        if sum <= 0.0 {
            return None;
        }
        let mut remaining = rng.next_f64() * sum;
        let mut result = None;
        for role in &self.npcs {
            result = Some(role);
            remaining -= self.effective_weight(role, moon_phase);
            if remaining < 0.0 {
                break;
            }
        }
        result
    }

    /// Java `WorldNPCSpawnStat.getMissingCount` without the live spawnable
    /// validation: the number of slots left, gated by the flock's minimum size.
    pub fn missing_count(
        &self,
        role: &RoleSpawnParameters,
        actual: i32,
        expected: f64,
        flocks: &FlockCatalog,
    ) -> f64 {
        let slots_left = (expected - f64::from(actual)).max(0.0);
        if slots_left.ceil() < f64::from(role.min_spawn_size(flocks)) {
            0.0
        } else {
            slots_left
        }
    }

    /// Java `WorldEnvironmentSpawnData.spawnWeight`.
    pub fn spawn_weight(expected: f64, actual: i32) -> f64 {
        (expected - f64::from(actual)).max(0.0)
    }
}

/// One role spawn entry (`RoleSpawnParameters`).
#[derive(Debug, Clone, PartialEq)]
pub struct RoleSpawnParameters {
    /// Role id.
    pub id: String,
    /// Relative weight (`Weight`).
    pub weight: f64,
    /// Optional `SpawnBlockSet`.
    pub spawn_block_set: Option<String>,
    /// Optional `SpawnFluidTag`.
    pub spawn_fluid_tag: Option<String>,
    /// Optional movement-mode weights.
    pub movement_modes: BTreeMap<String, f64>,
    /// Java `EnableSafeSpawning` (default `true`).
    pub enable_safe_spawning: bool,
    /// Optional flock asset reference or inline size rule.
    pub flock: Option<SpawnFlock>,
}

impl RoleSpawnParameters {
    /// Parse one `NPCs` entry.
    pub fn from_json(key: &str, raw: &Value) -> Result<Self, SpawnError> {
        let id = raw
            .get("Id")
            .and_then(Value::as_str)
            .ok_or_else(|| SpawnError::InvalidField {
                key: key.to_string(),
                field: "NPCs.Id",
            })?
            .to_string();
        let weight =
            raw.get("Weight")
                .and_then(Value::as_f64)
                .ok_or_else(|| SpawnError::InvalidField {
                    key: key.to_string(),
                    field: "NPCs.Weight",
                })?;
        let movement_modes = raw
            .get("MovementModes")
            .and_then(Value::as_object)
            .map(|modes| {
                modes
                    .iter()
                    .filter_map(|(mode, weight)| {
                        weight.as_f64().map(|weight| (mode.clone(), weight))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let flock = match raw.get("Flock") {
            Some(Value::String(asset)) => Some(SpawnFlock::Asset(asset.clone())),
            Some(value @ Value::Object(_)) => {
                Some(SpawnFlock::Inline(FlockSize::from_json(key, value)?))
            }
            _ => None,
        };
        Ok(Self {
            id,
            weight,
            spawn_block_set: raw
                .get("SpawnBlockSet")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            spawn_fluid_tag: raw
                .get("SpawnFluidTag")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            movement_modes,
            enable_safe_spawning: raw
                .get("EnableSafeSpawning")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            flock,
        })
    }

    /// Java `RoleSpawnParameters.getFlockDefinition` minimum size.
    pub fn min_spawn_size(&self, flocks: &FlockCatalog) -> i32 {
        match &self.flock {
            None => 1,
            Some(SpawnFlock::Inline(size)) => size.min_size(),
            Some(SpawnFlock::Asset(key)) => {
                flocks.get(key).map(|asset| asset.min_size()).unwrap_or(1)
            }
        }
    }
}

/// A flock reference or an inline size rule.
#[derive(Debug, Clone, PartialEq)]
pub enum SpawnFlock {
    /// `Server/NPC/Flocks/<key>`.
    Asset(String),
    /// Inline `Size`/`Type` document.
    Inline(FlockSize),
}

/// A spawn or despawn time/moon window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnTimeWindow {
    /// In-game hours.
    pub day_time_range: [f64; 2],
    /// Moon phase range.
    pub moon_phase_range: [i32; 2],
}

impl SpawnTimeWindow {
    pub(crate) fn from_json(key: &str, raw: &Value) -> Result<Self, SpawnError> {
        if !raw.is_object() {
            return Err(SpawnError::InvalidField {
                key: key.to_string(),
                field: "Despawn",
            });
        }
        Ok(Self {
            day_time_range: f64_pair(raw.get("DayTimeRange")).unwrap_or(DEFAULT_DAY_TIME_RANGE),
            moon_phase_range: i32_pair(raw.get("MoonPhaseRange"))
                .unwrap_or(DEFAULT_MOON_PHASE_RANGE),
        })
    }
}

/// All world spawn configurations keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct WorldSpawnCatalog {
    spawns: BTreeMap<String, WorldSpawn>,
}

impl WorldSpawnCatalog {
    /// Load `Server/NPC/Spawn/World/**/*.json`.
    pub fn load(pack: &AssetPack) -> Result<Self, SpawnError> {
        let mut catalog = Self::default();
        for path in pack.list()? {
            if !path.starts_with("Server/NPC/Spawn/World/") || !path.ends_with(".json") {
                continue;
            }
            let key = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| SpawnError::InvalidPath(path.clone()))?
                .to_string();
            let raw: Value = serde_json::from_slice(&pack.read(&path)?)?;
            catalog
                .spawns
                .insert(key.clone(), WorldSpawn::from_json(key, raw)?);
        }
        Ok(catalog)
    }

    /// Load from an extracted asset root.
    pub fn load_root(root: &Path) -> Result<Self, SpawnError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One configuration by key.
    pub fn get(&self, key: &str) -> Option<&WorldSpawn> {
        self.spawns.get(key)
    }

    /// Number of configurations.
    pub fn len(&self) -> usize {
        self.spawns.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.spawns.is_empty()
    }

    /// Configurations in key order.
    pub fn iter(&self) -> impl Iterator<Item = &WorldSpawn> {
        self.spawns.values()
    }

    /// Every configuration that lists `environment`.
    pub fn for_environment<'a>(
        &'a self,
        environment: &'a str,
    ) -> impl Iterator<Item = &'a WorldSpawn> + 'a {
        self.spawns
            .values()
            .filter(move |spawn| spawn.environments.iter().any(|value| value == environment))
    }

    /// Pick an eligible `(configuration, role)` for `environment`.
    ///
    /// Weights are the role weights multiplied by the moon-phase modifier,
    /// matching `WorldNPCSpawnStat.getWeight`; configs outside their day/moon
    /// window are ignored.
    pub fn pick_spawn<'a, R: FlockRng>(
        &'a self,
        environment: &'a str,
        clock: &WorldClock,
        moon_phase: i32,
        rng: &mut R,
    ) -> Option<(&'a WorldSpawn, &'a RoleSpawnParameters)> {
        let entries: Vec<_> = self
            .for_environment(environment)
            .filter(|spawn| spawn.is_eligible(clock))
            .flat_map(|spawn| {
                spawn
                    .npcs
                    .iter()
                    .map(move |role| (spawn, role, spawn.effective_weight(role, moon_phase)))
            })
            .filter(|(_, _, weight)| *weight > 0.0)
            .collect();
        let sum: f64 = entries.iter().map(|(_, _, weight)| *weight).sum();
        if sum <= 0.0 {
            return None;
        }
        let mut remaining = rng.next_f64() * sum;
        let mut result = None;
        for (spawn, role, weight) in entries {
            result = Some((spawn, role));
            remaining -= weight;
            if remaining < 0.0 {
                break;
            }
        }
        result
    }
}

/// `WorldEnvironmentSpawnData` accounting subset.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSpawnController {
    /// Environment spawn density (`Environment.SpawnDensity`).
    pub density: f64,
    /// Loaded 32³ segments/columns in this environment.
    pub segment_count: usize,
    /// Live NPCs tracked in this environment.
    pub actual: i32,
}

impl WorldSpawnController {
    /// Java `WorldEnvironmentSpawnData.K_COLUMNS`.
    pub const K_COLUMNS: f64 = 1024.0;

    /// An empty controller with the environment density.
    pub fn new(density: f64) -> Self {
        Self {
            density: density.max(0.0),
            segment_count: 0,
            actual: 0,
        }
    }

    /// Change the density and recompute the expected count.
    pub fn set_density(&mut self, density: f64) {
        self.density = density.max(0.0);
    }

    /// Change the tracked segment/column count.
    pub fn set_segment_count(&mut self, count: usize) {
        self.segment_count = count;
    }

    /// Java `WorldEnvironmentSpawnData.expectedNPCs`.
    pub fn expected(&self) -> f64 {
        self.segment_count as f64 * self.density / Self::K_COLUMNS
    }

    /// Java `WorldEnvironmentSpawnData.spawnWeight`.
    pub fn missing(&self) -> f64 {
        (self.expected() - f64::from(self.actual)).max(0.0)
    }

    /// Whether another spawn is allowed.
    pub fn has_room(&self) -> bool {
        self.missing() > 0.0
    }

    /// Track one spawn.
    pub fn record_spawn(&mut self) {
        self.actual += 1;
    }

    /// Track one despawn.
    pub fn record_despawn(&mut self) {
        self.actual = (self.actual - 1).max(0);
    }
}

fn normalize_hours(range: [f64; 2]) -> [f64; 2] {
    [range[0] / HOURS_PER_DAY, range[1] / HOURS_PER_DAY]
}

fn f64_pair(value: Option<&Value>) -> Option<[f64; 2]> {
    let values = value?.as_array()?;
    if values.len() != 2 {
        return None;
    }
    Some([values[0].as_f64()?, values[1].as_f64()?])
}

fn i32_pair(value: Option<&Value>) -> Option<[i32; 2]> {
    let values = value?.as_array()?;
    if values.len() != 2 {
        return None;
    }
    Some([
        i32::try_from(values[0].as_i64()?).ok()?,
        i32::try_from(values[1].as_i64()?).ok()?,
    ])
}

/// Spawn configuration errors.
#[derive(Debug, Error)]
pub enum SpawnError {
    /// Asset pack access failed.
    #[error(transparent)]
    Pack(#[from] hytale_assets::AssetError),
    /// A spawn document is not valid JSON.
    #[error("spawn document is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A required field is missing or invalid.
    #[error("spawn `{key}` has an invalid `{field}`")]
    InvalidField {
        /// Asset key.
        key: String,
        /// Field name.
        field: &'static str,
    },
    /// An asset path had no file name.
    #[error("invalid spawn asset path `{0}`")]
    InvalidPath(String),
    /// An inline flock definition is invalid.
    #[error(transparent)]
    Flock(#[from] FlockError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_world::world_clock::{GameInstant, WorldClockSettings, ZERO_YEAR_SECONDS};

    struct FixedRng(f64);

    impl FlockRng for FixedRng {
        fn next_f64(&mut self) -> f64 {
            self.0
        }
    }

    fn clock_at(hour: f64) -> WorldClock {
        WorldClock::from_game_time(
            Some(GameInstant::new(
                ZERO_YEAR_SECONDS + (hour * 3_600.0) as i64,
                0,
            )),
            WorldClockSettings {
                total_moon_phases: 5,
                ..WorldClockSettings::default()
            },
            false,
        )
    }

    fn spawn() -> WorldSpawn {
        WorldSpawn::from_json(
            "Test",
            serde_json::json!({
                "Environments": ["Env_Test"],
                "NPCs": [
                    {"Id": "Rat", "Weight": 1},
                    {"Id": "Bear", "Weight": 3, "Flock": {"Size": [2, 3]}},
                ],
                "DayTimeRange": [6, 18],
                "MoonPhaseRange": [0, 4],
                "ScaleDayTimeRange": false,
            }),
        )
        .expect("parse spawn")
    }

    #[test]
    fn weighted_selection_uses_java_boundaries() {
        let spawn = spawn();
        assert_eq!(spawn.pick_role(&mut FixedRng(0.0), 0).unwrap().id, "Rat");
        assert_eq!(spawn.pick_role(&mut FixedRng(0.2499), 0).unwrap().id, "Rat");
        assert_eq!(spawn.pick_role(&mut FixedRng(0.25), 0).unwrap().id, "Bear");
        assert_eq!(spawn.pick_role(&mut FixedRng(0.999), 0).unwrap().id, "Bear");
    }

    #[test]
    fn day_time_and_moon_gates_use_the_clock() {
        let spawn = spawn();
        assert!(!spawn.is_eligible(&clock_at(5.0)));
        assert!(spawn.is_eligible(&clock_at(12.0)));
        assert!(!spawn.is_eligible(&clock_at(19.0)));
        let mut moonless = spawn.clone();
        moonless.moon_phase_range = [3, 3];
        assert!(!moonless.is_eligible(&clock_at(12.0)));
    }

    #[test]
    fn moon_phase_modifiers_scale_weights_and_zero_out_of_range() {
        let mut spawn = spawn();
        spawn.moon_phase_weight_modifiers = Some(vec![0.0, 1.0, 2.0]);
        assert_eq!(spawn.moon_phase_weight_modifier(0), 0.0);
        assert_eq!(spawn.moon_phase_weight_modifier(2), 2.0);
        assert_eq!(spawn.moon_phase_weight_modifier(3), 0.0);
        assert_eq!(spawn.effective_weight(&spawn.npcs[1], 2), 6.0, "3 * 2");
        assert_eq!(spawn.pick_role(&mut FixedRng(0.0), 0), None);
    }

    #[test]
    fn environment_selection_skips_ineligible_configs_and_uses_weights() {
        let catalog = WorldSpawnCatalog {
            spawns: BTreeMap::from([("Test".to_string(), spawn())]),
        };
        let clock = clock_at(12.0);
        let (spawn, role) = catalog
            .pick_spawn("Env_Test", &clock, 0, &mut FixedRng(0.0))
            .expect("eligible role");
        assert_eq!(spawn.key, "Test");
        assert_eq!(role.id, "Rat");
        let (_, role) = catalog
            .pick_spawn("Env_Test", &clock, 0, &mut FixedRng(0.25))
            .expect("eligible role");
        assert_eq!(role.id, "Bear");
        assert!(
            catalog
                .pick_spawn("Env_Test", &clock_at(23.0), 0, &mut FixedRng(0.0))
                .is_none(),
            "the config is outside its day range"
        );
        assert!(
            catalog
                .pick_spawn("Env_Other", &clock, 0, &mut FixedRng(0.0))
                .is_none(),
            "the config is not registered for this environment"
        );
    }

    #[test]
    fn spawn_controller_uses_the_java_density_formula() {
        let mut controller = WorldSpawnController::new(0.75);
        assert_eq!(controller.expected(), 0.0);
        controller.set_segment_count(1024);
        assert!((controller.expected() - 0.75).abs() < 1e-9);
        controller.record_spawn();
        assert_eq!(controller.actual, 1);
        assert_eq!(controller.missing(), 0.0);
        assert!(!controller.has_room());
        controller.set_segment_count(4096);
        assert!((controller.expected() - 3.0).abs() < 1e-9);
        assert!(controller.has_room());
        controller.record_despawn();
        assert_eq!(controller.actual, 0);
        controller.record_despawn();
        assert_eq!(controller.actual, 0, "despawns saturate at zero");
    }

    #[test]
    fn missing_count_is_gated_by_the_flock_minimum_size() {
        let spawn = spawn();
        let flocks = FlockCatalog::default();
        assert_eq!(spawn.missing_count(&spawn.npcs[0], 0, 0.5, &flocks), 0.5);
        assert_eq!(spawn.missing_count(&spawn.npcs[1], 0, 1.0, &flocks), 0.0);
        assert_eq!(spawn.missing_count(&spawn.npcs[1], 0, 2.0, &flocks), 2.0);
        assert_eq!(WorldSpawn::spawn_weight(3.0, 1), 2.0);
        assert_eq!(WorldSpawn::spawn_weight(1.0, 3), 0.0);
    }
}
