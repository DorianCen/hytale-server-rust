// Ported from: `.hytale-source/.../procedurallib/json/FloatRangeJsonLoader.java`
//! The adapter between the `procedural` crate's float-range loader and the
//! `worldgen-hytale` float-range supplier.
//!
//! The `procedural` crate ports the *loader* (the JSON forms, the defaults
//! and the error messages) but not the `procedurallib.supplier.FloatRange`
//! result type — that port lives in [`crate::supplier`] (the float range is
//! used by the yaw and the pitch of a cave type).
//!
//! This module bridges the two: the `procedural` loader resolves the JSON
//! into its [`ParsedFloatRange`] and the adapter turns it into the
//! `Rc<dyn IFloatRange>` the cave loaders need.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::supplier::{FloatRange, IFloatRange};
use procedural::json::float_range_json_loader::{
    FloatRangeJsonLoader, FloatToFloatFunction, ParsedFloatRange,
};
use procedural::json::SeedString;

/// Java `new FloatRangeJsonLoader<>(seed, dataFolder, json, default1,
/// default2, function).load()`.
///
/// The seed suffix is the Java quirk of the loader (`.DoubleRange`).
pub fn load_float_range(
    seed: SeedString,
    data_folder: &Path,
    json: Option<Value>,
    default1: f32,
    default2: f32,
    function: FloatToFloatFunction,
) -> Rc<dyn IFloatRange> {
    let loader =
        FloatRangeJsonLoader::with_defaults(seed, data_folder, json, default1, default2, function);
    match loader.load_parsed() {
        ParsedFloatRange::Constant(result) => Rc::new(FloatRange::constant(result)),
        ParsedFloatRange::Normal(min, max) => Rc::new(FloatRange::normal(min, max)),
    }
}

/// Java `new FloatRangeJsonLoader<>(seed, dataFolder, json, default1,
/// function).load()` — the constructor that uses `default1` for both
/// defaults.
pub fn load_float_range_with_default(
    seed: SeedString,
    data_folder: &Path,
    json: Option<Value>,
    default1: f32,
    function: FloatToFloatFunction,
) -> Rc<dyn IFloatRange> {
    load_float_range(seed, data_folder, json, default1, default1, function)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_range_uses_the_defaults() {
        let range = load_float_range(
            SeedString::new("0"),
            Path::new("."),
            None,
            -180.0,
            180.0,
            |d| d,
        );
        assert_eq!(range.get_value(0.0), -180.0);
        // The normal range interpolates between the two defaults.
        let range = load_float_range(
            SeedString::new("0"),
            Path::new("."),
            None,
            -180.0,
            180.0,
            |d| d,
        );
        assert_eq!(range.get_value(0.5), 0.0);
    }

    #[test]
    fn the_equal_defaults_produce_a_constant() {
        let range = load_float_range(
            SeedString::new("0"),
            Path::new("."),
            None,
            -15.0,
            -15.0,
            |d| d,
        );
        assert_eq!(range.get_value(0.5), -15.0);
    }

    #[test]
    fn the_array_form_is_resolved() {
        let range = load_float_range(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!([0.0, 360.0])),
            0.0,
            0.0,
            |d| d,
        );
        assert_eq!(range.get_value(0.25), 90.0);
    }

    #[test]
    fn the_function_is_applied_to_the_values() {
        let range = load_float_range(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!([180.0])),
            0.0,
            0.0,
            |d| d * std::f32::consts::PI / 180.0,
        );
        assert!((range.get_value(0.0) - std::f32::consts::PI).abs() < 1e-6);
    }
}
