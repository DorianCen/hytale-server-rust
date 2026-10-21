// Ported from: `.hytale-source/.../worldgen/loader/container/WaterContainerJsonLoader.java`
//! Java `loader.container.WaterContainerJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::container::water_container::{Entry as WaterContainerEntry, WaterContainer};
use crate::loader::container::environment_container_json_loader::constants::{
    ERROR_LOADING_ENTRY, SEED_INDEX_SUFFIX,
};
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::util::constant_noise_property::default_zero;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::{DefaultCoordinateCondition, ICoordinateCondition};
use procedural::noise::property::NoiseProperty;
use procedural::noise::supplier::{
    DoubleRange, DoubleRangeNoiseSupplier, IDoubleCoordinateSupplier,
};

/// Java `WaterContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_ENTRIES: &str = "Entries";

    pub const KEY_ENTRY_BLOCK: &str = "Block";
    pub const ERROR_ENTRY_NO_BLOCK: &str = "Could not find block information. Keyword: Block";
    pub const ERROR_ENTRY_FLUID_BLOCK: &str = "Could not find BlockType for block: %s";

    pub const KEY_ENTRY_FLUID: &str = "Fluid";
    pub const ERROR_ENTRY_NO_FLUID: &str = "Could not find fluid information. Keyword: Fluid";
    pub const ERROR_ENTRY_FLUID_TYPE: &str = "Could not find Fluid for fluid: %s";

    pub const KEY_ENTRY_MIN: &str = "Min";
    pub const KEY_ENTRY_MIN_NOISE: &str = "MinNoise";
    pub const KEY_ENTRY_MAX: &str = "Max";
    pub const KEY_ENTRY_MAX_NOISE: &str = "MaxNoise";
    pub const KEY_ENTRY_NOISE_MASK: &str = "NoiseMask";
    pub const ERROR_ENTRY_NO_MAX: &str = "Could not find maximum of water container entry.";

    pub const KEY_BLOCK: &str = "Block";
    pub const KEY_FLUID: &str = "Fluid";
    pub const KEY_HEIGHT: &str = "Height";
    pub const KEY_HEIGHTMAP: &str = "Heightmap";

    pub const ERROR_FAILED_TO_LOAD: &str = "Failed to load water container.";
}

/// Java `WaterContainerJsonLoader`.
pub struct WaterContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, WaterContainer>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull private final BiomeFileContext biomeContext` — the Java
    /// `ModifyEvents` hooks are not ported.
    pub biome_context: Rc<BiomeFileContext>,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl WaterContainerJsonLoader {
    /// Java `WaterContainerJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, BiomeFileContext biomeContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".WaterContainer"), data_folder, json),
            biome_context,
            registries,
        }
    }

    /// Java `WaterContainer load()`.
    pub fn load(&self) -> WaterContainer {
        let mut entries = Vec::new();

        if self.base.has(constants::KEY_BLOCK) {
            // TODO Loading old water containers here, removed when everything
            // was moved to entries (Java comment).
            let block_type_key =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_BLOCK).unwrap());
            let index = self
                .registries
                .blocks
                .index_of(&block_type_key)
                .unwrap_or_else(|| {
                    panic!(
                        "{}",
                        constants::ERROR_ENTRY_FLUID_TYPE.replacen("%s", &block_type_key, 1)
                    )
                });

            let array = DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_HEIGHT),
                0.0,
            )
            .load();

            let mut heightmap_noise: Rc<dyn NoiseProperty> = default_zero();
            if self.base.has(constants::KEY_HEIGHTMAP) {
                heightmap_noise = NoisePropertyJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_HEIGHTMAP),
                )
                .load();
            }
            let height: Rc<dyn IDoubleCoordinateSupplier> = Rc::new(DoubleRangeNoiseSupplier {
                range: Rc::new(array),
                noise_property: heightmap_noise,
            });

            entries.push(WaterContainerEntry::new(
                index,
                0, // Fluid.EMPTY_ID
                Rc::new(DoubleRangeNoiseSupplier {
                    range: Rc::new(DoubleRange::ZERO),
                    noise_property: default_zero(),
                }),
                height,
                Rc::new(DefaultCoordinateCondition::True),
            ));
        } else if self.base.has(constants::KEY_FLUID) {
            // TODO Loading old water containers here, removed when everything
            // was moved to entries (Java comment).
            let fluid_string =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_FLUID).unwrap());
            let index = self
                .registries
                .fluids
                .index_of(&fluid_string)
                .unwrap_or_else(|| {
                    panic!(
                        "{}",
                        constants::ERROR_ENTRY_FLUID_TYPE.replacen("%s", &fluid_string, 1)
                    )
                });

            let array = DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_HEIGHT),
                0.0,
            )
            .load();

            let mut heightmap_noise: Rc<dyn NoiseProperty> = default_zero();
            if self.base.has(constants::KEY_HEIGHTMAP) {
                heightmap_noise = NoisePropertyJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_HEIGHTMAP),
                )
                .load();
            }
            let height: Rc<dyn IDoubleCoordinateSupplier> = Rc::new(DoubleRangeNoiseSupplier {
                range: Rc::new(array),
                noise_property: heightmap_noise,
            });

            entries.push(WaterContainerEntry::new(
                0, // BlockType.EMPTY_ID
                index,
                Rc::new(DoubleRangeNoiseSupplier {
                    range: Rc::new(DoubleRange::ZERO),
                    noise_property: default_zero(),
                }),
                height,
                Rc::new(DefaultCoordinateCondition::True),
            ));
        } else {
            let fluid_array = self
                .base
                .must_get_array(constants::KEY_ENTRIES, Some(Value::Array(Vec::new())));
            let fluid_array = fluid_array.as_array().unwrap();
            for (index, element) in fluid_array.iter().enumerate() {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    WaterContainerEntryJsonLoader::new(
                        self.base.seed.append(&SEED_INDEX_SUFFIX.replacen(
                            "%s",
                            &index.to_string(),
                            1,
                        )),
                        &self.base.data_folder,
                        Some(element.clone()),
                        Rc::clone(&self.registries),
                    )
                    .load()
                }));
                match result {
                    Ok(entry) => entries.push(entry),
                    Err(_) => panic!(
                        "{}",
                        ERROR_LOADING_ENTRY.replacen("%s", &index.to_string(), 1)
                    ),
                }
            }
        }

        WaterContainer::new(entries)
    }
}

/// Java `WaterContainerJsonLoader.WaterContainerEntryJsonLoader`.
struct WaterContainerEntryJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl WaterContainerEntryJsonLoader {
    /// Java `WaterContainerEntryJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json)`.
    fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".Entry"), data_folder, json),
            registries,
        }
    }

    /// Java `WaterContainer.Entry load()`.
    fn load(&self) -> WaterContainerEntry {
        if self.base.has(constants::KEY_ENTRY_FLUID) {
            let fluid_string =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_ENTRY_FLUID).unwrap());
            let index = self
                .registries
                .fluids
                .index_of(&fluid_string)
                .unwrap_or_else(|| panic!("{}", constants::ERROR_FAILED_TO_LOAD));

            return WaterContainerEntry::new(
                0, // BlockType.EMPTY_ID
                index,
                self.load_min(),
                self.load_max(),
                self.load_noise_mask(),
            );
        }

        if self.base.has(constants::KEY_ENTRY_BLOCK) {
            let block_type_key =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_ENTRY_BLOCK).unwrap());
            let index = self
                .registries
                .blocks
                .index_of(&block_type_key)
                .unwrap_or_else(|| panic!("{}", constants::ERROR_FAILED_TO_LOAD));

            return WaterContainerEntry::new(
                index,
                0, // Fluid.EMPTY_ID
                self.load_min(),
                self.load_max(),
                self.load_noise_mask(),
            );
        }

        panic!("{}", constants::ERROR_ENTRY_NO_FLUID);
    }

    /// Java `IDoubleCoordinateSupplier loadMin()`.
    fn load_min(&self) -> Rc<dyn IDoubleCoordinateSupplier> {
        let array = DoubleRangeJsonLoader::with_default1(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_MIN),
            0.0,
        )
        .load();
        let min_noise = self.load_noise(constants::KEY_ENTRY_MIN_NOISE);
        Rc::new(DoubleRangeNoiseSupplier {
            range: Rc::new(array),
            noise_property: min_noise,
        })
    }

    /// Java `IDoubleCoordinateSupplier loadMax()`.
    fn load_max(&self) -> Rc<dyn IDoubleCoordinateSupplier> {
        if !self.base.has(constants::KEY_ENTRY_MAX) {
            panic!("{}", constants::ERROR_ENTRY_NO_MAX);
        }
        let array = DoubleRangeJsonLoader::with_default1(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_MAX),
            0.0,
        )
        .load();
        let max_noise = self.load_noise(constants::KEY_ENTRY_MAX_NOISE);
        Rc::new(DoubleRangeNoiseSupplier {
            range: Rc::new(array),
            noise_property: max_noise,
        })
    }

    /// Java `NoiseProperty loadNoise(String key)`.
    fn load_noise(&self, key: &str) -> Rc<dyn NoiseProperty> {
        if self.base.has(key) {
            return NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(key),
            )
            .load();
        }
        default_zero()
    }

    /// Java `ICoordinateCondition loadNoiseMask()`.
    fn load_noise_mask(&self) -> Rc<dyn ICoordinateCondition> {
        let mut mask: Rc<dyn ICoordinateCondition> = Rc::new(DefaultCoordinateCondition::True);
        if self.base.has(constants::KEY_ENTRY_NOISE_MASK) {
            mask = NoiseMaskConditionJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ENTRY_NOISE_MASK),
            )
            .load();
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_seed_appends_the_entry_suffix() {
        let loader = WaterContainerEntryJsonLoader::new(
            SeedString::new("0").append("-0"),
            Path::new("."),
            None,
            Rc::new(Registries::empty()),
        );
        assert_eq!(loader.base.seed.seed(), "0-0.Entry");
    }

    #[test]
    fn an_entry_resolves_a_real_fluid_and_band() {
        let loader = WaterContainerEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Fluid": "Water_Source", "Min": 10, "Max": 20})),
            Rc::new(Registries::load()),
        );
        let entry = loader.load();
        assert_eq!(entry.get_block(), 0);
        assert_eq!(entry.get_fluid(), 2, "Water_Source");
        assert_eq!(entry.get_min(0, 0, 0), 10);
        assert_eq!(entry.get_max(0, 0, 0), 20);
    }
}
