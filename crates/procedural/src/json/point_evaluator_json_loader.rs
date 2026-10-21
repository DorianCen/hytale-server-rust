// Ported from: `.hytale-source/.../procedurallib/json/PointEvaluatorJsonLoader.java`
//! Java `PointEvaluatorJsonLoader`.
//!
//! ⚠ NOTE: the Java `PointEvaluator.of(...)` static composition helpers (used
//! by `loadCentrePointEvaluator` and `BranchNoiseJsonLoader`) are ported as
//! the free functions [`point_evaluator_of`] / [`point_evaluator_of_skippable`]
//! below — the composition cannot live on the trait itself without breaking
//! the 1:1 file mapping. The `density != DefaultDoubleCondition.DEFAULT_TRUE`
//! identity check collapses to `is_some` because the loaders only ever build
//! `DoubleThresholdCondition`s here.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::abstract_cell_jitter_json_loader::AbstractCellJitterJsonLoader;
use crate::json::cell_noise_json_loader::constants;
use crate::json::double_range_json_loader::DoubleRangeJsonLoader;
use crate::json::double_threshold_json_loader::DoubleThresholdJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::cell::distance_calculation_mode::DistanceCalculationMode;
use crate::noise::cell::evaluator::border_point_evaluator::BorderPointEvaluator;
use crate::noise::cell::evaluator::density_point_evaluator::DensityPointEvaluator;
use crate::noise::cell::evaluator::distance_point_evaluator::DistancePointEvaluator;
use crate::noise::cell::evaluator::jitter_point_evaluator::JitterPointEvaluator;
use crate::noise::cell::evaluator::normal_point_evaluator::NormalPointEvaluator;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::evaluator::skip_cell_point_evaluator::{
    SkipCellMode, SkipCellPointEvaluator, DEFAULT_MODE, DEFAULT_NO_SKIP,
};
use crate::noise::cell::jitter::CellJitter;
use crate::noise::cell::measurement_mode::MeasurementMode;
use crate::noise::cell::point_distance_function::PointDistanceFunction;
use crate::noise::condition::DoubleThresholdCondition;
use crate::noise::support::{IDoubleCondition, IDoubleRange};

/// Java `PointEvaluatorJsonLoader`.
pub struct PointEvaluatorJsonLoader {
    /// Java `extends AbstractCellJitterJsonLoader<T, PointEvaluator>`.
    pub base: JsonLoaderBase,
    /// Java `@Nonnull protected final MeasurementMode measurementMode`.
    pub measurement_mode: MeasurementMode,
    /// Java `protected final PointDistanceFunction pointDistanceFunction`.
    pub point_distance_function: Option<Rc<dyn PointDistanceFunction>>,
}

impl AbstractCellJitterJsonLoader for PointEvaluatorJsonLoader {
    fn base(&self) -> &JsonLoaderBase {
        &self.base
    }
}

impl PointEvaluatorJsonLoader {
    /// Java `PointEvaluatorJsonLoader(SeedString<T> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self::with_point_distance_function(seed, data_folder, json, None)
    }

    /// Java `PointEvaluatorJsonLoader(seed, dataFolder, json, @Nullable
    /// PointDistanceFunction)`.
    pub fn with_point_distance_function(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        point_distance_function: Option<Rc<dyn PointDistanceFunction>>,
    ) -> Self {
        Self::with_measurement_mode(
            seed,
            data_folder,
            json,
            MeasurementMode::CentreDistance,
            point_distance_function,
        )
    }

    /// Java `PointEvaluatorJsonLoader(seed, dataFolder, json,
    /// @Nonnull MeasurementMode, @Nullable PointDistanceFunction)`.
    pub fn with_measurement_mode(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        measurement_mode: MeasurementMode,
        point_distance_function: Option<Rc<dyn PointDistanceFunction>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".PointEvaluator"), data_folder, json),
            measurement_mode,
            point_distance_function,
        }
    }

    /// Java `PointEvaluator load()`.
    pub fn load(&self) -> Rc<dyn PointEvaluator> {
        match self.measurement_mode {
            MeasurementMode::CentreDistance => self.load_centre_point_evaluator(),
            MeasurementMode::BorderDistance => self.load_border_point_evaluator(),
        }
    }

    /// Java `public PointEvaluator loadCentrePointEvaluator()`.
    pub fn load_centre_point_evaluator(&self) -> Rc<dyn PointEvaluator> {
        point_evaluator_of_skippable(
            self.load_point_distance_function(),
            self.load_density(),
            self.load_distance_range(),
            self.load_skip_count(),
            self.load_skip_mode(),
            self.load_jitter(),
        )
    }

    /// Java `@Nonnull public PointEvaluator loadBorderPointEvaluator()`.
    pub fn load_border_point_evaluator(&self) -> Rc<dyn PointEvaluator> {
        let point_evaluator = BorderPointEvaluator::instance();
        let jitter = self.load_jitter();
        if jitter == CellJitter::DEFAULT_ONE {
            return point_evaluator;
        }

        Rc::new(JitterPointEvaluator::new(point_evaluator, jitter))
    }

    /// Java `public PointDistanceFunction loadPointDistanceFunction()`.
    pub fn load_point_distance_function(&self) -> Rc<dyn PointDistanceFunction> {
        if let Some(point_distance_function) = &self.point_distance_function {
            return point_distance_function.clone();
        }

        let mut distance_calculation_mode = constants::DEFAULT_DISTANCE_MODE;
        if self.base.has(constants::KEY_DISTANCE_MODE) {
            distance_calculation_mode =
                Self::distance_calculation_mode_value_of(&JsonLoaderBase::get_as_string(
                    &self.base.get(constants::KEY_DISTANCE_MODE).unwrap(),
                ));
        }

        Rc::new(distance_calculation_mode)
    }

    /// Java `@Nullable public IDoubleRange loadDistanceRange()`.
    pub fn load_distance_range(&self) -> Option<Rc<dyn IDoubleRange>> {
        if !self.base.has(constants::KEY_DISTANCE_RANGE) {
            return None;
        }

        Some(Rc::new(
            DoubleRangeJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_DISTANCE_RANGE),
            )
            .load(),
        ))
    }

    /// Java `@Nullable public IDoubleCondition loadDensity()`.
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

    /// Java `public int loadSkipCount()`.
    pub fn load_skip_count(&self) -> i32 {
        // Java: mustGetNumber(KEY_SKIP_CELLS, DEFAULT_NO_SKIP).intValue() — a
        // `(int)` cast of the double value.
        self.base
            .must_get_number(constants::KEY_SKIP_CELLS, Some(DEFAULT_NO_SKIP as f64)) as i32
    }

    /// Java `public SkipCellPointEvaluator.Mode loadSkipMode()`.
    pub fn load_skip_mode(&self) -> SkipCellMode {
        // Java: `SkipCellPointEvaluator.DEFAULT_MODE.name()` == "CHECKERBOARD".
        let name = self
            .base
            .must_get_string(constants::KEY_SKIP_MODE, Some("CHECKERBOARD"));
        Self::skip_cell_mode_value_of(&name)
    }

    /// Java `DistanceCalculationMode.valueOf(String)`.
    pub fn distance_calculation_mode_value_of(name: &str) -> DistanceCalculationMode {
        match name {
            "EUCLIDEAN" => DistanceCalculationMode::Euclidean,
            "MANHATTAN" => DistanceCalculationMode::Manhattan,
            "NATURAL" => DistanceCalculationMode::Natural,
            "MAX" => DistanceCalculationMode::Max,
            other => panic!("No enum constant DistanceCalculationMode.{other}"),
        }
    }

    /// Java `SkipCellPointEvaluator.Mode.valueOf(String)`.
    pub fn skip_cell_mode_value_of(name: &str) -> SkipCellMode {
        match name {
            "CHECKERBOARD" => SkipCellMode::Checkerboard,
            "GRID" => SkipCellMode::Grid,
            other => panic!("No enum constant SkipCellPointEvaluator.Mode.{other}"),
        }
    }
}

/// Java `PointEvaluator.of(PointDistanceFunction, @Nullable IDoubleCondition,
/// @Nullable IDoubleRange, CellJitter)` — the 4-arg overload (skip defaults).
pub fn point_evaluator_of(
    distance_function: Rc<dyn PointDistanceFunction>,
    density: Option<Rc<dyn IDoubleCondition>>,
    distance_mod: Option<Rc<dyn IDoubleRange>>,
    jitter: CellJitter,
) -> Rc<dyn PointEvaluator> {
    point_evaluator_of_skippable(
        distance_function,
        density,
        distance_mod,
        DEFAULT_NO_SKIP,
        DEFAULT_MODE,
        jitter,
    )
}

/// Java `PointEvaluator.of(PointDistanceFunction, @Nullable IDoubleCondition,
/// @Nullable IDoubleRange, int skipCount, @Nonnull Mode skipMode, CellJitter)`
/// — the exact Java composition order.
#[allow(clippy::too_many_arguments)]
pub fn point_evaluator_of_skippable(
    distance_function: Rc<dyn PointDistanceFunction>,
    density: Option<Rc<dyn IDoubleCondition>>,
    distance_mod: Option<Rc<dyn IDoubleRange>>,
    skip_count: i32,
    skip_mode: SkipCellMode,
    jitter: CellJitter,
) -> Rc<dyn PointEvaluator> {
    let mut point_evaluator: Rc<dyn PointEvaluator> =
        Rc::new(NormalPointEvaluator::new(distance_function.clone()));

    if let Some(distance_mod) = distance_mod {
        // Java: `new DistancePointEvaluator(distanceFunction, distanceMod)` —
        // the `IDoubleRange` overload wraps with `getDistanceModifier` first.
        point_evaluator = Rc::new(DistancePointEvaluator::with_double_range(
            distance_function.clone(),
            Some(distance_mod),
        ));
    }

    // Java: `density != null && density != DefaultDoubleCondition.DEFAULT_TRUE`
    // — an identity check against the DEFAULT_TRUE singleton; the loader-side
    // conditions are always `DoubleThresholdCondition`s, so `Some` suffices.
    // The Java ctor overload takes the `IDoubleCondition` and wraps it with
    // `getDensityCondition` first (`with_double_condition`).
    if let Some(density) = density {
        point_evaluator = Rc::new(DensityPointEvaluator::with_double_condition(
            point_evaluator,
            Some(density),
        ));
    }

    if skip_count > DEFAULT_NO_SKIP {
        point_evaluator = Rc::new(SkipCellPointEvaluator::new(
            point_evaluator,
            skip_mode,
            skip_count,
        ));
    }

    if jitter != CellJitter::DEFAULT_ONE {
        point_evaluator = Rc::new(JitterPointEvaluator::new(point_evaluator, jitter));
    }

    point_evaluator
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> PointEvaluatorJsonLoader {
        PointEvaluatorJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn empty_json_gives_default_evaluator() {
        let l = loader(serde_json::json!({}));
        assert_eq!(l.load_skip_count(), 0);
        assert_eq!(l.load_skip_mode(), SkipCellMode::Checkerboard);
        assert!(l.load_density().is_none());
        assert!(l.load_distance_range().is_none());
        assert_eq!(l.load_jitter(), CellJitter::DEFAULT_ONE);
        let evaluator = l.load();
        let _ = evaluator; // composition succeeded
    }

    #[test]
    fn skip_count_and_mode_parse() {
        let l = loader(serde_json::json!({ "Skip": 7, "SkipMode": "GRID" }));
        assert_eq!(l.load_skip_count(), 7);
        assert_eq!(l.load_skip_mode(), SkipCellMode::Grid);
    }

    #[test]
    fn distance_mode_parse() {
        let l = loader(serde_json::json!({ "DistanceMode": "MANHATTAN" }));
        let function = l.load_point_distance_function();
        // Manhattan 2d distance = |dx| + |dy|.
        assert_eq!(function.distance_2d(3.0, -4.0), 7.0);
    }

    #[test]
    fn density_threshold_evaluates() {
        let l = loader(serde_json::json!({ "Density": [0.25, 0.75] }));
        let density = l.load_density().expect("density expected");
        assert!(density.eval(0.5));
        assert!(!density.eval(0.1));
    }

    #[test]
    fn distance_range_parses_to_support_seam() {
        let l = loader(serde_json::json!({ "DistanceRange": [2.0, 4.0] }));
        let range = l.load_distance_range().expect("range expected");
        // supplier::DoubleRange semantics through the support seam.
        assert_eq!(IDoubleRange::get_value(range.as_ref(), 0.5), 3.0);
    }

    #[test]
    #[should_panic(expected = "No enum constant DistanceCalculationMode.CIRCLE")]
    fn bad_distance_mode_panics_verbatim() {
        PointEvaluatorJsonLoader::distance_calculation_mode_value_of("CIRCLE");
    }

    #[test]
    fn composition_wraps_skip_and_jitter() {
        // skip > 0 with non-default jitter: SkipCell wraps Density wraps
        // Normal, then Jitter wraps the outermost — only observable via the
        // jitter getter, which we check through the trait.
        let evaluator = point_evaluator_of_skippable(
            Rc::new(DistanceCalculationMode::Euclidean),
            None,
            None,
            4,
            SkipCellMode::Grid,
            CellJitter::of(0.5, 0.5, 1.0),
        );
        assert_eq!(evaluator.get_jitter(), CellJitter::of(0.5, 0.5, 1.0));

        // Default jitter composes down to a plain evaluator chain.
        let plain = point_evaluator_of(
            Rc::new(DistanceCalculationMode::Euclidean),
            None,
            None,
            CellJitter::DEFAULT_ONE,
        );
        assert_eq!(plain.get_jitter(), CellJitter::DEFAULT_ONE);
    }
}
