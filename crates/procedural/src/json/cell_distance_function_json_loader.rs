// Ported from: `.hytale-source/.../procedurallib/json/CellDistanceFunctionJsonLoader.java`
//! Java `CellDistanceFunctionJsonLoader`.
//!
//! ⚠ NOTE: the Java `pointDistanceFunction` field is stored but never read by
//! the class (the callers hand it in so *other* code paths — e.g.
//! `PointGeneratorJsonLoader` — can thread it through constructors); kept for
//! 1:1 state mapping. The `@Nullable load()` result is never `null` in
//! practice, so the port returns `Rc<dyn CellDistanceFunction>`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::cell_border_distance_function_json_loader::CellBorderDistanceFunctionJsonLoader;
use crate::json::cell_noise_json_loader::constants;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::cell_type::CellType;
use crate::noise::cell::grid_cell_distance_function::GridCellDistanceFunction;
use crate::noise::cell::hex_cell_distance_function::HexCellDistanceFunction;
use crate::noise::cell::measurement_mode::MeasurementMode;
use crate::noise::cell::point_distance_function::PointDistanceFunction;

/// Java `CellDistanceFunctionJsonLoader`.
pub struct CellDistanceFunctionJsonLoader {
    /// Java `extends JsonLoader<K, CellDistanceFunction>`.
    pub base: JsonLoaderBase,
    /// Java `protected final MeasurementMode measurementMode`.
    pub measurement_mode: MeasurementMode,
    /// Java `protected final PointDistanceFunction pointDistanceFunction`
    /// (unread by the Java class — kept for 1:1 state mapping).
    pub point_distance_function: Option<Rc<dyn PointDistanceFunction>>,
}

impl CellDistanceFunctionJsonLoader {
    /// Java `CellDistanceFunctionJsonLoader(seed, dataFolder, json,
    /// @Nullable PointDistanceFunction)` — defaults `CENTRE_DISTANCE`.
    pub fn new(
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

    /// Java `CellDistanceFunctionJsonLoader(seed, dataFolder, json,
    /// MeasurementMode, @Nullable PointDistanceFunction)`.
    pub fn with_measurement_mode(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        measurement_mode: MeasurementMode,
        point_distance_function: Option<Rc<dyn PointDistanceFunction>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CellDistanceFunction"), data_folder, json),
            measurement_mode,
            point_distance_function,
        }
    }

    /// Java `CellDistanceFunction load()`.
    pub fn load(&self) -> Rc<dyn CellDistanceFunction> {
        let distance_function = self.load_distance_function();
        match self.measurement_mode {
            MeasurementMode::CentreDistance => distance_function,
            MeasurementMode::BorderDistance => Rc::new(
                CellBorderDistanceFunctionJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.json.clone(),
                    distance_function,
                )
                .load(),
            ),
        }
    }

    /// Java `@Nonnull CellType loadCellType()`.
    pub fn load_cell_type(&self) -> CellType {
        let mut cell_type = constants::DEFAULT_CELL_TYPE;
        if self.base.has(constants::KEY_CELL_TYPE) {
            cell_type = Self::cell_type_value_of(&JsonLoaderBase::get_as_string(
                &self.base.get(constants::KEY_CELL_TYPE).unwrap(),
            ));
        }
        cell_type
    }

    /// Java `@Nonnull CellDistanceFunction loadDistanceFunction()`.
    pub fn load_distance_function(&self) -> Rc<dyn CellDistanceFunction> {
        match self.load_cell_type() {
            CellType::Square => GridCellDistanceFunction::distance_function(),
            CellType::Hex => HexCellDistanceFunction::distance_function(),
        }
    }

    /// Java `CellType.valueOf(String)`.
    pub fn cell_type_value_of(name: &str) -> CellType {
        match name {
            "SQUARE" => CellType::Square,
            "HEX" => CellType::Hex,
            other => panic!("No enum constant CellType.{other}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> CellDistanceFunctionJsonLoader {
        CellDistanceFunctionJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
            None,
        )
    }

    #[test]
    fn square_defaults_to_grid_distance_function() {
        let l = loader(serde_json::json!({}));
        assert_eq!(l.load_cell_type(), CellType::Square);
        let f = l.load();
        // Smoke: grid distance functions are stateless and deterministic.
        let a = f.scale(3.0);
        assert_eq!(a.to_bits(), f.scale(3.0).to_bits());
    }

    #[test]
    fn hex_cell_type_resolves() {
        let l = loader(serde_json::json!({ "CellType": "HEX" }));
        assert_eq!(l.load_cell_type(), CellType::Hex);
        let _ = l.load_distance_function();
    }

    #[test]
    #[should_panic(expected = "No enum constant CellType.TRIANGLE")]
    fn bad_cell_type_panics_verbatim() {
        CellDistanceFunctionJsonLoader::cell_type_value_of("TRIANGLE");
    }
}
