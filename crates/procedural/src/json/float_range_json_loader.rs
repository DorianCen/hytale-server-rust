// Ported from: `.hytale-source/.../procedurallib/json/FloatRangeJsonLoader.java`
//! Java `FloatRangeJsonLoader`.
//!
//! The Java return type `IFloatRange` is an object graph shared through the
//! world configuration, so the Rust `load()` returns an `Rc<dyn IFloatRange>`
//! (the same ownership the cave loaders use).

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::supplier::{FloatRange, IFloatRange};

/// Java `FloatRangeJsonLoader.FloatToFloatFunction`.
pub type FloatToFloatFunction = fn(f32) -> f32;

/// Java `FloatRangeJsonLoader.Constants`.
mod constants {
    pub const KEY_MIN: &str = "Min";
    pub const KEY_MAX: &str = "Max";

    pub const ERROR_ARRAY_SIZE: &str =
        "Range array contains %s values. Only 1 or 2 entries are allowed.";
    pub const ERROR_NO_MIN: &str = "Minimum value of range is not defined. Keyword: Min";
    pub const ERROR_NO_MAX: &str = "Maximum value of range is not defined. Keyword: Max";
}

/// Java `FloatRangeJsonLoader`.
pub struct FloatRangeJsonLoader {
    /// Java `extends JsonLoader<K, IFloatRange>` — note the Java quirk: the
    /// seed suffix is `.DoubleRange`.
    pub base: JsonLoaderBase,
    /// Java `protected final float default1`.
    pub default1: f32,
    /// Java `protected final float default2`.
    pub default2: f32,
    /// Java `@Nonnull protected final FloatToFloatFunction function`.
    pub function: FloatToFloatFunction,
}

impl FloatRangeJsonLoader {
    /// Java `FloatRangeJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self::with_defaults(seed, data_folder, json, 0.0, 0.0, |d| d)
    }

    /// Java `FloatRangeJsonLoader(seed, dataFolder, json, FloatToFloatFunction function)`.
    pub fn with_function(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        function: FloatToFloatFunction,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, 0.0, 0.0, function)
    }

    /// Java `FloatRangeJsonLoader(seed, dataFolder, json, float default1)`.
    pub fn with_default1(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f32,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, default1, default1, |d| d)
    }

    /// Java `FloatRangeJsonLoader(seed, dataFolder, json, float default1,
    /// FloatToFloatFunction function)`.
    pub fn with_default1_and_function(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f32,
        function: FloatToFloatFunction,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, default1, default1, function)
    }

    /// Java `FloatRangeJsonLoader(seed, dataFolder, json, float default1, float default2)`.
    pub fn with_default1_and_default2(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f32,
        default2: f32,
    ) -> Self {
        Self::with_defaults(seed, data_folder, json, default1, default2, |d| d)
    }

    /// Java `FloatRangeJsonLoader(seed, dataFolder, json, float default1,
    /// float default2, FloatToFloatFunction function)`.
    pub fn with_defaults(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        default1: f32,
        default2: f32,
        function: FloatToFloatFunction,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".DoubleRange"), data_folder, json),
            default1,
            default2,
            function,
        }
    }

    /// Java `IFloatRange load()`.
    pub fn load(&self) -> Rc<dyn IFloatRange> {
        // Java: `json == null || json.isJsonNull()`.
        if matches!(&self.base.json, None | Some(Value::Null)) {
            if self.default1 == self.default2 {
                return Rc::new(FloatRange::constant((self.function)(self.default1)));
            } else {
                return Rc::new(FloatRange::normal(
                    (self.function)(self.default1),
                    (self.function)(self.default2),
                ));
            }
        }

        let json = self.base.json.as_ref().unwrap();

        if json.is_array() {
            let array = match json.as_array() {
                Some(array) => array,
                None => unreachable!(),
            };
            if array.len() != 1 && array.len() != 2 {
                // Java: IllegalStateException(String.format(ERROR_ARRAY_SIZE, size)).
                panic!(
                    "{}",
                    constants::ERROR_ARRAY_SIZE.replace("%s", &array.len().to_string())
                );
            }
            if array.len() == 1 {
                Rc::new(FloatRange::constant((self.function)(Self::get_as_float(
                    &array[0],
                ))))
            } else {
                Rc::new(FloatRange::normal(
                    (self.function)(Self::get_as_float(&array[0])),
                    (self.function)(Self::get_as_float(&array[1])),
                ))
            }
        } else if json.is_object() {
            if !self.base.has(constants::KEY_MIN) {
                panic!("{}", constants::ERROR_NO_MIN);
            }
            if !self.base.has(constants::KEY_MAX) {
                panic!("{}", constants::ERROR_NO_MAX);
            }
            let min = Self::get_as_float(&self.base.get(constants::KEY_MIN).unwrap());
            let max = Self::get_as_float(&self.base.get(constants::KEY_MAX).unwrap());
            Rc::new(FloatRange::normal(
                (self.function)(min),
                (self.function)(max),
            ))
        } else {
            Rc::new(FloatRange::constant((self.function)(Self::get_as_float(
                json,
            ))))
        }
    }

    /// Java `JsonElement.getAsFloat()` == `(float) getAsDouble()`.
    fn get_as_float(element: &Value) -> f32 {
        JsonLoaderBase::get_as_double(element) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> FloatRangeJsonLoader {
        FloatRangeJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn array_forms_resolve() {
        let range = loader(serde_json::json!([2.5])).load();
        assert_eq!(range.get_value(123.0), 2.5);

        let range = loader(serde_json::json!([2.5, 4.5])).load();
        assert_eq!(range.get_value(0.0), 2.5);
        assert_eq!(range.get_value(1.0), 4.5);

        // f32 precision: the value is parsed through (float) getAsDouble().
        let range = loader(serde_json::json!([0.1, 0.3])).load();
        assert_eq!(range.get_value(0.0), 0.1f32);
        assert_eq!(range.get_value(1.0), 0.3f32);
    }

    #[test]
    fn object_and_constant_forms_resolve() {
        let range = loader(serde_json::json!({ "Min": 1.5, "Max": 3.5 })).load();
        assert_eq!(range.get_value(0.0), 1.5);
        assert_eq!(range.get_value(1.0), 3.5);

        let range = loader(serde_json::json!(2.25)).load();
        assert_eq!(range.get_value(-1.0), 2.25);
    }

    #[test]
    fn null_json_uses_the_defaults() {
        let equal_defaults =
            FloatRangeJsonLoader::new(SeedString::new("test"), std::path::Path::new("."), None);
        assert_eq!(equal_defaults.load().get_value(0.5), 0.0);

        let split_defaults = FloatRangeJsonLoader::with_default1_and_default2(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(Value::Null),
            1.0,
            2.0,
        );
        let range = split_defaults.load();
        assert_eq!(range.get_value(0.0), 1.0);
        assert_eq!(range.get_value(1.0), 2.0);
    }

    #[test]
    fn function_is_applied() {
        let l = FloatRangeJsonLoader::with_function(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(serde_json::json!([3.0])),
            |d| d * 2.0,
        );
        assert_eq!(l.load().get_value(0.0), 6.0);
    }

    #[test]
    #[should_panic(expected = "Range array contains 3 values. Only 1 or 2 entries are allowed.")]
    fn bad_array_size_panics_verbatim() {
        loader(serde_json::json!([1.0, 2.0, 3.0])).load();
    }

    #[test]
    #[should_panic(expected = "Minimum value of range is not defined. Keyword: Min")]
    fn missing_min_panics_verbatim() {
        loader(serde_json::json!({ "Max": 1.0 })).load();
    }

    #[test]
    #[should_panic(expected = "Maximum value of range is not defined. Keyword: Max")]
    fn missing_max_panics_verbatim() {
        loader(serde_json::json!({ "Min": 1.0 })).load();
    }
}
