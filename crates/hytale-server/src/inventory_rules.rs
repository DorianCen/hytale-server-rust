//! `ItemRules` backed by the decoded `Server/Item/Items` registry.
//!
//! Inventory transactions need max stack/durability facts. Java reads them from
//! `Item`; this adapter keeps the inventory crate asset-free while the server
//! passes the loaded registry through a borrowed view.

use hytale_assets::AssetRegistry;
use hytale_inventory::{ItemRules, QUALITY_NOT_FOUND};
/// Borrowed item rules over one asset registry.
#[derive(Debug, Clone, Copy)]
pub struct AssetItemRules<'a> {
    assets: &'a AssetRegistry,
}

impl<'a> AssetItemRules<'a> {
    /// Wrap an asset registry.
    pub fn new(assets: &'a AssetRegistry) -> Self {
        Self { assets }
    }

    fn value(&self, item_id: &str) -> Option<&serde_json::Value> {
        self.assets.items.get(item_id).map(|item| &item.value)
    }
}

impl ItemRules for AssetItemRules<'_> {
    fn item_exists(&self, item_id: &str) -> bool {
        self.assets.items.get(item_id).is_some()
    }

    fn max_stack(&self, item_id: &str) -> i32 {
        self.value(item_id)
            .and_then(|value| value.get("MaxStack"))
            .and_then(serde_json::Value::as_i64)
            .filter(|value| *value > 0)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(100)
    }

    fn max_durability(&self, item_id: &str) -> f64 {
        self.value(item_id)
            .and_then(|value| value.get("MaxDurability"))
            .and_then(serde_json::Value::as_f64)
            .filter(|value| *value > 0.0)
            .unwrap_or(0.0)
    }

    fn quality_index(&self, item_id: &str) -> i32 {
        self.value(item_id)
            .and_then(|value| value.get("QualityIndex"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or(QUALITY_NOT_FOUND)
    }
}
