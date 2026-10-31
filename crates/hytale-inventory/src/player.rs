//! Player inventory components and save integration.
//!
//! The save shape mirrors Java `InventoryComponent` / `SimpleItemContainer`:
//! each component is `{"Inventory": {...}, "ActiveSlot": n}`. Modern
//! `HotbarInventory`/`UtilityInventory`/`ArmorInventory` components win over the
//! legacy `Inventory` document, matching the player-data decoder.

use std::collections::BTreeSet;

use hytale_protocol::generated::common::InventorySection;
use hytale_protocol::generated::inventory::UpdatePlayerInventory;
use serde_json::{Map, Value};

use crate::container::{InventoryError, SimpleItemContainer};
use crate::item_stack::{ItemRules, ItemStack, EMPTY_ITEM_ID};

/// Java section ids used by `SetActiveSlot`/`MoveItemStack`.
pub const HOTBAR_SECTION_ID: i32 = -1;
/// Java section id for storage.
pub const STORAGE_SECTION_ID: i32 = -2;
/// Java section id for armor.
pub const ARMOR_SECTION_ID: i32 = -3;
/// Java section id for utility items.
pub const UTILITY_SECTION_ID: i32 = -5;
/// Java section id for tools.
pub const TOOLS_SECTION_ID: i32 = -8;
/// Java section id for the backpack.
pub const BACKPACK_SECTION_ID: i32 = -9;

/// Java default hotbar capacity.
pub const HOTBAR_CAPACITY: u16 = 9;
/// Java default utility capacity.
pub const UTILITY_CAPACITY: u16 = 4;
/// Java default armor capacity.
pub const ARMOR_CAPACITY: u16 = 4;
/// Java default storage capacity.
pub const STORAGE_CAPACITY: u16 = 36;
/// Java default tools capacity.
pub const TOOLS_CAPACITY: u16 = 23;

/// Component id of the modern hotbar inventory.
pub const HOTBAR_COMPONENT: &str = "HotbarInventory";
/// Component id of the modern utility inventory.
pub const UTILITY_COMPONENT: &str = "UtilityInventory";
/// Component id of the modern armor inventory.
pub const ARMOR_COMPONENT: &str = "ArmorInventory";
/// Component id of the modern storage inventory.
pub const STORAGE_COMPONENT: &str = "StorageInventory";
/// Component id of the modern tools inventory.
pub const TOOLS_COMPONENT: &str = "ToolsInventory";
/// Component id of the modern backpack inventory.
pub const BACKPACK_COMPONENT: &str = "BackpackInventory";
/// Legacy combined inventory component.
pub const LEGACY_INVENTORY_COMPONENT: &str = "Inventory";

/// One inventory section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    /// Hotbar.
    Hotbar,
    /// Storage.
    Storage,
    /// Armor.
    Armor,
    /// Utility.
    Utility,
    /// Tools.
    Tools,
    /// Backpack.
    Backpack,
}

impl Section {
    /// Java section id.
    pub const fn section_id(self) -> i32 {
        match self {
            Self::Hotbar => HOTBAR_SECTION_ID,
            Self::Storage => STORAGE_SECTION_ID,
            Self::Armor => ARMOR_SECTION_ID,
            Self::Utility => UTILITY_SECTION_ID,
            Self::Tools => TOOLS_SECTION_ID,
            Self::Backpack => BACKPACK_SECTION_ID,
        }
    }

    /// Resolve a section id.
    pub const fn from_section_id(id: i32) -> Option<Self> {
        match id {
            HOTBAR_SECTION_ID => Some(Self::Hotbar),
            STORAGE_SECTION_ID => Some(Self::Storage),
            ARMOR_SECTION_ID => Some(Self::Armor),
            UTILITY_SECTION_ID => Some(Self::Utility),
            TOOLS_SECTION_ID => Some(Self::Tools),
            BACKPACK_SECTION_ID => Some(Self::Backpack),
            _ => None,
        }
    }

    /// Modern component id.
    pub const fn component(self) -> &'static str {
        match self {
            Self::Hotbar => HOTBAR_COMPONENT,
            Self::Storage => STORAGE_COMPONENT,
            Self::Armor => ARMOR_COMPONENT,
            Self::Utility => UTILITY_COMPONENT,
            Self::Tools => TOOLS_COMPONENT,
            Self::Backpack => BACKPACK_COMPONENT,
        }
    }

    /// Default capacity for this section.
    pub const fn default_capacity(self) -> u16 {
        match self {
            Self::Hotbar => HOTBAR_CAPACITY,
            Self::Storage => STORAGE_CAPACITY,
            Self::Armor => ARMOR_CAPACITY,
            Self::Utility => UTILITY_CAPACITY,
            Self::Tools => TOOLS_CAPACITY,
            Self::Backpack => 0,
        }
    }

    /// Whether this section tracks an active slot.
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Hotbar | Self::Utility | Self::Tools)
    }
}

/// A container plus optional active slot.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveContainer {
    /// Item container.
    pub container: SimpleItemContainer,
    /// Active slot; `None` means Java's `INACTIVE_SLOT_INDEX`.
    pub active_slot: Option<u8>,
}

impl ActiveContainer {
    /// Build an empty active container.
    pub const fn new(capacity: u16, active_slot: Option<u8>) -> Self {
        Self {
            container: SimpleItemContainer::new(capacity),
            active_slot,
        }
    }

    /// Clamp the active slot using Java's rules.
    pub fn clamp_active(&mut self, default_zero: bool) {
        let capacity = self.container.capacity();
        if self
            .active_slot
            .is_none_or(|slot| u16::from(slot) >= capacity)
        {
            self.active_slot = if default_zero && capacity > 0 {
                Some(0)
            } else {
                None
            };
        }
    }

    /// Active item.
    pub fn active_item(&self) -> Option<&ItemStack> {
        self.active_slot
            .and_then(|slot| self.container.get_slot(u16::from(slot)).ok().flatten())
            .filter(|stack| stack.item_id != EMPTY_ITEM_ID)
    }
}

/// All player inventory components.
#[derive(Debug, Clone)]
pub struct PlayerInventory {
    /// Hotbar (right-hand item comes from the active slot).
    pub hotbar: ActiveContainer,
    /// Utility items (left-hand item comes from the active slot).
    pub utility: ActiveContainer,
    /// Armor slots.
    pub armor: SimpleItemContainer,
    /// Storage slots.
    pub storage: SimpleItemContainer,
    /// Tool slots.
    pub tools: ActiveContainer,
    /// Backpack slots.
    pub backpack: SimpleItemContainer,
    present: BTreeSet<Section>,
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for PlayerInventory {
    fn eq(&self, other: &Self) -> bool {
        self.hotbar == other.hotbar
            && self.utility == other.utility
            && self.armor == other.armor
            && self.storage == other.storage
            && self.tools == other.tools
            && self.backpack == other.backpack
    }
}

impl PlayerInventory {
    /// Empty Java-default inventory (hotbar active on slot 0).
    pub fn new() -> Self {
        Self {
            hotbar: ActiveContainer::new(HOTBAR_CAPACITY, Some(0)),
            utility: ActiveContainer::new(UTILITY_CAPACITY, None),
            armor: SimpleItemContainer::new(ARMOR_CAPACITY),
            storage: SimpleItemContainer::new(STORAGE_CAPACITY),
            tools: ActiveContainer::new(TOOLS_CAPACITY, None),
            backpack: SimpleItemContainer::new(0),
            present: BTreeSet::new(),
        }
    }

    /// Load the modern/legacy inventory components from a player `Components` map.
    pub fn from_components(components: &Map<String, Value>) -> Result<Self, InventoryError> {
        let mut inventory = Self::new();
        let legacy = components.get(LEGACY_INVENTORY_COMPONENT);

        inventory.load_section(components, legacy, Section::Hotbar)?;
        inventory.load_section(components, legacy, Section::Utility)?;
        inventory.load_section(components, legacy, Section::Armor)?;
        inventory.load_section(components, legacy, Section::Storage)?;
        inventory.load_section(components, legacy, Section::Tools)?;
        inventory.load_section(components, legacy, Section::Backpack)?;
        Ok(inventory)
    }

    /// Write modern components back into a player `Components` map.
    pub fn write_components(&self, components: &mut Map<String, Value>) {
        for section in [
            Section::Hotbar,
            Section::Utility,
            Section::Armor,
            Section::Storage,
            Section::Tools,
            Section::Backpack,
        ] {
            let container = self.container(section);
            if !self.present.contains(&section) && container.is_empty() {
                continue;
            }
            let mut value = Map::new();
            value.insert("Inventory".to_string(), container.to_component_json());
            if section.is_active() {
                let active = self
                    .active_container(section)
                    .and_then(|container| container.active_slot)
                    .map_or(-1, i32::from);
                value.insert("ActiveSlot".to_string(), Value::from(active));
            }
            components.insert(section.component().to_string(), Value::Object(value));
        }
    }

    /// One section's container.
    pub const fn container(&self, section: Section) -> &SimpleItemContainer {
        match section {
            Section::Hotbar => &self.hotbar.container,
            Section::Storage => &self.storage,
            Section::Armor => &self.armor,
            Section::Utility => &self.utility.container,
            Section::Tools => &self.tools.container,
            Section::Backpack => &self.backpack,
        }
    }

    /// Mutable section container.
    pub fn container_mut(&mut self, section: Section) -> &mut SimpleItemContainer {
        self.present.insert(section);
        match section {
            Section::Hotbar => &mut self.hotbar.container,
            Section::Storage => &mut self.storage,
            Section::Armor => &mut self.armor,
            Section::Utility => &mut self.utility.container,
            Section::Tools => &mut self.tools.container,
            Section::Backpack => &mut self.backpack,
        }
    }

    /// Active container for hotbar/utility/tools.
    pub const fn active_container(&self, section: Section) -> Option<&ActiveContainer> {
        match section {
            Section::Hotbar => Some(&self.hotbar),
            Section::Utility => Some(&self.utility),
            Section::Tools => Some(&self.tools),
            _ => None,
        }
    }

    /// Mutable active container.
    pub fn active_container_mut(&mut self, section: Section) -> Option<&mut ActiveContainer> {
        self.present.insert(section);
        match section {
            Section::Hotbar => Some(&mut self.hotbar),
            Section::Utility => Some(&mut self.utility),
            Section::Tools => Some(&mut self.tools),
            _ => None,
        }
    }

    /// Set one active slot using Java clamping rules.
    pub fn set_active_slot(&mut self, section: Section, slot: i32) -> bool {
        let Some(active) = self.active_container_mut(section) else {
            return false;
        };
        let capacity = active.container.capacity();
        if slot < 0 || u16::try_from(slot).map_or(true, |slot| slot >= capacity) {
            active.clamp_active(section == Section::Hotbar);
            return false;
        }
        let next = u8::try_from(slot).expect("capacity fits in u8");
        if active.active_slot == Some(next) {
            return false;
        }
        active.active_slot = Some(next);
        true
    }

    /// Total quantity of one item id across every inventory section.
    pub fn count_item(&self, item_id: &str) -> i32 {
        self.sections()
            .map(|section| {
                self.container(section)
                    .iter()
                    .filter(|(_, stack)| stack.item_id == item_id)
                    .map(|(_, stack)| stack.quantity)
                    .sum::<i32>()
            })
            .sum()
    }

    /// Remove up to `quantity` of one item across every section in Java's
    /// hotbar/utility/armor/storage/tools/backpack order. Returns the removed
    /// quantity.
    pub fn remove_item(&mut self, item_id: &str, quantity: i32) -> i32 {
        let mut remaining = quantity.max(0);
        for section in [
            Section::Hotbar,
            Section::Utility,
            Section::Armor,
            Section::Storage,
            Section::Tools,
            Section::Backpack,
        ] {
            if remaining <= 0 {
                break;
            }
            let container = self.container_mut(section);
            let slots: Vec<u16> = container
                .iter()
                .filter(|(_, stack)| stack.item_id == item_id)
                .map(|(slot, _)| slot)
                .collect();
            for slot in slots {
                if remaining <= 0 {
                    break;
                }
                let available = container
                    .get_slot(slot)
                    .ok()
                    .flatten()
                    .map_or(0, |stack| stack.quantity);
                let take = available.min(remaining);
                if take <= 0 {
                    continue;
                }
                let Ok(Some(mut stack)) = container.remove_from_slot(slot, take) else {
                    continue;
                };
                remaining -= take;
                if stack.quantity > take {
                    stack.quantity -= take;
                    let _ = container.set_slot(slot, Some(stack));
                }
            }
        }
        quantity.max(0) - remaining
    }

    /// The six player sections in deterministic order.
    pub fn sections(&self) -> impl Iterator<Item = Section> {
        [
            Section::Hotbar,
            Section::Utility,
            Section::Armor,
            Section::Storage,
            Section::Tools,
            Section::Backpack,
        ]
        .into_iter()
    }

    /// The active hotbar item (Java right hand).
    pub fn right_hand(&self) -> Option<&ItemStack> {
        self.hotbar.active_item()
    }

    /// The active utility item (Java left hand).
    pub fn left_hand(&self) -> Option<&ItemStack> {
        self.utility.active_item()
    }

    /// Remove one item from the active hotbar slot.
    pub fn consume_right_hand(
        &mut self,
        quantity: i32,
    ) -> Result<Option<ItemStack>, InventoryError> {
        let Some(slot) = self.hotbar.active_slot else {
            return Ok(None);
        };
        let removed = self
            .hotbar
            .container
            .remove_from_slot(u16::from(slot), quantity)?;
        if removed.is_some() {
            self.present.insert(Section::Hotbar);
        }
        Ok(removed)
    }

    /// Pick up a stack using Java's hotbar → storage → backpack priority.
    pub fn pickup(&mut self, stack: ItemStack, rules: &dyn ItemRules) -> Option<ItemStack> {
        let mut remainder = stack;
        for section in [Section::Hotbar, Section::Storage, Section::Backpack] {
            let container = self.container_mut(section);
            let outcome = container.add_item_stack(remainder, rules);
            remainder = outcome.remainder?;
        }
        Some(remainder)
    }

    /// Drop/remove a quantity from a section slot (data-level drop).
    pub fn drop_from_slot(
        &mut self,
        section: Section,
        slot: u16,
        quantity: i32,
    ) -> Result<Option<ItemStack>, InventoryError> {
        let removed = self
            .container_mut(section)
            .remove_from_slot(slot, quantity)?;
        if removed.is_some() {
            self.present.insert(section);
        }
        Ok(removed)
    }

    /// Java `UpdatePlayerInventory` packet with every standard section.
    pub fn to_update_packet(&self) -> UpdatePlayerInventory {
        UpdatePlayerInventory {
            storage: Some(self.storage.to_packet()),
            armor: Some(self.armor.to_packet()),
            hotbar: Some(self.hotbar.container.to_packet()),
            utility: Some(self.utility.container.to_packet()),
            tools: Some(self.tools.container.to_packet()),
            backpack: Some(self.backpack.to_packet()),
            ability_slots: None,
            rune_bag: None,
        }
    }

    /// A section as a packet.
    pub fn section_packet(&self, section: Section) -> InventorySection {
        self.container(section).to_packet()
    }

    fn load_section(
        &mut self,
        components: &Map<String, Value>,
        legacy: Option<&Value>,
        section: Section,
    ) -> Result<(), InventoryError> {
        let component = components.get(section.component()).or_else(|| {
            let key = match section {
                Section::Hotbar => "HotBar",
                Section::Armor => "Armor",
                Section::Utility => "Utility",
                Section::Storage => "Storage",
                Section::Tools => "Tool",
                Section::Backpack => "Backpack",
            };
            legacy?.get(key)
        });
        let Some(component) = component else {
            return Ok(());
        };
        let container_value = if component.get("Inventory").is_some() {
            component.get("Inventory").expect("checked above")
        } else {
            component
        };
        let container = SimpleItemContainer::from_component_json(container_value)?;
        let active = component
            .get("ActiveSlot")
            .or_else(|| {
                let key = match section {
                    Section::Hotbar => "ActiveHotbarSlot",
                    Section::Utility => "ActiveUtilitySlot",
                    Section::Tools => "ActiveToolsSlot",
                    _ => return None,
                };
                legacy?.get(key)
            })
            .and_then(Value::as_i64)
            .and_then(|slot| i32::try_from(slot).ok())
            .and_then(|slot| u8::try_from(slot).ok());
        self.replace_section(section, container, active);
        self.present.insert(section);
        Ok(())
    }

    fn replace_section(
        &mut self,
        section: Section,
        container: SimpleItemContainer,
        active: Option<u8>,
    ) {
        match section {
            Section::Hotbar => {
                self.hotbar = ActiveContainer {
                    container,
                    active_slot: active,
                };
                self.hotbar.clamp_active(true);
            }
            Section::Utility => {
                self.utility = ActiveContainer {
                    container,
                    active_slot: active,
                };
                self.utility.clamp_active(false);
            }
            Section::Tools => {
                self.tools = ActiveContainer {
                    container,
                    active_slot: active,
                };
                self.tools.clamp_active(false);
            }
            Section::Armor => self.armor = container,
            Section::Storage => self.storage = container,
            Section::Backpack => self.backpack = container,
        }
    }
}
