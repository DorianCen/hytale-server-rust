// Ported from: `.hytale-source/.../worldgen/loader/util/ResolvedVariantsBlockArrayLoader.java`
//! Java `loader.util.ResolvedVariantsBlockArrayLoader`.
//!
//! ⚠ Deviation: `BlockTypeAssetMap.getSubKeys` returns the *child variants*
//! of a base block. The golden dump carries no parent/child data, so
//! [`worldgen_model::registry::BlockRegistry::sub_keys`] falls back to Java's
//! no-children branch (`ObjectSets.singleton(key)` = the base block itself).
//! A variants array therefore resolves to one entry per base name today.
//!
//! ⚠ Deviation: the Java loader's global
//! `ResolvedBlockArray.RESOLVED_BLOCKS_WITH_VARIANTS` cache is not ported
//! (unobservable, like `RESOLVED_BLOCKS`).

use std::path::Path;
use std::rc::Rc;

use serde_json::{Map, Value};
use worldgen_model::registry::Registries;

use crate::loader::util::resolved_block_array_json_loader::ResolvedBlockArrayJsonLoader;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::resolved_block_array::ResolvedBlockArray;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ResolvedVariantsBlockArrayLoader`.
pub struct ResolvedVariantsBlockArrayLoader {
    /// Java `extends JsonLoader<SeedStringResource, ResolvedBlockArray>`.
    pub base: JsonLoaderBase,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl ResolvedVariantsBlockArrayLoader {
    /// Java `ResolvedVariantsBlockArrayLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            registries,
        }
    }

    /// Java `ResolvedBlockArray load()`.
    pub fn load(&self) -> ResolvedBlockArray {
        let Some(json) = self.base.json.clone() else {
            return ResolvedBlockArray::empty();
        };
        if json.is_null() {
            return ResolvedBlockArray::empty();
        }

        if !json.is_array() {
            return self.load_single_block(&JsonLoaderBase::get_as_string(&json));
        }

        let json_array = json.as_array().unwrap();

        // Shortcut for lists of size 1, includes caching (Java comment).
        if json_array.len() == 1 {
            return self.load_single_block(&JsonLoaderBase::get_as_string(&json_array[0]));
        }

        let mut resolved_blocks: Vec<Vec<BlockFluidEntry>> = Vec::new();
        let mut size = 0usize;
        for element in json_array {
            let block_name = JsonLoaderBase::get_as_string(element);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // Java checks `getAsset(name) == null` first (raw-name error),
                // then `getIndex(name) == NOT_FOUND` ("Unknown block!"); both
                // consult the same map, so the second branch is unreachable.
                if self.registries.blocks.index_of(&block_name).is_none() {
                    panic!("{block_name}");
                }
                Self::resolve_block_array_with_variants(&block_name, &self.registries, 0)
            }));
            match result {
                Ok(block_variant_array) => {
                    size += block_variant_array.len();
                    resolved_blocks.push(block_variant_array);
                }
                Err(_) => panic!("BlockLayer does not exist in BlockTypes"),
            }
        }

        // Java writes the blocks in reverse within each block array:
        // `blocks[--size] = block;`
        let mut blocks = vec![BlockFluidEntry::EMPTY; size];
        for block_array in &resolved_blocks {
            for block in block_array {
                size -= 1;
                blocks[size] = *block;
            }
        }

        ResolvedBlockArray::new(blocks)
    }

    /// Java `static ResolvedBlockArray loadSingleBlock(String blockName)`.
    pub fn load_single_block(&self, block_name: &str) -> ResolvedBlockArray {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if self.registries.blocks.index_of(block_name).is_none() {
                panic!("{block_name}");
            }
            Self::resolve_block_array_with_variants(block_name, &self.registries, 0)
        }));
        match result {
            Ok(blocks) => ResolvedBlockArray::new(blocks),
            Err(_) => panic!("BlockLayer does not exist in BlockTypes"),
        }
    }

    /// Java `static ResolvedBlockArray loadSingleBlock(JsonObject object)`.
    pub fn load_single_block_object(
        object: &Map<String, Value>,
        registries: Rc<Registries>,
    ) -> ResolvedBlockArray {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(block) = object.get("Block") {
                let block_name = JsonLoaderBase::get_as_string(block);
                if registries.blocks.index_of(&block_name).is_none() {
                    panic!("{block_name}");
                }

                let fluid_id = if let Some(fluid) = object.get("Fluid") {
                    let fluid_name = JsonLoaderBase::get_as_string(fluid);
                    registries
                        .fluids
                        .index_of(&fluid_name)
                        .unwrap_or_else(|| panic!("Unknown fluid! {fluid_name}"))
                } else {
                    0
                };

                ResolvedBlockArray::new(Self::resolve_block_array_with_variants(
                    &block_name,
                    &registries,
                    fluid_id,
                ))
            } else if object.contains_key("Fluid") {
                ResolvedBlockArrayJsonLoader::load_single_block_object(object, registries)
            } else {
                panic!("Required either Block or Fluid key");
            }
        }));
        match result {
            Ok(blocks) => blocks,
            Err(_) => panic!("BlockLayer does not exist in BlockTypes"),
        }
    }

    /// Java `static BlockFluidEntry[] resolveBlockArrayWithVariants(String
    /// baseKey, BlockTypeAssetMap assetMap, int fluidId)`.
    pub fn resolve_block_array_with_variants(
        base_key: &str,
        registries: &Registries,
        fluid_id: i32,
    ) -> Vec<BlockFluidEntry> {
        let variants = registries.blocks.sub_keys(base_key);
        let mut blocks = Vec::with_capacity(variants.len());
        for key in variants {
            let index = registries
                .blocks
                .index_of(&key)
                .unwrap_or_else(|| panic!("Unknown key! {key}"));
            blocks.push(BlockFluidEntry::new(index, 0, fluid_id));
        }
        blocks
    }
}
