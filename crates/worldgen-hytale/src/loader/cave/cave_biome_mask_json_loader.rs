// Ported from: `.hytale-source/.../worldgen/loader/cave/CaveBiomeMaskJsonLoader.java`
//! Java `loader.cave.CaveBiomeMaskJsonLoader` — the biome mask of a cave
//! type.
//!
//! The mask answers with the bit flags of
//! [`CaveBiomeMaskFlags`](crate::cave::cave_biome_mask_flags): whether the
//! cave node may continue to grow, whether its geometry is added to the
//! world and whether it may retry with another direction.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::cave_biome_mask_flags;
use crate::loader::biome::biome_mask_json_loader::{BiomeMaskJsonLoader, LoadedBiomeMask};
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::util::condition::flag::{CompositeInt2Flags, FlagCondition, FlagOperator};
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::IIntCondition;

/// Java `CaveBiomeMaskJsonLoader.Constants`.
pub mod constants {
    // Keys
    pub const KEY_GENERATION: &str = "Generate";
    pub const KEY_POPULATION: &str = "Populate";
    pub const KEY_TERMINATE: &str = "Terminate";
    // Defaults
    /// Java `DEFAULT_TERMINATE_SETTING` — `!canContinue(DEFAULT_RESULT)`.
    ///
    /// The default result is `CONTINUE`, so `canContinue` is `true` and the
    /// setting is `false`.
    pub const DEFAULT_TERMINATE_SETTING: bool = false;
}

/// Java `loader.cave.CaveBiomeMaskJsonLoader`.
pub struct CaveBiomeMaskJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, Int2FlagsCondition>`.
    pub base: JsonLoaderBase,
    /// Java `private final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
}

impl CaveBiomeMaskJsonLoader {
    /// Java `CaveBiomeMaskJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CaveBiomeMask"), data_folder, json),
            zone_context,
        }
    }

    /// Java `Int2FlagsCondition load()`.
    pub fn load(&self) -> Rc<dyn crate::util::condition::flag::Int2FlagsCondition> {
        let generate = self.load_generation_mask();
        if generate.is_default_false() {
            return Rc::new(cave_biome_mask_flags::DEFAULT_DENY);
        }

        let populate = self.load_population_mask();
        if generate.is_default_true() && populate.is_default_true() {
            return Rc::new(cave_biome_mask_flags::DEFAULT_ALLOW);
        }

        let default_result = self.load_default_result();
        let flag_conditions = self.load_flag_conditions(&generate, &populate);
        Rc::new(CompositeInt2Flags::new(default_result, flag_conditions))
    }

    /// Java `IIntCondition loadGenerationMask()`.
    fn load_generation_mask(&self) -> LoadedBiomeMask {
        self.load_biome_mask(constants::KEY_GENERATION)
    }

    /// Java `IIntCondition loadPopulationMask()`.
    fn load_population_mask(&self) -> LoadedBiomeMask {
        self.load_biome_mask(constants::KEY_POPULATION)
    }

    /// Java `CompositeInt2Flags.FlagCondition[] loadFlagConditions(...)`.
    fn load_flag_conditions(
        &self,
        generate: &LoadedBiomeMask,
        populate: &LoadedBiomeMask,
    ) -> Vec<FlagCondition> {
        let generate: Rc<dyn IIntCondition> = Rc::new(generate.clone());
        let populate: Rc<dyn IIntCondition> = Rc::new(populate.clone());
        vec![
            FlagCondition::new(generate, FlagOperator::Or, cave_biome_mask_flags::GENERATE),
            FlagCondition::new(populate, FlagOperator::Or, cave_biome_mask_flags::POPULATE),
        ]
    }

    /// Java `int loadDefaultResult()`.
    fn load_default_result(&self) -> i32 {
        let mut result = cave_biome_mask_flags::defaults::DEFAULT_RESULT;
        if self.load_flag_setting(
            constants::KEY_TERMINATE,
            constants::DEFAULT_TERMINATE_SETTING,
        ) {
            result ^= cave_biome_mask_flags::CONTINUE;
        }
        result
    }

    /// Java `IIntCondition loadBiomeMask(String maskName)`.
    fn load_biome_mask(&self, mask_name: &str) -> LoadedBiomeMask {
        if !self.base.has(mask_name) {
            // Java `ConstantIntCondition.DEFAULT_TRUE`.
            return LoadedBiomeMask::Default;
        }
        BiomeMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get_raw(mask_name),
            mask_name,
            Rc::clone(&self.zone_context),
        )
        .load()
    }

    /// Java `boolean loadFlagSetting(String key, boolean defaultValue)`.
    fn load_flag_setting(&self, key: &str, default_value: bool) -> bool {
        if !self.base.has(key) {
            return default_value;
        }
        JsonLoaderBase::get_as_bool(&self.base.get_raw(key).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;
    use std::path::PathBuf;

    fn loader(json: Option<Value>) -> CaveBiomeMaskJsonLoader {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        CaveBiomeMaskJsonLoader::new(SeedString::new("0"), Path::new("."), json, zone)
    }

    #[test]
    fn the_seed_appends_the_cave_biome_mask_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CaveBiomeMask");
    }

    #[test]
    fn an_empty_mask_allows_everything() {
        let mask = loader(None).load();
        // The default allow mask has all three flags.
        assert_eq!(mask.eval(0), cave_biome_mask_flags::defaults::ALLOW_ALL);
    }

    #[test]
    fn an_empty_mask_allows_everything_even_with_the_terminate_flag() {
        // The Java loader returns the default allow mask as soon as both
        // the generation and the population mask are the default ones — the
        // terminate flag is not even read.
        let mask = loader(Some(serde_json::json!({"Terminate": true}))).load();
        assert_eq!(mask.eval(0), cave_biome_mask_flags::defaults::ALLOW_ALL);
    }

    #[test]
    fn the_terminate_flag_clears_the_continue_flag() {
        // With a biome mask the loader builds the composite mask: the
        // default result (CONTINUE) is cleared by the terminate flag.
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        let biome = zone.create_biome(
            "Plains",
            PathBuf::from("/tmp/world/Zones/Oceans/Tile.Plains.json"),
            crate::loader::context::biome_file_context::BiomeType::Tile,
        );
        zone.get_tile_biomes()
            .borrow_mut()
            .register("Plains", biome);
        let loader = CaveBiomeMaskJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({
                "Generate": ["Plains"],
                "Terminate": true
            })),
            zone,
        );
        let mask = loader.load();
        // The generation flag is set, the continue flag is not.
        assert!(cave_biome_mask_flags::can_generate(mask.eval(0)));
        assert!(!cave_biome_mask_flags::can_continue(mask.eval(0)));
    }
}
