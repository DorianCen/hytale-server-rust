// Ported from: `.hytale-source/.../worldgen/loader/container/CoverContainerJsonLoader.java`
//! Java `loader.container.CoverContainerJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::container::cover_container::{
    CoverContainer, CoverContainerEntry, CoverContainerEntryPart,
};
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::loader::util::resolved_block_array_json_loader::ResolvedBlockArrayJsonLoader;
use crate::util::condition::HashSetBlockFluidCondition;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::{
    ConstantBlockFluidCondition, DefaultCoordinateRndCondition, HeightCondition,
    IBlockFluidCondition, ICoordinateCondition, ICoordinateRndCondition,
};

/// Java `CoverContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_ENTRY_TYPE: &str = "Type";
    pub const KEY_ENTRY_WEIGHT: &str = "Weight";
    pub const KEY_ENTRY_DENSITY: &str = "Density";
    pub const KEY_ENTRY_NOISE_MASK: &str = "NoiseMask";
    pub const KEY_ENTRY_HEIGHT_THRESHOLD: &str = "HeightThreshold";
    pub const KEY_ENTRY_OFFSET: &str = "Offset";
    pub const KEY_ENTRY_PARENT: &str = "Parent";
    pub const KEY_ENTRY_ON_WATER: &str = "OnWater";

    pub const ERROR_NO_TYPE: &str = "Could not find type array for cover container! Keyword: Type";
    pub const ERROR_NO_ENTRIES: &str = "There are no blocks in this cover container!";
    pub const ERROR_WEIGHTS_ARRAY_SIZE: &str =
        "Weight array size does not equal size of types array";
    pub const ERROR_OFFSETS_ARRAY_SIZE: &str =
        "Offset array size does not equal size of types array";
}

/// Java `CoverContainerJsonLoader`.
pub struct CoverContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CoverContainer>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull protected final BiomeFileContext biomeContext` — the
    /// Java `ModifyEvents` hooks are not ported.
    pub biome_context: Rc<BiomeFileContext>,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl CoverContainerJsonLoader {
    /// Java `CoverContainerJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, BiomeFileContext biomeContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            biome_context,
            registries,
        }
    }

    /// Java `CoverContainer load()`.
    pub fn load(&self) -> CoverContainer {
        let mut entries = Vec::new();
        match self.base.json.as_ref() {
            Some(Value::Array(covers)) => {
                for (index, element) in covers.iter().enumerate() {
                    entries.push(
                        CoverContainerEntryJsonLoader::new(
                            self.base.seed.append(&format!("-{index}")),
                            &self.base.data_folder,
                            Some(element.clone()),
                            Rc::clone(&self.registries),
                        )
                        .load(),
                    );
                }
            }
            Some(Value::Object(_)) => {
                entries.push(
                    CoverContainerEntryJsonLoader::new(
                        self.base.seed.append("-0"),
                        &self.base.data_folder,
                        self.base.json.clone(),
                        Rc::clone(&self.registries),
                    )
                    .load(),
                );
            }
            _ => {}
        }
        CoverContainer::new(entries)
    }
}

/// Java `CoverContainerJsonLoader.CoverContainerEntryJsonLoader`.
pub struct CoverContainerEntryJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl CoverContainerEntryJsonLoader {
    /// Java `CoverContainerEntryJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CoverContainerEntry"), data_folder, json),
            registries,
        }
    }

    /// Java `CoverContainerEntry load()`.
    pub fn load(&self) -> CoverContainerEntry {
        CoverContainerEntry::new(
            self.load_entries(),
            self.load_map_condition(),
            self.load_height_condition(),
            self.load_parents(),
            self.load_density(),
            self.load_on_water(),
        )
    }

    /// Java `IWeightedMap<CoverContainerEntryPart> loadEntries()`.
    fn load_entries(&self) -> Rc<dyn IWeightedMap<CoverContainerEntryPart>> {
        if !self.base.has(constants::KEY_ENTRY_TYPE) {
            panic!("{}", constants::ERROR_NO_TYPE);
        }
        let types = ResolvedBlockArrayJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_TYPE),
            Rc::clone(&self.registries),
        )
        .load();
        let offsets = self.load_offset_array(types.size());
        let weights = if self.base.has(constants::KEY_ENTRY_WEIGHT) {
            Some(self.base.get(constants::KEY_ENTRY_WEIGHT).unwrap())
        } else {
            None
        };
        if let Some(weights) = &weights {
            if weights.as_array().map(Vec::len) != Some(types.size()) {
                panic!("{}", constants::ERROR_WEIGHTS_ARRAY_SIZE);
            }
        }

        let mut builder = WeightedMap::<CoverContainerEntryPart>::builder();
        for (index, block_entry) in types.get_entries().iter().enumerate() {
            let offset = offsets[index];
            let weight = match &weights {
                Some(weights) => JsonLoaderBase::get_as_double(&weights.as_array().unwrap()[index]),
                None => 1.0,
            };
            builder.put(CoverContainerEntryPart::new(*block_entry, offset), weight);
        }
        if builder.size() == 0 {
            panic!("{}", constants::ERROR_NO_ENTRIES);
        }
        Rc::new(builder.build())
    }

    /// Java `int[] loadOffsetArray(int length)`.
    fn load_offset_array(&self, length: usize) -> Vec<i32> {
        let offset_element = self.base.get(constants::KEY_ENTRY_OFFSET);
        let mut offsets = vec![0; length];
        match offset_element {
            None | Some(Value::Null) => {}
            Some(Value::Array(offset_array)) => {
                if offset_array.len() != length {
                    panic!("{}", constants::ERROR_OFFSETS_ARRAY_SIZE);
                }
                for (index, offset) in offset_array.iter().enumerate() {
                    offsets[index] = JsonLoaderBase::get_as_int(offset);
                }
            }
            Some(offset) => {
                let offset = JsonLoaderBase::get_as_int(&offset);
                offsets.fill(offset);
            }
        }
        offsets
    }

    /// Java `double loadDensity()`.
    fn load_density(&self) -> f64 {
        let mut density = 1.0;
        if self.base.has(constants::KEY_ENTRY_DENSITY) {
            density = JsonLoaderBase::get_as_double(
                &self.base.get(constants::KEY_ENTRY_DENSITY).unwrap(),
            );
        }
        density
    }

    /// Java `ICoordinateCondition loadMapCondition()`.
    fn load_map_condition(&self) -> Rc<dyn ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_NOISE_MASK),
        )
        .load()
    }

    /// Java `ICoordinateRndCondition loadHeightCondition()`.
    fn load_height_condition(&self) -> Rc<dyn ICoordinateRndCondition> {
        if self.base.has(constants::KEY_ENTRY_HEIGHT_THRESHOLD) {
            return Rc::new(HeightCondition::new(
                HeightThresholdInterpreterJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_ENTRY_HEIGHT_THRESHOLD),
                    #[allow(deprecated)]
                    // deprecated in hytale-math but this is its exact Java value
                    hytale_math::chunk_util::HEIGHT,
                )
                .load(),
            ));
        }
        Rc::new(DefaultCoordinateRndCondition::True)
    }

    /// Java `IBlockFluidCondition loadParents()`.
    fn load_parents(&self) -> Rc<dyn IBlockFluidCondition> {
        if self.base.has(constants::KEY_ENTRY_PARENT) {
            let block_array = ResolvedBlockArrayJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ENTRY_PARENT),
                Rc::clone(&self.registries),
            )
            .load();
            return Rc::new(HashSetBlockFluidCondition::new(
                block_array.get_entry_set().clone(),
            ));
        }
        Rc::new(ConstantBlockFluidCondition::True)
    }

    /// Java `boolean loadOnWater()`.
    fn load_on_water(&self) -> bool {
        self.base.has(constants::KEY_ENTRY_ON_WATER)
            && JsonLoaderBase::get_as_bool(&self.base.get(constants::KEY_ENTRY_ON_WATER).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_seed_appends_the_cover_entry_suffix() {
        let loader = CoverContainerEntryJsonLoader::new(
            SeedString::new("0").append("-0"),
            Path::new("."),
            None,
            Rc::new(Registries::empty()),
        );
        assert_eq!(loader.base.seed.seed(), "0-0.CoverContainerEntry");
    }

    #[test]
    fn an_entry_resolves_real_block_names_and_options() {
        let loader = CoverContainerEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({
                "Type": "Soil_Grass",
                "Weight": [1.0],
                "Offset": [2],
                "Density": 0.5,
                "OnWater": true,
                "Parent": "Rock_Bedrock"
            })),
            Rc::new(Registries::load()),
        );
        let entry = loader.load();
        assert_eq!(entry.get_cover_density(), 0.5);
        assert!(entry.is_on_water());
        let part = entry.get_entries().get(0.0).expect("one cover part");
        assert_eq!(part.get_entry().block_id, 1250, "Soil_Grass");
        assert_eq!(part.get_offset(), 2);
        assert!(
            entry.get_parent_condition().eval(445, 0),
            "Rock_Bedrock parent"
        );
    }
}
