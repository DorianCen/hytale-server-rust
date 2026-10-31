//! `SimpleItemContainer` transactions.
//!
//! Ported from `CoreServer/.../inventory/container/SimpleItemContainer.java`,
//! `ItemContainer.java` and `InternalContainerUtilItemStack.java`. The default
//! transaction rules match Java: adds are partial, removals are all-or-nothing,
//! and a target-slot move merges or swaps only when the complete result can be
//! applied atomically.

use std::collections::BTreeMap;

use hytale_protocol::generated::common::{InventorySection, ItemWithAllMetadata};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::item_stack::{ItemError, ItemRules, ItemStack, EMPTY_ITEM_ID};

/// Inventory transaction errors.
#[derive(Debug, Error, PartialEq)]
pub enum InventoryError {
    /// A saved stack is invalid.
    #[error(transparent)]
    Item(#[from] ItemError),
    /// A slot is outside the container capacity.
    #[error("slot {slot} is outside capacity {capacity}")]
    InvalidSlot {
        /// Requested slot.
        slot: u16,
        /// Container capacity.
        capacity: u16,
    },
    /// Quantity must be positive.
    #[error("quantity {0} must be > 0")]
    InvalidQuantity(i32),
    /// The saved container shape is malformed.
    #[error("invalid inventory container: {0}")]
    InvalidContainer(String),
}

/// Result of adding a stack to one slot or a whole container.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AddOutcome {
    /// Items accepted by the container.
    pub added: i32,
    /// Stack that did not fit (Java transaction remainder).
    pub remainder: Option<ItemStack>,
}

impl AddOutcome {
    /// Whether anything was accepted.
    pub const fn changed(&self) -> bool {
        self.added > 0
    }
}

/// Result of moving items between slots.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MoveOutcome {
    /// Items moved to the destination (or swapped).
    pub moved: i32,
    /// A swap happened instead of a merge.
    pub swapped: bool,
    /// Destination remainder left in the source slot.
    pub remainder: Option<ItemStack>,
}

impl MoveOutcome {
    /// Whether the transaction changed either container.
    pub const fn changed(&self) -> bool {
        self.moved > 0 || self.swapped
    }
}

/// One fixed-size item container.
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleItemContainer {
    capacity: u16,
    slots: BTreeMap<u16, ItemStack>,
}

impl SimpleItemContainer {
    /// An empty container with `capacity` slots.
    pub const fn new(capacity: u16) -> Self {
        Self {
            capacity,
            slots: BTreeMap::new(),
        }
    }

    /// Capacity.
    pub const fn capacity(&self) -> u16 {
        self.capacity
    }

    /// Number of occupied slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether every slot is empty.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// One slot's stack.
    pub fn get_slot(&self, slot: u16) -> Result<Option<&ItemStack>, InventoryError> {
        self.check_slot(slot)?;
        Ok(self.slots.get(&slot))
    }

    /// Replace one slot (empty stack means clear).
    pub fn set_slot(
        &mut self,
        slot: u16,
        stack: Option<ItemStack>,
    ) -> Result<Option<ItemStack>, InventoryError> {
        self.check_slot(slot)?;
        match stack {
            Some(stack) => Ok(self.slots.insert(slot, stack)),
            None => Ok(self.slots.remove(&slot)),
        }
    }

    /// Remove and return one slot.
    pub fn remove_slot(&mut self, slot: u16) -> Result<Option<ItemStack>, InventoryError> {
        self.check_slot(slot)?;
        Ok(self.slots.remove(&slot))
    }

    /// Clear every slot, returning the removed stacks in slot order.
    pub fn clear(&mut self) -> Vec<ItemStack> {
        std::mem::take(&mut self.slots).into_values().collect()
    }

    /// Add a stack, filling compatible partial stacks first (slot order), then
    /// empty slots. This is Java `addItemStack` with `fullStacks == false`.
    pub fn add_item_stack(&mut self, mut stack: ItemStack, rules: &dyn ItemRules) -> AddOutcome {
        let max_stack = stack.max_stack(rules).max(1);
        let original = stack.quantity;
        let mut added = 0;

        for slot in 0..self.capacity {
            if stack.quantity <= 0 {
                break;
            }
            let Some(existing) = self.slots.get(&slot).cloned() else {
                continue;
            };
            if !existing.is_stackable_with(&stack, rules) {
                continue;
            }
            let room = (max_stack - existing.quantity).max(0);
            if room <= 0 {
                continue;
            }
            let moved = room.min(stack.quantity);
            if let Some(next) = existing.with_quantity(existing.quantity + moved) {
                self.slots.insert(slot, next);
                stack.quantity -= moved;
                added += moved;
            }
        }

        for slot in 0..self.capacity {
            if stack.quantity <= 0 {
                break;
            }
            if self.slots.contains_key(&slot) {
                continue;
            }
            let moved = max_stack.min(stack.quantity);
            if let Some(next) = stack.with_quantity(moved) {
                self.slots.insert(slot, next);
                stack.quantity -= moved;
                added += moved;
            }
        }

        AddOutcome {
            added,
            remainder: (added < original).then_some(stack),
        }
    }

    /// Add a stack to one specific slot, merging up to the item's max stack.
    pub fn add_to_slot(
        &mut self,
        slot: u16,
        mut stack: ItemStack,
        rules: &dyn ItemRules,
    ) -> Result<AddOutcome, InventoryError> {
        self.check_slot(slot)?;
        let max_stack = stack.max_stack(rules).max(1);
        let Some(existing) = self.slots.get(&slot).cloned() else {
            let moved = max_stack.min(stack.quantity);
            if let Some(next) = stack.with_quantity(moved) {
                self.slots.insert(slot, next);
                stack.quantity -= moved;
            }
            return Ok(AddOutcome {
                added: moved,
                remainder: (stack.quantity > 0).then_some(stack),
            });
        };
        if !existing.is_stackable_with(&stack, rules) {
            return Ok(AddOutcome {
                added: 0,
                remainder: Some(stack),
            });
        }
        let room = (max_stack - existing.quantity).max(0);
        let moved = room.min(stack.quantity);
        if moved <= 0 {
            return Ok(AddOutcome {
                added: 0,
                remainder: Some(stack),
            });
        }
        if let Some(next) = existing.with_quantity(existing.quantity + moved) {
            self.slots.insert(slot, next);
            stack.quantity -= moved;
        }
        Ok(AddOutcome {
            added: moved,
            remainder: (stack.quantity > 0).then_some(stack),
        })
    }

    /// Remove `quantity` from one slot. Java's default is all-or-nothing.
    pub fn remove_from_slot(
        &mut self,
        slot: u16,
        quantity: i32,
    ) -> Result<Option<ItemStack>, InventoryError> {
        self.check_slot(slot)?;
        if quantity <= 0 {
            return Err(InventoryError::InvalidQuantity(quantity));
        }
        let Some(existing) = self.slots.get(&slot).cloned() else {
            return Ok(None);
        };
        if existing.quantity < quantity {
            return Ok(None);
        }
        let removed = existing
            .with_quantity(quantity)
            .expect("quantity is positive");
        if let Some(remaining) = existing.with_quantity(existing.quantity - quantity) {
            self.slots.insert(slot, remaining);
        } else {
            self.slots.remove(&slot);
        }
        Ok(Some(removed))
    }

    /// Move a quantity into one target slot. Either the complete result applies
    /// or neither container changes:
    /// - empty target: move up to the item's max stack;
    /// - same stack: merge up to max stack and leave the overflow in the source;
    /// - different stack: swap only when the whole source stack moves.
    pub fn move_to_slot(
        &mut self,
        from_slot: u16,
        quantity: i32,
        destination: &mut SimpleItemContainer,
        to_slot: u16,
        rules: &dyn ItemRules,
    ) -> Result<MoveOutcome, InventoryError> {
        self.check_slot(from_slot)?;
        destination.check_slot(to_slot)?;
        if quantity <= 0 {
            return Err(InventoryError::InvalidQuantity(quantity));
        }
        let same_container = std::ptr::eq(self, destination);
        if same_container && from_slot == to_slot {
            return Ok(MoveOutcome::default());
        }

        let Some(source) = self.slots.get(&from_slot).cloned() else {
            return Ok(MoveOutcome::default());
        };
        if source.quantity < quantity {
            return Ok(MoveOutcome::default());
        }
        let max_stack = source.max_stack(rules).max(1);
        let target = destination.slots.get(&to_slot).cloned();

        let (source_after, target_after, moved, swapped) = match target {
            None => {
                let moved = max_stack.min(quantity);
                let moved_stack = source.with_quantity(moved);
                let source_after = source.with_quantity(source.quantity - moved);
                (source_after, moved_stack, moved, false)
            }
            Some(target) if target.is_stackable_with(&source, rules) => {
                let room = (max_stack - target.quantity).max(0);
                let moved = room.min(quantity);
                if moved <= 0 {
                    return Ok(MoveOutcome::default());
                }
                let target_after = target.with_quantity(target.quantity + moved);
                let source_after = source.with_quantity(source.quantity - moved);
                (source_after, target_after, moved, false)
            }
            Some(target) => {
                if quantity != source.quantity {
                    return Ok(MoveOutcome::default());
                }
                // Swap whole stacks only.
                (Some(target), Some(source.clone()), source.quantity, true)
            }
        };

        if same_container {
            match source_after {
                Some(stack) => {
                    self.slots.insert(from_slot, stack);
                }
                None => {
                    self.slots.remove(&from_slot);
                }
            }
            match target_after {
                Some(stack) => {
                    self.slots.insert(to_slot, stack);
                }
                None => {
                    self.slots.remove(&to_slot);
                }
            }
        } else {
            match source_after {
                Some(stack) => {
                    self.slots.insert(from_slot, stack);
                }
                None => {
                    self.slots.remove(&from_slot);
                }
            }
            match target_after {
                Some(stack) => {
                    destination.slots.insert(to_slot, stack);
                }
                None => {
                    destination.slots.remove(&to_slot);
                }
            }
        }

        let remainder = if moved < quantity && !swapped {
            source.with_quantity(quantity - moved)
        } else {
            None
        };
        Ok(MoveOutcome {
            moved,
            swapped,
            remainder,
        })
    }

    /// Move a quantity between two slots of this same container.
    pub fn move_within(
        &mut self,
        from_slot: u16,
        quantity: i32,
        to_slot: u16,
        rules: &dyn ItemRules,
    ) -> Result<MoveOutcome, InventoryError> {
        if from_slot == to_slot {
            return Ok(MoveOutcome::default());
        }
        let mut destination = self.clone();
        let outcome = self.move_to_slot(from_slot, quantity, &mut destination, to_slot, rules)?;
        if outcome.changed() {
            let target = destination.get_slot(to_slot)?.cloned();
            self.set_slot(to_slot, target)?;
        }
        Ok(outcome)
    }

    /// Move every stack into another container in slot order.
    pub fn move_all_to(
        &mut self,
        destination: &mut SimpleItemContainer,
        rules: &dyn ItemRules,
    ) -> i32 {
        let mut moved = 0;
        for slot in 0..self.capacity {
            let Some(stack) = self.slots.get(&slot).cloned() else {
                continue;
            };
            let outcome = destination.add_item_stack(stack, rules);
            moved += outcome.added;
            if let Some(remainder) = outcome.remainder {
                self.slots.insert(slot, remainder);
            } else {
                self.slots.remove(&slot);
            }
        }
        moved
    }

    /// Java `InventorySection` packet projection.
    pub fn to_packet(&self) -> InventorySection {
        let mut items = BTreeMap::new();
        for (slot, stack) in &self.slots {
            items.insert(i32::from(*slot), stack.to_packet());
        }
        InventorySection {
            items: Some(items),
            capacity: self.capacity as i16,
        }
    }

    /// The occupied slots in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &ItemStack)> {
        self.slots.iter().map(|(slot, stack)| (*slot, stack))
    }

    /// Find the first slot containing an equivalent stack.
    pub fn find_equivalent(&self, stack: &ItemStack) -> Option<u16> {
        self.slots
            .iter()
            .find(|(_, existing)| existing.is_equivalent_type(stack))
            .map(|(slot, _)| *slot)
    }

    /// Load a `SimpleItemContainer` from the Java component `Inventory` value.
    pub fn from_component_json(value: &Value) -> Result<Self, InventoryError> {
        let object = value
            .as_object()
            .ok_or_else(|| InventoryError::InvalidContainer(value.to_string()))?;
        let capacity = object
            .get("Capacity")
            .and_then(Value::as_i64)
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| InventoryError::InvalidContainer("missing Capacity".to_string()))?;
        let mut container = Self::new(capacity);
        if let Some(items) = object.get("Items").and_then(Value::as_object) {
            for (slot, stack) in items {
                let Ok(slot) = slot.parse::<u16>() else {
                    continue;
                };
                if slot >= capacity {
                    continue;
                }
                if let Some(id) = stack.get("Id").and_then(Value::as_str) {
                    if id.is_empty() || id == EMPTY_ITEM_ID {
                        continue;
                    }
                }
                let stack = ItemStack::from_component_json(stack)?;
                container.slots.insert(slot, stack);
            }
        }
        Ok(container)
    }

    /// Encode in the Java `SimpleItemContainer.CODEC` shape.
    pub fn to_component_json(&self) -> Value {
        let mut items = Map::new();
        for (slot, stack) in &self.slots {
            items.insert(slot.to_string(), stack.to_component_json());
        }
        let mut object = Map::new();
        object.insert("Id".to_string(), Value::String("Simple".to_string()));
        object.insert("Capacity".to_string(), Value::from(self.capacity));
        object.insert("Items".to_string(), Value::Object(items));
        Value::Object(object)
    }

    fn check_slot(&self, slot: u16) -> Result<(), InventoryError> {
        if slot >= self.capacity {
            return Err(InventoryError::InvalidSlot {
                slot,
                capacity: self.capacity,
            });
        }
        Ok(())
    }
}

/// Build a `SimpleItemContainer` from a `capacity` and packet items.
pub fn container_from_packet(section: &InventorySection) -> Result<SimpleItemContainer, ItemError> {
    let mut container =
        SimpleItemContainer::new(u16::try_from(section.capacity.max(0)).unwrap_or(0));
    if let Some(items) = &section.items {
        for (slot, item) in items {
            if *slot < 0 {
                continue;
            }
            let Ok(slot) = u16::try_from(*slot) else {
                continue;
            };
            if slot >= container.capacity() {
                continue;
            }
            if let Some(stack) = item_stack_from_packet(item) {
                let _ = container.set_slot(slot, Some(stack));
            }
        }
    }
    Ok(container)
}

fn item_stack_from_packet(item: &ItemWithAllMetadata) -> Option<ItemStack> {
    let metadata = item
        .metadata
        .as_ref()
        .and_then(|value| serde_json::from_str(value).ok());
    ItemStack::from_fields(
        item.item_id.clone(),
        item.quantity,
        item.durability,
        item.max_durability,
        (item.quality != crate::item_stack::QUALITY_NOT_FOUND).then_some(item.quality),
        metadata,
        item.override_dropped_item_animation,
    )
    .ok()
}
