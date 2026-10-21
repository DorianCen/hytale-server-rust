// Ported from: `.hytale-source/.../procedurallib/json/CoordinateRandomizerJsonLoader.java`
//! Java `CoordinateRandomizerJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::coordinate_rotator_json_loader::{CoordinateRotatorJsonLoader, LoadedRotator};
use crate::json::noise_property_json_loader::NoisePropertyJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::random::{
    AmplitudeNoiseProperty, CoordinateRandomizer, EmptyCoordinateRandomizer, ICoordinateRandomizer,
    RotatedCoordinateRandomizer,
};

/// `Rc<dyn ICoordinateRandomizer>` — the Java `ICoordinateRandomizer` loader
/// result type.
pub type CoordinateRandomizerObj = Rc<dyn ICoordinateRandomizer>;

/// Java `CoordinateRandomizerJsonLoader`.
pub struct CoordinateRandomizerJsonLoader {
    /// Java `extends JsonLoader<K, ICoordinateRandomizer>` — note: the seed is
    /// passed through **without** a suffix append.
    pub base: JsonLoaderBase,
}

/// Java `String.format(seedSuffix, i)` with the `%s` templates
/// `.X-Noise#%s` / `.Y-Noise#%s` / `.Z-Noise#%s`.
fn format_seed_suffix(template: &str, i: usize) -> String {
    template.replace("%s", &i.to_string())
}

impl CoordinateRandomizerJsonLoader {
    /// Java `CoordinateRandomizerJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `ICoordinateRandomizer load()`.
    pub fn load(&self) -> CoordinateRandomizerObj {
        // Java `json == null || json.isJsonNull()` → EMPTY_RANDOMIZER.
        if matches!(&self.base.json, None | Some(Value::Null)) {
            return Rc::new(EmptyCoordinateRandomizer);
        }
        self.load_randomizer()
    }

    /// Java `ICoordinateRandomizer loadRandomizer()`.
    pub fn load_randomizer(&self) -> Rc<dyn ICoordinateRandomizer> {
        const KEY_ROTATE: &str = "Rotate";
        let mut randomizer: Rc<dyn ICoordinateRandomizer> = Rc::new(CoordinateRandomizer::new(
            self.load_generators(".X-Noise#%s"),
            self.load_generators(".Y-Noise#%s"),
            self.load_generators(".Z-Noise#%s"),
        ));

        if self.base.has(KEY_ROTATE) {
            let rotation = CoordinateRotatorJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(KEY_ROTATE),
            )
            .load();

            // Java `if (rotation != CoordinateRotator.NONE)` — note the
            // `RotatedCoordinateRandomizer` only ever calls the concrete
            // matrix methods, so the origin variant collapses to its base
            // matrix here (Java quirk, see `LoadedRotator`).
            if !matches!(rotation, LoadedRotator::None) {
                randomizer = Rc::new(RotatedCoordinateRandomizer::new(
                    randomizer,
                    rotation.into_matrix_rotator(),
                ));
            }
        }

        randomizer
    }

    /// Java `CoordinateRandomizer.AmplitudeNoiseProperty[] loadGenerators(String seedSuffix)`.
    pub fn load_generators(&self, seed_suffix_template: &str) -> Vec<AmplitudeNoiseProperty> {
        const KEY_GENERATORS: &str = "Generators";
        const KEY_GENERATORS_AMPLITUDE: &str = "Amplitude";

        // Java `get(KEY_GENERATORS).getAsJsonArray()` — a missing key is a
        // NullPointerException and a non-array a ClassCastException; both are
        // load-time fatal (panics).
        let array = match self.base.get(KEY_GENERATORS) {
            Some(Value::Array(items)) => items,
            Some(other) => panic!("Generators must be a JSON array, got: {other:?}"),
            None => panic!("Cannot invoke \"com.google.gson.JsonArray.size()\" because the return value of \"get(String)\" is null"),
        };

        let mut generators = Vec::with_capacity(array.len());
        for (i, object) in array.iter().enumerate() {
            let property = NoisePropertyJsonLoader::new(
                self.base
                    .seed
                    .alternate_original(&format_seed_suffix(seed_suffix_template, i)),
                &self.base.data_folder,
                Some(object.clone()),
            )
            .load();
            // Java `object.get(KEY_GENERATORS_AMPLITUDE).getAsDouble()` — a
            // missing Amplitude is a NullPointerException in Java.
            let amplitude = match object.get(KEY_GENERATORS_AMPLITUDE) {
                Some(v) => JsonLoaderBase::get_as_double(v),
                None => panic!(
                    "Cannot invoke \"com.google.gson.JsonElement.getAsDouble()\" because the return value of \"JsonObject.get(String)\" is null"
                ),
            };
            generators.push(AmplitudeNoiseProperty::new(property, amplitude));
        }
        generators
    }
}
