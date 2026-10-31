//! Portal types and instance catalogs.
//!
//! Java ships `Server/PortalTypes/*.json` and instance saves under
//! `Server/Instances/**`. This module resolves the data a portal transport
//! needs (target instance, spawn point, time limit and entry rules) without
//! owning the multi-world session handoff. `UniverseRuntime`/the session layer
//! can turn a [`PortalTransportPlan`] into a world switch or teleport.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use crate::player_data::{PlayerTransform, Position, Rotation};
/// Portal asset folder below an extracted asset root.
pub const PORTAL_TYPES_ROOT: &str = "Server/PortalTypes";
/// Instance save folder below an extracted asset root.
pub const INSTANCES_ROOT: &str = "Server/Instances";

/// Errors from portal/instance catalogs.
#[derive(Debug, Error)]
pub enum InstanceError {
    /// A directory could not be read.
    #[error("instance catalog I/O at {path}: {source}")]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A portal document is invalid JSON.
    #[error("invalid portal document {path}: {source}")]
    Json {
        /// Path that failed.
        path: PathBuf,
        /// serde error.
        #[source]
        source: serde_json::Error,
    },
    /// A required portal field is missing.
    #[error("portal `{portal}` is missing `{field}`")]
    MissingField {
        /// Portal key.
        portal: String,
        /// Missing field.
        field: &'static str,
    },
    /// A plan referenced an unknown portal.
    #[error("unknown portal type `{0}`")]
    UnknownPortal(String),
}

/// One `Server/PortalTypes` document.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalType {
    /// Asset key (file stem).
    pub key: String,
    /// Instance save id (`Portals/Portals_Goblins`).
    pub instance_id: String,
    /// Gameplay config override.
    pub gameplay_config: Option<String>,
    /// Spawn point defined by the portal type, if any.
    pub spawn: Option<Position>,
    /// Close the instance when the last player leaves.
    pub close_when_empty: bool,
    /// Void invasion toggle.
    pub void_invasion_enabled: bool,
    /// Maximum entries (0 = unlimited).
    pub allowed_entry_count: i32,
    /// Instance time limit in seconds.
    pub time_limit_seconds: Option<f64>,
    /// Items deleted when the player leaves the instance.
    pub ephemeral_items: Vec<String>,
    /// Allow leaving through the map.
    pub allow_players_to_leave_via_map: bool,
}

impl PortalType {
    /// Decode one portal document.
    pub fn from_value(key: &str, value: &Value) -> Result<Self, InstanceError> {
        let instance_id = value
            .get("InstanceId")
            .and_then(Value::as_str)
            .ok_or_else(|| InstanceError::MissingField {
                portal: key.to_string(),
                field: "InstanceId",
            })?
            .to_string();
        let spawn = value.get("Spawn").and_then(|spawn| {
            let point = spawn.get("SpawnProviderOverride")?.get("SpawnPoint")?;
            Some(Position {
                x: point.get("X")?.as_f64()?,
                y: point.get("Y")?.as_f64()?,
                z: point.get("Z")?.as_f64()?,
            })
        });
        Ok(Self {
            key: key.to_string(),
            instance_id,
            gameplay_config: value
                .get("GameplayConfig")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            spawn,
            close_when_empty: value
                .get("CloseWhenEmpty")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            void_invasion_enabled: value
                .get("VoidInvasionEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            allowed_entry_count: value
                .get("AllowedEntryCount")
                .and_then(Value::as_i64)
                .and_then(|value| i32::try_from(value).ok())
                .unwrap_or(0),
            time_limit_seconds: value
                .get("TimeLimitSeconds")
                .and_then(Value::as_f64)
                .filter(|value| *value > 0.0),
            ephemeral_items: value
                .get("EphemeralItems")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect(),
            allow_players_to_leave_via_map: value
                .get("AllowPlayersToLeaveViaMap")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }
}

/// All portal type documents.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PortalCatalog {
    portals: BTreeMap<String, PortalType>,
}

impl PortalCatalog {
    /// Load `<assets_root>/Server/PortalTypes/*.json`.
    pub fn load_root(assets_root: &Path) -> Result<Self, InstanceError> {
        let root = assets_root.join(PORTAL_TYPES_ROOT);
        let mut portals = BTreeMap::new();
        for path in json_files(&root)? {
            let key = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| InstanceError::MissingField {
                    portal: path.display().to_string(),
                    field: "file name",
                })?
                .to_string();
            let bytes = std::fs::read(&path).map_err(|source| InstanceError::Io {
                path: path.clone(),
                source,
            })?;
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|source| InstanceError::Json {
                    path: path.clone(),
                    source,
                })?;
            portals.insert(key.clone(), PortalType::from_value(&key, &value)?);
        }
        Ok(Self { portals })
    }

    /// One portal by asset key.
    pub fn get(&self, key: &str) -> Option<&PortalType> {
        self.portals.get(key)
    }

    /// Number of portals.
    pub fn len(&self) -> usize {
        self.portals.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.portals.is_empty()
    }

    /// Every portal in key order.
    pub fn iter(&self) -> impl Iterator<Item = &PortalType> {
        self.portals.values()
    }

    /// Resolve a transport plan. The fallback spawn is used when the portal
    /// type has no `SpawnProviderOverride`.
    pub fn plan(
        &self,
        key: &str,
        fallback: PlayerTransform,
    ) -> Result<PortalTransportPlan, InstanceError> {
        let portal = self
            .get(key)
            .ok_or_else(|| InstanceError::UnknownPortal(key.to_string()))?;
        Ok(PortalTransportPlan::from_portal(portal, fallback))
    }
}

/// One instance save directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceDescriptor {
    /// Instance id relative to `Server/Instances` (for example
    /// `Portals/Portals_Goblins`).
    pub id: String,
    /// Directory containing `instance.bson` and `resources/`.
    pub directory: PathBuf,
    /// Whether `instance.bson` exists.
    pub has_instance_bson: bool,
}

/// All instance saves below an extracted asset root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceCatalog {
    instances: BTreeMap<String, InstanceDescriptor>,
}

impl InstanceCatalog {
    /// Scan `<assets_root>/Server/Instances/**/instance.bson`.
    pub fn load_root(assets_root: &Path) -> Result<Self, InstanceError> {
        let root = assets_root.join(INSTANCES_ROOT);
        let mut instances = BTreeMap::new();
        walk(&root, &mut |directory| {
            let bson = directory.join("instance.bson");
            if !bson.is_file() {
                return;
            }
            let id = directory
                .strip_prefix(&root)
                .unwrap_or(directory)
                .to_string_lossy()
                .replace('\\', "/");
            instances.insert(
                id.clone(),
                InstanceDescriptor {
                    id,
                    directory: directory.to_path_buf(),
                    has_instance_bson: true,
                },
            );
        })?;
        Ok(Self { instances })
    }

    /// One instance by id.
    pub fn get(&self, id: &str) -> Option<&InstanceDescriptor> {
        self.instances.get(id)
    }

    /// Number of instances.
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// Every instance in id order.
    pub fn iter(&self) -> impl Iterator<Item = &InstanceDescriptor> {
        self.instances.values()
    }
}

/// Everything a portal handoff needs.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalTransportPlan {
    /// Source portal key.
    pub portal: String,
    /// Target instance id.
    pub instance_id: String,
    /// Gameplay config override.
    pub gameplay_config: Option<String>,
    /// Target spawn transform.
    pub spawn: PlayerTransform,
    /// Instance time limit.
    pub time_limit_seconds: Option<f64>,
    /// Items removed on return.
    pub ephemeral_items: Vec<String>,
    /// Entry limit (0 = unlimited).
    pub allowed_entry_count: i32,
    /// Close the instance when the last player leaves.
    pub close_when_empty: bool,
    /// Void-invasion toggle.
    pub void_invasion_enabled: bool,
    /// Allow leaving through the map.
    pub allow_players_to_leave_via_map: bool,
}

impl PortalTransportPlan {
    fn from_portal(portal: &PortalType, fallback: PlayerTransform) -> Self {
        let spawn = portal
            .spawn
            .map(|position| PlayerTransform::new(position, neutral_rotation()))
            .unwrap_or(fallback);
        Self {
            portal: portal.key.clone(),
            instance_id: portal.instance_id.clone(),
            gameplay_config: portal.gameplay_config.clone(),
            spawn,
            time_limit_seconds: portal.time_limit_seconds,
            ephemeral_items: portal.ephemeral_items.clone(),
            allowed_entry_count: portal.allowed_entry_count,
            close_when_empty: portal.close_when_empty,
            void_invasion_enabled: portal.void_invasion_enabled,
            allow_players_to_leave_via_map: portal.allow_players_to_leave_via_map,
        }
    }
}

fn neutral_rotation() -> Rotation {
    Rotation {
        pitch: 0.0,
        yaw: 0.0,
        roll: 0.0,
    }
}

fn json_files(root: &Path) -> Result<Vec<PathBuf>, InstanceError> {
    let mut files = Vec::new();
    if !root.is_dir() {
        return Ok(files);
    }
    let entries = std::fs::read_dir(root).map_err(|source| InstanceError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| InstanceError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn walk(root: &Path, visit: &mut impl FnMut(&Path)) -> Result<(), InstanceError> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root).map_err(|source| InstanceError::Io {
        path: root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| InstanceError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit)?;
        }
    }
    visit(root);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted")
    }

    #[test]
    fn portal_types_decode_the_real_goblin_breach() {
        if !assets_root().join(PORTAL_TYPES_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = PortalCatalog::load_root(&assets_root()).expect("portals");
        assert_eq!(catalog.len(), 7);
        let portal = catalog.get("Goblin_Breach").expect("Goblin_Breach");
        assert_eq!(portal.instance_id, "Portals/Portals_Goblins");
        assert_eq!(portal.gameplay_config.as_deref(), Some("PortalsGoblins"));
        assert_eq!(
            portal.spawn,
            Some(Position {
                x: 0.5,
                y: 149.0,
                z: 290.5
            })
        );
        assert_eq!(portal.time_limit_seconds, Some(600.0));
        assert_eq!(portal.ephemeral_items, vec!["Coffer_Key"]);
        assert!(!portal.close_when_empty);
        assert!(portal.void_invasion_enabled);

        let fallback = PlayerTransform::new(
            Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            Rotation {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
            },
        );
        let plan = catalog.plan("Goblin_Breach", fallback).expect("plan");
        assert_eq!(plan.instance_id, "Portals/Portals_Goblins");
        assert_eq!(plan.spawn.position.y, 149.0);
        assert_eq!(plan.time_limit_seconds, Some(600.0));
    }

    #[test]
    fn the_deepworks_uses_its_spawn_and_map_leave_flag() {
        if !assets_root().join(PORTAL_TYPES_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = PortalCatalog::load_root(&assets_root()).expect("portals");
        let portal = catalog.get("The_Deepworks").expect("The_Deepworks");
        assert!(portal.allow_players_to_leave_via_map);
        assert!(!portal.close_when_empty);
        assert_eq!(portal.spawn.expect("spawn").y, 288.0);
        assert_eq!(portal.time_limit_seconds, None);
    }

    #[test]
    fn instance_catalog_finds_the_portal_instance_saves() {
        if !assets_root().join(INSTANCES_ROOT).is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let catalog = InstanceCatalog::load_root(&assets_root()).expect("instances");
        assert!(!catalog.is_empty());
        assert!(
            catalog
                .get("Portals/Portals_Goblins")
                .or_else(|| catalog
                    .iter()
                    .find(|instance| instance.id.contains("Portals")))
                .is_some(),
            "the portal instance folder is catalogued"
        );
    }

    #[test]
    fn missing_instance_id_is_rejected() {
        assert!(matches!(
            PortalType::from_value("Bad", &serde_json::json!({"Description": {}})),
            Err(InstanceError::MissingField {
                field: "InstanceId",
                ..
            })
        ));
    }
}
