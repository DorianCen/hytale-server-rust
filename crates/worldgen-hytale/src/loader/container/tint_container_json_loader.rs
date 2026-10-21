// Ported from: `.hytale-source/.../worldgen/loader/container/TintContainerJsonLoader.java`
//! Java `loader.container.TintContainerJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::container::tint_container::{
    DefaultTintContainerEntry, TintContainer, TintContainerEntry,
};
use crate::loader::color_util::ColorUtil;
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::util::constant_noise_property::default_zero;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::ICoordinateCondition;
use procedural::noise::property::NoiseProperty;

/// Java `TintContainerJsonLoader.Constants`.
pub mod constants {
    pub const KEY_DEFAULT: &str = "Default";
    pub const KEY_ENTRIES: &str = "Entries";
    pub const KEY_COLORS: &str = "Colors";
    pub const KEY_WEIGHTS: &str = "Weights";
    pub const KEY_ENTRY_NOISE: &str = "Noise";
    pub const KEY_NOISE_MASK: &str = "NoiseMask";

    pub const ERROR_COLORS_NOT_FOUND: &str = "Could not find colors. Keyword: Colors";
    pub const ERROR_WEIGHT_SIZE: &str = "Tint weights array size does not fit color array size.";
    pub const ERROR_NO_VALUE_NOISE: &str = "Could not find value noise. Keyword: Noise";
    pub const ERROR_LOADING_ENTRY: &str = "Failed to load TintContainerEntry #%s";

    pub const SEED_INDEX_SUFFIX: &str = "-%s";

    pub const DEFAULT_TINT_COLOR: i32 = 0xFF0000;
}

/// Java `TintContainerJsonLoader`.
pub struct TintContainerJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, TintContainer>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull private final BiomeFileContext biomeContext` — used by
    /// the Java `ModifyEvents` hooks. The modifier package is not ported, so
    /// the context is kept for constructor parity only.
    pub biome_context: Rc<BiomeFileContext>,
}

impl TintContainerJsonLoader {
    /// Java `TintContainerJsonLoader(SeedString seed, Path dataFolder, JsonElement json, BiomeFileContext biomeContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".TintContainer"), data_folder, json),
            biome_context,
        }
    }

    /// Java `TintContainer load()`.
    pub fn load(&self) -> TintContainer {
        TintContainer::new(self.load_default(), self.load_entries())
    }

    /// Java `DefaultTintContainerEntry loadDefault()`.
    fn load_default(&self) -> DefaultTintContainerEntry {
        let element = match self.base.json.as_ref() {
            None | Some(Value::Null) | Some(Value::Array(_)) => None,
            Some(Value::Object(_)) if self.base.has(constants::KEY_DEFAULT) => {
                self.base.get(constants::KEY_DEFAULT)
            }
            Some(Value::String(_))
            | Some(Value::Number(_))
            | Some(Value::Bool(_))
            | Some(Value::Object(_)) => self.base.json.clone(),
        };
        DefaultTintContainerEntryJsonLoader {
            inner: TintContainerEntryJsonLoader::new(
                self.base.seed.append(".DefaultTintContainerEntry"),
                &self.base.data_folder,
                element,
            ),
        }
        .load()
    }

    /// Java `List<TintContainerEntry> loadEntries()`.
    fn load_entries(&self) -> Vec<TintContainerEntry> {
        let entries_value = self
            .base
            .must_get_array(constants::KEY_ENTRIES, Some(Value::Array(Vec::new())));
        let entries_array = entries_value.as_array().unwrap();
        let mut entries = Vec::with_capacity(entries_array.len());
        for (index, element) in entries_array.iter().enumerate() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                TintContainerEntryJsonLoader::new(
                    self.base.seed.append(&format!("-{index}")),
                    &self.base.data_folder,
                    Some(element.clone()),
                )
                .load()
            }));
            match result {
                Ok(entry) => entries.push(entry),
                Err(_) => panic!(
                    "{}",
                    constants::ERROR_LOADING_ENTRY.replacen("%s", &index.to_string(), 1)
                ),
            }
        }
        entries
    }
}

/// Java `TintContainerJsonLoader.TintContainerEntryJsonLoader`.
pub struct TintContainerEntryJsonLoader {
    base: JsonLoaderBase,
}

impl TintContainerEntryJsonLoader {
    /// Java `TintContainerEntryJsonLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".TintContainerEntry"), data_folder, json),
        }
    }

    /// Java `TintContainerEntry load()`.
    pub fn load(&self) -> TintContainerEntry {
        let color_mapping = self.load_color_mapping();
        TintContainerEntry::new(
            Rc::clone(&color_mapping),
            if color_mapping.size() > 1 {
                self.load_value_noise()
            } else {
                default_zero()
            },
            self.load_map_condition(),
        )
    }

    /// Java `IWeightedMap<Integer> loadColorMapping()`.
    fn load_color_mapping(&self) -> Rc<dyn IWeightedMap<i32>> {
        let mut builder = WeightedMap::<i32>::builder();
        match self.base.json.as_ref() {
            None | Some(Value::Null) => {
                builder.put(constants::DEFAULT_TINT_COLOR, 1.0);
            }
            Some(Value::Object(_)) => {
                if !self.base.has(constants::KEY_COLORS) {
                    panic!("{}", constants::ERROR_COLORS_NOT_FOUND);
                }
                let colors_element = self.base.get(constants::KEY_COLORS).unwrap();
                if let Some(colors) = colors_element.as_array() {
                    let weights = if self.base.has(constants::KEY_WEIGHTS) {
                        Some(self.base.get(constants::KEY_WEIGHTS).unwrap())
                    } else {
                        None
                    };
                    if let Some(weights) = &weights {
                        if weights.as_array().map(Vec::len) != Some(colors.len()) {
                            panic!("{}", constants::ERROR_WEIGHT_SIZE);
                        }
                    }
                    for (index, color) in colors.iter().enumerate() {
                        let color = ColorUtil::hex_string(&JsonLoaderBase::get_as_string(color));
                        let weight = match &weights {
                            Some(weights) => {
                                JsonLoaderBase::get_as_double(&weights.as_array().unwrap()[index])
                            }
                            None => 1.0,
                        };
                        builder.put(color, weight);
                    }
                } else {
                    let color =
                        ColorUtil::hex_string(&JsonLoaderBase::get_as_string(&colors_element));
                    builder.put(color, 1.0);
                }
            }
            Some(primitive) => {
                builder.put(
                    ColorUtil::hex_string(&JsonLoaderBase::get_as_string(primitive)),
                    1.0,
                );
            }
        }
        Rc::new(builder.build())
    }

    /// Java `NoiseProperty loadValueNoise()`.
    fn load_value_noise(&self) -> Rc<dyn NoiseProperty> {
        if !self.base.has(constants::KEY_ENTRY_NOISE) {
            panic!("{}", constants::ERROR_NO_VALUE_NOISE);
        }
        NoisePropertyJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ENTRY_NOISE),
        )
        .load()
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
}

/// Java `TintContainerJsonLoader.DefaultTintContainerEntryJsonLoader`.
struct DefaultTintContainerEntryJsonLoader {
    inner: TintContainerEntryJsonLoader,
}

impl DefaultTintContainerEntryJsonLoader {
    /// Java `DefaultTintContainerEntry load()`.
    fn load(&self) -> DefaultTintContainerEntry {
        let color_mapping: Rc<dyn IWeightedMap<i32>> = match self.inner.base.json.as_ref() {
            None | Some(Value::Null) => {
                let mut builder = WeightedMap::<i32>::builder();
                builder.put(constants::DEFAULT_TINT_COLOR, 1.0);
                Rc::new(builder.build())
            }
            Some(primitive) if !primitive.is_object() && !primitive.is_array() => {
                let mut builder = WeightedMap::<i32>::builder();
                builder.put(
                    ColorUtil::hex_string(&JsonLoaderBase::get_as_string(primitive)),
                    1.0,
                );
                Rc::new(builder.build())
            }
            _ => self.inner.load_color_mapping(),
        };
        DefaultTintContainerEntry::new(
            Rc::clone(&color_mapping),
            if color_mapping.size() > 1 {
                self.inner.load_value_noise()
            } else {
                default_zero()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_seed_appends_the_tint_entry_suffix() {
        let loader = TintContainerEntryJsonLoader::new(
            SeedString::new("0").append("-0"),
            Path::new("."),
            None,
        );
        assert_eq!(loader.base.seed.seed(), "0-0.TintContainerEntry");
    }

    #[test]
    fn default_entry_seed_appends_both_java_suffixes() {
        // Java `DefaultTintContainerEntryJsonLoader` passes
        // `seed.append(".DefaultTintContainerEntry")` to the
        // `TintContainerEntryJsonLoader` constructor.
        let loader = TintContainerEntryJsonLoader::new(
            SeedString::new("0").append(".DefaultTintContainerEntry"),
            Path::new("."),
            None,
        );
        assert_eq!(
            loader.base.seed.seed(),
            "0.DefaultTintContainerEntry.TintContainerEntry"
        );
    }

    #[test]
    fn a_primitive_entry_is_a_single_color() {
        let loader = TintContainerEntryJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!("#abcdef")),
        );
        let entry = loader.load();
        assert_eq!(entry.get_color_mapping().get(0.0).copied(), Some(0xabcdef));
    }
}
