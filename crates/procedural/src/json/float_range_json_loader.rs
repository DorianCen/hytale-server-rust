// Ported from: `.hytale-source/.../procedurallib/json/FloatRangeJsonLoader.java`
//! Java `FloatRangeJsonLoader`.
//!
//! ⚠ SEAM: the result type `procedurallib.supplier.FloatRange` /
//! `IFloatRange` has **no Rust port yet** (the `hytale_math::range::FloatRange`
//! is a different Java class — `com.hypixel.hytale.math.range.FloatRange`).
//! `load()` therefore panics with a documented seam message; the loader
//! surface (constructor defaults, the `.DoubleRange` seed-suffix quirk, the
//! error strings and the `Min`/`Max` keyword forms) is otherwise transcribed
//! 1:1, and the numeric parsing is exercised through [`Self::load_constant`]
//! / [`Self::load_min_max`] so a future `supplier::FloatRange` port can be
//! wired in without re-deriving the logic.

use std::path::Path;

use serde_json::Value;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;

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

/// The parsed range forms of Java `FloatRange.Constant` / `.Normal` — the
/// `supplier.FloatRange` port does not exist yet, so the loader exposes its
/// resolution as these plain values (see the module-level SEAM note).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParsedFloatRange {
    /// Java `FloatRange.Constant(float result)`.
    Constant(f32),
    /// Java `FloatRange.Normal(float min, float max)`.
    Normal(f32, f32),
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
    ///
    /// ⚠ SEAM: `procedurallib.supplier.FloatRange` / `IFloatRange` are not
    /// ported yet — this panics with a documented message instead of
    /// returning an invented type.
    pub fn load(&self) -> ! {
        panic!(
            "FloatRangeJsonLoader: `procedurallib.supplier.FloatRange` / `IFloatRange` are not ported yet (documented seam); use load_constant/load_min_max for the parsed forms"
        );
    }

    /// The `load()` resolution logic, up to the (not yet ported) supplier
    /// construction. Deterministic and seam-free.
    pub fn load_parsed(&self) -> ParsedFloatRange {
        // Java: `json == null || json.isJsonNull()`.
        if matches!(&self.base.json, None | Some(Value::Null)) {
            if self.default1 == self.default2 {
                return ParsedFloatRange::Constant((self.function)(self.default1));
            } else {
                return ParsedFloatRange::Normal(
                    (self.function)(self.default1),
                    (self.function)(self.default2),
                );
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
                ParsedFloatRange::Constant((self.function)(Self::get_as_float(&array[0])))
            } else {
                ParsedFloatRange::Normal(
                    (self.function)(Self::get_as_float(&array[0])),
                    (self.function)(Self::get_as_float(&array[1])),
                )
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
            ParsedFloatRange::Normal((self.function)(min), (self.function)(max))
        } else {
            ParsedFloatRange::Constant((self.function)(Self::get_as_float(json)))
        }
    }

    /// The `Constant` form of `load()` — the seam-free half of the Java
    /// return value.
    pub fn load_constant(&self) -> f32 {
        match self.load_parsed() {
            ParsedFloatRange::Constant(result) => result,
            ParsedFloatRange::Normal(min, max) => {
                panic!("FloatRange is not a Constant: Normal({min}, {max})")
            }
        }
    }

    /// The `Normal` form of `load()` — the seam-free half of the Java return
    /// value.
    pub fn load_min_max(&self) -> (f32, f32) {
        match self.load_parsed() {
            ParsedFloatRange::Normal(min, max) => (min, max),
            ParsedFloatRange::Constant(result) => {
                panic!("FloatRange is not a Normal: Constant({result})")
            }
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
        assert_eq!(
            loader(serde_json::json!([2.5])).load_parsed(),
            ParsedFloatRange::Constant(2.5)
        );
        assert_eq!(
            loader(serde_json::json!([2.5, 4.5])).load_parsed(),
            ParsedFloatRange::Normal(2.5, 4.5)
        );
        // f32 precision: the value is parsed through (float) getAsDouble().
        let (min, max) = loader(serde_json::json!([0.1, 0.3])).load_min_max();
        assert_eq!(min, 0.1f32);
        assert_eq!(max, 0.3f32);
    }

    #[test]
    fn object_and_constant_forms_resolve() {
        let (min, max) = loader(serde_json::json!({ "Min": 1.5, "Max": 3.5 })).load_min_max();
        assert_eq!((min, max), (1.5, 3.5));

        assert_eq!(loader(serde_json::json!(2.25)).load_constant(), 2.25);
    }

    #[test]
    fn null_json_uses_the_defaults() {
        let equal_defaults =
            FloatRangeJsonLoader::new(SeedString::new("test"), std::path::Path::new("."), None);
        assert_eq!(
            equal_defaults.load_parsed(),
            ParsedFloatRange::Constant(0.0)
        );

        let split_defaults = FloatRangeJsonLoader::with_default1_and_default2(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(Value::Null),
            1.0,
            2.0,
        );
        assert_eq!(
            split_defaults.load_parsed(),
            ParsedFloatRange::Normal(1.0, 2.0)
        );
    }

    #[test]
    fn function_is_applied() {
        let l = FloatRangeJsonLoader::with_function(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(serde_json::json!([3.0])),
            |d| d * 2.0,
        );
        assert_eq!(l.load_constant(), 6.0);
    }

    #[test]
    #[should_panic(expected = "Range array contains 3 values. Only 1 or 2 entries are allowed.")]
    fn bad_array_size_panics_verbatim() {
        loader(serde_json::json!([1.0, 2.0, 3.0])).load_parsed();
    }

    #[test]
    #[should_panic(expected = "Minimum value of range is not defined. Keyword: Min")]
    fn missing_min_panics_verbatim() {
        loader(serde_json::json!({ "Max": 1.0 })).load_parsed();
    }

    #[test]
    #[should_panic(expected = "Maximum value of range is not defined. Keyword: Max")]
    fn missing_max_panics_verbatim() {
        loader(serde_json::json!({ "Min": 1.0 })).load_parsed();
    }

    #[test]
    #[should_panic(expected = "not ported yet")]
    fn load_is_a_documented_seam() {
        loader(serde_json::json!([1.0])).load();
    }
}
