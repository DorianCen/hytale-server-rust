//! `ItemStack` and the item metadata rules needed by inventory transactions.
//!
//! Ported from `CoreServer/.../inventory/ItemStack.java`. The saved/wire shape
//! matches the Java codec (`Id`, `Quantity`, `Durability`, `MaxDurability`,
//! `QualityOverride`, `Metadata`, `OverrideDroppedItemAnimation`).

use hytale_protocol::generated::common::ItemWithAllMetadata;
use serde_json::{Map, Value};
use thiserror::Error;

/// Java `BlockType.EMPTY_KEY`.
pub const EMPTY_ITEM_ID: &str = "Empty";
/// Java `AssetMapWithIndexes.NOT_FOUND`.
pub const QUALITY_NOT_FOUND: i32 = -1;

/// Item-asset facts used by stack validation and merging.
pub trait ItemRules {
    /// Whether the registry knows `item_id`.
    fn item_exists(&self, item_id: &str) -> bool;
    /// `Item.maxStack` after the Java default/classification rules.
    fn max_stack(&self, item_id: &str) -> i32;
    /// `Item.maxDurability` (0 means unbreakable).
    fn max_durability(&self, item_id: &str) -> f64;
    /// `Item.qualityIndex`, or -1 when absent.
    fn quality_index(&self, _item_id: &str) -> i32 {
        QUALITY_NOT_FOUND
    }
}

/// Errors from item validation.
#[derive(Debug, Error, PartialEq)]
pub enum ItemError {
    /// The id is null/empty/the Java empty sentinel.
    #[error("invalid item id `{0}`")]
    InvalidItemId(String),
    /// The quantity must be positive.
    #[error("quantity {0} must be > 0")]
    InvalidQuantity(i32),
    /// Durability values must be finite and non-negative.
    #[error("invalid durability {0}")]
    InvalidDurability(f64),
    /// The item is not in the asset registry.
    #[error("unknown item `{0}`")]
    UnknownItem(String),
}

/// A quantified item stack.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    /// Item asset id.
    pub item_id: String,
    /// Positive quantity.
    pub quantity: i32,
    /// Current durability (Java initializes to max).
    pub durability: f64,
    /// Maximum durability (0 = unbreakable).
    pub max_durability: f64,
    /// `QualityOverride`; `None` means use the item asset's quality.
    pub quality_override: Option<i32>,
    /// Opaque saved metadata (the Java `Metadata` BSON/JSON document).
    pub metadata: Option<Value>,
    /// `OverrideDroppedItemAnimation`.
    pub override_dropped_item_animation: bool,
}

impl ItemStack {
    /// Build a stack at full durability from item-asset rules (Java constructor).
    pub fn new(
        item_id: impl Into<String>,
        quantity: i32,
        rules: &dyn ItemRules,
    ) -> Result<Self, ItemError> {
        let item_id = item_id.into();
        if item_id.is_empty() || item_id == EMPTY_ITEM_ID {
            return Err(ItemError::InvalidItemId(item_id));
        }
        if quantity <= 0 {
            return Err(ItemError::InvalidQuantity(quantity));
        }
        if !rules.item_exists(&item_id) {
            return Err(ItemError::UnknownItem(item_id));
        }
        let max_durability = rules.max_durability(&item_id);
        Ok(Self {
            item_id,
            quantity,
            durability: max_durability,
            max_durability,
            quality_override: None,
            metadata: None,
            override_dropped_item_animation: false,
        })
    }

    /// Build from explicit saved fields (Java codec path).
    pub fn from_fields(
        item_id: impl Into<String>,
        quantity: i32,
        durability: f64,
        max_durability: f64,
        quality_override: Option<i32>,
        metadata: Option<Value>,
        override_dropped_item_animation: bool,
    ) -> Result<Self, ItemError> {
        let item_id = item_id.into();
        if item_id.is_empty() || item_id == EMPTY_ITEM_ID {
            return Err(ItemError::InvalidItemId(item_id));
        }
        if quantity <= 0 {
            return Err(ItemError::InvalidQuantity(quantity));
        }
        if !durability.is_finite() || durability < 0.0 {
            return Err(ItemError::InvalidDurability(durability));
        }
        if !max_durability.is_finite() || max_durability < 0.0 {
            return Err(ItemError::InvalidDurability(max_durability));
        }
        Ok(Self {
            item_id,
            quantity,
            durability: durability.min(max_durability),
            max_durability,
            quality_override,
            metadata,
            override_dropped_item_animation,
        })
    }

    /// Decode one saved `ItemStack` JSON object.
    pub fn from_component_json(value: &Value) -> Result<Self, ItemError> {
        let Some(object) = value.as_object() else {
            return Err(ItemError::InvalidItemId(value.to_string()));
        };
        let item_id = object
            .get("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let quantity = object
            .get("Quantity")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(1);
        let durability = object
            .get("Durability")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let max_durability = object
            .get("MaxDurability")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let quality_override = object
            .get("QualityOverride")
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok());
        let metadata = object.get("Metadata").cloned();
        let override_dropped_item_animation = object
            .get("OverrideDroppedItemAnimation")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Self::from_fields(
            item_id,
            quantity,
            durability,
            max_durability,
            quality_override,
            metadata,
            override_dropped_item_animation,
        )
    }

    /// Encode in the Java `ItemStack.CODEC` shape.
    pub fn to_component_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("Id".to_string(), Value::String(self.item_id.clone()));
        object.insert("Quantity".to_string(), Value::from(self.quantity));
        object.insert("Durability".to_string(), Value::from(self.durability));
        object.insert(
            "MaxDurability".to_string(),
            Value::from(self.max_durability),
        );
        if let Some(quality) = self.quality_override {
            object.insert("QualityOverride".to_string(), Value::from(quality));
        }
        if let Some(metadata) = &self.metadata {
            object.insert("Metadata".to_string(), metadata.clone());
        }
        object.insert(
            "OverrideDroppedItemAnimation".to_string(),
            Value::Bool(self.override_dropped_item_animation),
        );
        Value::Object(object)
    }

    /// Encode for the `ItemWithAllMetadata` packet field.
    pub fn to_packet(&self) -> ItemWithAllMetadata {
        ItemWithAllMetadata {
            item_id: self.item_id.clone(),
            quantity: self.quantity,
            durability: self.durability,
            max_durability: self.max_durability,
            quality: self.quality_override.unwrap_or(QUALITY_NOT_FOUND),
            override_dropped_item_animation: self.override_dropped_item_animation,
            metadata: self.metadata.as_ref().map(|metadata| metadata.to_string()),
        }
    }

    /// A copy with a different quantity; `None` for zero (Java `withQuantity`).
    pub fn with_quantity(&self, quantity: i32) -> Option<Self> {
        if quantity <= 0 {
            return None;
        }
        if quantity == self.quantity {
            return Some(self.clone());
        }
        let mut stack = self.clone();
        stack.quantity = quantity;
        Some(stack)
    }

    /// Whether two stacks may merge (Java `isStackableWith`).
    pub fn is_stackable_with(&self, other: &Self, rules: &dyn ItemRules) -> bool {
        self.item_id == other.item_id
            && self.durability == other.durability
            && self.max_durability == other.max_durability
            && self.quality(rules) == other.quality(rules)
            && self.metadata == other.metadata
    }

    /// Whether two stacks share an item type (Java `isEquivalentType`).
    pub fn is_equivalent_type(&self, other: &Self) -> bool {
        self.item_id == other.item_id && self.metadata == other.metadata
    }

    /// Effective quality index.
    pub fn quality(&self, rules: &dyn ItemRules) -> i32 {
        self.quality_override
            .unwrap_or_else(|| rules.quality_index(&self.item_id))
    }

    /// Maximum stack size for this item.
    pub fn max_stack(&self, rules: &dyn ItemRules) -> i32 {
        rules.max_stack(&self.item_id)
    }

    /// Java `isUnbreakable`.
    pub const fn is_unbreakable(&self) -> bool {
        self.max_durability <= 0.0
    }

    /// Java `isBroken`.
    pub fn is_broken(&self) -> bool {
        !self.is_unbreakable() && self.durability == 0.0
    }
}
