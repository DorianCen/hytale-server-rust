//! Runtime entities and block-component state.
//!
//! Java keeps entities in the `EntityStore` and per-block state in
//! `BlockComponentSection` holders, both serialized by the ECS codec. The Rust
//! port does not have that ECS; this module owns the runtime side of T56 in a
//! small, deterministic store:
//!
//! * [`RuntimeEntityStore`] — spawned prefab/item/projectile/NPC entities with
//!   stable ids and a `Transform`.
//! * [`BlockComponentStore`] — the `"components"` document of a prefab block,
//!   keyed by world position.
//!
//! Both stores persist as versioned JSON in the world save root. The chunk
//! regions stay Java-readable and never embed this sidecar state (documented
//! T56 deviation).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::player_data::{PlayerTransform, Position, Rotation};

/// Stable id of a runtime entity.
pub type RuntimeEntityId = u64;

/// Current on-disk schema of the runtime stores.
pub const RUNTIME_STATE_VERSION: i64 = 1;
/// File name of the runtime entity store.
pub const ENTITIES_FILE: &str = "entities.json";
/// File name of the block-component store.
pub const BLOCK_COMPONENTS_FILE: &str = "block-components.json";

/// Errors from loading or saving runtime state.
#[derive(Debug, Error)]
pub enum RuntimeStateError {
    /// The file could not be read or written.
    #[error("runtime state io error: {0}")]
    Io(#[from] std::io::Error),
    /// The JSON document is malformed.
    #[error("runtime state is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The file was written by a newer schema.
    #[error("unsupported runtime state version {version} (expected {expected})")]
    UnsupportedVersion {
        /// Version found in the file.
        version: i64,
        /// Version this build supports.
        expected: i64,
    },
    /// The entity store contains a duplicate id.
    #[error("runtime entity id {id} appears more than once")]
    DuplicateEntity {
        /// Duplicated id.
        id: u64,
    },
    /// A persisted block container could not be decoded.
    #[error("block container state: {0}")]
    Container(String),
}

/// What spawned an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEntityKind {
    /// An entity pasted from a prefab file.
    Prefab,
    /// A dropped item stack.
    Item,
    /// A projectile body.
    Projectile,
    /// An NPC (T58/T59).
    Npc,
    /// A future entity kind the current build does not model.
    Other,
}

/// One entity in the runtime store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEntity {
    /// Stable runtime id.
    pub id: u64,
    /// What produced the entity.
    pub kind: RuntimeEntityKind,
    /// Source prefab name, for prefab-spawned entities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefab: Option<String>,
    /// World transform.
    pub transform: PlayerTransform,
    /// Component document. Prefab entities keep the original `Components`
    /// object; item entities use the small `Item` document below.
    #[serde(default)]
    pub components: Value,
}

impl RuntimeEntity {
    /// Build an entity. Ids are assigned by [`RuntimeEntityStore::spawn`].
    pub fn new(
        id: u64,
        kind: RuntimeEntityKind,
        prefab: Option<String>,
        transform: PlayerTransform,
        components: Value,
    ) -> Self {
        Self {
            id,
            kind,
            prefab,
            transform,
            components,
        }
    }

    /// The item id when this entity is an item drop.
    pub fn item_id(&self) -> Option<&str> {
        self.components
            .get("Item")
            .and_then(|item| item.get("Id"))
            .and_then(Value::as_str)
    }

    /// The item quantity of an item drop (`1` when absent).
    pub fn item_quantity(&self) -> i32 {
        self.components
            .get("Item")
            .and_then(|item| item.get("Quantity"))
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32
    }
}

/// Sorted runtime entities with stable ids.
#[derive(Debug, Clone)]
pub struct RuntimeEntityStore {
    next_id: u64,
    entities: BTreeMap<u64, RuntimeEntity>,
    dirty: bool,
}

impl Default for RuntimeEntityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeEntityStore {
    /// An empty store with ids starting at `1`.
    pub fn new() -> Self {
        Self {
            next_id: 1,
            entities: BTreeMap::new(),
            dirty: false,
        }
    }

    /// The next id that will be assigned.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Number of live entities.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Whether no entity is live.
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// Whether the store changed since the last save/load.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag after a save.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// One entity by id.
    pub fn get(&self, id: u64) -> Option<&RuntimeEntity> {
        self.entities.get(&id)
    }

    /// Mutable access to one entity (marks the store dirty).
    pub fn get_mut(&mut self, id: u64) -> Option<&mut RuntimeEntity> {
        self.dirty = true;
        self.entities.get_mut(&id)
    }

    /// Every entity, ordered by id.
    pub fn iter(&self) -> impl Iterator<Item = &RuntimeEntity> {
        self.entities.values()
    }

    /// Spawn an entity and return its new id.
    pub fn spawn(&mut self, mut entity: RuntimeEntity) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        entity.id = id;
        self.entities.insert(id, entity);
        self.dirty = true;
        id
    }

    /// Re-insert an entity with its persisted id (loading only).
    pub fn insert_loaded(&mut self, entity: RuntimeEntity) -> Result<(), RuntimeStateError> {
        if self.entities.contains_key(&entity.id) {
            return Err(RuntimeStateError::DuplicateEntity { id: entity.id });
        }
        self.next_id = self.next_id.max(entity.id.saturating_add(1));
        self.entities.insert(entity.id, entity);
        Ok(())
    }

    /// Remove one entity.
    pub fn despawn(&mut self, id: u64) -> Option<RuntimeEntity> {
        let removed = self.entities.remove(&id);
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    /// Remove every entity of one kind; returns the removed ids.
    pub fn despawn_kind(&mut self, kind: RuntimeEntityKind) -> Vec<u64> {
        let ids: Vec<u64> = self
            .entities
            .iter()
            .filter(|(_, entity)| entity.kind == kind)
            .map(|(id, _)| *id)
            .collect();
        for id in &ids {
            self.entities.remove(id);
        }
        if !ids.is_empty() {
            self.dirty = true;
        }
        ids
    }

    /// Entities whose position is inside the horizontal radius (squared
    /// distance, matching the view filter).
    pub fn in_radius(&self, center: Position, radius: f64) -> Vec<&RuntimeEntity> {
        let radius_squared = radius * radius;
        self.entities
            .values()
            .filter(|entity| {
                let dx = entity.transform.position.x - center.x;
                let dz = entity.transform.position.z - center.z;
                dx * dx + dz * dz <= radius_squared
            })
            .collect()
    }

    /// Load `<save>/entities.json` (missing file = empty store).
    pub fn load(save_dir: &Path) -> Result<Self, RuntimeStateError> {
        let path = save_dir.join(ENTITIES_FILE);
        if !path.is_file() {
            return Ok(Self::new());
        }
        let document: EntitiesDocument = serde_json::from_slice(&std::fs::read(&path)?)?;
        if document.version != RUNTIME_STATE_VERSION {
            return Err(RuntimeStateError::UnsupportedVersion {
                version: document.version,
                expected: RUNTIME_STATE_VERSION,
            });
        }
        let mut store = Self::new();
        store.next_id = document.next_id.max(1);
        for entity in document.entities {
            store.insert_loaded(entity)?;
        }
        store.dirty = false;
        Ok(store)
    }

    /// Save `<save>/entities.json` atomically. A clean store is left alone.
    pub fn save(&mut self, save_dir: &Path) -> Result<bool, RuntimeStateError> {
        if !self.dirty {
            return Ok(false);
        }
        std::fs::create_dir_all(save_dir)?;
        let document = EntitiesDocument {
            version: RUNTIME_STATE_VERSION,
            next_id: self.next_id,
            entities: self.entities.values().cloned().collect(),
        };
        write_json_atomic(&save_dir.join(ENTITIES_FILE), &document)?;
        self.dirty = false;
        Ok(true)
    }
}

/// One block's component document.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockComponent {
    /// Block id the component was pasted with.
    pub block_id: i32,
    /// The original `"components"` document.
    pub components: Value,
}

/// Per-position block-component state.
#[derive(Debug, Clone, Default)]
pub struct BlockComponentStore {
    blocks: BTreeMap<(i32, i32, i32), BlockComponent>,
    dirty: bool,
}

impl BlockComponentStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of blocks with components.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether no block has a component.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Whether the store changed since the last save/load.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag after a save.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// One block component.
    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<&BlockComponent> {
        self.blocks.get(&(x, y, z))
    }

    /// Every block component in coordinate order.
    pub fn iter(&self) -> impl Iterator<Item = ((i32, i32, i32), &BlockComponent)> {
        self.blocks
            .iter()
            .map(|(position, component)| (*position, component))
    }

    /// Install/replace a block component.
    pub fn set(&mut self, x: i32, y: i32, z: i32, component: BlockComponent) -> bool {
        let changed = self.blocks.insert((x, y, z), component).is_none();
        self.dirty = true;
        changed
    }

    /// Remove one block component.
    pub fn remove(&mut self, x: i32, y: i32, z: i32) -> Option<BlockComponent> {
        let removed = self.blocks.remove(&(x, y, z));
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    /// Load `<save>/block-components.json` (missing file = empty store).
    pub fn load(save_dir: &Path) -> Result<Self, RuntimeStateError> {
        let path = save_dir.join(BLOCK_COMPONENTS_FILE);
        if !path.is_file() {
            return Ok(Self::new());
        }
        let document: BlocksDocument = serde_json::from_slice(&std::fs::read(&path)?)?;
        if document.version != RUNTIME_STATE_VERSION {
            return Err(RuntimeStateError::UnsupportedVersion {
                version: document.version,
                expected: RUNTIME_STATE_VERSION,
            });
        }
        let mut store = Self::new();
        for record in document.blocks {
            store.blocks.insert(
                (record.x, record.y, record.z),
                BlockComponent {
                    block_id: record.block_id,
                    components: record.components,
                },
            );
        }
        store.dirty = false;
        Ok(store)
    }

    /// Save `<save>/block-components.json` atomically. A clean store is left
    /// alone.
    pub fn save(&mut self, save_dir: &Path) -> Result<bool, RuntimeStateError> {
        if !self.dirty {
            return Ok(false);
        }
        std::fs::create_dir_all(save_dir)?;
        let document = BlocksDocument {
            version: RUNTIME_STATE_VERSION,
            blocks: self
                .blocks
                .iter()
                .map(|(&(x, y, z), component)| BlockRecord {
                    x,
                    y,
                    z,
                    block_id: component.block_id,
                    components: component.components.clone(),
                })
                .collect(),
        };
        write_json_atomic(&save_dir.join(BLOCK_COMPONENTS_FILE), &document)?;
        self.dirty = false;
        Ok(true)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntitiesDocument {
    version: i64,
    next_id: u64,
    entities: Vec<RuntimeEntity>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BlocksDocument {
    version: i64,
    blocks: Vec<BlockRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlockRecord {
    x: i32,
    y: i32,
    z: i32,
    block_id: i32,
    components: Value,
}

/// Write `document` to `path` through a temporary sibling + rename so a crash
/// never leaves a truncated save.
pub(crate) fn write_json_atomic<T: Serialize>(
    path: &Path,
    document: &T,
) -> Result<(), RuntimeStateError> {
    let bytes = serde_json::to_vec(document)?;
    let temporary = temporary_path(path);
    std::fs::write(&temporary, bytes)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".tmp");
    PathBuf::from(name)
}

/// Convenience constructor for a transform.
pub fn transform(position: [f64; 3], rotation: [f32; 3]) -> PlayerTransform {
    PlayerTransform::new(
        Position {
            x: position[0],
            y: position[1],
            z: position[2],
        },
        Rotation {
            pitch: rotation[0],
            yaw: rotation[1],
            roll: rotation[2],
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(kind: RuntimeEntityKind, x: f64, z: f64) -> RuntimeEntity {
        RuntimeEntity::new(
            0,
            kind,
            None,
            transform([x, 64.0, z], [0.0, 0.0, 0.0]),
            Value::Null,
        )
    }

    #[test]
    fn ids_are_stable_and_despawn_removes_entities() {
        let mut store = RuntimeEntityStore::new();
        let first = store.spawn(entity(RuntimeEntityKind::Prefab, 0.0, 0.0));
        let second = store.spawn(entity(RuntimeEntityKind::Item, 2.0, 0.0));
        assert_eq!((first, second), (1, 2));
        assert_eq!(store.len(), 2);
        assert_eq!(
            store
                .in_radius(
                    Position {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0
                    },
                    3.0
                )
                .len(),
            2
        );
        assert_eq!(
            store
                .in_radius(
                    Position {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0
                    },
                    1.0
                )
                .len(),
            1
        );
        assert!(store.despawn(first).is_some());
        assert!(store.despawn(first).is_none());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn runtime_state_round_trips_through_the_save_root() {
        let dir = std::env::temp_dir().join(format!(
            "hytale-runtime-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut entities = RuntimeEntityStore::new();
        let id = entities.spawn(RuntimeEntity::new(
            0,
            RuntimeEntityKind::Prefab,
            Some("Hunt.Generic.Site".to_string()),
            transform([1.5, 64.0, -2.5], [0.0, 1.0, 0.0]),
            serde_json::json!({"Transform": {"Position": {"X": 1.5}}}),
        ));
        let mut blocks = BlockComponentStore::new();
        blocks.set(
            1,
            64,
            -2,
            BlockComponent {
                block_id: 7,
                components: serde_json::json!({"Components": {"BlockMapMarker": {}}}),
            },
        );
        entities.save(&dir).expect("save entities");
        blocks.save(&dir).expect("save blocks");

        let loaded_entities = RuntimeEntityStore::load(&dir).expect("load entities");
        let loaded_blocks = BlockComponentStore::load(&dir).expect("load blocks");
        assert_eq!(loaded_entities.len(), 1);
        assert_eq!(
            loaded_entities.get(id).unwrap().prefab.as_deref(),
            Some("Hunt.Generic.Site")
        );
        assert_eq!(loaded_blocks.len(), 1);
        assert_eq!(loaded_blocks.get(1, 64, -2).unwrap().block_id, 7);
        assert!(!loaded_entities.is_dirty());
        assert!(!loaded_blocks.is_dirty());

        std::fs::remove_dir_all(dir).ok();
    }
}
