//! Inventory data model and transactions.
//!
//! This crate is the pure Rust port of the Java `ItemStack`,
//! `SimpleItemContainer` and player inventory components. Server/session state
//! owns a [`PlayerInventory`]; the world thread loads/saves it through the
//! player holder components. Item max-stack/durability facts come from an
//! [`ItemRules`] implementation backed by the asset registry.

#![deny(clippy::all)]

pub mod container;
pub mod item_stack;
pub mod player;

pub use container::{
    container_from_packet, AddOutcome, InventoryError, MoveOutcome, SimpleItemContainer,
};
pub use item_stack::{ItemError, ItemRules, ItemStack, EMPTY_ITEM_ID, QUALITY_NOT_FOUND};
pub use player::{
    ActiveContainer, PlayerInventory, Section, ARMOR_CAPACITY, ARMOR_COMPONENT, ARMOR_SECTION_ID,
    BACKPACK_COMPONENT, BACKPACK_SECTION_ID, HOTBAR_CAPACITY, HOTBAR_COMPONENT, HOTBAR_SECTION_ID,
    LEGACY_INVENTORY_COMPONENT, STORAGE_CAPACITY, STORAGE_COMPONENT, STORAGE_SECTION_ID,
    TOOLS_CAPACITY, TOOLS_COMPONENT, TOOLS_SECTION_ID, UTILITY_CAPACITY, UTILITY_COMPONENT,
    UTILITY_SECTION_ID,
};
