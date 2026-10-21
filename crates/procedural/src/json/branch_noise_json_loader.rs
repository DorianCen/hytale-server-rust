// Ported from: `.hytale-source/.../procedurallib/json/BranchNoiseJsonLoader.java`
//! Java `BranchNoiseJsonLoader`.
//!
//! ⚠ NOTE: Java subclasses `BranchNoise` (`LoadedBranchNoise`) only to wire
//! the `SeedResource`'s thread-local scratch buffers into `localBuffer2d` and
//! to override `toString`; the Rust port allocates per-call buffers, so the
//! plain [`BranchNoise`] is the faithful equivalent.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::abstract_cell_jitter_json_loader::AbstractCellJitterJsonLoader;
use crate::json::cell_distance_function_json_loader::CellDistanceFunctionJsonLoader;
use crate::json::distance_noise_json_loader::DistanceNoiseJsonLoader;
use crate::json::double_condition_json_loader::DoubleConditionJsonLoader;
use crate::json::double_range_json_loader::DoubleRangeJsonLoader;
use crate::json::noise_property_json_loader::NoisePropertyJsonLoader;
use crate::json::point_evaluator_json_loader::{point_evaluator_of, PointEvaluatorJsonLoader};
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::branch_noise::BranchNoise;
use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::cell_point_function::CellPointFunction;
use crate::noise::cell::cell_type::CellType;
use crate::noise::cell::evaluator::branch_evaluator::{BranchDirection, BranchEvaluator};
use crate::noise::cell::evaluator::density_point_evaluator::DensityPointEvaluator;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::grid_cell_distance_function::{
    GridCellDistanceFunction, GridCellPointFunction,
};
use crate::noise::cell::hex_cell_distance_function::{
    HexCellDistanceFunction, HexCellPointFunction,
};
use crate::noise::cell::jitter::CellJitter;
use crate::noise::condition::ConstantIntCondition;
use crate::noise::distance_noise::Distance2Function;
use crate::noise::supplier::DoubleRange;
use crate::noise::support::{IDoubleRange, IIntCondition, NoiseFormula};

/// Java `BranchNoiseJsonLoader.Constant`.
///
/// `DEFAULT_JITTER` is `CellNoiseJsonLoader.Constants.DEFAULT_JITTER`.
mod constants {
    use crate::noise::cell::cell_type::CellType;
    use crate::noise::cell::distance_calculation_mode::DistanceCalculationMode;
    use crate::noise::cell::evaluator::branch_evaluator::BranchDirection;
    use crate::noise::distance_noise::Distance2Mode;

    pub const KEY_PARENT_TYPE: &str = "ParentType";
    pub const KEY_PARENT_DISTANCE_CALCULATION_MODE: &str = "ParentDistanceMode";
    pub const KEY_PARENT_DISTANCE_TYPE: &str = "ParentDistanceType";
    pub const KEY_PARENT_FORMULA: &str = "ParentFormula";
    pub const KEY_PARENT_JITTER: &str = "ParentJitter";
    pub const KEY_PARENT_JITTER_X: &str = "ParentJitterX";
    pub const KEY_PARENT_JITTER_Y: &str = "ParentJitterY";
    pub const KEY_PARENT_VALUE: &str = "ParentValue";
    pub const KEY_PARENT_FADE: &str = "ParentFade";
    pub const KEY_PARENT_DENSITY: &str = "ParentDensity";

    pub const KEY_LINE_TYPE: &str = "LineType";
    pub const KEY_LINE_DIRECTION_MODE: &str = "LineDirection";
    pub const KEY_LINE_SCALE: &str = "LineScale";
    pub const KEY_LINE_JITTER: &str = "LineJitter";
    pub const KEY_LINE_JITTER_X: &str = "LineJitterX";
    pub const KEY_LINE_JITTER_Y: &str = "LineJitterY";
    pub const KEY_LINE_THICKNESS: &str = "LineThickness";

    pub const DEFAULT_JITTER: f64 = crate::json::cell_noise_json_loader::constants::DEFAULT_JITTER;
    pub const DEFAULT_PARENT_VALUE: f64 = 0.0;
    pub const DEFAULT_PARENT_FADE: f64 = 0.0;
    pub const DEFAULT_LINE_SCALE: f64 = 0.1;
    pub const DEFAULT_LINE_THICKNESS: f64 = 0.1;

    pub const DEFAULT_PARENT_CELL_TYPE: CellType = CellType::Square;
    pub const DEFAULT_LINE_CELL_TYPE: CellType = CellType::Square;
    pub const DEFAULT_LINE_DIRECTION: BranchDirection = BranchDirection::Outward;
    pub const DEFAULT_PARENT_DISTANCE_CAL_MODE: DistanceCalculationMode =
        DistanceCalculationMode::Euclidean;
    pub const DEFAULT_PARENT_DISTANCE_TYPE: Distance2Mode = Distance2Mode::Div;
    // Java: NoiseFormulaProperty.NoiseFormula.SQRT — the enum-name default is
    // kept as the string to stay in lockstep with the shared `valueOf` parse.
    pub const DEFAULT_PARENT_FORMULA: &str = "SQRT";
}

/// Java `BranchNoiseJsonLoader`.
pub struct BranchNoiseJsonLoader {
    /// Java `extends AbstractCellJitterJsonLoader<T, BranchNoise>`.
    pub base: JsonLoaderBase,
}

impl AbstractCellJitterJsonLoader for BranchNoiseJsonLoader {
    fn base(&self) -> &JsonLoaderBase {
        &self.base
    }
}

impl BranchNoiseJsonLoader {
    /// Java `BranchNoiseJsonLoader(SeedString<T> seed, Path dataFolder, JsonElement json)`
    /// — no seed suffix append.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `BranchNoise load()`.
    pub fn load(&self) -> BranchNoise {
        let parent_cell_type = self.load_parent_cell_type();
        let parent_function = get_cell_distance_function(parent_cell_type);
        let parent_evaluator = self.load_parent_evaluator();
        let parent_value =
            self.load_double(constants::KEY_PARENT_VALUE, constants::DEFAULT_PARENT_VALUE);
        let parent_fade =
            self.load_range(constants::KEY_PARENT_FADE, constants::DEFAULT_PARENT_FADE);
        let parent_density = self.load_parent_density();
        let parent_distance_type = self.load_parent_distance2_function();
        let parent_formula = self.load_parent_formula();

        let line_cell_type = self.load_line_cell_type();
        let line_point_function = get_cell_point_function(line_cell_type);
        let line_scale = self.load_double(constants::KEY_LINE_SCALE, constants::DEFAULT_LINE_SCALE);
        let line_thickness = self.load_range(
            constants::KEY_LINE_THICKNESS,
            constants::DEFAULT_LINE_THICKNESS,
        );
        // Wrap DistanceFunction in a non-normalized version
        let line_function = get_cell_distance_function(line_cell_type);
        let line_evaluator =
            self.load_line_evaluator(parent_function.clone(), line_point_function, line_scale);

        BranchNoise::new(
            parent_function,
            parent_evaluator,
            parent_value,
            parent_fade,
            parent_density,
            parent_distance_type,
            parent_formula,
            line_function,
            line_evaluator,
            line_scale,
            line_thickness,
        )
    }

    /// Java `CellType loadParentCellType()`.
    pub fn load_parent_cell_type(&self) -> CellType {
        self.load_enum(
            constants::KEY_PARENT_TYPE,
            CellDistanceFunctionJsonLoader::cell_type_value_of,
            constants::DEFAULT_PARENT_CELL_TYPE,
        )
    }

    /// Java `CellType loadLineCellType()`.
    pub fn load_line_cell_type(&self) -> CellType {
        self.load_enum(
            constants::KEY_LINE_TYPE,
            CellDistanceFunctionJsonLoader::cell_type_value_of,
            constants::DEFAULT_LINE_CELL_TYPE,
        )
    }

    /// Java `PointEvaluator loadParentEvaluator()`.
    pub fn load_parent_evaluator(&self) -> Rc<dyn PointEvaluator> {
        let default_jitter =
            self.load_double(constants::KEY_PARENT_JITTER, constants::DEFAULT_JITTER);
        let jitter_x = self.load_double(constants::KEY_PARENT_JITTER_X, default_jitter);
        let jitter_y = self.load_double(constants::KEY_PARENT_JITTER_Y, default_jitter);
        let jitter = CellJitter::of(jitter_x, jitter_y, 1.0);

        let distance_mode = self.load_enum(
            constants::KEY_PARENT_DISTANCE_CALCULATION_MODE,
            PointEvaluatorJsonLoader::distance_calculation_mode_value_of,
            constants::DEFAULT_PARENT_DISTANCE_CAL_MODE,
        );

        point_evaluator_of(Rc::new(distance_mode), None, None, jitter)
    }

    /// Java `@Nonnull IIntCondition loadParentDensity()`.
    pub fn load_parent_density(&self) -> Rc<dyn IIntCondition> {
        let mut density: Rc<dyn IIntCondition> = Rc::new(ConstantIntCondition::DEFAULT_TRUE);
        if self.base.has(constants::KEY_PARENT_DENSITY) {
            let density_range = DoubleConditionJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_PARENT_DENSITY),
            )
            .load();

            density = DensityPointEvaluator::get_density_condition(Some(density_range));
        }
        density
    }

    /// Java `DistanceNoise.Distance2Function loadParentDistance2Function()`.
    pub fn load_parent_distance2_function(&self) -> Rc<dyn Distance2Function> {
        Rc::new(self.load_enum(
            constants::KEY_PARENT_DISTANCE_TYPE,
            DistanceNoiseJsonLoader::distance2_mode_value_of,
            constants::DEFAULT_PARENT_DISTANCE_TYPE,
        ))
    }

    /// Java `NoiseFormulaProperty.NoiseFormula.Formula loadParentFormula()`.
    pub fn load_parent_formula(&self) -> Rc<dyn NoiseFormula> {
        let formula = self.load_enum(
            constants::KEY_PARENT_FORMULA,
            NoisePropertyJsonLoader::noise_formula_value_of,
            NoisePropertyJsonLoader::noise_formula_value_of(constants::DEFAULT_PARENT_FORMULA),
        );
        Rc::new(formula)
    }

    /// Java `@Nonnull PointEvaluator loadLineEvaluator(CellDistanceFunction
    /// parentFunction, CellPointFunction linePointFunction, double lineScale)`.
    pub fn load_line_evaluator(
        &self,
        parent_function: Rc<dyn CellDistanceFunction>,
        line_point_function: Rc<dyn CellPointFunction>,
        line_scale: f64,
    ) -> Rc<dyn PointEvaluator> {
        let default_jitter =
            self.load_double(constants::KEY_LINE_JITTER, constants::DEFAULT_JITTER);
        let jitter_x = self.load_double(constants::KEY_LINE_JITTER_X, default_jitter);
        let jitter_y = self.load_double(constants::KEY_LINE_JITTER_Y, default_jitter);
        let jitter = CellJitter::of(jitter_x, jitter_y, 1.0);

        let direction = self.load_enum(
            constants::KEY_LINE_DIRECTION_MODE,
            Self::branch_direction_value_of,
            constants::DEFAULT_LINE_DIRECTION,
        );

        Rc::new(BranchEvaluator::new(
            parent_function,
            line_point_function,
            direction,
            jitter,
            line_scale,
        ))
    }

    /// Java `double loadDouble(String key, double def)`.
    pub fn load_double(&self, key: &str, default_value: f64) -> f64 {
        if !self.base.has(key) {
            return default_value;
        }

        JsonLoaderBase::get_as_double(&self.base.get(key).unwrap())
    }

    /// Java `IDoubleRange loadRange(String key, double def)` — a missing key
    /// yields `new DoubleRange.Constant(def)` (Java's `@Nullable` is vacuous).
    pub fn load_range(&self, key: &str, default_value: f64) -> Rc<dyn IDoubleRange> {
        if !self.base.has(key) {
            return Rc::new(DoubleRange::constant(default_value));
        }

        Rc::new(
            DoubleRangeJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(key),
            )
            .load(),
        )
    }

    /// Java `<E extends Enum<E>> E loadEnum(String key, Function<String, E>
    /// valueOf, E def)`.
    pub fn load_enum<E>(&self, key: &str, value_of: fn(&str) -> E, default_value: E) -> E {
        if !self.base.has(key) {
            return default_value;
        }

        value_of(&JsonLoaderBase::get_as_string(&self.base.get(key).unwrap()))
    }

    /// Java `BranchEvaluator.Direction.valueOf(String)`.
    pub fn branch_direction_value_of(name: &str) -> BranchDirection {
        match name {
            "OUTWARD" => BranchDirection::Outward,
            "INWARD" => BranchDirection::Inward,
            "RANDOM" => BranchDirection::Random,
            other => panic!("No enum constant BranchEvaluator.Direction.{other}"),
        }
    }
}

/// Java static `getCellDistanceFunction(@Nonnull CellType)`.
pub fn get_cell_distance_function(cell_type: CellType) -> Rc<dyn CellDistanceFunction> {
    match cell_type {
        CellType::Square => GridCellDistanceFunction::distance_function(),
        CellType::Hex => HexCellDistanceFunction::distance_function(),
    }
}

/// Java static `getCellPointFunction(@Nonnull CellType)` — the Java
/// `POINT_FUNCTION` statics live on the anonymous nested classes
/// (`GridCellPointFunction` / `HexCellPointFunction`).
pub fn get_cell_point_function(cell_type: CellType) -> Rc<dyn CellPointFunction> {
    match cell_type {
        CellType::Square => GridCellPointFunction::point_function(),
        CellType::Hex => HexCellPointFunction::point_function(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::function::NoiseFunction2d;

    fn loader(json: Value) -> BranchNoiseJsonLoader {
        BranchNoiseJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn empty_json_applies_every_default() {
        let l = loader(serde_json::json!({}));
        assert_eq!(l.load_parent_cell_type(), CellType::Square);
        assert_eq!(l.load_line_cell_type(), CellType::Square);
        assert_eq!(
            l.load_parent_distance2_function().eval(2.0, 4.0),
            0.5 // DIV
        );
        // Default fade/thickness are constant ranges through the support seam.
        let fade = l.load_range(constants::KEY_PARENT_FADE, constants::DEFAULT_PARENT_FADE);
        assert_eq!(IDoubleRange::get_value(fade.as_ref(), 0.37), 0.0);
        // ParentDensity defaults to always-true.
        assert!(l.load_parent_density().eval(7));

        let noise = l.load();
        let a = NoiseFunction2d::get(&noise, 7, 3, 1.5, -2.25);
        assert_eq!(
            a.to_bits(),
            NoiseFunction2d::get(&noise, 7, 3, 1.5, -2.25).to_bits()
        );
        assert!(a.is_finite());
    }

    #[test]
    fn keys_parse_for_parent_and_line_phases() {
        let l = loader(serde_json::json!({
            "ParentType": "HEX",
            "LineType": "HEX",
            "ParentValue": 0.3,
            "LineScale": 0.4,
            "LineDirection": "INWARD",
            "ParentJitter": 0.6,
            "LineThickness": [0.05, 0.2]
        }));
        assert_eq!(l.load_parent_cell_type(), CellType::Hex);
        assert_eq!(l.load_line_cell_type(), CellType::Hex);
        assert_eq!(l.load_double(constants::KEY_PARENT_VALUE, 0.0), 0.3);
        let thickness = l.load_range(constants::KEY_LINE_THICKNESS, 0.0);
        assert_eq!(IDoubleRange::get_value(thickness.as_ref(), 0.5), 0.125);

        // The full load composes the branch evaluator with the INWARD
        // direction (only observable via a deterministic sample here).
        let noise = l.load();
        let _ = NoiseFunction2d::get(&noise, 7, 3, 1.5, -2.25);
    }

    #[test]
    #[should_panic(expected = "No enum constant BranchEvaluator.Direction.SPIRAL")]
    fn bad_direction_panics_verbatim() {
        BranchNoiseJsonLoader::branch_direction_value_of("SPIRAL");
    }

    #[test]
    #[should_panic(expected = "No enum constant CellType.TRIANGLE")]
    fn bad_parent_type_panics_verbatim() {
        loader(serde_json::json!({ "ParentType": "TRIANGLE" })).load();
    }
}
