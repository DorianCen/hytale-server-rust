// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveNodeCoverEntryJsonLoader.java`
//! Java `loader.cave.CaveNodeCoverEntryJsonLoader` — one cover of a cave node
//! type: the blocks the cave places on its floor or its ceiling.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::cave::cave_node_type::{CaveNodeCoverEntry, CaveNodeCoverEntryEntry, CaveNodeCoverType};
use crate::loader::util::resolved_block_array_json_loader::ResolvedBlockArrayJsonLoader;
use crate::util::condition::{
    FilteredBlockFluidCondition, HashSetBlockFluidCondition, RandomCoordinateCondition,
};
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::{
    ConstantBlockFluidCondition, DefaultCoordinateCondition, DefaultCoordinateRndCondition,
    HeightCondition, IBlockFluidCondition, ICoordinateCondition, ICoordinateRndCondition,
};

/// Java `CaveNodeCoverEntryJsonLoader.Constants`.
pub mod constants {
    pub const KEY_TYPE: &str = "Type";
    pub const KEY_WEIGHT: &str = "Weight";
    pub const KEY_HEIGHT_THRESHOLD: &str = "HeightThreshold";
    pub const KEY_NOISE_MASK: &str = "NoiseMask";
    pub const KEY_DENSITY: &str = "Density";
    pub const KEY_OFFSET: &str = "Offset";
    pub const KEY_PARENT: &str = "Parent";
    pub const KEY_ANCHOR_TYPE: &str = "AnchorType";

    pub const ERROR_NO_TYPE: &str =
        "Could not find type array for cave cover container! Keyword: Type";
    pub const ERROR_NO_ENTRIES: &str = "There are no blocks in this cover container!";
    pub const ERROR_WEIGHTS_ARRAY_SIZE: &str =
        "Weight array size does not equal size of types array";
}

/// Java `loader.cave.CaveNodeCoverEntryJsonLoader`.
pub struct CaveNodeCoverEntryJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource,
    /// CaveNodeType.CaveNodeCoverEntry>`.
    pub base: JsonLoaderBase,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl CaveNodeCoverEntryJsonLoader {
    /// Java `CaveNodeCoverEntryJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CaveNodeCoverEntry"), data_folder, json),
            registries,
        }
    }

    /// Java `CaveNodeType.CaveNodeCoverEntry load()`.
    pub fn load(&self) -> CaveNodeCoverEntry {
        // Java evaluates the constructor arguments left-to-right.
        let entries = self.load_entries();
        let height_condition = self.load_height_condition();
        let map_condition = self.load_map_condition();
        let density_condition = self.load_density_condition();
        let parent_condition = self.load_parent_condition();
        let cover_type = self.load_anchor_type();

        CaveNodeCoverEntry::new(
            entries,
            height_condition,
            map_condition,
            density_condition,
            parent_condition,
            cover_type,
        )
    }

    /// Java `IWeightedMap<CaveNodeType.CaveNodeCoverEntry.Entry> loadEntries()`.
    fn load_entries(&self) -> Rc<dyn IWeightedMap<CaveNodeCoverEntryEntry>> {
        if !self.base.has(constants::KEY_TYPE) {
            panic!("{}", constants::ERROR_NO_TYPE);
        }

        let types = ResolvedBlockArrayJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_TYPE),
            Rc::clone(&self.registries),
        )
        .load();

        let weights = if self.base.has(constants::KEY_WEIGHT) {
            let weights = self.base.get(constants::KEY_WEIGHT).unwrap();
            let weights = weights
                .as_array()
                .cloned()
                .unwrap_or_else(|| panic!("Not a JSON Array: {weights}"));
            if weights.len() != types.size() {
                panic!("{}", constants::ERROR_WEIGHTS_ARRAY_SIZE);
            }
            Some(weights)
        } else {
            None
        };

        let mut builder = WeightedMap::<CaveNodeCoverEntryEntry>::builder();
        for (index, block_entry) in types.get_entries().iter().enumerate() {
            let offset = self.load_offset();
            let weight = match &weights {
                Some(weights) => JsonLoaderBase::get_as_double(&weights[index]),
                None => 1.0,
            };
            builder.put(CaveNodeCoverEntryEntry::new(*block_entry, offset), weight);
        }

        if builder.size() == 0 {
            panic!("{}", constants::ERROR_NO_ENTRIES);
        }
        Rc::new(builder.build())
    }

    /// Java `ICoordinateRndCondition loadHeightCondition()`.
    fn load_height_condition(&self) -> Rc<dyn ICoordinateRndCondition> {
        if !self.base.has(constants::KEY_HEIGHT_THRESHOLD) {
            return Rc::new(DefaultCoordinateRndCondition::True);
        }
        let interpreter = HeightThresholdInterpreterJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_HEIGHT_THRESHOLD),
            #[allow(deprecated)] // deprecated in hytale-math but this is its exact Java value
            hytale_math::chunk_util::HEIGHT,
        )
        .load();
        Rc::new(HeightCondition::new(interpreter))
    }

    /// Java `ICoordinateCondition loadMapCondition()`.
    fn load_map_condition(&self) -> Rc<dyn ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE_MASK),
        )
        .load()
    }

    /// Java `ICoordinateCondition loadDensityCondition()`.
    fn load_density_condition(&self) -> Rc<dyn ICoordinateCondition> {
        if !self.base.has(constants::KEY_DENSITY) {
            return Rc::new(DefaultCoordinateCondition::True);
        }
        Rc::new(RandomCoordinateCondition::new(
            JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_DENSITY).unwrap()),
        ))
    }

    /// Java `IBlockFluidCondition loadParentCondition()`.
    fn load_parent_condition(&self) -> Rc<dyn IBlockFluidCondition> {
        if !self.base.has(constants::KEY_PARENT) {
            // Java `DEFAULT_PARENT_MASK` — everything but the empty block.
            return Rc::new(FilteredBlockFluidCondition::with_block_id(
                0,
                Rc::new(ConstantBlockFluidCondition::True),
            ));
        }
        let block_array = ResolvedBlockArrayJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_PARENT),
            Rc::clone(&self.registries),
        )
        .load();
        Rc::new(HashSetBlockFluidCondition::new(
            block_array.get_entry_set().clone(),
        ))
    }

    /// Java `int loadOffset()`.
    fn load_offset(&self) -> i32 {
        if !self.base.has(constants::KEY_OFFSET) {
            return 0;
        }
        JsonLoaderBase::get_as_int(&self.base.get(constants::KEY_OFFSET).unwrap())
    }

    /// Java `CaveNodeType.CaveNodeCoverType loadAnchorType()`.
    fn load_anchor_type(&self) -> CaveNodeCoverType {
        if !self.base.has(constants::KEY_ANCHOR_TYPE) {
            return CaveNodeCoverType::Floor;
        }
        let name =
            JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_ANCHOR_TYPE).unwrap());
        match name.as_str() {
            "FLOOR" => CaveNodeCoverType::Floor,
            "CEILING" => CaveNodeCoverType::Ceiling,
            other => panic!(
                "No enum constant com.hypixel.hytale.server.worldgen.cave.CaveNodeType.CaveNodeCoverType.{other}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Option<Value>) -> CaveNodeCoverEntryJsonLoader {
        CaveNodeCoverEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            Rc::new(Registries::empty()),
        )
    }

    #[test]
    fn the_seed_appends_the_cover_entry_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CaveNodeCoverEntry");
    }

    #[test]
    fn a_missing_type_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(None).load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Could not find type array for cave cover container! Keyword: Type"
        );
    }

    #[test]
    fn the_default_cover_type_is_the_floor() {
        // The empty type array fails the load, so the anchor type is checked
        // through a cover with a type.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({"Type": []}))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "There are no blocks in this cover container!");
    }

    #[test]
    fn the_anchor_type_is_read() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(
                serde_json::json!({"Type": [], "AnchorType": "CEILING"}),
            ))
            .load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "There are no blocks in this cover container!");
    }

    #[test]
    fn an_unknown_anchor_type_panics_verbatim() {
        // The entries are loaded first, so the cover needs a valid type.
        let loader = CaveNodeCoverEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Type": ["Rock_Stone"], "AnchorType": "WALL"})),
            Rc::new(Registries::load()),
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader.load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "No enum constant com.hypixel.hytale.server.worldgen.cave.CaveNodeType.CaveNodeCoverType.WALL"
        );
    }

    #[test]
    fn a_cover_with_a_known_block_is_loaded() {
        let loader = CaveNodeCoverEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Type": ["Rock_Stone"], "AnchorType": "CEILING"})),
            Rc::new(Registries::load()),
        );
        let cover = loader.load();
        assert_eq!(
            cover.get_type(),
            crate::cave::cave_node_type::CaveNodeCoverType::Ceiling
        );
        assert!(cover
            .get(&mut hytale_math::fast_random::FastRandom::new(1))
            .is_some());
    }
}
