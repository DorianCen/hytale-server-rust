// Ported from: `.hytale-source/.../worldgen/loader/util/ResolvedBlockArrayJsonLoader.java`
//! Java `loader.util.ResolvedBlockArrayJsonLoader`.
//!
//! ⚠ Deviation: Java's `BlockType.getBlockIdOrUnknown` /
//! `Fluid.getFluidIdOrUnknown` dynamically register an *unknown* asset for a
//! missing name and retry the lookup. The Rust registries are immutable
//! snapshots of the golden dump, so an unknown name panics with the same
//! `"Unknown key! ..."` message the Java method throws when the retry still
//! fails.
//!
//! ⚠ Deviation: the Java loader caches `ResolvedBlockArray` instances in the
//! global `ResolvedBlockArray.RESOLVED_BLOCKS` map. The cache is unobservable
//! (the arrays are immutable and value-equal), so it is not ported.

use std::path::Path;
use std::rc::Rc;

use serde_json::{Map, Value};
use worldgen_model::block_key::BlockEntry;
use worldgen_model::registry::Registries;

use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::resolved_block_array::ResolvedBlockArray;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `ResolvedBlockArrayJsonLoader`.
pub struct ResolvedBlockArrayJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, ResolvedBlockArray>`.
    pub base: JsonLoaderBase,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl ResolvedBlockArrayJsonLoader {
    /// Java `ResolvedBlockArrayJsonLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the seed suffix is `"ResolvedBlockArray"` (no dot) in Java.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append("ResolvedBlockArray"), data_folder, json),
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
            if json.is_object() {
                return Self::load_single_block_object(
                    json.as_object().unwrap(),
                    Rc::clone(&self.registries),
                );
            }
            return self.load_single_block(&JsonLoaderBase::get_as_string(&json));
        }

        let json_array = json.as_array().unwrap();

        // Shortcut for lists of size 1, includes caching (Java comment).
        if json_array.len() == 1 {
            let element = &json_array[0];
            if element.is_object() {
                return Self::load_single_block_object(
                    element.as_object().unwrap(),
                    Rc::clone(&self.registries),
                );
            }
            return self.load_single_block(&JsonLoaderBase::get_as_string(element));
        }

        let mut blocks = Vec::with_capacity(json_array.len());
        for element in json_array {
            if element.is_object() {
                let object = element.as_object().unwrap();
                let mut block_index = 0;
                let mut rotation = 0;
                let mut fluid_index = 0;
                if let Some(block) = object.get("Block") {
                    let key = BlockEntry::decode(&JsonLoaderBase::get_as_string(block));
                    // Java: `getBlockIdOrUnknown` (see the module deviation).
                    block_index = self
                        .registries
                        .blocks
                        .index_of(&key.block_type_key)
                        .unwrap_or_else(|| panic!("Unknown key! {key}"));
                    rotation = key.rotation;
                }
                if let Some(fluid) = object.get("Fluid") {
                    let key = JsonLoaderBase::get_as_string(fluid);
                    fluid_index = self
                        .registries
                        .fluids
                        .index_of(&key)
                        .unwrap_or_else(|| panic!("Unknown key! {key}"));
                }
                blocks.push(BlockFluidEntry::new(block_index, rotation, fluid_index));
                continue;
            }

            let block_name = JsonLoaderBase::get_as_string(element);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let key = BlockEntry::decode(&block_name);
                let index = self
                    .registries
                    .blocks
                    .index_of(&key.block_type_key)
                    .unwrap_or_else(|| panic!("Unknown key! {key}"));
                BlockFluidEntry::new(index, key.rotation, 0)
            }));
            match result {
                Ok(entry) => blocks.push(entry),
                Err(_) => panic!("BlockLayer {block_name} does not exist in BlockTypes"),
            }
        }

        ResolvedBlockArray::new(blocks)
    }

    /// Java `ResolvedBlockArray loadSingleBlock(String blockName)`.
    pub fn load_single_block(&self, block_name: &str) -> ResolvedBlockArray {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let key = BlockEntry::decode(block_name);
            let index = self
                .registries
                .blocks
                .index_of(&key.block_type_key)
                .unwrap_or_else(|| panic!("Unknown key! {key}"));
            BlockFluidEntry::new(index, key.rotation, 0)
        }));
        match result {
            Ok(entry) => ResolvedBlockArray::new(vec![entry]),
            Err(_) => panic!("BlockLayer does not exist in BlockTypes"),
        }
    }

    /// Java `static ResolvedBlockArray loadSingleBlock(JsonObject obj)`.
    pub fn load_single_block_object(
        object: &Map<String, Value>,
        registries: Rc<Registries>,
    ) -> ResolvedBlockArray {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut block_index = 0;
            let mut rotation = 0;
            let mut fluid_index = 0;
            if let Some(block) = object.get("Block") {
                let key = BlockEntry::decode(&JsonLoaderBase::get_as_string(block));
                block_index = registries
                    .blocks
                    .index_of(&key.block_type_key)
                    .unwrap_or_else(|| panic!("Unknown key! {key}"));
                rotation = key.rotation;
            }
            if let Some(fluid) = object.get("Fluid") {
                let key = JsonLoaderBase::get_as_string(fluid);
                fluid_index = registries
                    .fluids
                    .index_of(&key)
                    .unwrap_or_else(|| panic!("Unknown key! {key}"));
            }
            BlockFluidEntry::new(block_index, rotation, fluid_index)
        }));
        match result {
            Ok(entry) => ResolvedBlockArray::new(vec![entry]),
            Err(_) => panic!("BlockLayer does not exist in BlockTypes"),
        }
    }
}
