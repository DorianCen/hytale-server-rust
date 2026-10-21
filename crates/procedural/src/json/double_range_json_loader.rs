// Ported from: `.hytale-source/.../procedurallib/json/DoubleRangeJsonLoader.java`
//! Java `DoubleRangeJsonLoader`.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::supplier::DoubleRange;

/// Java `DoubleRangeJsonLoader.DoubleToDoubleFunction`.
pub type DoubleToDoubleFunction = fn(f64) -> f64;

/// Bridge: the noise core consumers ([`crate::noise::branch_noise::BranchNoise`],
/// [`crate::noise::cell::evaluator::distance_point_evaluator::DistancePointEvaluator`])
/// take the reduced `support::IDoubleRange` seam (only `getValue(double)` —
/// the only overload they call), while this loader produces the real
/// [`DoubleRange`]. Implementing the seam trait for the real range keeps the
/// loaded object usable at both call sites (integration note: this impl moves
/// into `supplier.rs` when the `support` seams are retired).
impl crate::noise::support::IDoubleRange for DoubleRange {
    fn get_value(&self, v: f64) -> f64 {
        crate::noise::supplier::IDoubleRange::get_value(self, v)
    }
}

/// Java `DoubleRangeJsonLoader`.
pub struct DoubleRangeJsonLoader {
    /// Java `extends JsonLoader<K, IDoubleRange>`.
    pub base: JsonLoaderBase,
    /// Java `protected final double default1`.
    default1: f64,
    /// Java `protected final double default2`.
    default2: f64,
    /// Java `@Nonnull protected final DoubleToDoubleFunction function`.
    function: DoubleToDoubleFunction,
}

impl DoubleRangeJsonLoader {
    /// Java `DoubleRangeJsonLoader(seed, dataFolder, json)` — defaults `0, identity`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self::with_defaults(seed, data_folder, json, 0.0, 0.0, |d| d)
    }

    /// Java `DoubleRangeJsonLoader(seed, dataFolder, json, DoubleToDoubleFunction function)`.
    pub fn with_function(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        function: DoubleToDoubleFunction,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, 0.0, 0.0, function)
    }

    /// Java `DoubleRangeJsonLoader(seed, dataFolder, json, double default1)`.
    pub fn with_default1(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f64,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, default1, default1, |d| d)
    }

    /// Java `DoubleRangeJsonLoader(seed, dataFolder, json, double default1, DoubleToDoubleFunction function)`.
    pub fn with_default1_and_function(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f64,
        function: DoubleToDoubleFunction,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, default1, default1, function)
    }

    /// Java `DoubleRangeJsonLoader(seed, dataFolder, json, double default1, double default2)`.
    pub fn with_default1_and_default2(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f64,
        default2: f64,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, default1, default2, |d| d)
    }

    /// Java `DoubleRangeJsonLoader(seed, dataFolder, json, double default1, double default2, DoubleToDoubleFunction function)`.
    pub fn with_defaults(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f64,
        default2: f64,
        function: DoubleToDoubleFunction,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".DoubleRange"), data_folder, json),
            default1,
            default2,
            function,
        }
    }

    /// Java `IDoubleRange load()`.
    pub fn load(&self) -> DoubleRange {
        const KEY_MIN: &str = "Min";
        const KEY_MAX: &str = "Max";
        const KEY_THRESHOLDS: &str = "Thresholds";
        const KEY_VALUES: &str = "Values";

        if matches!(&self.base.json, None | Some(Value::Null)) {
            if self.default1 == self.default2 {
                DoubleRange::constant((self.function)(self.default1))
            } else {
                DoubleRange::normal(
                    (self.function)(self.default1),
                    (self.function)(self.default2),
                )
            }
        } else if self.base.json.as_ref().unwrap().is_array() {
            let array = self.base.json.as_ref().unwrap().as_array().unwrap();
            if array.len() != 1 && array.len() != 2 {
                panic!(
                    "Range array contains {} values. Only 1 or 2 entries are allowed.",
                    array.len()
                );
            }
            if array.len() == 1 {
                DoubleRange::constant((self.function)(JsonLoaderBase::get_as_double(&array[0])))
            } else {
                DoubleRange::normal(
                    (self.function)(JsonLoaderBase::get_as_double(&array[0])),
                    (self.function)(JsonLoaderBase::get_as_double(&array[1])),
                )
            }
        } else if self.base.json.as_ref().unwrap().is_object() {
            if self.base.has(KEY_THRESHOLDS) && self.base.has(KEY_VALUES) {
                return self.load_threshold();
            }

            if !self.base.has(KEY_MIN) {
                panic!("Minimum value of range is not defined. Keyword: {KEY_MIN}");
            }
            if !self.base.has(KEY_MAX) {
                panic!("Maximum value of range is not defined. Keyword: {KEY_MAX}");
            }
            let min = JsonLoaderBase::get_as_double(&self.base.get(KEY_MIN).unwrap());
            let max = JsonLoaderBase::get_as_double(&self.base.get(KEY_MAX).unwrap());
            DoubleRange::normal((self.function)(min), (self.function)(max))
        } else {
            DoubleRange::constant((self.function)(JsonLoaderBase::get_as_double(
                self.base.json.as_ref().unwrap(),
            )))
        }
    }

    /// Java `IDoubleRange loadThreshold()`.
    fn load_threshold(&self) -> DoubleRange {
        const KEY_THRESHOLDS: &str = "Thresholds";
        const KEY_VALUES: &str = "Values";

        let thresholds_json = self
            .base
            .get(KEY_THRESHOLDS)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_else(|| panic!("Thresholds must be a JSON array"));
        let values_json = self
            .base
            .get(KEY_VALUES)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_else(|| panic!("Values must be a JSON array"));

        let mut thresholds = vec![0.0; thresholds_json.len()];
        let mut values = vec![0.0; thresholds_json.len()];
        for i in 0..thresholds.len() {
            thresholds[i] = JsonLoaderBase::get_as_double(&thresholds_json[i]);
            values[i] = JsonLoaderBase::get_as_double(&values_json[i]);
        }

        DoubleRange::multiple(thresholds, values)
    }
}
