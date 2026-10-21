// Ported from: `.hytale-source/.../worldgen/loader/util/NoiseBlockArrayJsonLoader.java`
//! Java `loader.util.NoiseBlockArrayJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::supplier::{DoubleRange, IDoubleRange};
use procedural::noise::support::NoiseProperty;
use worldgen_model::block_key::BlockEntry;
use worldgen_model::registry::Registries;

use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::constant_noise_property::default_zero;
use crate::util::noise_block_array::{Entry, NoiseBlockArray};

/// Java `NoiseBlockArrayJsonLoader.Constants`.
pub mod constants {
    pub const KEY_ENTRIES: &str = "Entries";
    pub const KEY_ENTRY_TYPE: &str = "Type";
    pub const KEY_ENTRY_REPEAT: &str = "Repeat";
    pub const KEY_ENTRY_REPEAT_NOISE: &str = "RepeatNoise";

    pub const ERROR_NO_ENTRIES: &str = "Could not find entries in block array. Keyword: Entries";
    pub const ERROR_ENTRY_FAIL: &str = "Failed to load block array entry #%s";
}

/// Java `NoiseBlockArrayJsonLoader`.
pub struct NoiseBlockArrayJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, NoiseBlockArray>`.
    pub base: JsonLoaderBase,
    /// The resolved block / fluid registries (`BlockType.getAssetMap()` /
    /// `Fluid.getAssetMap()` in Java).
    pub registries: Rc<Registries>,
}

impl NoiseBlockArrayJsonLoader {
    /// Java `NoiseBlockArrayJsonLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".NoiseBlockArray"), data_folder, json),
            registries,
        }
    }

    /// Java `NoiseBlockArray load()`.
    pub fn load(&self) -> NoiseBlockArray {
        let Some(json) = self.base.json.clone() else {
            panic!("{}", constants::ERROR_NO_ENTRIES);
        };

        if json.is_string() || json.is_number() || json.is_boolean() {
            return NoiseBlockArray::new(vec![self
                .entry_loader(self.base.seed.clone(), Some(json))
                .load()]);
        }

        if json.is_object() && !self.base.has(constants::KEY_ENTRIES) {
            return NoiseBlockArray::new(vec![self.load_entry(json, 0)]);
        }

        let entries_element = if json.is_array() {
            Some(json)
        } else {
            self.base.get(constants::KEY_ENTRIES)
        };
        let Some(entries_element) = entries_element.filter(|element| !element.is_null()) else {
            panic!("{}", constants::ERROR_NO_ENTRIES);
        };
        let entries_array = entries_element
            .as_array()
            .unwrap_or_else(|| panic!("{}", constants::ERROR_NO_ENTRIES));

        let mut entries = Vec::with_capacity(entries_array.len());
        for (index, element) in entries_array.iter().enumerate() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.load_entry(element.clone(), index)
            }));
            match result {
                Ok(entry) => entries.push(entry),
                Err(_) => panic!(
                    "{}",
                    constants::ERROR_ENTRY_FAIL.replacen("%s", &index.to_string(), 1)
                ),
            }
        }
        NoiseBlockArray::new(entries)
    }

    /// Java `NoiseBlockArray.Entry loadEntry(JsonElement element, int i)`.
    fn load_entry(&self, element: Value, index: usize) -> Entry {
        self.entry_loader(self.base.seed.append(&format!("-{index}")), Some(element))
            .load()
    }

    fn entry_loader(&self, seed: SeedString, json: Option<Value>) -> EntryJsonLoader {
        EntryJsonLoader {
            base: JsonLoaderBase::new(seed.append(".Entry"), &self.base.data_folder, json),
            registries: Rc::clone(&self.registries),
        }
    }
}

/// Java `NoiseBlockArrayJsonLoader.EntryJsonLoader`.
struct EntryJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl EntryJsonLoader {
    fn load(&self) -> Entry {
        let Some(json) = self.base.json.clone() else {
            panic!("Unsupported Json Entry: null");
        };

        if json.is_string() || json.is_number() || json.is_boolean() {
            let mut block_name = JsonLoaderBase::get_as_string(&json);
            let mut repetitions = 1;
            if block_name.contains(':') {
                // Java `blockName.split(":", -1)`: keep the trailing empty
                // fields and only read the first two parts.
                let (name, repeat) = {
                    let mut parts = block_name.split(':');
                    (
                        parts.next().unwrap_or("").to_string(),
                        parts.next().unwrap_or("").to_string(),
                    )
                };
                block_name = name;
                repetitions = repeat.parse::<i32>().unwrap_or_else(|_| {
                    panic!("NumberFormatException: For input string: \"{}\"", repeat)
                });
            }
            let block_entry = self.resolve_block_id(&block_name);
            return Entry::new(
                block_name,
                block_entry,
                Rc::new(DoubleRange::constant(f64::from(repetitions))),
                default_zero(),
            );
        }

        if json.is_object() {
            let block_name =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_ENTRY_TYPE).unwrap());
            let block_entry = self.resolve_block_id(&block_name);
            let repetitions = self.load_repetitions();
            let noise = self.load_noise();
            return Entry::new(block_name, block_entry, repetitions, noise);
        }

        panic!("Unsupported Json Entry: {json}");
    }

    /// Java `IDoubleRange loadRepetitions()`.
    fn load_repetitions(&self) -> Rc<dyn IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ENTRY_REPEAT),
                1.0,
            )
            .load(),
        )
    }

    /// Java `NoiseProperty loadNoise()`.
    fn load_noise(&self) -> Rc<dyn NoiseProperty> {
        let mut noise = default_zero();
        if self.base.has(constants::KEY_ENTRY_REPEAT_NOISE) {
            noise = NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ENTRY_REPEAT_NOISE),
            )
            .load();
        }
        noise
    }

    /// Java `BlockFluidEntry resolveBlockId(String name)`.
    fn resolve_block_id(&self, name: &str) -> BlockFluidEntry {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // TODO: Hack for fluid support in noise arrays (Java comment).
            if let Some(key) = name.strip_prefix("fluid#") {
                let index = self
                    .registries
                    .fluids
                    .index_of(key)
                    .unwrap_or_else(|| panic!("Unknown key! {key}"));
                return BlockFluidEntry::new(0, 0, index);
            }
            let key = BlockEntry::decode(name);
            let index = self
                .registries
                .blocks
                .index_of(&key.block_type_key)
                .unwrap_or_else(|| panic!("Unknown key! {key}"));
            BlockFluidEntry::new(index, key.rotation, 0)
        }));
        match result {
            Ok(entry) => entry,
            Err(_) => panic!("BlockLayer does not exist in BlockTypes"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value, registries: Rc<Registries>) -> NoiseBlockArrayJsonLoader {
        NoiseBlockArrayJsonLoader::new(SeedString::new("0"), Path::new("."), Some(json), registries)
    }

    #[test]
    fn the_seed_suffixes_match_java() {
        let loader = loader(serde_json::json!([]), Rc::new(Registries::empty()));
        assert_eq!(loader.base.seed.seed(), "0.NoiseBlockArray");

        // Java `loadEntry` appends "-i" before the `EntryJsonLoader`
        // constructor appends ".Entry".
        let entry = loader.entry_loader(
            SeedString::new("0").append("-3"),
            Some(serde_json::json!("Rock_Stone")),
        );
        assert_eq!(entry.base.seed.seed(), "0-3.Entry");
    }

    #[test]
    fn primitive_entries_split_on_colons_like_java() {
        // Java `blockName.split(":", -1)` reads the first two fields and
        // ignores any further colons.
        let array = loader(
            serde_json::json!("Rock_Stone:2:ignored"),
            Rc::new(Registries::load()),
        )
        .load();
        assert_eq!(array.get_entries().len(), 1);
        let entry = &array.get_entries()[0];
        assert_eq!(entry.get_block_name(), "Rock_Stone");
        assert_eq!(entry.get_block_entry().block_id, 1527);
        assert_eq!(entry.get_repetitions(0, 0.0, 0.0), 2);
    }

    #[test]
    fn entry_arrays_resolve_real_block_names() {
        let array = loader(
            serde_json::json!(["Rock_Stone", "Rock_Bedrock"]),
            Rc::new(Registries::load()),
        )
        .load();
        assert_eq!(array.get_entries().len(), 2);
        assert_eq!(array.get_entries()[0].get_block_entry().block_id, 1527);
        assert_eq!(array.get_entries()[1].get_block_entry().block_id, 445);
    }

    #[test]
    fn the_fluid_prefix_resolves_a_fluid_id() {
        let array = loader(
            serde_json::json!("fluid#Water_Source"),
            Rc::new(Registries::load()),
        )
        .load();
        let entry = array.get_entries()[0].get_block_entry();
        assert_eq!(entry.block_id, 0);
        assert_eq!(entry.fluid_id, 2, "Water_Source");
    }
}
