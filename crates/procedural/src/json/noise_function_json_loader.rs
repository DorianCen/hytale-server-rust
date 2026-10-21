// Ported from: `.hytale-source/.../procedurallib/json/NoiseFunctionJsonLoader.java`
//! Java `NoiseFunctionJsonLoader` — dispatches on the `NoiseType` keyword to
//! the per-type noise loaders (see [`noise_type_json`]).

use std::path::Path;

use serde_json::Value;

use crate::json::noise_type_json::NoiseTypeJson;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::function::NoiseFunction;

/// `Rc<dyn NoiseFunction>` — the Java `NoiseFunction` loader result type.
pub type NoiseFunctionObj = std::rc::Rc<dyn NoiseFunction>;

/// Java `NoiseFunctionJsonLoader`.
pub struct NoiseFunctionJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl NoiseFunctionJsonLoader {
    /// Java `NoiseFunctionJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".NoiseFunction"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> NoiseFunctionObj {
        const KEY_NOISE_TYPE: &str = "NoiseType";
        const ERROR_NO_NOISE_TYPE: &str =
            "Could not find noise type for noise map! Keyword: NoiseType";
        if !self.base.has(KEY_NOISE_TYPE) {
            panic!("{ERROR_NO_NOISE_TYPE}");
        }
        let noise_type = NoiseTypeJson::value_of(&JsonLoaderBase::get_as_string(
            &self.base.get(KEY_NOISE_TYPE).unwrap(),
        ));
        // Java: `noiseTypeJson.newLoader(seed, dataFolder, json).load()` — the
        // FULL json (including the `NoiseType` keyword) goes to the loader.
        noise_type.new_loader(
            &self.base.seed,
            &self.base.data_folder,
            self.base.json.clone(),
        )
    }
}
