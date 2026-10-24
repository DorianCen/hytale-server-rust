//! Universe catalog and world lifecycle.
//!
//! A universe is a directory of world save roots. Each world is a normal
//! `WorldSave` directory (`config.json`, `players/`, `chunks/`) plus the
//! provider choice recorded in `<root>/universe.json`. The catalog owns
//! create/list/resolve/default/save/delete metadata; live chunk work is owned by
//! a world thread.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::chunk::column::ChunkColumn;
use crate::world::{WorldError, WorldSave};
use crate::world_config::{WorldConfig, WorldConfigError, WorldUuid};

/// How a world generates chunks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorldProviderKind {
    /// The existing `worldgen-server` pipeline.
    #[default]
    Default,
    /// Flat placeholder provider (reserved for M5 parity).
    Flat,
    /// Void placeholder provider.
    Void,
    /// Dummy placeholder provider.
    Dummy,
}

impl WorldProviderKind {
    /// Lowercase provider name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Flat => "flat",
            Self::Void => "void",
            Self::Dummy => "dummy",
        }
    }
}

impl std::fmt::Display for WorldProviderKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for WorldProviderKind {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" | "hytale" => Ok(Self::Default),
            "flat" => Ok(Self::Flat),
            "void" => Ok(Self::Void),
            "dummy" => Ok(Self::Dummy),
            other => Err(format!(
                "unknown world provider `{other}` (expected default, flat, void, dummy)"
            )),
        }
    }
}

/// A provider that produces runtime columns for one world.
///
/// `Default` is intentionally not implemented here; `hytale-server` wraps the
/// existing `WorldGenerator` for it. The static implementations are stubs until
/// M5 gives Flat/Void/Dummy real parity.
pub trait WorldProvider: Send + Sync {
    /// Provider kind.
    fn kind(&self) -> WorldProviderKind;

    /// Generate one runtime column.
    fn generate(&self, seed: i64, x: i32, z: i32) -> std::result::Result<ChunkColumn, WorldError>;
}

/// Empty-column provider for the Flat/Void/Dummy seam.
#[derive(Debug, Clone, Copy)]
pub struct StaticWorldProvider {
    kind: WorldProviderKind,
}

impl StaticWorldProvider {
    /// Create a static provider. `Default` needs the real worldgen pipeline and
    /// is rejected here.
    pub fn new(kind: WorldProviderKind) -> Result<Self> {
        if kind == WorldProviderKind::Default {
            return Err(UniverseError::Provider(
                "the default provider is implemented by hytale-server's WorldGenerator".to_string(),
            ));
        }
        Ok(Self { kind })
    }
}

impl WorldProvider for StaticWorldProvider {
    fn kind(&self) -> WorldProviderKind {
        self.kind
    }

    fn generate(&self, _seed: i64, x: i32, z: i32) -> std::result::Result<ChunkColumn, WorldError> {
        // Flat/Void/Dummy are empty-column stubs. M5 replaces these with real
        // provider parity.
        Ok(ChunkColumn::new(x, z))
    }
}

/// One catalogued world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldDescriptor {
    /// World UUID from `config.json`.
    pub uuid: WorldUuid,
    /// Player-facing name (`DisplayName`, or the directory name when absent).
    pub name: String,
    /// Save root.
    pub save_dir: PathBuf,
    /// Generation seed.
    pub seed: i64,
    /// Selected provider.
    pub provider: WorldProviderKind,
}

impl WorldDescriptor {
    /// `config.json` path.
    pub fn config_path(&self) -> PathBuf {
        WorldConfig::path_in(&self.save_dir)
    }
}

/// Universe errors.
#[derive(Debug, Error)]
pub enum UniverseError {
    /// A filesystem operation failed.
    #[error("universe I/O `{path}`: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// `universe.json` failed to parse or write.
    #[error("universe manifest `{path}`: {source}")]
    Manifest {
        /// Manifest path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: serde_json::Error,
    },
    /// A world config failed to load.
    #[error(transparent)]
    Config(#[from] WorldConfigError),
    /// A world save failed.
    #[error(transparent)]
    World(#[from] WorldError),
    /// A world name is invalid.
    #[error("invalid world name `{0}`")]
    InvalidName(String),
    /// A world with that name already exists.
    #[error("world `{0}` already exists")]
    DuplicateName(String),
    /// A world with that UUID already exists.
    #[error("world UUID `{0}` already exists")]
    DuplicateUuid(WorldUuid),
    /// A world cannot be resolved.
    #[error("world `{0}` was not found")]
    NotFound(String),
    /// A world directory has unexpected contents.
    #[error("world directory `{0}` is not empty and has no config.json")]
    DirectoryNotEmpty(PathBuf),
    /// A world directory exists but has no config.
    #[error("world `{0}` is missing config.json")]
    MissingConfig(PathBuf),
    /// The config UUID does not match the directory descriptor.
    #[error("world UUID mismatch at `{path}`: expected {expected}, found {actual}")]
    UuidMismatch {
        /// World directory.
        path: PathBuf,
        /// Expected UUID.
        expected: WorldUuid,
        /// Actual UUID.
        actual: WorldUuid,
    },
    /// A provider is not available.
    #[error("world provider error: {0}")]
    Provider(String),
}

/// Result alias for universe operations.
pub type Result<T> = std::result::Result<T, UniverseError>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct UniverseManifest {
    default_world: Option<String>,
    providers: BTreeMap<String, WorldProviderKind>,
}

/// A directory of worlds with a persistent default/provider manifest.
#[derive(Debug, Clone)]
pub struct Universe {
    root: PathBuf,
    worlds: BTreeMap<WorldUuid, WorldDescriptor>,
    default_world: Option<WorldUuid>,
}

impl Universe {
    /// Open (or create) a universe root and discover every world below it.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root).map_err(|source| UniverseError::Io {
            path: root.clone(),
            source,
        })?;
        let manifest_path = root.join("universe.json");
        let manifest = if manifest_path.is_file() {
            let text =
                std::fs::read_to_string(&manifest_path).map_err(|source| UniverseError::Io {
                    path: manifest_path.clone(),
                    source,
                })?;
            serde_json::from_str(&text).map_err(|source| UniverseError::Manifest {
                path: manifest_path.clone(),
                source,
            })?
        } else {
            UniverseManifest::default()
        };

        let mut worlds = BTreeMap::new();
        let mut names = BTreeSet::new();
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&root)
            .map_err(|source| UniverseError::Io {
                path: root.clone(),
                source,
            })?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::result::Result<_, _>>()
            .map_err(|source| UniverseError::Io {
                path: root.clone(),
                source,
            })?;
        entries.sort();
        for entry in entries {
            if !entry.is_dir() {
                continue;
            }
            let config_path = WorldConfig::path_in(&entry);
            if !config_path.is_file() {
                continue;
            }
            let config = WorldConfig::load(&config_path)?;
            let name = config
                .display_name
                .clone()
                .unwrap_or_else(|| directory_name(&entry));
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(UniverseError::DuplicateName(name));
            }
            let provider = manifest
                .providers
                .get(&config.uuid.to_string())
                .copied()
                .unwrap_or_default();
            let descriptor = WorldDescriptor {
                uuid: config.uuid,
                name,
                save_dir: entry.clone(),
                seed: config.seed,
                provider,
            };
            if worlds.insert(config.uuid, descriptor).is_some() {
                return Err(UniverseError::DuplicateUuid(config.uuid));
            }
        }

        let default_world = manifest
            .default_world
            .as_deref()
            .and_then(|name| find_by_name(&worlds, name))
            .or_else(|| worlds.keys().next().copied());

        Ok(Self {
            root,
            worlds,
            default_world,
        })
    }

    /// Universe root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every world, sorted by name.
    pub fn list_worlds(&self) -> Vec<&WorldDescriptor> {
        let mut worlds: Vec<&WorldDescriptor> = self.worlds.values().collect();
        worlds.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        });
        worlds
    }

    /// Look up one world by name (case-insensitive) or UUID.
    pub fn resolve(&self, name_or_uuid: &str) -> Result<&WorldDescriptor> {
        if let Ok(uuid) = name_or_uuid.parse::<WorldUuid>() {
            if let Some(descriptor) = self.worlds.get(&uuid) {
                return Ok(descriptor);
            }
        }
        find_by_name(&self.worlds, name_or_uuid)
            .and_then(|uuid| self.worlds.get(&uuid))
            .ok_or_else(|| UniverseError::NotFound(name_or_uuid.to_string()))
    }

    /// The default world, when the universe is non-empty.
    pub fn default_world(&self) -> Option<&WorldDescriptor> {
        self.default_world.and_then(|uuid| self.worlds.get(&uuid))
    }

    /// Make an existing world the default.
    pub fn set_default_world(&mut self, name_or_uuid: &str) -> Result<()> {
        let uuid = self.resolve(name_or_uuid)?.uuid;
        self.default_world = Some(uuid);
        self.save_manifest()
    }

    /// Create a world and its `config.json`.
    pub fn create_world(
        &mut self,
        name: &str,
        seed: i64,
        provider: WorldProviderKind,
    ) -> Result<&WorldDescriptor> {
        let name = validate_name(name)?;
        if find_by_name(&self.worlds, &name).is_some() {
            return Err(UniverseError::DuplicateName(name));
        }
        let save_dir = self.root.join(&name);
        if save_dir.exists() {
            if WorldConfig::path_in(&save_dir).is_file() {
                return Err(UniverseError::DuplicateName(name));
            }
            let mut entries = std::fs::read_dir(&save_dir).map_err(|source| UniverseError::Io {
                path: save_dir.clone(),
                source,
            })?;
            if entries.next().is_some() {
                return Err(UniverseError::DirectoryNotEmpty(save_dir));
            }
        }
        let uuid = WorldUuid::derive(seed, &name);
        if self.worlds.contains_key(&uuid) {
            return Err(UniverseError::DuplicateUuid(uuid));
        }

        let (_save, _created) = WorldSave::open(&save_dir, seed, Some(name.clone()), uuid)?;
        let descriptor = WorldDescriptor {
            uuid,
            name,
            save_dir,
            seed,
            provider,
        };
        self.worlds.insert(uuid, descriptor.clone());
        if self.default_world.is_none() {
            self.default_world = Some(uuid);
        }
        self.save_manifest()?;
        self.worlds
            .get(&uuid)
            .ok_or_else(|| UniverseError::NotFound(descriptor.name))
    }

    /// Open one world's save bundle. The world must already have a config.
    pub fn open_world(&self, name_or_uuid: &str) -> Result<WorldSave> {
        let descriptor = self.resolve(name_or_uuid)?;
        if !descriptor.config_path().is_file() {
            return Err(UniverseError::MissingConfig(descriptor.save_dir.clone()));
        }
        let (save, _created) = WorldSave::open(
            &descriptor.save_dir,
            descriptor.seed,
            Some(descriptor.name.clone()),
            descriptor.uuid,
        )?;
        if save.config().uuid != descriptor.uuid {
            return Err(UniverseError::UuidMismatch {
                path: descriptor.save_dir.clone(),
                expected: descriptor.uuid,
                actual: save.config().uuid,
            });
        }
        Ok(save)
    }

    /// Save one world's config. Live chunk saving belongs to its `WorldThread`.
    pub fn save_world(&self, name_or_uuid: &str) -> Result<()> {
        self.open_world(name_or_uuid)?.save_config()?;
        Ok(())
    }

    /// Load a player from a world, creating the save at the spawn point when
    /// absent.
    pub fn load_or_create_player(
        &self,
        name_or_uuid: &str,
        player_uuid: WorldUuid,
    ) -> Result<(crate::player_data::PlayerData, bool)> {
        Ok(self
            .open_world(name_or_uuid)?
            .load_or_create_player(player_uuid)?)
    }

    /// Write a player save on world leave.
    pub fn save_player(
        &self,
        name_or_uuid: &str,
        player_uuid: WorldUuid,
        data: &crate::player_data::PlayerData,
    ) -> Result<()> {
        self.open_world(name_or_uuid)?
            .save_player(player_uuid, data)?;
        Ok(())
    }

    /// Remove a world from the in-memory catalog without touching its files.
    ///
    /// Reopening the universe rediscovers it. Use [`Self::delete_world`] to
    /// remove the save directory and manifest entry.
    pub fn unload_world(&mut self, name_or_uuid: &str) -> Result<()> {
        let uuid = self.resolve(name_or_uuid)?.uuid;
        self.worlds.remove(&uuid);
        if self.default_world == Some(uuid) {
            self.default_world = self.worlds.keys().next().copied();
        }
        Ok(())
    }

    /// Delete a world's catalog entry and its save directory.
    pub fn delete_world(&mut self, name_or_uuid: &str) -> Result<()> {
        let uuid = self.resolve(name_or_uuid)?.uuid;
        let descriptor = self
            .worlds
            .remove(&uuid)
            .ok_or_else(|| UniverseError::NotFound(name_or_uuid.to_string()))?;
        if self.default_world == Some(uuid) {
            self.default_world = self.worlds.keys().next().copied();
        }
        self.save_manifest()?;
        if descriptor.save_dir.exists() {
            std::fs::remove_dir_all(&descriptor.save_dir).map_err(|source| UniverseError::Io {
                path: descriptor.save_dir,
                source,
            })?;
        }
        Ok(())
    }

    fn save_manifest(&self) -> Result<()> {
        let manifest = UniverseManifest {
            default_world: self
                .default_world
                .and_then(|uuid| self.worlds.get(&uuid))
                .map(|descriptor| descriptor.name.clone()),
            providers: self
                .worlds
                .iter()
                .map(|(uuid, descriptor)| (uuid.to_string(), descriptor.provider))
                .collect(),
        };
        let path = self.root.join("universe.json");
        let text =
            serde_json::to_string_pretty(&manifest).map_err(|source| UniverseError::Manifest {
                path: path.clone(),
                source,
            })?;
        std::fs::write(&path, text).map_err(|source| UniverseError::Io { path, source })
    }
}

fn find_by_name(worlds: &BTreeMap<WorldUuid, WorldDescriptor>, name: &str) -> Option<WorldUuid> {
    worlds
        .iter()
        .find(|(_, descriptor)| descriptor.name.eq_ignore_ascii_case(name))
        .map(|(uuid, _)| *uuid)
}

fn directory_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "world".to_string())
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(UniverseError::InvalidName(name.to_string()));
    }
    if name == "."
        || name == ".."
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return Err(UniverseError::InvalidName(name.to_string()));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::palette::NameLookup;
    use crate::chunk::storage::ChunkCodec;
    use crate::world::WorldStorage;

    struct TestNames;

    impl NameLookup for TestNames {
        fn name_of(&self, id: i32) -> Option<&str> {
            Some(Box::leak(format!("id_{id}").into_boxed_str()))
        }

        fn id_of(&self, name: &str) -> Option<i32> {
            name.strip_prefix("id_")?.parse().ok()
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hytale-universe-{}-{nonce}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn creates_lists_resolves_and_reopens_two_worlds() {
        let root = temp_root("two");
        let mut universe = Universe::open(&root).unwrap();
        let alpha = universe
            .create_world("Alpha", 11, WorldProviderKind::Default)
            .unwrap()
            .clone();
        let beta = universe
            .create_world("Beta", 22, WorldProviderKind::Void)
            .unwrap()
            .clone();
        assert_eq!(universe.list_worlds().len(), 2);
        assert_eq!(universe.default_world().unwrap().name, "Alpha");
        assert_eq!(universe.resolve("alpha").unwrap().uuid, alpha.uuid);
        assert_eq!(
            universe.resolve(&beta.uuid.to_string()).unwrap().name,
            "Beta"
        );
        universe.set_default_world("Beta").unwrap();

        let reopened = Universe::open(&root).unwrap();
        assert_eq!(reopened.list_worlds().len(), 2);
        assert_eq!(reopened.default_world().unwrap().name, "Beta");
        assert_eq!(
            reopened.resolve("Beta").unwrap().provider,
            WorldProviderKind::Void
        );
        assert!(alpha.config_path().is_file());

        let player_uuid = WorldUuid::derive(1, "player");
        let (mut player, created) = reopened
            .load_or_create_player("Alpha", player_uuid)
            .unwrap();
        assert!(created);
        player.set_world_gen_id(4);
        reopened.save_player("Alpha", player_uuid, &player).unwrap();
        let (reloaded, created) = reopened
            .load_or_create_player("Alpha", player_uuid)
            .unwrap();
        assert!(!created);
        assert_eq!(reloaded.world_gen_id(), Some(4));

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn duplicate_missing_and_invalid_worlds_fail_closed() {
        let root = temp_root("errors");
        let mut universe = Universe::open(&root).unwrap();
        universe
            .create_world("Alpha", 1, WorldProviderKind::Default)
            .unwrap();
        assert!(matches!(
            universe.create_world("alpha", 2, WorldProviderKind::Default),
            Err(UniverseError::DuplicateName(_))
        ));
        assert!(matches!(
            universe.create_world("../escape", 3, WorldProviderKind::Default),
            Err(UniverseError::InvalidName(_))
        ));
        assert!(matches!(
            universe.resolve("missing"),
            Err(UniverseError::NotFound(_))
        ));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn static_provider_round_trips_a_stored_chunk() {
        let root = temp_root("provider");
        let mut universe = Universe::open(&root).unwrap();
        let descriptor = universe
            .create_world("Void", 7, WorldProviderKind::Void)
            .unwrap()
            .clone();
        let provider = StaticWorldProvider::new(WorldProviderKind::Void).unwrap();
        let names = TestNames;
        let codec = ChunkCodec::from_lookups(&names, &names, &names);

        let mut save = universe.open_world("Void").unwrap();
        let report = save
            .ensure_chunks(&codec, [(0, 0)], |x, z| {
                provider.generate(descriptor.seed, x, z)
            })
            .unwrap();
        assert_eq!(report.generated, 1);
        save.save_all(&codec).unwrap();
        drop(save);

        let reopened = Universe::open(&root).unwrap();
        let mut save = reopened.open_world(&descriptor.uuid.to_string()).unwrap();
        assert!(save
            .world_mut()
            .load_chunk(&codec, 0, 0)
            .expect("load chunk"));
        assert_eq!(save.world().chunk(0, 0).unwrap().get_block(1, 64, 1), 0);
        assert_eq!(
            WorldStorage::new(save.save_dir())
                .stored_chunk_coordinates()
                .unwrap()
                .len(),
            1
        );

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn delete_removes_the_directory_and_catalog_entry() {
        let root = temp_root("delete");
        let mut universe = Universe::open(&root).unwrap();
        universe
            .create_world("Gone", 1, WorldProviderKind::Default)
            .unwrap();
        let save_dir = universe.resolve("Gone").unwrap().save_dir.clone();
        universe.delete_world("Gone").unwrap();
        assert!(!save_dir.exists());
        assert!(universe.list_worlds().is_empty());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn provider_names_parse_and_declared_provider_is_preserved() {
        assert_eq!(
            "flat".parse::<WorldProviderKind>().unwrap(),
            WorldProviderKind::Flat
        );
        assert_eq!(
            "Default".parse::<WorldProviderKind>().unwrap(),
            WorldProviderKind::Default
        );
        assert!("nope".parse::<WorldProviderKind>().is_err());
        assert!(StaticWorldProvider::new(WorldProviderKind::Default).is_err());
    }
}
