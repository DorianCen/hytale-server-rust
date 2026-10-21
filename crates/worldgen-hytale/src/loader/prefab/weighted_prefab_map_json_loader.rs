// Ported from: `.hytale-source/.../worldgen/loader/prefab/WeightedPrefabMapJsonLoader.java`
//! Java `loader.prefab.WeightedPrefabMapJsonLoader` — the weighted map of
//! the prefab suppliers of a prefab container entry.
//!
//! The Java loader resolves every prefab name through the prefab store; a
//! name may match several prefab files (the `".*"` wildcard names do), and
//! the weight of the entry is divided by the number of the matching files
//! so that every file keeps the weight of the entry.

use std::rc::Rc;

use serde_json::Value;

use crate::loader::world_gen_prefab_loader::WorldGenPrefabLoader;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::weighted_map::WeightedMap;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `WeightedPrefabMapJsonLoader.Constants`.
pub mod constants {
    /// Java `public static final String ERROR_ENTRY_NO_PREFAB` =
    /// `"Could not find prefab names. Keyword: %s"`.
    pub const ERROR_ENTRY_NO_PREFAB: &str = "Could not find prefab names. Keyword: %s";
    /// Java `public static final String ERROR_ENTRY_WEIGHT_SIZE` =
    /// `"Weight array size is different from prefab name array."`.
    pub const ERROR_ENTRY_WEIGHT_SIZE: &str =
        "Weight array size is different from prefab name array.";
    /// Java `public static final String ERROR_NO_PREFABS` =
    /// `"Prefabs are defined but could not find a valid entry!"`.
    pub const ERROR_NO_PREFABS: &str = "Prefabs are defined but could not find a valid entry!";
}

/// Java `loader.prefab.WeightedPrefabMapJsonLoader`.
pub struct WeightedPrefabMapJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource,
    /// IWeightedMap<WorldGenPrefabSupplier>>`.
    pub base: JsonLoaderBase,
    /// Java `protected final String prefabsKey`.
    prefabs_key: String,
    /// Java `protected final String weightsKey`.
    weights_key: String,
    /// The prefab loader the names are resolved through (Java
    /// `seed.get().getLoader()`).
    loader: Rc<WorldGenPrefabLoader>,
}

impl WeightedPrefabMapJsonLoader {
    /// Java `WeightedPrefabMapJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, String prefabsKey, String weightsKey)`.
    pub fn new(
        seed: SeedString,
        data_folder: &std::path::Path,
        json: Option<Value>,
        prefabs_key: &str,
        weights_key: &str,
        loader: Rc<WorldGenPrefabLoader>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".WeightedPrefabMap"), data_folder, json),
            prefabs_key: prefabs_key.to_string(),
            weights_key: weights_key.to_string(),
            loader,
        }
    }

    /// Java `IWeightedMap<WorldGenPrefabSupplier> load()`.
    pub fn load(&self) -> WeightedMap<WorldGenPrefabSupplier> {
        let mut builder = WeightedMap::<WorldGenPrefabSupplier>::builder();
        if !self.base.has(&self.prefabs_key) {
            panic!("{}", self.prefabs_key);
        }
        let prefab_element = self
            .base
            .get(&self.prefabs_key)
            .expect("the prefabs key is present");
        match &prefab_element {
            Value::Array(prefab_array) => {
                let weight_array = if self.base.has(&self.weights_key) {
                    let weights = self
                        .base
                        .get(&self.weights_key)
                        .expect("the weights key is present");
                    let weights = weights
                        .as_array()
                        .cloned()
                        .unwrap_or_else(|| panic!("Not a JSON Array: {weights}"));
                    if weights.len() != prefab_array.len() {
                        panic!("{}", constants::ERROR_ENTRY_WEIGHT_SIZE);
                    }
                    Some(weights)
                } else {
                    None
                };

                for (index, prefab) in prefab_array.iter().enumerate() {
                    let name = JsonLoaderBase::get_as_string(prefab);
                    let suppliers = self.loader.get(&name);
                    let weight = match &weight_array {
                        Some(weights) => {
                            JsonLoaderBase::get_as_double(&weights[index]) / suppliers.len() as f64
                        }
                        None => 1.0,
                    };
                    for supplier in suppliers {
                        builder.put(supplier, weight);
                    }
                }
            }
            other => {
                let name = JsonLoaderBase::get_as_string(other);
                for supplier in self.loader.get(&name) {
                    builder.put(supplier, 1.0);
                }
            }
        }

        if builder.size() == 0 {
            panic!("{}", constants::ERROR_NO_PREFABS);
        }
        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::test_prefab_store::test_prefab_loader;
    use crate::weighted_map::IWeightedMap;
    use procedural::json::SeedString;

    fn loader(json: Option<Value>) -> WeightedPrefabMapJsonLoader {
        WeightedPrefabMapJsonLoader::new(
            SeedString::new("0"),
            std::path::Path::new("."),
            json,
            "Prefab",
            "Weight",
            test_prefab_loader(),
        )
    }

    #[test]
    fn the_seed_appends_the_weighted_prefab_map_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.WeightedPrefabMap");
    }

    #[test]
    fn a_missing_prefab_key_panics_with_the_key_name() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| loader(None).load()));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(message, "Prefab");
    }

    #[test]
    fn the_prefab_names_are_resolved_through_the_store() {
        let map = loader(Some(serde_json::json!({"Prefab": ["A"]}))).load();
        assert_eq!(map.size(), 1);
    }

    #[test]
    fn the_weight_is_divided_by_the_number_of_the_matching_files() {
        // Every name matches exactly one file in the test store, so the
        // weights are unchanged. (A *singleton* map reports the weight 1.0
        // — the Java `SingletonWeightedMap.forEachEntry` quirk — so the test
        // uses two entries.)
        let map = loader(Some(serde_json::json!({
            "Prefab": ["A", "B"],
            "Weight": [3.0, 5.0]
        })))
        .load();
        let mut weights = Vec::new();
        map.for_each_entry(&mut |_supplier, weight| weights.push(weight));
        assert_eq!(weights, vec![3.0, 5.0]);
    }

    #[test]
    fn the_weight_array_size_must_match() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({
                "Prefab": ["A", "B"],
                "Weight": [1.0]
            })))
            .load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Weight array size is different from prefab name array."
        );
    }
}
