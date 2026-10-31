//! Block container inventories.
//!
//! Java stores a container block's items in the `ItemContainerBlock` block
//! component and persists it through the ECS chunk store. The Rust port keeps
//! the same state in a small JSON sidecar keyed by block position; chunks stay
//! Java-readable and this state is not embedded in the region files (the same
//! T56 deviation as runtime entities and block components).
//!
//! T57b lands the presentation seam: opening a container sends its
//! `InventorySection`, and the items survive save/reopen. T60b adds live
//! `MoveItemStack` transactions through the world-thread container accessors.

use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::path::Path;

use hytale_inventory::{InventoryError, SimpleItemContainer};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::{write_json_atomic, RuntimeStateError, RUNTIME_STATE_VERSION};

/// File name of the block-container store.
pub const BLOCK_CONTAINERS_FILE: &str = "block-containers.json";

/// Per-position block container inventories.
#[derive(Debug, Clone, Default)]
pub struct BlockContainerStore {
    containers: BTreeMap<(i32, i32, i32), SimpleItemContainer>,
    dirty: bool,
}

#[derive(Serialize, Deserialize)]
struct ContainersDocument {
    version: i64,
    containers: Vec<StoredContainer>,
}

#[derive(Serialize, Deserialize)]
struct StoredContainer {
    x: i32,
    y: i32,
    z: i32,
    inventory: Value,
}

impl BlockContainerStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the store changed since the last save/load.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag after a save.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Number of stored containers.
    pub fn len(&self) -> usize {
        self.containers.len()
    }

    /// Whether no container is stored.
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
    }

    /// One block's container.
    pub fn get(&self, x: i32, y: i32, z: i32) -> Option<&SimpleItemContainer> {
        self.containers.get(&(x, y, z))
    }

    /// Mutable access to one block's container (marks the store dirty).
    pub fn get_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut SimpleItemContainer> {
        self.dirty = true;
        self.containers.get_mut(&(x, y, z))
    }

    /// Install/replace one block's container.
    pub fn set(&mut self, x: i32, y: i32, z: i32, container: SimpleItemContainer) {
        self.containers.insert((x, y, z), container);
        self.dirty = true;
    }

    /// Get the container at a position, creating an empty one with `capacity`.
    pub fn get_or_create(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        capacity: u16,
    ) -> &mut SimpleItemContainer {
        if let Entry::Vacant(entry) = self.containers.entry((x, y, z)) {
            entry.insert(SimpleItemContainer::new(capacity));
            self.dirty = true;
        }
        self.containers
            .get_mut(&(x, y, z))
            .expect("container inserted above")
    }

    /// Remove one block's container.
    pub fn remove(&mut self, x: i32, y: i32, z: i32) -> Option<SimpleItemContainer> {
        let removed = self.containers.remove(&(x, y, z));
        if removed.is_some() {
            self.dirty = true;
        }
        removed
    }

    /// Every stored container in position order.
    pub fn iter(&self) -> impl Iterator<Item = ((i32, i32, i32), &SimpleItemContainer)> {
        self.containers
            .iter()
            .map(|(position, container)| (*position, container))
    }

    /// Load `<save>/block-containers.json` (missing file = empty store).
    pub fn load(save_dir: &Path) -> Result<Self, RuntimeStateError> {
        let path = save_dir.join(BLOCK_CONTAINERS_FILE);
        if !path.is_file() {
            return Ok(Self::new());
        }
        let document: ContainersDocument = serde_json::from_slice(&std::fs::read(&path)?)?;
        if document.version != RUNTIME_STATE_VERSION {
            return Err(RuntimeStateError::UnsupportedVersion {
                version: document.version,
                expected: RUNTIME_STATE_VERSION,
            });
        }
        let mut store = Self::new();
        for record in document.containers {
            let container = SimpleItemContainer::from_component_json(&record.inventory)
                .map_err(container_state_error)?;
            store
                .containers
                .insert((record.x, record.y, record.z), container);
        }
        store.dirty = false;
        Ok(store)
    }

    /// Save `<save>/block-containers.json` atomically. A clean store is left
    /// alone.
    pub fn save(&mut self, save_dir: &Path) -> Result<bool, RuntimeStateError> {
        if !self.dirty {
            return Ok(false);
        }
        std::fs::create_dir_all(save_dir)?;
        let document = ContainersDocument {
            version: RUNTIME_STATE_VERSION,
            containers: self
                .containers
                .iter()
                .map(|((x, y, z), container)| StoredContainer {
                    x: *x,
                    y: *y,
                    z: *z,
                    inventory: container.to_component_json(),
                })
                .collect(),
        };
        write_json_atomic(&save_dir.join(BLOCK_CONTAINERS_FILE), &document)?;
        self.dirty = false;
        Ok(true)
    }
}

fn container_state_error(error: InventoryError) -> RuntimeStateError {
    RuntimeStateError::Container(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_inventory::ItemStack;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hytale-containers-{}-{nonce}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn container_items_survive_save_and_load() {
        let dir = temp_dir("roundtrip");
        let stack = ItemStack::from_component_json(&serde_json::json!({
            "Id": "Rock_Stone",
            "Quantity": 3,
        }))
        .expect("stack");
        let mut store = BlockContainerStore::new();
        store
            .get_or_create(4, 200, -3, 36)
            .set_slot(0, Some(stack))
            .expect("set slot");
        assert!(store.save(&dir).expect("save"));
        assert!(!store.save(&dir).expect("second save"));

        let loaded = BlockContainerStore::load(&dir).expect("load");
        let container = loaded.get(4, 200, -3).expect("container");
        assert_eq!(container.capacity(), 36);
        assert_eq!(
            container
                .get_slot(0)
                .expect("slot")
                .map(|stack| (stack.item_id.as_str(), stack.quantity)),
            Some(("Rock_Stone", 3))
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn removing_a_container_marks_the_store_dirty() {
        let mut store = BlockContainerStore::new();
        store.get_or_create(1, 2, 3, 4);
        assert!(store.is_dirty());
        store.clear_dirty();
        assert!(store.remove(1, 2, 3).is_some());
        assert!(store.is_dirty());
        assert!(store.remove(1, 2, 3).is_none());
    }
}
