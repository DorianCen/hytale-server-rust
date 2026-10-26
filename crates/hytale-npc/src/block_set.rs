//! `Server/Item/Block/Sets` block-set membership for spawn floors.
//!
//! Java's `BlockSet` is deprecated and its `Parent` fields are not inherited by
//! the codec (`append`, not `appendInherited`), so each document is evaluated
//! on its own fields. Include/exclude block-type and block-group patterns are
//! supported with `*` globs; hitbox/category filters are preserved but not
//! evaluated yet.

use std::collections::BTreeSet;
use std::path::Path;

use hytale_assets::{AssetPack, AssetRegistry};
use serde_json::Value;
use thiserror::Error;

/// One `BlockSet` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSet {
    /// Asset key.
    pub key: String,
    /// Raw `Parent` (not inherited by the Java codec).
    pub parent: Option<String>,
    /// Java `IncludeAll`.
    pub include_all: bool,
    /// Include/exclude block-type globs.
    pub include_block_types: Vec<String>,
    /// Excluded block-type globs.
    pub exclude_block_types: Vec<String>,
    /// Include/exclude block-group globs.
    pub include_block_groups: Vec<String>,
    /// Excluded block-group globs.
    pub exclude_block_groups: Vec<String>,
    /// Preserved for the later hitbox/category pass.
    pub raw: Value,
}

impl BlockSet {
    /// Parse one block-set document.
    pub fn from_json(key: impl Into<String>, raw: Value) -> Self {
        let strings = |name: &str| -> Vec<String> {
            raw.get(name)
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        Self {
            key: key.into(),
            parent: raw
                .get("Parent")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            include_all: raw
                .get("IncludeAll")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            include_block_types: strings("IncludeBlockTypes"),
            exclude_block_types: strings("ExcludeBlockTypes"),
            include_block_groups: strings("IncludeBlockGroups"),
            exclude_block_groups: strings("ExcludeBlockGroups"),
            raw,
        }
    }

    /// Whether `block_key`/`group` belong to this set.
    pub fn matches(&self, block_key: &str, group: Option<&str>) -> bool {
        let included = self.include_all
            || self
                .include_block_types
                .iter()
                .any(|pattern| wildcard_matches(pattern, block_key))
            || group.is_some_and(|group| {
                self.include_block_groups
                    .iter()
                    .any(|pattern| wildcard_matches(pattern, group))
            });
        if !included {
            return false;
        }
        if self
            .exclude_block_types
            .iter()
            .any(|pattern| wildcard_matches(pattern, block_key))
        {
            return false;
        }
        if group.is_some_and(|group| {
            self.exclude_block_groups
                .iter()
                .any(|pattern| wildcard_matches(pattern, group))
        }) {
            return false;
        }
        true
    }
}

/// All block sets keyed by file stem.
#[derive(Debug, Clone, Default)]
pub struct BlockSetCatalog {
    sets: std::collections::BTreeMap<String, BlockSet>,
}

impl BlockSetCatalog {
    /// Load `Server/Item/Block/Sets/**/*.json`.
    pub fn load(pack: &AssetPack) -> Result<Self, BlockSetError> {
        let mut catalog = Self::default();
        for path in pack.list()? {
            if !path.starts_with("Server/Item/Block/Sets/") || !path.ends_with(".json") {
                continue;
            }
            let key = path
                .rsplit('/')
                .next()
                .and_then(|name| name.strip_suffix(".json"))
                .ok_or_else(|| BlockSetError::InvalidPath(path.clone()))?
                .to_string();
            let raw: Value = serde_json::from_slice(&pack.read(&path)?)?;
            catalog
                .sets
                .insert(key.clone(), BlockSet::from_json(key, raw));
        }
        Ok(catalog)
    }

    /// Load from an extracted asset root.
    pub fn load_root(root: &Path) -> Result<Self, BlockSetError> {
        let pack = AssetPack::open(root)?;
        Self::load(&pack)
    }

    /// One set by key.
    pub fn get(&self, key: &str) -> Option<&BlockSet> {
        self.sets.get(key)
    }

    /// Number of block sets.
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Whether the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Assets in key order.
    pub fn iter(&self) -> impl Iterator<Item = &BlockSet> {
        self.sets.values()
    }

    /// Resolve one set to the loaded registry's block ids.
    pub fn resolve(&self, assets: &AssetRegistry, key: &str) -> Option<BTreeSet<i32>> {
        let set = self.get(key)?;
        let mut ids = BTreeSet::new();
        for record in assets.block_types.iter() {
            let Some(id) = record.id else {
                continue;
            };
            let group = record.value.json.get("Group").and_then(Value::as_str);
            if set.matches(&record.key, group) {
                ids.insert(id);
            }
        }
        Some(ids)
    }
}

/// Match a `*` glob against `text`. `*` may appear anywhere.
pub fn wildcard_matches(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let anchored_start = !pattern.starts_with('*');
    let anchored_end = !pattern.ends_with('*');
    let mut offset = 0usize;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 && anchored_start {
            if !text.starts_with(part) {
                return false;
            }
            offset = part.len();
            continue;
        }
        let Some(found) = text[offset..].find(part) else {
            return false;
        };
        offset += found + part.len();
    }
    !anchored_end || parts.last().is_some_and(|last| text.ends_with(last))
}

/// Block-set loading errors.
#[derive(Debug, Error)]
pub enum BlockSetError {
    /// Asset pack access failed.
    #[error(transparent)]
    Pack(#[from] hytale_assets::AssetError),
    /// A document is not valid JSON.
    #[error("block set is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// An asset path had no file name.
    #[error("invalid block-set asset path `{0}`")]
    InvalidPath(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching_handles_anchors_and_interior_wildcards() {
        assert!(wildcard_matches("Rock*", "Rock_Stone"));
        assert!(!wildcard_matches("Rock*", "Soil_Grass"));
        assert!(wildcard_matches("*Water*", "Plant_Water_Lily"));
        assert!(wildcard_matches("*Mud", "Soil_Mud"));
        assert!(wildcard_matches("Plant_Leaves*", "Plant_Leaves_Oak"));
        assert!(wildcard_matches("*_Trash*", "Goblin_Trash_Pile"));
        assert!(wildcard_matches("*", "anything"));
        assert!(!wildcard_matches("Rock", "Rock_Stone"));
    }

    #[test]
    fn include_exclude_and_include_all_follow_the_document() {
        let set = BlockSet::from_json(
            "Test",
            serde_json::json!({
                "IncludeBlockTypes": ["Rock*", "Soil_*"],
                "ExcludeBlockTypes": ["Soil_Mud"]
            }),
        );
        assert!(set.matches("Rock_Stone", None));
        assert!(set.matches("Soil_Grass", None));
        assert!(!set.matches("Soil_Mud", None));
        assert!(!set.matches("Wood_Oak", None));

        let all = BlockSet::from_json("All", serde_json::json!({"IncludeAll": true}));
        assert!(all.matches("Anything", None));
    }
}
