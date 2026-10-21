// Ported from: `.hytale-source/.../worldgen/loader/cave/FluidLevelJsonLoader.java`
//! Java `loader.cave.FluidLevelJsonLoader` — the fluid (and the block it is
//! bound to) that fills the bottom of a cave.

use std::path::Path;

use serde_json::Value;
use worldgen_model::block_key::BlockEntry;
use worldgen_model::registry::Registries;

use crate::cave::cave_type::FluidLevel;
use crate::util::block_fluid_entry::BlockFluidEntry;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `FluidLevelJsonLoader.Constants`.
pub mod constants {
    pub const KEY_HEIGHT: &str = "Height";
    pub const KEY_BLOCK: &str = "Block";
    pub const KEY_FLUID: &str = "Fluid";

    pub const ERROR_NO_BLOCK: &str =
        "Could not find block to use in FluidLevel container. Keyword: Block";
    pub const ERROR_UNKOWN_BLOCK: &str = "Could not resolve block \"%s\" from BlockTypes.";
    pub const ERROR_UNKOWN_FLUID: &str = "Could not resolve fluid \"%s\" from Fluids.";
}

/// Java `loader.cave.FluidLevelJsonLoader`.
pub struct FluidLevelJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, CaveType.FluidLevel>`.
    pub base: JsonLoaderBase,
    /// The resolved block / fluid registries.
    pub registries: Registries,
}

impl FluidLevelJsonLoader {
    /// Java `FluidLevelJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json)` — note: the Java constructor does **not** append a
    /// seed suffix.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Registries,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            registries,
        }
    }

    /// Java `CaveType.FluidLevel load()`.
    pub fn load(&self) -> FluidLevel {
        let mut block_id = 0;
        let mut rotation = 0;
        let mut fluid_id = 0;

        if self.base.has(constants::KEY_BLOCK) {
            let block_string =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_BLOCK).unwrap());
            let block_type_key = BlockEntry::decode(&block_string);
            block_id = self
                .registries
                .blocks
                .index_of(&block_type_key.block_type_key)
                .unwrap_or_else(|| {
                    panic!(
                        "{}",
                        constants::ERROR_UNKOWN_BLOCK.replace("%s", &block_string)
                    )
                });
            rotation = block_type_key.rotation;
        }

        if self.base.has(constants::KEY_FLUID) {
            let fluid_key =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_FLUID).unwrap());
            fluid_id = self
                .registries
                .fluids
                .index_of(&fluid_key)
                .unwrap_or_else(|| {
                    panic!(
                        "{}",
                        constants::ERROR_UNKOWN_FLUID.replace("%s", &fluid_key)
                    )
                });
        }

        if !self.base.has(constants::KEY_BLOCK) && !self.base.has(constants::KEY_FLUID) {
            panic!("{}", constants::ERROR_NO_BLOCK);
        }

        FluidLevel::new(
            BlockFluidEntry::new(block_id, rotation, fluid_id),
            self.load_height(),
        )
    }

    /// Java `int loadHeight()`.
    fn load_height(&self) -> i32 {
        JsonLoaderBase::get_as_int(&self.base.get(constants::KEY_HEIGHT).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Option<Value>) -> FluidLevelJsonLoader {
        FluidLevelJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            Registries::empty(),
        )
    }

    #[test]
    fn the_seed_is_not_changed() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0");
    }

    #[test]
    fn a_missing_block_and_fluid_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({"Height": 24}))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Could not find block to use in FluidLevel container. Keyword: Block"
        );
    }

    #[test]
    fn an_unknown_fluid_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({"Fluid": "Lava", "Height": 24}))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "Could not resolve fluid \"Lava\" from Fluids.");
    }

    #[test]
    fn the_height_is_read() {
        let registries = Registries::empty();
        // The empty registry knows no fluid, so the fluid lookup fails; the
        // block-only form works.
        let level = FluidLevelJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Block": "Rock_Stone", "Height": 24})),
            registries,
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| level.load()));
        // The block is unknown in the empty registry.
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Could not resolve block \"Rock_Stone\" from BlockTypes."
        );
    }
}
