//! Player save documents (`DiskPlayerStorageProvider`) for the runtime world.
//!
//! Java writes one `<uuid>.json` per player under `<world>/players/`. The file is
//! `EntityStore.REGISTRY.serialize(holder)` written with
//! `BsonUtil.writeDocument`: a BSON document encoded as strict extended JSON
//! with a single top-level `Components` map.
//!
//! This port types the core holder components needed by the first join path
//! (`UUID`, `Transform`, `WorldGenId`) and preserves every other component as an
//! ordered JSON value. A full Java player save therefore survives a Rust
//! load/save cycle even before inventory and gameplay components are ported.
//!
//! Ported from:
//! - `CoreServer/.../universe/playerdata/DiskPlayerStorageProvider.java`
//! - `CoreServer/.../entity/UUIDComponent.java`
//! - `CoreServer/.../modules/entity/component/TransformComponent.java`
//! - `CoreServer/.../modules/entity/component/WorldGenId.java`
//! - `Component/.../ComponentRegistry.java` (the `Components` holder codec)

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;

use crate::world_config::WorldUuid;

/// Component id of `UUIDComponent`.
pub const UUID_COMPONENT: &str = "UUID";
/// Component id of `TransformComponent`.
pub const TRANSFORM_COMPONENT: &str = "Transform";
/// Component id of `WorldGenId`.
pub const WORLD_GEN_ID_COMPONENT: &str = "WorldGenId";

/// Errors from the player data codec and store.
#[derive(Debug, Error)]
pub enum PlayerDataError {
    /// Reading or writing the file failed.
    #[error("player data I/O error at {path}: {source}")]
    Io {
        /// File that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The document is not valid JSON or has the wrong shape.
    #[error("invalid player data {path}: {source}")]
    Json {
        /// File that failed.
        path: PathBuf,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
    /// A UUID argument is malformed.
    #[error("invalid player UUID `{0}`")]
    InvalidUuid(String),
}

/// Result alias for player data operations.
pub type Result<T> = std::result::Result<T, PlayerDataError>;

/// A 3D position in a `Transform` component (`Vector3dUtil`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Position {
    /// X coordinate.
    #[serde(rename = "X")]
    pub x: f64,
    /// Y coordinate.
    #[serde(rename = "Y")]
    pub y: f64,
    /// Z coordinate.
    #[serde(rename = "Z")]
    pub z: f64,
}

/// A rotation in a `Transform` component (`Rotation3f`, radians).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rotation {
    /// Pitch in radians.
    #[serde(rename = "Pitch")]
    pub pitch: f32,
    /// Yaw in radians.
    #[serde(rename = "Yaw")]
    pub yaw: f32,
    /// Roll in radians.
    #[serde(rename = "Roll")]
    pub roll: f32,
}

/// A `TransformComponent` document.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerTransform {
    /// Entity position.
    #[serde(rename = "Position")]
    pub position: Position,
    /// Entity rotation.
    #[serde(rename = "Rotation")]
    pub rotation: Rotation,
}

impl PlayerTransform {
    /// Build a transform from position and rotation.
    pub const fn new(position: Position, rotation: Rotation) -> Self {
        Self { position, rotation }
    }
}

/// One player's `EntityStore` holder document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerData {
    /// The `Components` map keyed by Java component id.
    #[serde(rename = "Components", default)]
    components: Map<String, Value>,
}

impl Default for PlayerData {
    fn default() -> Self {
        Self {
            components: Map::new(),
        }
    }
}

impl PlayerData {
    /// A holder containing the core components this port owns.
    pub fn new(uuid: WorldUuid, transform: PlayerTransform) -> Self {
        let mut data = Self::default();
        data.set_uuid(uuid);
        data.set_transform(transform);
        data
    }

    /// Build from an existing `Components` map (unknown components included).
    pub fn from_components(components: Map<String, Value>) -> Self {
        Self { components }
    }

    /// The raw `Components` map, in document order.
    pub fn components(&self) -> &Map<String, Value> {
        &self.components
    }

    /// Numerical component count (known and unknown).
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Component ids in document order.
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(String::as_str)
    }

    /// One raw component document.
    pub fn component(&self, id: &str) -> Option<&Value> {
        self.components.get(id)
    }

    /// Replace a component document; returns the previous value.
    pub fn insert_component(&mut self, id: impl Into<String>, value: Value) -> Option<Value> {
        self.components.insert(id.into(), value)
    }

    /// Remove a component document.
    pub fn remove_component(&mut self, id: &str) -> Option<Value> {
        self.components.remove(id)
    }

    /// The `UUID` component, if present.
    pub fn uuid(&self) -> Option<WorldUuid> {
        let value = self.components.get(UUID_COMPONENT)?.get("UUID")?;
        serde_json::from_value(value.clone()).ok()
    }

    /// Set or replace the `UUID` component.
    pub fn set_uuid(&mut self, uuid: WorldUuid) {
        let encoded = serde_json::to_value(uuid).expect("WorldUuid always serializes");
        self.components
            .insert(UUID_COMPONENT.to_string(), json!({ "UUID": encoded }));
    }

    /// The `Transform` component, if present and well-formed.
    pub fn transform(&self) -> Option<PlayerTransform> {
        let value = self.components.get(TRANSFORM_COMPONENT)?;
        serde_json::from_value(value.clone()).ok()
    }

    /// Set or replace the `Transform` component.
    pub fn set_transform(&mut self, transform: PlayerTransform) {
        let encoded = serde_json::to_value(transform).expect("PlayerTransform always serializes");
        self.components
            .insert(TRANSFORM_COMPONENT.to_string(), encoded);
    }

    /// The `WorldGenId` component, if present.
    pub fn world_gen_id(&self) -> Option<i32> {
        self.components
            .get(WORLD_GEN_ID_COMPONENT)?
            .get("WorldGenId")?
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
    }

    /// Set or replace the `WorldGenId` component.
    pub fn set_world_gen_id(&mut self, world_gen_id: i32) {
        self.components.insert(
            WORLD_GEN_ID_COMPONENT.to_string(),
            json!({ "WorldGenId": world_gen_id }),
        );
    }

    /// Parse a document string (`{"Components": ...}`).
    pub fn from_json(text: &str) -> std::result::Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Render in Java's pretty JSON style.
    pub fn to_json_pretty(&self) -> std::result::Result<String, serde_json::Error> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}

/// Player save files under `<world>/players/`.
#[derive(Debug, Clone)]
pub struct PlayerStorage {
    players_dir: PathBuf,
}

impl PlayerStorage {
    /// A store rooted at an explicit `players/` directory.
    pub fn new(players_dir: impl Into<PathBuf>) -> Self {
        Self {
            players_dir: players_dir.into(),
        }
    }

    /// The Java layout: `<world save>/players/`.
    pub fn for_save_dir(save_dir: impl AsRef<Path>) -> Self {
        Self::new(save_dir.as_ref().join("players"))
    }

    /// The directory holding `<uuid>.json`.
    pub fn players_dir(&self) -> &Path {
        &self.players_dir
    }

    /// Path of one player file.
    pub fn path_for(&self, uuid: WorldUuid) -> PathBuf {
        self.players_dir.join(format!("{uuid}.json"))
    }

    /// Load a player, or `None` when the file does not exist.
    pub fn load(&self, uuid: WorldUuid) -> Result<Option<PlayerData>> {
        let path = self.path_for(uuid);
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|source| PlayerDataError::Io {
            path: path.clone(),
            source,
        })?;
        let data =
            serde_json::from_str(&text).map_err(|source| PlayerDataError::Json { path, source })?;
        Ok(Some(data))
    }

    /// Save atomically, keeping Java's `.bak` copy of a replaced file.
    pub fn save(&self, uuid: WorldUuid, data: &PlayerData) -> Result<()> {
        fs::create_dir_all(&self.players_dir).map_err(|source| PlayerDataError::Io {
            path: self.players_dir.clone(),
            source,
        })?;
        let path = self.path_for(uuid);
        if path.is_file() {
            let backup = backup_path(&path);
            fs::copy(&path, &backup).map_err(|source| PlayerDataError::Io {
                path: backup,
                source,
            })?;
        }
        let text = data
            .to_json_pretty()
            .map_err(|source| PlayerDataError::Json {
                path: path.clone(),
                source,
            })?;
        let temporary = temporary_path(&path);
        fs::write(&temporary, text).map_err(|source| PlayerDataError::Io {
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, &path).map_err(|source| PlayerDataError::Io { path, source })?;
        Ok(())
    }

    /// Delete a player file; returns whether it existed.
    pub fn remove(&self, uuid: WorldUuid) -> Result<bool> {
        match fs::remove_file(self.path_for(uuid)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(PlayerDataError::Io {
                path: self.path_for(uuid),
                source,
            }),
        }
    }

    /// Every player UUID stored in the directory, sorted.
    pub fn players(&self) -> Result<Vec<WorldUuid>> {
        if !self.players_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut players = Vec::new();
        for entry in fs::read_dir(&self.players_dir).map_err(|source| PlayerDataError::Io {
            path: self.players_dir.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| PlayerDataError::Io {
                path: self.players_dir.clone(),
                source,
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = name.strip_suffix(".json") else {
                continue;
            };
            if let Ok(uuid) = stem.parse() {
                players.push(uuid);
            }
        }
        players.sort_by_key(ToString::to_string);
        Ok(players)
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

    fn uuid() -> WorldUuid {
        "12345678-1234-5678-9abc-def012345678".parse().unwrap()
    }

    fn transform() -> PlayerTransform {
        PlayerTransform::new(
            Position {
                x: 10.5,
                y: 80.0,
                z: -3.5,
            },
            Rotation {
                pitch: 0.25,
                yaw: 1.570_796_4,
                roll: -0.125,
            },
        )
    }

    #[test]
    fn core_components_round_trip() {
        let mut data = PlayerData::new(uuid(), transform());
        data.set_world_gen_id(42);
        let text = data.to_json_pretty().unwrap();
        assert!(text.contains("\"Components\""));
        let reloaded = PlayerData::from_json(&text).unwrap();
        assert_eq!(reloaded, data);
        assert_eq!(reloaded.uuid(), Some(uuid()));
        assert_eq!(reloaded.transform(), Some(transform()));
        assert_eq!(reloaded.world_gen_id(), Some(42));
        assert_eq!(reloaded.component_count(), 3);
    }

    #[test]
    fn unknown_components_are_preserved() {
        let source = r#"{
            "Components": {
                "UUID": {"UUID": {"$binary": "EjRWeBI0VniavN7wEjRWeA==", "$type": "04"}},
                "Inventory": {"Items": [], "Version": 3}
            }
        }"#;
        let data = PlayerData::from_json(source).unwrap();
        assert_eq!(data.uuid(), Some(uuid()));
        assert_eq!(data.component("Inventory").unwrap()["Version"], 3);
        let text = data.to_json_pretty().unwrap();
        let reloaded = PlayerData::from_json(&text).unwrap();
        assert_eq!(reloaded.component("Inventory").unwrap()["Version"], 3);
    }
}
