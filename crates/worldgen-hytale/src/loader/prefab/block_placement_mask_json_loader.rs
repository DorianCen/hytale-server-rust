// Ported from: `.hytale-source/.../worldgen/loader/prefab/BlockPlacementMaskJsonLoader.java`
//! Java `loader.prefab.BlockPlacementMaskJsonLoader` — the block mask of a
//! cave type, of a cave prefab configuration and of a prefab cave node.
//!
//! The mask has a *default* rule (the blocks the cave may replace) and a
//! list of *specific* rules (the blocks that get their own rule).
//!
//! # Deviations
//!
//! * The Java loader caches the masks and the mask entries in the global
//!   `BlockPlacementMaskRegistry` (a pool of value-equal objects). The cache
//!   is unobservable — the pooled objects are value-equal to the freshly
//!   built ones — so it is not ported.
//! * The Java `loadFileConstructor` override caches the *file* of a
//!   `"File"` reference; the Rust port resolves the reference through
//!   [`JsonLoaderBase`] (the resolved document is the same).

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::loader::util::resolved_block_array_json_loader::ResolvedBlockArrayJsonLoader;
use crate::loader::util::resolved_variants_block_array_loader::ResolvedVariantsBlockArrayLoader;
use crate::util::condition::{BlockMaskCondition, Mask, MaskEntry};
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `BlockPlacementMaskJsonLoader.Constants`.
pub mod constants {
    pub const KEY_DEFAULT: &str = "Default";
    pub const KEY_SPECIFIC: &str = "Specific";
    pub const KEY_BLOCK: &str = "Block";
    pub const KEY_RULE: &str = "Rule";

    pub const ERROR_FAIL_SPECIFIC: &str = "Error while reading specific block mask #%s!";
    pub const ERROR_BLOCK_INVALID: &str = "Failed to resolve block \"%s\"";
}

/// Java `loader.prefab.BlockPlacementMaskJsonLoader`.
pub struct BlockPlacementMaskJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, BlockMaskCondition>`.
    pub base: JsonLoaderBase,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl BlockPlacementMaskJsonLoader {
    /// Java `BlockPlacementMaskJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".BlockMaskCondition"), data_folder, json),
            registries,
        }
    }

    /// Java `BlockMaskCondition load()`.
    pub fn load(&self) -> BlockMaskCondition {
        let mut condition = BlockMaskCondition::new();

        if matches!(&self.base.json, None | Some(Value::Null)) {
            // Java: `defaultMask = BlockMaskCondition.DEFAULT_MASK`.
            return condition;
        }

        let mut default_mask = Mask::new_match_empty(true, Vec::new());
        if self.base.has(constants::KEY_DEFAULT) {
            let array = self
                .base
                .get(constants::KEY_DEFAULT)
                .unwrap()
                .as_array()
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "Not a JSON Array: {}",
                        self.base.get(constants::KEY_DEFAULT).unwrap()
                    )
                });
            default_mask = Mask::new(self.load_entries(&array));
        }

        let mut specific_masks = std::collections::HashMap::new();
        if self.base.has(constants::KEY_SPECIFIC) {
            let array = self
                .base
                .get(constants::KEY_SPECIFIC)
                .unwrap()
                .as_array()
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "Not a JSON Array: {}",
                        self.base.get(constants::KEY_SPECIFIC).unwrap()
                    )
                });
            for (index, element) in array.iter().enumerate() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.load_specific_mask(element, &mut specific_masks)
                }));
                if result.is_err() {
                    panic!(
                        "{}",
                        constants::ERROR_FAIL_SPECIFIC.replace("%s", &index.to_string())
                    );
                }
            }
        }

        condition.set(default_mask, specific_masks);
        condition
    }

    /// The body of the `Specific` branch — one entry of the array.
    fn load_specific_mask(
        &self,
        element: &Value,
        specific_masks: &mut std::collections::HashMap<i64, Mask>,
    ) {
        let object = element
            .as_object()
            .unwrap_or_else(|| panic!("Not a JSON Object: {element}"));
        let blocks_element = object.get(constants::KEY_BLOCK).cloned();
        let blocks = ResolvedBlockArrayJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            blocks_element,
            Rc::clone(&self.registries),
        )
        .load();

        for block_entry in blocks.get_entries() {
            // Java: `assetMap.getAsset(blockEntry.blockId()).getId()` — the
            // *name* of the block type.
            let key = self
                .registries
                .blocks
                .name(block_entry.block_id)
                .map(|name| name.to_string())
                .unwrap_or_else(|| {
                    panic!(
                        "Cannot invoke \"com.hypixel.hytale.assetstore.map.BlockType.getId()\" because the return value of \"com.hypixel.hytale.assetstore.map.BlockTypeAssetMap.getAsset(int)\" is null"
                    )
                });
            for variant in self.registries.blocks.sub_keys(&key) {
                let index = self
                    .registries
                    .blocks
                    .index_of(&variant)
                    .unwrap_or_else(|| panic!("Unknown key! {variant}"));
                let rule = object
                    .get(constants::KEY_RULE)
                    .and_then(|value| value.as_array().cloned())
                    .unwrap_or_else(|| {
                        panic!(
                            "Not a JSON Array: {}",
                            object.get(constants::KEY_RULE).unwrap_or(&Value::Null)
                        )
                    });
                let mask = Mask::new(self.load_entries(&rule));
                specific_masks.insert(
                    hytale_math::math_util::pack_long(index, block_entry.fluid_id),
                    mask,
                );
            }
        }
    }

    /// Java `BlockMaskCondition.MaskEntry[] loadEntries(JsonArray
    /// jsonArray)`.
    ///
    /// The Java loader writes the *normal* entries from the front of the
    /// array and the *wildcard* entries (`*` and `!*`) from the back, so the
    /// wildcards end up behind the normal entries. The port reproduces the
    /// two-ended fill exactly.
    pub fn load_entries(&self, json_array: &[Value]) -> Vec<MaskEntry> {
        let mut entries: Vec<Option<MaskEntry>> = vec![None; json_array.len()];
        let mut head = 0usize;
        let mut tail = json_array.len();

        for element in json_array {
            if element.is_object() {
                let object = element.as_object().unwrap();
                let mut replace = true;
                if let Some(value) = object.get("Replace") {
                    replace = JsonLoaderBase::get_as_bool(value);
                }
                let blocks = ResolvedVariantsBlockArrayLoader::load_single_block_object(
                    object,
                    Rc::clone(&self.registries),
                );
                let mut entry = MaskEntry::new(false, replace);
                entry.set(blocks, replace);
                entries[head] = Some(entry);
                head += 1;
                continue;
            }

            let string = JsonLoaderBase::get_as_string(element);
            let mut replace = true;
            let mut begin_index = 0usize;
            if string.starts_with('!') {
                replace = false;
                begin_index = 1;
            }
            let bytes = string.as_bytes();
            if bytes.len() == begin_index + 1 && bytes[begin_index] == b'*' {
                // A wildcard: it is placed at the end of the array. The Java
                // code shifts the tail one position to the left before it
                // writes the wildcard.
                if tail < entries.len() {
                    // Java: `System.arraycopy(entries, tail, entries, tail - 1,
                    // entries.length - tail)` — the copy runs from the back
                    // to the front, so the overlapping region is shifted
                    // correctly.
                    let count = entries.len() - tail;
                    for offset in (0..count).rev() {
                        entries[tail - 1 + offset] = entries[tail + offset].clone();
                    }
                }
                let last = entries.len() - 1;
                entries[last] = Some(if replace {
                    MaskEntry::wildcard_true()
                } else {
                    MaskEntry::wildcard_false()
                });
                tail -= 1;
            } else {
                let name = &string[begin_index..];
                let blocks = self.load_single_block(name);
                let mut entry = MaskEntry::new(false, replace);
                entry.set(blocks, replace);
                entries[head] = Some(entry);
                head += 1;
            }
        }

        entries
            .into_iter()
            .map(|entry| entry.unwrap_or_else(MaskEntry::default_entry))
            .collect()
    }

    /// Java `ResolvedVariantsBlockArrayLoader.loadSingleBlock(String)`.
    fn load_single_block(
        &self,
        block_name: &str,
    ) -> crate::util::resolved_block_array::ResolvedBlockArray {
        ResolvedVariantsBlockArrayLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            None,
            Rc::clone(&self.registries),
        )
        .load_single_block(block_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Option<Value>) -> BlockPlacementMaskJsonLoader {
        loader_with(json, Rc::new(Registries::empty()))
    }

    /// A loader with the golden registries (the block names resolve).
    fn loader_with(
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> BlockPlacementMaskJsonLoader {
        BlockPlacementMaskJsonLoader::new(SeedString::new("0"), Path::new("."), json, registries)
    }

    #[test]
    fn the_seed_appends_the_block_mask_condition_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.BlockMaskCondition");
    }

    #[test]
    fn an_absent_mask_is_the_default_one() {
        let condition = loader(None).load();
        // The default mask matches an empty current block.
        assert!(condition.eval(0, 0, 1, 0));
    }

    #[test]
    fn the_wildcards_are_placed_behind_the_normal_entries() {
        let loader = loader_with(None, Rc::new(Registries::load()));
        let entries = loader.load_entries(&[
            serde_json::json!("Rock_Stone"),
            serde_json::json!("*"),
            serde_json::json!("!*"),
        ]);
        assert_eq!(entries.len(), 3);
        // The normal entry is first, the wildcards follow in their order.
        assert!(!entries[0].should_handle(0, 0));
        assert!(entries[1].should_handle(0, 0));
        assert!(entries[1].should_replace());
        assert!(entries[2].should_handle(0, 0));
        assert!(!entries[2].should_replace());
    }

    #[test]
    fn a_wildcard_only_mask_replaces_everything() {
        let loader = loader_with(None, Rc::new(Registries::load()));
        let entries = loader.load_entries(&[serde_json::json!("*")]);
        assert!(entries[0].should_handle(123, 456));
        assert!(entries[0].should_replace());
    }
}
