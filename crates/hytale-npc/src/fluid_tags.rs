//! Fluid tag membership for spawn constraints.
//!
//! Java resolves `RoleSpawnParameters.SpawnFluidTag` through
//! `Fluid.getAssetMap().getIndexesForTag(tag)`. This module builds the same
//! `Tag -> fluid ids` index from the real `Fluid` store (`Tags.Fluid` on the
//! merged documents).

use std::collections::{BTreeMap, BTreeSet};

use hytale_assets::AssetRegistry;
use serde_json::Value;

/// Fluid ids grouped by tag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FluidTagIndex {
    tags: BTreeMap<String, BTreeSet<i32>>,
}

impl FluidTagIndex {
    /// Build the index from the loaded `Fluid` asset store.
    pub fn from_assets(assets: &AssetRegistry) -> Self {
        let mut tags: BTreeMap<String, BTreeSet<i32>> = BTreeMap::new();
        for record in assets.fluids.iter() {
            let Some(id) = record.id else {
                continue;
            };
            let Some(values) = record
                .value
                .json
                .get("Tags")
                .and_then(|tags| tags.get("Fluid"))
                .and_then(Value::as_array)
            else {
                continue;
            };
            for tag in values.iter().filter_map(Value::as_str) {
                tags.entry(tag.to_string()).or_default().insert(id);
            }
        }
        Self { tags }
    }

    /// Build from explicit tag/id pairs (tests and asset-mod seams).
    pub fn from_map(tags: BTreeMap<String, BTreeSet<i32>>) -> Self {
        Self { tags }
    }

    /// Fluid ids for `tag`.
    pub fn get(&self, tag: &str) -> Option<&BTreeSet<i32>> {
        self.tags.get(tag)
    }

    /// Whether `fluid_id` belongs to `tag`.
    pub fn contains(&self, tag: &str, fluid_id: i32) -> bool {
        self.tags
            .get(tag)
            .is_some_and(|fluids| fluids.contains(&fluid_id))
    }

    /// Number of tags.
    pub fn len(&self) -> usize {
        self.tags.len()
    }

    /// Whether no tag exists.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    /// Tags in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &BTreeSet<i32>)> {
        self.tags.iter().map(|(tag, fluids)| (tag.as_str(), fluids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_lookup_uses_the_explicit_index() {
        let index = FluidTagIndex::from_map(BTreeMap::from([
            ("Water".to_string(), BTreeSet::from([1, 2])),
            ("Lava".to_string(), BTreeSet::from([3])),
        ]));
        assert!(index.contains("Water", 1));
        assert!(!index.contains("Water", 3));
        assert!(index.contains("Lava", 3));
        assert_eq!(index.get("Lava"), Some(&BTreeSet::from([3])));
        assert_eq!(index.len(), 2);
    }
}
