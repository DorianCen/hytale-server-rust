// Ported from: `.hytale-source/.../procedurallib/json/MeshNoiseJsonLoader.java`
//! Java `MeshNoiseJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::abstract_cell_jitter_json_loader::AbstractCellJitterJsonLoader;
use crate::json::cell_distance_function_json_loader::CellDistanceFunctionJsonLoader;
use crate::json::cell_noise_json_loader::constants as cell_noise;
use crate::json::double_threshold_json_loader::DoubleThresholdJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::cell::cell_type::CellType;
use crate::noise::cell::evaluator::density_point_evaluator::DensityPointEvaluator;
use crate::noise::condition::DoubleThresholdCondition;
use crate::noise::condition::{IDoubleCondition, IIntCondition};
use crate::noise::function::NoiseFunction;
use crate::noise::hex_mesh_noise::HexMeshNoise;
use crate::noise::mesh_noise::MeshNoise;

/// Java `MeshNoiseJsonLoader.Constants`.
mod constants {
    pub const KEY_THICKNESS: &str = "Thickness";
    pub const KEY_LINES_X: &str = "LinesX";
    pub const KEY_LINES_Y: &str = "LinesY";
    pub const KEY_LINES_Z: &str = "LinesZ";
    pub const ERROR_NO_THICKNESS: &str = "Could not find thickness. Keyword: Thickness";

    pub const DEFAULT_LINES_X: bool = true;
    pub const DEFAULT_LINES_Y: bool = true;
    pub const DEFAULT_LINES_Z: bool = true;
}

/// Java `MeshNoiseJsonLoader`.
pub struct MeshNoiseJsonLoader {
    /// Java `extends AbstractCellJitterJsonLoader<K, NoiseFunction>`.
    pub base: JsonLoaderBase,
}

impl AbstractCellJitterJsonLoader for MeshNoiseJsonLoader {
    fn base(&self) -> &JsonLoaderBase {
        &self.base
    }
}

impl MeshNoiseJsonLoader {
    /// Java `MeshNoiseJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".MeshNoise"), data_folder, json),
        }
    }

    /// Java `NoiseFunction load()`.
    pub fn load(&self) -> Rc<dyn NoiseFunction> {
        match self.load_cell_type() {
            CellType::Square => Rc::new(self.load_grid_mesh_noise()),
            CellType::Hex => Rc::new(self.load_hex_mesh_noise()),
        }
    }

    /// Java `@Nonnull MeshNoise loadGridMeshNoise()`.
    pub fn load_grid_mesh_noise(&self) -> MeshNoise {
        let default_jitter = self.load_default_jitter();
        MeshNoise::new(
            self.load_density(),
            self.load_thickness(),
            self.load_jitter_x(default_jitter),
            self.load_jitter_y(default_jitter),
        )
    }

    /// Java `@Nonnull HexMeshNoise loadHexMeshNoise()`.
    pub fn load_hex_mesh_noise(&self) -> HexMeshNoise {
        HexMeshNoise::new(
            self.load_density(),
            self.load_thickness(),
            self.load_jitter(),
            self.load_lines_x(),
            self.load_lines_y(),
            self.load_lines_z(),
        )
    }

    /// Java `@Nonnull CellType loadCellType()`.
    pub fn load_cell_type(&self) -> CellType {
        let mut cell_type = cell_noise::DEFAULT_CELL_TYPE;
        if self.base.has(cell_noise::KEY_CELL_TYPE) {
            cell_type = CellDistanceFunctionJsonLoader::cell_type_value_of(
                &JsonLoaderBase::get_as_string(&self.base.get(cell_noise::KEY_CELL_TYPE).unwrap()),
            );
        }
        cell_type
    }

    /// Java `protected double loadThickness()`.
    pub fn load_thickness(&self) -> f64 {
        if !self.base.has(constants::KEY_THICKNESS) {
            // Java: `throw new IllegalStateException(ERROR_NO_THICKNESS)`.
            panic!("{}", constants::ERROR_NO_THICKNESS);
        }
        JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_THICKNESS).unwrap())
    }

    /// Java `@Nonnull IIntCondition loadDensity()`.
    pub fn load_density(&self) -> Rc<dyn IIntCondition> {
        DensityPointEvaluator::get_density_condition(if self.base.has(cell_noise::KEY_DENSITY) {
            let threshold = DoubleThresholdJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(cell_noise::KEY_DENSITY),
            )
            .load();
            Some(Rc::new(DoubleThresholdCondition::new(threshold)) as Rc<dyn IDoubleCondition>)
        } else {
            None
        })
    }

    /// Java `protected boolean loadLinesX()`.
    pub fn load_lines_x(&self) -> bool {
        self.load_lines_flag(constants::KEY_LINES_X, constants::DEFAULT_LINES_X)
    }

    /// Java `protected boolean loadLinesY()`.
    pub fn load_lines_y(&self) -> bool {
        self.load_lines_flag(constants::KEY_LINES_Y, constants::DEFAULT_LINES_Y)
    }

    /// Java `protected boolean loadLinesZ()`.
    pub fn load_lines_z(&self) -> bool {
        self.load_lines_flag(constants::KEY_LINES_Z, constants::DEFAULT_LINES_Z)
    }

    /// Java `protected boolean loadLinesFlag(String key, boolean defaulValue)`.
    pub fn load_lines_flag(&self, key: &str, default_value: bool) -> bool {
        if !self.base.has(key) {
            return default_value;
        }

        JsonLoaderBase::get_as_bool(&self.base.get(key).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::function::NoiseFunction2d;

    fn loader(json: Value) -> MeshNoiseJsonLoader {
        MeshNoiseJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn square_is_the_default_cell_type() {
        let l = loader(serde_json::json!({ "Thickness": 0.2 }));
        assert_eq!(l.load_cell_type(), CellType::Square);
        let noise = l.load();
        let a = NoiseFunction2d::get(noise.as_ref(), 7, 3, 1.5, -2.25);
        assert_eq!(
            a.to_bits(),
            NoiseFunction2d::get(noise.as_ref(), 7, 3, 1.5, -2.25).to_bits()
        );
        assert!(a.is_finite());
    }

    #[test]
    fn hex_cell_type_resolves_with_line_flags() {
        let l = loader(serde_json::json!({
            "CellType": "HEX",
            "Thickness": 0.2,
            "Jitter": 0.5,
            "LinesX": false
        }));
        assert_eq!(l.load_cell_type(), CellType::Hex);
        assert!(!l.load_lines_x());
        assert!(l.load_lines_y());
        assert!(l.load_lines_z());
        let _ = l.load(); // hex composition succeeds
    }

    #[test]
    fn density_defaults_to_always_true() {
        let l = loader(serde_json::json!({ "Thickness": 0.2 }));
        assert!(l.load_density().eval(1234));
        assert!(l.load_density().eval(-1234));
    }

    #[test]
    #[should_panic(expected = "Could not find thickness. Keyword: Thickness")]
    fn missing_thickness_panics_verbatim() {
        loader(serde_json::json!({})).load();
    }
}
