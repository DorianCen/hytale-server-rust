// Ported from: `.hytale-source/.../worldgen/loader/container/LayerContainerJsonLoader.java`
//! Java `loader.container.LayerContainerJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::container::layer_container::{
    DynamicLayer, DynamicLayerEntry, LayerContainer, StaticLayer, StaticLayerEntry,
};
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::loader::util::noise_block_array_json_loader::NoiseBlockArrayJsonLoader;
use crate::util::constant_noise_property::default_zero;
use crate::util::noise_block_array::NoiseBlockArray;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::ICoordinateCondition;
use procedural::noise::supplier::{
    DoubleRange, DoubleRangeNoiseSupplier, IDoubleCoordinateSupplier, IDoubleRange,
};
use procedural::noise::support::NoiseProperty;

/// Java `WorldGenConstants.ENVIRONMENT_NOT_SET`.
pub const ENVIRONMENT_NOT_SET: i32 = i32::MIN;

/// Java `LayerContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_DEFAULT: &str = "Default";
    pub const KEY_DYNAMIC: &str = "Dynamic";
    pub const KEY_STATIC: &str = "Static";
    pub const KEY_ENTRY_ENTRIES: &str = "Entries";
    pub const KEY_ENTRY_BLOCKS: &str = "Blocks";
    pub const KEY_ENTRY_NOISE_MASK: &str = "NoiseMask";
    pub const KEY_ENTRY_DYNAMIC_OFFSET: &str = "Offset";
    pub const KEY_ENTRY_DYNAMIC_OFFSET_NOISE: &str = "OffsetNoise";
    pub const KEY_ENTRY_STATIC_MIN: &str = "Min";
    pub const KEY_ENTRY_STATIC_MIN_NOISE: &str = "MinNoise";
    pub const KEY_ENTRY_STATIC_MAX: &str = "Max";
    pub const KEY_ENTRY_STATIC_MAX_NOISE: &str = "MaxNoise";
    pub const KEY_ENVIRONMENT: &str = "Environment";

    pub const ERROR_NO_DEFAULT: &str = "Could not find default material. Keyword: Default";
    pub const ERROR_DEFAULT_INVALID: &str =
        "Default block for LayerContainer could not be found! BlockType: %s";
    pub const ERROR_FAIL_DYNAMIC_LAYER: &str = "Error while loading DynamicLayer #%s";
    pub const ERROR_FAIL_STATIC_LAYER: &str = "Error while loading StaticLayer #%s";
    pub const ERROR_NO_BLOCKS: &str = "Could not find block data for layer entry. Keyword: Blocks";
    pub const ERROR_UNKOWN_STATIC: &str = "Unknown type for static Layer";
    pub const ERROR_UNKOWN_DYNAMIC: &str = "Unknown type for dynamic Layer";
    pub const ERROR_FAIL_DYNAMIC_ENTRY: &str = "Error while loading DynamicLayerEntry #%s";
    pub const ERROR_FAIL_STATIC_ENTRY: &str = "Error while loading StaticLayerEntry #%s";
    pub const ERROR_STATIC_NO_MIN: &str = "Could not find minimum of static layer entry.";
    pub const ERROR_STATIC_NO_MAX: &str = "Could not find maximum of static layer entry.";
}

/// Java `PrefabContainerJsonLoader.Constants.ERROR_LOADING_ENVIRONMENT`.
pub const ERROR_LOADING_ENVIRONMENT: &str = "Error while looking up environment \"%s\"!";

/// Java `LayerContainerJsonLoader`.
pub struct LayerContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, LayerContainer>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull protected final BiomeFileContext biomeContext` — the
    /// Java `ModifyEvents` hooks are not ported.
    pub biome_context: Rc<BiomeFileContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl LayerContainerJsonLoader {
    /// Java `LayerContainerJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, BiomeFileContext biomeContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".LayerContainer"), data_folder, json),
            biome_context,
            registries,
        }
    }

    /// Java `LayerContainer load()`.
    pub fn load(&self) -> LayerContainer {
        LayerContainer::new(
            self.load_default(),
            self.load_default_environment(),
            self.load_static_layers(),
            self.load_dynamic_layers(),
        )
    }

    /// Java `int loadDefault()`.
    fn load_default(&self) -> i32 {
        if !self.base.has(constants::KEY_DEFAULT) {
            panic!("{}", constants::ERROR_NO_DEFAULT);
        }
        let block_name =
            JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_DEFAULT).unwrap());
        match self.registries.blocks.index_of(&block_name) {
            Some(index) => index,
            None => panic!(
                "{}",
                constants::ERROR_DEFAULT_INVALID.replacen("%s", &block_name, 1)
            ),
        }
    }

    /// Java `int loadDefaultEnvironment()`.
    fn load_default_environment(&self) -> i32 {
        let mut environment = ENVIRONMENT_NOT_SET;
        if self.base.has(constants::KEY_ENVIRONMENT) {
            let environment_id =
                JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_ENVIRONMENT).unwrap());
            environment = match self.registries.environments.index_of(&environment_id) {
                Some(index) => index,
                None => panic!(
                    "{}",
                    ERROR_LOADING_ENVIRONMENT.replacen("%s", &environment_id, 1)
                ),
            };
        }
        environment
    }

    /// Java `StaticLayer[] loadStaticLayers()`.
    fn load_static_layers(&self) -> Vec<Rc<StaticLayer>> {
        let layer_array = self
            .base
            .must_get_array(constants::KEY_STATIC, Some(Value::Array(Vec::new())));
        let layer_array = layer_array.as_array().unwrap();
        let mut entries = Vec::with_capacity(layer_array.len());
        for (index, element) in layer_array.iter().enumerate() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                StaticLayerJsonLoader::new(
                    self.base.seed.append(&format!("-{index}")),
                    &self.base.data_folder,
                    Some(element.clone()),
                    Rc::clone(&self.registries),
                )
                .load()
            }));
            match result {
                Ok(layer) => entries.push(Rc::new(layer)),
                Err(_) => panic!(
                    "{}",
                    constants::ERROR_FAIL_STATIC_LAYER.replacen("%s", &index.to_string(), 1)
                ),
            }
        }
        entries
    }

    /// Java `DynamicLayer[] loadDynamicLayers()`.
    fn load_dynamic_layers(&self) -> Vec<Rc<DynamicLayer>> {
        let layer_array = self
            .base
            .must_get_array(constants::KEY_DYNAMIC, Some(Value::Array(Vec::new())));
        let layer_array = layer_array.as_array().unwrap();
        let mut entries = Vec::with_capacity(layer_array.len());
        for (index, element) in layer_array.iter().enumerate() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                DynamicLayerJsonLoader::new(
                    self.base.seed.append(&format!("-{index}")),
                    &self.base.data_folder,
                    Some(element.clone()),
                    Rc::clone(&self.registries),
                )
                .load()
            }));
            match result {
                Ok(layer) => entries.push(Rc::new(layer)),
                Err(_) => panic!(
                    "{}",
                    constants::ERROR_FAIL_DYNAMIC_LAYER.replacen("%s", &index.to_string(), 1)
                ),
            }
        }
        entries
    }
}

/// Shared helpers of the Java `LayerEntryJsonLoader` base class.
fn load_blocks(base: &JsonLoaderBase, registries: Rc<Registries>) -> Rc<NoiseBlockArray> {
    if !base.has(constants::KEY_ENTRY_BLOCKS) {
        return Rc::new(NoiseBlockArray::empty());
    }
    Rc::new(
        NoiseBlockArrayJsonLoader::new(
            base.seed.clone(),
            &base.data_folder,
            base.get(constants::KEY_ENTRY_BLOCKS),
            registries,
        )
        .load(),
    )
}

fn load_entry_map_condition(base: &JsonLoaderBase) -> Rc<dyn ICoordinateCondition> {
    NoiseMaskConditionJsonLoader::new(
        base.seed.clone(),
        &base.data_folder,
        base.get(constants::KEY_ENTRY_NOISE_MASK),
    )
    .load()
}

/// Java `LayerContainerJsonLoader.StaticLayerJsonLoader`.
struct StaticLayerJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl StaticLayerJsonLoader {
    fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".StaticLayer"), data_folder, json),
            registries,
        }
    }

    /// Java `StaticLayer load()`.
    fn load(&self) -> StaticLayer {
        StaticLayer::new(
            self.load_entries(),
            self.load_map_condition(),
            self.load_environment(),
        )
    }

    fn load_map_condition(&self) -> Rc<dyn ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_NOISE_MASK),
        )
        .load()
    }

    /// Java `StaticLayerEntry[] loadEntries()`.
    fn load_entries(&self) -> Vec<Rc<StaticLayerEntry>> {
        match self.base.json.as_ref() {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Object(_)) => {
                if self.base.has(constants::KEY_ENTRY_ENTRIES) {
                    let array = self
                        .base
                        .get(constants::KEY_ENTRY_ENTRIES)
                        .unwrap()
                        .as_array()
                        .unwrap()
                        .clone();
                    let mut entries = Vec::with_capacity(array.len());
                    for (index, element) in array.iter().enumerate() {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            StaticLayerEntryJsonLoader::new(
                                self.base.seed.append(&format!("-{index}")),
                                &self.base.data_folder,
                                Some(element.clone()),
                                Rc::clone(&self.registries),
                            )
                            .load()
                        }));
                        match result {
                            Ok(entry) => entries.push(Rc::new(entry)),
                            Err(_) => panic!(
                                "{}",
                                constants::ERROR_FAIL_STATIC_ENTRY.replacen(
                                    "%s",
                                    &index.to_string(),
                                    1
                                )
                            ),
                        }
                    }
                    entries
                } else {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        StaticLayerEntryJsonLoader::new(
                            self.base.seed.clone(),
                            &self.base.data_folder,
                            self.base.json.clone(),
                            Rc::clone(&self.registries),
                        )
                        .load()
                    }));
                    match result {
                        Ok(entry) => vec![Rc::new(entry)],
                        Err(_) => panic!(
                            "{}",
                            constants::ERROR_FAIL_STATIC_ENTRY.replacen("%s", "0", 1)
                        ),
                    }
                }
            }
            Some(_) => panic!("{}", constants::ERROR_UNKOWN_STATIC),
        }
    }

    /// Java `int loadEnvironment()`.
    fn load_environment(&self) -> i32 {
        load_environment(&self.base, &self.registries)
    }
}

/// Java `LayerContainerJsonLoader.DynamicLayerJsonLoader`.
struct DynamicLayerJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl DynamicLayerJsonLoader {
    fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".DynamicLayer"), data_folder, json),
            registries,
        }
    }

    /// Java `DynamicLayer load()`.
    fn load(&self) -> DynamicLayer {
        DynamicLayer::new(
            self.load_entries(),
            self.load_map_condition(),
            self.load_environment(),
            self.load_offset(),
        )
    }

    fn load_map_condition(&self) -> Rc<dyn ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_NOISE_MASK),
        )
        .load()
    }

    /// Java `IDoubleCoordinateSupplier loadOffset()`.
    fn load_offset(&self) -> Rc<dyn IDoubleCoordinateSupplier> {
        let mut offset: Rc<dyn IDoubleRange> = Rc::new(DoubleRange::ZERO);
        let mut offset_noise: Rc<dyn NoiseProperty> = default_zero();
        if self.base.has(constants::KEY_ENTRY_DYNAMIC_OFFSET) {
            offset = Rc::new(
                DoubleRangeJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_ENTRY_DYNAMIC_OFFSET),
                )
                .load(),
            );
            if self.base.has(constants::KEY_ENTRY_DYNAMIC_OFFSET_NOISE) {
                offset_noise = NoisePropertyJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_ENTRY_DYNAMIC_OFFSET_NOISE),
                )
                .load();
            }
        }
        Rc::new(DoubleRangeNoiseSupplier {
            range: offset,
            noise_property: offset_noise,
        })
    }

    /// Java `DynamicLayerEntry[] loadEntries()`.
    fn load_entries(&self) -> Vec<Rc<DynamicLayerEntry>> {
        match self.base.json.as_ref() {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Object(_)) => {
                if self.base.has(constants::KEY_ENTRY_ENTRIES) {
                    let array = self
                        .base
                        .get(constants::KEY_ENTRY_ENTRIES)
                        .unwrap()
                        .as_array()
                        .unwrap()
                        .clone();
                    let mut entries = Vec::with_capacity(array.len());
                    for (index, element) in array.iter().enumerate() {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            DynamicLayerEntryJsonLoader::new(
                                self.base.seed.append(&format!("-{index}")),
                                &self.base.data_folder,
                                Some(element.clone()),
                                Rc::clone(&self.registries),
                            )
                            .load()
                        }));
                        match result {
                            Ok(entry) => entries.push(Rc::new(entry)),
                            Err(_) => panic!(
                                "{}",
                                constants::ERROR_FAIL_DYNAMIC_ENTRY.replacen(
                                    "%s",
                                    &index.to_string(),
                                    1
                                )
                            ),
                        }
                    }
                    entries
                } else {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        DynamicLayerEntryJsonLoader::new(
                            self.base.seed.clone(),
                            &self.base.data_folder,
                            self.base.json.clone(),
                            Rc::clone(&self.registries),
                        )
                        .load()
                    }));
                    match result {
                        Ok(entry) => vec![Rc::new(entry)],
                        Err(_) => panic!(
                            "{}",
                            constants::ERROR_FAIL_DYNAMIC_ENTRY.replacen("%s", "0", 1)
                        ),
                    }
                }
            }
            Some(_) => panic!("{}", constants::ERROR_UNKOWN_DYNAMIC),
        }
    }

    /// Java `int loadEnvironment()`.
    fn load_environment(&self) -> i32 {
        load_environment(&self.base, &self.registries)
    }
}

/// Shared Java `loadEnvironment()` of the static/dynamic layer loaders.
fn load_environment(base: &JsonLoaderBase, registries: &Registries) -> i32 {
    let mut environment = ENVIRONMENT_NOT_SET;
    if base.has(constants::KEY_ENVIRONMENT) {
        let environment_id =
            JsonLoaderBase::get_as_string(&base.get(constants::KEY_ENVIRONMENT).unwrap());
        environment = match registries.environments.index_of(&environment_id) {
            Some(index) => index,
            None => panic!(
                "{}",
                ERROR_LOADING_ENVIRONMENT.replacen("%s", &environment_id, 1)
            ),
        };
    }
    environment
}

/// Java `LayerContainerJsonLoader.StaticLayerJsonLoader.StaticLayerEntryJsonLoader`.
struct StaticLayerEntryJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl StaticLayerEntryJsonLoader {
    fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            // Java `StaticLayerEntryJsonLoader` calls
            // `super(seed.append(".StaticLayerEntry"), ...)`, and the shared
            // `LayerEntryJsonLoader` constructor then appends `".LayerEntry"`.
            base: JsonLoaderBase::new(
                seed.append(".StaticLayerEntry").append(".LayerEntry"),
                data_folder,
                json,
            ),
            registries,
        }
    }

    /// Java `StaticLayerEntry load()`.
    fn load(&self) -> StaticLayerEntry {
        StaticLayerEntry::new(
            load_blocks(&self.base, Rc::clone(&self.registries)),
            load_entry_map_condition(&self.base),
            self.load_min(),
            self.load_max(),
        )
    }

    /// Java `IDoubleCoordinateSupplier loadMin()`.
    fn load_min(&self) -> Rc<dyn IDoubleCoordinateSupplier> {
        if !self.base.has(constants::KEY_ENTRY_STATIC_MIN) {
            panic!("{}", constants::ERROR_STATIC_NO_MIN);
        }
        let range = DoubleRangeJsonLoader::with_default1(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_STATIC_MIN),
            0.0,
        )
        .load();
        Rc::new(DoubleRangeNoiseSupplier {
            range: Rc::new(range),
            noise_property: self.load_min_noise(),
        })
    }

    /// Java `IDoubleCoordinateSupplier loadMax()`.
    fn load_max(&self) -> Rc<dyn IDoubleCoordinateSupplier> {
        if !self.base.has(constants::KEY_ENTRY_STATIC_MAX) {
            panic!("{}", constants::ERROR_STATIC_NO_MAX);
        }
        let range = DoubleRangeJsonLoader::with_default1(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_STATIC_MAX),
            0.0,
        )
        .load();
        Rc::new(DoubleRangeNoiseSupplier {
            range: Rc::new(range),
            noise_property: self.load_max_noise(),
        })
    }

    /// Java `NoiseProperty loadMinNoise()`.
    fn load_min_noise(&self) -> Rc<dyn NoiseProperty> {
        if self.base.has(constants::KEY_ENTRY_STATIC_MIN_NOISE) {
            return NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ENTRY_STATIC_MIN_NOISE),
            )
            .load();
        }
        default_zero()
    }

    /// Java `NoiseProperty loadMaxNoise()`.
    fn load_max_noise(&self) -> Rc<dyn NoiseProperty> {
        if self.base.has(constants::KEY_ENTRY_STATIC_MAX_NOISE) {
            return NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ENTRY_STATIC_MAX_NOISE),
            )
            .load();
        }
        default_zero()
    }
}

/// Java `LayerContainerJsonLoader.DynamicLayerJsonLoader.DynamicLayerEntryJsonLoader`.
struct DynamicLayerEntryJsonLoader {
    base: JsonLoaderBase,
    registries: Rc<Registries>,
}

impl DynamicLayerEntryJsonLoader {
    fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".LayerEntry"), data_folder, json),
            registries,
        }
    }

    /// Java `DynamicLayerEntry load()`.
    fn load(&self) -> DynamicLayerEntry {
        DynamicLayerEntry::new(
            load_blocks(&self.base, Rc::clone(&self.registries)),
            load_entry_map_condition(&self.base),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::layer_container::LayerEntry;

    fn empty_registries() -> Rc<Registries> {
        Rc::new(Registries::empty())
    }

    #[test]
    fn static_entry_seed_appends_static_layer_entry_then_layer_entry() {
        let loader = StaticLayerEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Blocks": "Rock_Stone", "Min": 0, "Max": 1})),
            empty_registries(),
        );
        // Java: `StaticLayerEntryJsonLoader` passes
        // `seed.append(".StaticLayerEntry")` to `LayerEntryJsonLoader`, which
        // appends ".LayerEntry".
        assert_eq!(loader.base.seed.seed(), "0.StaticLayerEntry.LayerEntry");
    }

    #[test]
    fn dynamic_entry_seed_appends_only_layer_entry() {
        let loader = DynamicLayerEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Blocks": "Rock_Stone"})),
            empty_registries(),
        );
        // Java: `DynamicLayerEntryJsonLoader` calls `super(seed, ...)`, so the
        // only suffix is ".LayerEntry".
        assert_eq!(loader.base.seed.seed(), "0.LayerEntry");
    }

    #[test]
    fn static_entry_resolves_real_block_names() {
        let loader = StaticLayerEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Blocks": "Rock_Stone", "Min": 10, "Max": 20})),
            Rc::new(Registries::load()),
        );
        let entry = loader.load();
        assert_eq!(
            entry
                .get_block_array()
                .get_top_block_at(0, 0.0, 0.0)
                .block_id,
            1527,
            "Rock_Stone"
        );
        assert_eq!(entry.get_min_int(0, 0, 0), 10);
        assert_eq!(entry.get_max_int(0, 0, 0), 20);
    }
}
