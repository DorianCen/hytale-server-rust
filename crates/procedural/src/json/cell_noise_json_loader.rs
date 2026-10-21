// Ported from: `.hytale-source/.../procedurallib/json/CellNoiseJsonLoader.java`
//! Java `CellNoiseJsonLoader`.
//!
//! ⚠ NOTE: Java subclasses `CellNoise` (`LoadedCellNoise`) only to wire the
//! `SeedResource`'s thread-local scratch buffers into `localBuffer2d/3d`; the
//! Rust port allocates per-call buffers, so the plain [`CellNoise`] is the
//! faithful equivalent. The `@Nullable` results of `loadCellDistanceFunction`
//! / `loadPointEvaluator` are never actually `null` (the sub-loaders always
//! return a value), so the port surfaces them non-optional.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::cell_distance_function_json_loader::CellDistanceFunctionJsonLoader;
use crate::json::noise_property_json_loader::NoisePropertyJsonLoader;
use crate::json::point_evaluator_json_loader::PointEvaluatorJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell_noise::{CellMode, CellNoise};
use crate::noise::support::NoiseProperty;

/// `Rc<dyn NoiseFunction>` — the Java `NoiseFunction` loader result type
/// (re-exported from [`crate::json::noise_function_json_loader`]).
pub type NoiseFunctionObj = crate::json::noise_function_json_loader::NoiseFunctionObj;

/// Java `CellNoiseJsonLoader.Constants` — shared keywords/defaults for the
/// whole cell-family (Java static-imports them).
pub mod constants {
    use crate::noise::cell::cell_type::CellType;
    use crate::noise::cell::distance_calculation_mode::DistanceCalculationMode;
    use crate::noise::cell_noise::CellMode;

    pub const KEY_JITTER: &str = "Jitter";
    pub const KEY_JITTER_X: &str = "JitterX";
    pub const KEY_JITTER_Y: &str = "JitterY";
    pub const KEY_JITTER_Z: &str = "JitterZ";

    pub const KEY_DENSITY: &str = "Density";
    pub const KEY_CELL_MODE: &str = "CellMode";
    pub const KEY_NOISE_LOOKUP: &str = "NoiseLookup";
    pub const KEY_DISTANCE_MODE: &str = "DistanceMode";
    pub const KEY_DISTANCE_RANGE: &str = "DistanceRange";
    pub const KEY_CELL_TYPE: &str = "CellType";

    pub const KEY_SKIP_CELLS: &str = "Skip";
    pub const KEY_SKIP_MODE: &str = "SkipMode";

    pub const DEFAULT_JITTER: f64 = 1.0;
    pub const DEFAULT_DISTANCE_RANGE: f64 = 1.0;
    pub const DEFAULT_DENSITY_LOWER: f64 = 0.0;
    pub const DEFAULT_DENSITY_UPPER: f64 = 1.0;
    pub const DEFAULT_DISTANCE_MODE: DistanceCalculationMode = DistanceCalculationMode::Euclidean;
    pub const DEFAULT_CELL_MODE: CellMode = CellMode::CellValue;
    pub const DEFAULT_CELL_TYPE: CellType = CellType::Square;
}

/// Java `CellNoiseJsonLoader`.
pub struct CellNoiseJsonLoader {
    /// Java `extends JsonLoader<K, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl CellNoiseJsonLoader {
    /// Java `CellNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CellNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> NoiseFunctionObj {
        let cell_distance_function = self.load_cell_distance_function();
        let point_evaluator = self.load_point_evaluator();
        let cell_function = self.load_cell_function();
        let noise_lookup = self.load_noise_lookup();
        Rc::new(CellNoise::new(
            cell_distance_function,
            point_evaluator,
            Rc::new(cell_function),
            noise_lookup,
        ))
    }

    /// Java `@Nullable CellDistanceFunction loadCellDistanceFunction()` —
    /// default `CENTRE_DISTANCE` measurement mode and no override point
    /// distance function (the value is never `null` in practice).
    pub fn load_cell_distance_function(&self) -> Rc<dyn CellDistanceFunction> {
        CellDistanceFunctionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
            None,
        )
        .load()
    }

    /// Java `@Nullable PointEvaluator loadPointEvaluator()` (never `null` in
    /// practice).
    pub fn load_point_evaluator(&self) -> Rc<dyn PointEvaluator> {
        PointEvaluatorJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
        )
        .load()
    }

    /// Java `CellNoise.CellFunction loadCellFunction()` — the returned
    /// [`CellMode`] implements [`CellFunction`].
    pub fn load_cell_function(&self) -> CellMode {
        let mut cell_mode = constants::DEFAULT_CELL_MODE;
        if self.base.has(constants::KEY_CELL_MODE) {
            cell_mode = Self::cell_mode_value_of(&JsonLoaderBase::get_as_string(
                &self.base.get(constants::KEY_CELL_MODE).unwrap(),
            ));
        }
        cell_mode
    }

    /// Java `@Nullable NoiseProperty loadNoiseLookup()`.
    pub fn load_noise_lookup(&self) -> Option<Rc<dyn NoiseProperty>> {
        if self.base.has(constants::KEY_NOISE_LOOKUP) {
            Some(
                NoisePropertyJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_NOISE_LOOKUP),
                )
                .load(),
            )
        } else {
            None
        }
    }

    /// Java `CellNoise.CellMode.valueOf(String)`.
    pub fn cell_mode_value_of(name: &str) -> CellMode {
        match name {
            "CELL_VALUE" => CellMode::CellValue,
            "NOISE_LOOKUP" => CellMode::NoiseLookup,
            "DISTANCE" => CellMode::Distance,
            "DIRECTION" => CellMode::Direction,
            other => panic!("No enum constant CellNoise.CellMode.{other}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::function::NoiseFunction2d;

    fn loader(json: Value) -> CellNoiseJsonLoader {
        CellNoiseJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn defaults_are_applied() {
        let l = loader(serde_json::json!({}));
        assert_eq!(l.load_cell_function(), CellMode::CellValue);
        assert!(l.load_noise_lookup().is_none());
        // Smoke: the loaded noise samples deterministically.
        let noise = loader(serde_json::json!({})).load();
        let a = NoiseFunction2d::get(noise.as_ref(), 7, 3, 1.5, -2.25);
        let b = NoiseFunction2d::get(noise.as_ref(), 7, 3, 1.5, -2.25);
        assert_eq!(a.to_bits(), b.to_bits());
        assert!(a.is_finite());
    }

    #[test]
    fn cell_mode_is_parsed() {
        let l = loader(serde_json::json!({ "CellMode": "DISTANCE" }));
        assert_eq!(l.load_cell_function(), CellMode::Distance);
        // The enum parse uses the exact Java names.
        assert_eq!(
            CellNoiseJsonLoader::cell_mode_value_of("NOISE_LOOKUP"),
            CellMode::NoiseLookup
        );
    }

    #[test]
    #[should_panic(expected = "No enum constant CellNoise.CellMode.CELLVALUE")]
    fn bad_cell_mode_panics_verbatim() {
        CellNoiseJsonLoader::cell_mode_value_of("CELLVALUE");
    }

    #[test]
    fn noise_lookup_is_loaded() {
        // A constant noise lookup: NoiseType CONSTANT (Value default 0.5).
        let l = loader(serde_json::json!({
            "NoiseLookup": { "NoiseType": "CONSTANT", "Value": 0.25 }
        }));
        let lookup = l.load_noise_lookup().expect("lookup expected");
        // ConstantNoise(0.25) remapped by SingleNoiseProperty: 0.25*0.5+0.5.
        assert_eq!(lookup.get(1, 0.0, 0.0).to_bits(), 0.625f64.to_bits());
    }
}
