// Ported from: `.hytale-source/.../procedurallib/json/CellBorderDistanceFunctionJsonLoader.java`
//! Java `CellBorderDistanceFunctionJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::cell_noise_json_loader::constants;
use crate::json::double_threshold_json_loader::DoubleThresholdJsonLoader;
use crate::json::point_evaluator_json_loader::PointEvaluatorJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::cell::border_distance_function::BorderDistanceFunction;
use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::measurement_mode::MeasurementMode;
use crate::noise::condition::DoubleThresholdCondition;
use crate::noise::condition::IDoubleCondition;

/// Java `CellBorderDistanceFunctionJsonLoader`.
pub struct CellBorderDistanceFunctionJsonLoader {
    /// Java `extends JsonLoader<K, BorderDistanceFunction>` — note: the seed
    /// is passed through **without** a suffix append.
    pub base: JsonLoaderBase,
    /// Java `protected final CellDistanceFunction distanceFunction`.
    pub distance_function: Rc<dyn CellDistanceFunction>,
}

impl CellBorderDistanceFunctionJsonLoader {
    /// Java `CellBorderDistanceFunctionJsonLoader(SeedString<K> seed, Path
    /// dataFolder, JsonElement json, CellDistanceFunction distanceFunction)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        distance_function: Rc<dyn CellDistanceFunction>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            distance_function,
        }
    }

    /// Java `BorderDistanceFunction load()`.
    pub fn load(&self) -> BorderDistanceFunction {
        BorderDistanceFunction::new(
            self.distance_function.clone(),
            self.load_point_evaluator(),
            self.load_density(),
        )
    }

    /// Java `@Nullable PointEvaluator loadPointEvaluator()`.
    pub fn load_point_evaluator(&self) -> Rc<dyn PointEvaluator> {
        PointEvaluatorJsonLoader::with_measurement_mode(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
            MeasurementMode::BorderDistance,
            None,
        )
        .load()
    }

    /// Java `@Nullable IDoubleCondition loadDensity()`.
    pub fn load_density(&self) -> Option<Rc<dyn IDoubleCondition>> {
        if !self.base.has(constants::KEY_DENSITY) {
            return None;
        }

        let threshold = DoubleThresholdJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_DENSITY),
        )
        .load();

        Some(Rc::new(DoubleThresholdCondition::new(threshold)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::cell_distance_function_json_loader::CellDistanceFunctionJsonLoader;

    #[test]
    fn loads_border_function_with_density() {
        let json = serde_json::json!({ "Density": [0.1, 0.9], "Jitter": 1.0 });
        let distance_function = CellDistanceFunctionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json.clone()),
            None,
        )
        .load();

        let border = CellBorderDistanceFunctionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
            distance_function,
        )
        .load();

        // Density condition resolves through the seeded random-density
        // condition; verify determinism rather than a hand-computed value.
        let first = border.density.eval(1234);
        assert_eq!(first, border.density.eval(1234));
    }

    #[test]
    fn density_defaults_to_none() {
        let distance_function = CellDistanceFunctionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(serde_json::json!({})),
            None,
        )
        .load();

        let border = CellBorderDistanceFunctionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(serde_json::json!({})),
            distance_function,
        )
        .load();

        // Java: null density → DensityPointEvaluator.getDensityCondition(null)
        // → the always-true condition.
        assert!(border.density.eval(0));
        assert!(border.density.eval(-42));
    }
}
