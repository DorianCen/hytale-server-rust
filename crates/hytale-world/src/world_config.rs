//! Per-world `config.json` (`WorldConfig`) load and save.
//!
//! Java stores the world configuration next to the chunk regions as a BSON
//! document written in strict extended JSON (`BsonUtil.writeDocument` plus the
//! `WorldConfig` builder codec). This module ports the stable, server-owned
//! fields needed to boot and save a world, and preserves every unknown field so
//! a newer Java config survives a Rust load/save unchanged.
//!
//! Provider sections (`WorldGen`, `WorldMap`, `ChunkStorage`, `ResourceStorage`
//! and `SpawnProvider`) are kept as ordered JSON objects: their `Type`/`Id` and
//! provider-specific fields belong to provider codecs, not to the world config
//! itself.
//!
//! Ported from: `CoreServer/.../universe/world/WorldConfig.java`
//! (`WorldConfig.CODEC`, `VERSION = 4`) and
//! `CoreServer/.../universe/world/WorldConfigProvider.java`.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use thiserror::Error;

/// Current `WorldConfig` codec version (`WorldConfig.VERSION`).
pub const WORLD_CONFIG_VERSION: i32 = 4;

/// Java `GameplayConfig.DEFAULT_ID`.
pub const DEFAULT_GAMEPLAY_CONFIG: &str = "Default";

/// Java `WorldConfig.INITIAL_GAME_DAY_START_HOUR`.
pub const INITIAL_GAME_DAY_START_HOUR: u32 = 5;
/// Java `WorldConfig.INITIAL_GAME_DAY_START_MINS`.
pub const INITIAL_GAME_DAY_START_MINS: u32 = 30;

/// Default world time (`WorldTimeResource.ZERO_YEAR` + 05:30).
pub const DEFAULT_GAME_TIME: &str = "0001-01-01T05:30:00Z";

/// Errors from the world config codec.
#[derive(Debug, Error)]
pub enum WorldConfigError {
    /// Reading or writing the file failed.
    #[error("world config I/O error at {path}: {source}")]
    Io {
        /// File that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The document is not valid JSON or does not match the config schema.
    #[error("invalid world config {path}: {source}")]
    Json {
        /// File that failed.
        path: PathBuf,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
    /// A UUID field is malformed.
    #[error("invalid world UUID `{0}`")]
    InvalidUuid(String),
}

/// Result alias for world config operations.
pub type Result<T> = std::result::Result<T, WorldConfigError>;

/// A world UUID in the Java extended-JSON shape
/// `{"$binary":"<base64>","$type":"04"}`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldUuid {
    bytes: [u8; 16],
}

impl WorldUuid {
    /// Build from the 16 big-endian bytes Java writes
    /// (`UUID.getMostSignificantBits` then `getLeastSignificantBits`).
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    /// The 16 big-endian bytes.
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
}

impl fmt::Debug for WorldUuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("WorldUuid")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for WorldUuid {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.bytes;
        write!(
            formatter,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9], b[10], b[11], b[12],
            b[13], b[14], b[15]
        )
    }
}

impl std::str::FromStr for WorldUuid {
    type Err = WorldConfigError;

    fn from_str(value: &str) -> Result<Self> {
        let mut nibbles = Vec::with_capacity(32);
        for ch in value.chars() {
            match ch {
                '-' => continue,
                '0'..='9' => nibbles.push(ch as u8 - b'0'),
                'a'..='f' => nibbles.push(ch as u8 - b'a' + 10),
                'A'..='F' => nibbles.push(ch as u8 - b'A' + 10),
                _ => return Err(WorldConfigError::InvalidUuid(value.to_string())),
            }
        }
        if nibbles.len() != 32 {
            return Err(WorldConfigError::InvalidUuid(value.to_string()));
        }
        let mut bytes = [0u8; 16];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (nibbles[index * 2] << 4) | nibbles[index * 2 + 1];
        }
        Ok(Self { bytes })
    }
}

impl Serialize for WorldUuid {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = Map::new();
        map.insert(
            "$binary".to_string(),
            Value::String(base64_encode(&self.bytes)),
        );
        map.insert("$type".to_string(), Value::String("04".to_string()));
        map.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WorldUuid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        parse_uuid_value(&value).map_err(de::Error::custom)
    }
}

fn parse_uuid_value(value: &Value) -> Result<WorldUuid> {
    match value {
        Value::String(text) => text.parse(),
        Value::Object(map) => {
            let binary = map
                .get("$binary")
                .and_then(Value::as_str)
                .ok_or_else(|| WorldConfigError::InvalidUuid(value.to_string()))?;
            if let Some(subtype) = map.get("$type").and_then(Value::as_str) {
                if subtype != "04" {
                    return Err(WorldConfigError::InvalidUuid(value.to_string()));
                }
            }
            let bytes = base64_decode(binary)?;
            let bytes: [u8; 16] = bytes
                .try_into()
                .map_err(|_| WorldConfigError::InvalidUuid(value.to_string()))?;
            Ok(WorldUuid { bytes })
        }
        _ => Err(WorldConfigError::InvalidUuid(value.to_string())),
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let packed = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(packed >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(packed >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(packed >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[packed as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

fn base64_decode(text: &str) -> Result<Vec<u8>> {
    let invalid = || WorldConfigError::InvalidUuid(text.to_string());
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut accumulator: u32 = 0;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return Err(invalid()),
        } as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Ok(out)
}

/// An ordered JSON object used for provider sections and preserved documents.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConfigObject(pub Map<String, Value>);

impl ConfigObject {
    /// Build an object from key/value pairs (preserving their order).
    pub fn new(entries: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Self {
        let mut map = Map::new();
        for (key, value) in entries {
            map.insert(key.into(), value);
        }
        Self(map)
    }

    /// Provider id from `Type` (world gen / map / storage) or `Id` (spawn).
    pub fn type_id(&self) -> Option<&str> {
        self.0
            .get("Type")
            .or_else(|| self.0.get("Id"))
            .and_then(Value::as_str)
    }

    /// A string field.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str)
    }

    /// A raw field.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }
}

/// A 2D vector in a `Box2D` (`{"X":..,"Y":..}`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vector2d {
    /// X coordinate.
    #[serde(rename = "X")]
    pub x: f64,
    /// Y coordinate.
    #[serde(rename = "Y")]
    pub y: f64,
}

/// Java `Box2D` (`Min`/`Max` `Vector2d`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Box2d {
    /// Minimum corner.
    #[serde(rename = "Min")]
    pub min: Vector2d,
    /// Maximum corner.
    #[serde(rename = "Max")]
    pub max: Vector2d,
}

/// Java `WorldConfig.ChunkConfig`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ChunkConfig {
    /// Region pregenerated on world start (block coordinates).
    #[serde(
        rename = "PregenerateRegion",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub pregenerate_region: Option<Box2d>,
    /// Region kept loaded (block coordinates).
    #[serde(
        rename = "KeepLoadedRegion",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub keep_loaded_region: Option<Box2d>,
}

/// Java `ClientEffectWorldSettings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientEffects {
    /// `SunHeightPercent`.
    #[serde(rename = "SunHeightPercent")]
    pub sun_height_percent: f64,
    /// `SunAngleDegrees` (the codec stores radians and re-encodes degrees).
    #[serde(rename = "SunAngleDegrees")]
    pub sun_angle_degrees: f64,
    /// `BloomIntensity`.
    #[serde(rename = "BloomIntensity")]
    pub bloom_intensity: f64,
    /// `BloomPower`.
    #[serde(rename = "BloomPower")]
    pub bloom_power: f64,
    /// `SunIntensity`.
    #[serde(rename = "SunIntensity")]
    pub sun_intensity: f64,
    /// `SunshaftIntensity`.
    #[serde(rename = "SunshaftIntensity")]
    pub sunshaft_intensity: f64,
    /// `SunshaftScaleFactor`.
    #[serde(rename = "SunshaftScaleFactor")]
    pub sunshaft_scale_factor: f64,
}

impl Default for ClientEffects {
    fn default() -> Self {
        // The Java field initialisers; `SunAngleDegrees` is 0.
        Self {
            sun_height_percent: 100.0,
            sun_angle_degrees: 0.0,
            bloom_intensity: 0.3,
            bloom_power: 8.0,
            sun_intensity: 0.25,
            sunshaft_intensity: 0.3,
            sunshaft_scale_factor: 4.0,
        }
    }
}

/// A per-world `config.json` document.
///
/// The known fields mirror `WorldConfig.CODEC` v4 in codec order. Unknown
/// fields are preserved in [`Self::extra`] so newer Java configs survive a
/// Rust load/save cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldConfig {
    /// Codec version (`Version`).
    #[serde(rename = "Version", default = "default_version")]
    pub version: i32,
    /// World UUID (`UUID`).
    #[serde(rename = "UUID")]
    pub uuid: WorldUuid,
    /// Player-facing name (`DisplayName`).
    #[serde(
        rename = "DisplayName",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub display_name: Option<String>,
    /// Generation seed (`Seed`).
    #[serde(rename = "Seed", default)]
    pub seed: i64,
    /// Spawn provider (`SpawnProvider`).
    #[serde(
        rename = "SpawnProvider",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub spawn_provider: Option<ConfigObject>,
    /// World generator provider (`WorldGen`).
    #[serde(rename = "WorldGen", default = "default_world_gen")]
    pub world_gen: ConfigObject,
    /// World map provider (`WorldMap`).
    #[serde(rename = "WorldMap", default = "default_world_map")]
    pub world_map: ConfigObject,
    /// Optional per-world world-map overrides (`WorldMapConfig`).
    #[serde(
        rename = "WorldMapConfig",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub world_map_config: Option<Value>,
    /// Chunk storage provider (`ChunkStorage`).
    #[serde(rename = "ChunkStorage", default = "default_chunk_storage")]
    pub chunk_storage: ConfigObject,
    /// Chunk settings (`ChunkConfig`).
    #[serde(rename = "ChunkConfig", default)]
    pub chunk_config: ChunkConfig,
    /// `IsTicking`.
    #[serde(rename = "IsTicking", default = "default_true")]
    pub is_ticking: bool,
    /// `IsBlockTicking`.
    #[serde(rename = "IsBlockTicking", default = "default_true")]
    pub is_block_ticking: bool,
    /// `IsPvpEnabled`.
    #[serde(rename = "IsPvpEnabled", default)]
    pub is_pvp_enabled: bool,
    /// `IsFallDamageEnabled`.
    #[serde(rename = "IsFallDamageEnabled", default = "default_true")]
    pub is_fall_damage_enabled: bool,
    /// `IsGameTimePaused`.
    #[serde(rename = "IsGameTimePaused", default)]
    pub is_game_time_paused: bool,
    /// `GameTime` (ISO-8601 instant string).
    #[serde(rename = "GameTime", default = "default_game_time")]
    pub game_time: String,
    /// `ForcedWeather`.
    #[serde(
        rename = "ForcedWeather",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub forced_weather: Option<String>,
    /// `ClientEffects`.
    #[serde(rename = "ClientEffects", default)]
    pub client_effects: ClientEffects,
    /// `RequiredPlugins` (plugin id → version range).
    #[serde(rename = "RequiredPlugins", default)]
    pub required_plugins: Map<String, Value>,
    /// `GameMode` (`Adventure`/`Creative`; absent means server default).
    #[serde(rename = "GameMode", skip_serializing_if = "Option::is_none", default)]
    pub game_mode: Option<String>,
    /// `DefaultPermissionGroup`.
    #[serde(
        rename = "DefaultPermissionGroup",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub default_permission_group: Option<String>,
    /// `IsSpawningNPC`.
    #[serde(rename = "IsSpawningNPC", default = "default_true")]
    pub is_spawning_npc: bool,
    /// `IsBlockSpawnersEnabled`.
    #[serde(rename = "IsBlockSpawnersEnabled", default = "default_true")]
    pub is_block_spawners_enabled: bool,
    /// `IsSpawnMarkersEnabled`.
    #[serde(rename = "IsSpawnMarkersEnabled", default = "default_true")]
    pub is_spawn_markers_enabled: bool,
    /// `IsAllNPCFrozen`.
    #[serde(rename = "IsAllNPCFrozen", default)]
    pub is_all_npc_frozen: bool,
    /// `GameplayConfig` asset id; absent means [`DEFAULT_GAMEPLAY_CONFIG`].
    #[serde(
        rename = "GameplayConfig",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub gameplay_config: Option<String>,
    /// `Death` overrides.
    #[serde(rename = "Death", skip_serializing_if = "Option::is_none", default)]
    pub death_config_override: Option<Value>,
    /// `DaytimeDurationSeconds`.
    #[serde(
        rename = "DaytimeDurationSeconds",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub daytime_duration_seconds: Option<i32>,
    /// `NighttimeDurationSeconds`.
    #[serde(
        rename = "NighttimeDurationSeconds",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub nighttime_duration_seconds: Option<i32>,
    /// `IsCompassUpdating`.
    #[serde(rename = "IsCompassUpdating", default = "default_true")]
    pub is_compass_updating: bool,
    /// `IsSavingPlayers`.
    #[serde(rename = "IsSavingPlayers", default = "default_true")]
    pub is_saving_players: bool,
    /// `IsSavingChunks`.
    #[serde(rename = "IsSavingChunks", default = "default_true")]
    pub is_saving_chunks: bool,
    /// `SaveNewChunks`.
    #[serde(rename = "SaveNewChunks", default = "default_true")]
    pub save_new_chunks: bool,
    /// `IsUnloadingChunks`.
    #[serde(rename = "IsUnloadingChunks", default = "default_true")]
    pub is_unloading_chunks: bool,
    /// `IsObjectiveMarkersEnabled`.
    #[serde(rename = "IsObjectiveMarkersEnabled", default = "default_true")]
    pub is_objective_markers_enabled: bool,
    /// `DeleteOnUniverseStart`.
    #[serde(rename = "DeleteOnUniverseStart", default)]
    pub delete_on_universe_start: bool,
    /// `DeleteOnRemove`.
    #[serde(rename = "DeleteOnRemove", default)]
    pub delete_on_remove: bool,
    /// `CrashRecovery`.
    #[serde(
        rename = "CrashRecovery",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub crash_recovery: Option<Value>,
    /// `DisabledFluidTickers`.
    #[serde(rename = "DisabledFluidTickers", default)]
    pub disabled_fluid_tickers: BTreeSet<String>,
    /// Resource storage provider (`ResourceStorage`).
    #[serde(rename = "ResourceStorage", default = "default_resource_storage")]
    pub resource_storage: ConfigObject,
    /// Plugin configuration (`Plugin`).
    #[serde(rename = "Plugin", default)]
    pub plugin: Map<String, Value>,
    /// Fields this port does not model yet, preserved verbatim.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn default_version() -> i32 {
    WORLD_CONFIG_VERSION
}
fn default_true() -> bool {
    true
}
fn default_game_time() -> String {
    DEFAULT_GAME_TIME.to_string()
}
fn default_world_gen() -> ConfigObject {
    ConfigObject::new([
        ("Type", Value::String("Hytale".to_string())),
        ("Name", Value::String("Default".to_string())),
    ])
}
fn default_world_map() -> ConfigObject {
    ConfigObject::new([("Type", Value::String("WorldGen".to_string()))])
}
fn default_chunk_storage() -> ConfigObject {
    ConfigObject::new([("Type", Value::String("Hytale".to_string()))])
}
fn default_resource_storage() -> ConfigObject {
    ConfigObject::new([("Type", Value::String("Hytale".to_string()))])
}

impl WorldConfig {
    /// A new config with Java-compatible defaults and a deterministic UUID seed.
    pub fn new(uuid: WorldUuid, seed: i64, display_name: Option<String>) -> Self {
        Self {
            version: WORLD_CONFIG_VERSION,
            uuid,
            display_name,
            seed,
            spawn_provider: None,
            world_gen: default_world_gen(),
            world_map: default_world_map(),
            world_map_config: None,
            chunk_storage: default_chunk_storage(),
            chunk_config: ChunkConfig::default(),
            is_ticking: true,
            is_block_ticking: true,
            is_pvp_enabled: false,
            is_fall_damage_enabled: true,
            is_game_time_paused: false,
            game_time: default_game_time(),
            forced_weather: None,
            client_effects: ClientEffects::default(),
            required_plugins: Map::new(),
            game_mode: None,
            default_permission_group: None,
            is_spawning_npc: true,
            is_block_spawners_enabled: true,
            is_spawn_markers_enabled: true,
            is_all_npc_frozen: false,
            gameplay_config: Some(DEFAULT_GAMEPLAY_CONFIG.to_string()),
            death_config_override: None,
            daytime_duration_seconds: None,
            nighttime_duration_seconds: None,
            is_compass_updating: true,
            is_saving_players: true,
            is_saving_chunks: true,
            save_new_chunks: true,
            is_unloading_chunks: true,
            is_objective_markers_enabled: true,
            delete_on_universe_start: false,
            delete_on_remove: false,
            crash_recovery: None,
            disabled_fluid_tickers: BTreeSet::new(),
            resource_storage: default_resource_storage(),
            plugin: Map::new(),
            extra: Map::new(),
        }
    }

    /// The gameplay-config asset id, resolving Java's default.
    pub fn gameplay_config_id(&self) -> &str {
        self.gameplay_config
            .as_deref()
            .unwrap_or(DEFAULT_GAMEPLAY_CONFIG)
    }

    /// `config.json` inside a world save directory.
    pub fn path_in(save_dir: impl AsRef<Path>) -> PathBuf {
        save_dir.as_ref().join("config.json")
    }

    /// Load a config document from disk.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| WorldConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let config = serde_json::from_str(&text).map_err(|source| WorldConfigError::Json {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(config)
    }

    /// Parse a config document from a string.
    pub fn from_json(text: &str) -> std::result::Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Render the document in Java's pretty JSON style.
    pub fn to_json_pretty(&self) -> std::result::Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }

    /// Save atomically, keeping Java's `.bak` copy of a replaced file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| WorldConfigError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        if path.is_file() {
            let backup = backup_path(path);
            fs::copy(path, &backup).map_err(|source| WorldConfigError::Io {
                path: backup,
                source,
            })?;
        }
        let text = self
            .to_json_pretty()
            .map_err(|source| WorldConfigError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        let temporary = temporary_path(path);
        fs::write(&temporary, text).map_err(|source| WorldConfigError::Io {
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, path).map_err(|source| WorldConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_json_matches_java_extended_json() {
        let uuid = WorldUuid::from_bytes([
            0x12, 0x34, 0x56, 0x78, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34,
            0x56, 0x78,
        ]);
        assert_eq!(uuid.to_string(), "12345678-1234-5678-9abc-def012345678");
        let json = serde_json::to_string(&uuid).unwrap();
        assert_eq!(
            json,
            r#"{"$binary":"EjRWeBI0VniavN7wEjRWeA==","$type":"04"}"#
        );
        let decoded: WorldUuid = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, uuid);
    }

    #[test]
    fn defaults_match_java() {
        let config = WorldConfig::new(
            "00000000-0000-0000-0000-000000000001".parse().unwrap(),
            7,
            None,
        );
        assert_eq!(config.version, 4);
        assert!(config.is_ticking);
        assert!(!config.is_pvp_enabled);
        assert!(config.is_fall_damage_enabled);
        assert!(config.save_new_chunks);
        assert_eq!(config.game_time, DEFAULT_GAME_TIME);
        assert_eq!(config.gameplay_config_id(), DEFAULT_GAMEPLAY_CONFIG);
        assert_eq!(config.world_gen.type_id(), Some("Hytale"));
        assert_eq!(config.chunk_storage.type_id(), Some("Hytale"));
    }
}
