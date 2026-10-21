// Ported from: `.hytale-source/.../procedurallib/json/DistanceNoiseJsonLoader.java`
//! Java `DistanceNoiseJsonLoader`.
//!
//! ⚠ NOTE: Java subclasses `DistanceNoise` (`LoadedDistanceNoise`) only to
//! wire the `SeedResource`'s thread-local scratch buffers into
//! `localBuffer2d/3d`; the Rust port allocates per-call buffers, so the plain
//! [`DistanceNoise`] is the faithful equivalent.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::cell_distance_function_json_loader::CellDistanceFunctionJsonLoader;
use crate::json::point_evaluator_json_loader::PointEvaluatorJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::measurement_mode::MeasurementMode;
use crate::noise::distance_noise::{Distance2Function, Distance2Mode, DistanceNoise};
use crate::noise::function::NoiseFunction;

/// Java `DistanceNoiseJsonLoader.Constants`.
mod constants {
    use crate::noise::cell::measurement_mode::MeasurementMode;
    use crate::noise::distance_noise::Distance2Mode;

    pub const KEY_MEASUREMENT: &str = "Measurement";
    pub const KEY_DISTANCE_2_MODE: &str = "Distance2Mode";
    pub const DEFAULT_MEASUREMENT: MeasurementMode = MeasurementMode::CentreDistance;
    pub const DEFAULT_DISTANCE_2_MODE: Distance2Mode = Distance2Mode::Sub;
}

/// Java `DistanceNoiseJsonLoader`.
pub struct DistanceNoiseJsonLoader {
    /// Java `extends JsonLoader<K, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl DistanceNoiseJsonLoader {
    /// Java `DistanceNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".DistanceNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> Rc<dyn NoiseFunction> {
        let cell_distance_function = self.load_cell_distance_function();
        let point_evaluator = self.load_point_evaluator();
        let distance2_function = self.load_distance2_function();
        Rc::new(DistanceNoise::new(
            cell_distance_function,
            point_evaluator,
            distance2_function,
        ))
    }

    /// Java `@Nullable CellDistanceFunction loadCellDistanceFunction()`
    /// (never `null` in practice).
    pub fn load_cell_distance_function(&self) -> Rc<dyn CellDistanceFunction> {
        let measurement_mode = self.load_measurement_mode();
        CellDistanceFunctionJsonLoader::with_measurement_mode(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
            measurement_mode,
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

    /// Java `@Nonnull MeasurementMode loadMeasurementMode()`.
    pub fn load_measurement_mode(&self) -> MeasurementMode {
        if self.base.has(constants::KEY_MEASUREMENT) {
            return Self::measurement_mode_value_of(&JsonLoaderBase::get_as_string(
                &self.base.get(constants::KEY_MEASUREMENT).unwrap(),
            ));
        }
        constants::DEFAULT_MEASUREMENT
    }

    /// Java `DistanceNoise.Distance2Function loadDistance2Function()`.
    pub fn load_distance2_function(&self) -> Rc<dyn Distance2Function> {
        let mut distance2_mode = constants::DEFAULT_DISTANCE_2_MODE;
        if self.base.has(constants::KEY_DISTANCE_2_MODE) {
            distance2_mode = Self::distance2_mode_value_of(&JsonLoaderBase::get_as_string(
                &self.base.get(constants::KEY_DISTANCE_2_MODE).unwrap(),
            ));
        }
        Rc::new(distance2_mode)
    }

    /// Java `MeasurementMode.valueOf(String)`.
    pub fn measurement_mode_value_of(name: &str) -> MeasurementMode {
        match name {
            "CENTRE_DISTANCE" => MeasurementMode::CentreDistance,
            "BORDER_DISTANCE" => MeasurementMode::BorderDistance,
            other => panic!("No enum constant MeasurementMode.{other}"),
        }
    }

    /// Java `DistanceNoise.Distance2Mode.valueOf(String)`.
    pub fn distance2_mode_value_of(name: &str) -> Distance2Mode {
        match name {
            "ADD" => Distance2Mode::Add,
            "SUB" => Distance2Mode::Sub,
            "MUL" => Distance2Mode::Mul,
            "DIV" => Distance2Mode::Div,
            "MIN" => Distance2Mode::Min,
            "MAX" => Distance2Mode::Max,
            other => panic!("No enum constant DistanceNoise.Distance2Mode.{other}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::function::NoiseFunction2d;

    fn loader(json: Value) -> DistanceNoiseJsonLoader {
        DistanceNoiseJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn defaults_resolve_to_centre_sub() {
        let l = loader(serde_json::json!({}));
        assert_eq!(l.load_measurement_mode(), MeasurementMode::CentreDistance);
        // SUB: distance2 - distance.
        assert_eq!(
            DistanceNoiseJsonLoader::distance2_mode_value_of("SUB").eval(1.0, 3.0),
            2.0
        );
        let noise = l.load();
        let a = NoiseFunction2d::get(noise.as_ref(), 7, 3, 1.5, -2.25);
        assert_eq!(
            a.to_bits(),
            NoiseFunction2d::get(noise.as_ref(), 7, 3, 1.5, -2.25).to_bits()
        );
        assert!(a.is_finite());
    }

    #[test]
    fn measurement_and_distance2_mode_parse() {
        let l = loader(serde_json::json!({
            "Measurement": "BORDER_DISTANCE",
            "Distance2Mode": "DIV"
        }));
        assert_eq!(l.load_measurement_mode(), MeasurementMode::BorderDistance);
        let function = l.load_distance2_function();
        assert_eq!(function.eval(1.0, 4.0), 0.25); // distance / distance2
    }

    #[test]
    #[should_panic(expected = "No enum constant MeasurementMode.PERIMETER")]
    fn bad_measurement_panics_verbatim() {
        DistanceNoiseJsonLoader::measurement_mode_value_of("PERIMETER");
    }

    #[test]
    #[should_panic(expected = "No enum constant DistanceNoise.Distance2Mode.POW")]
    fn bad_distance2_mode_panics_verbatim() {
        DistanceNoiseJsonLoader::distance2_mode_value_of("POW");
    }
}
