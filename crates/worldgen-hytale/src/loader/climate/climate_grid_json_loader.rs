// Ported from: `.hytale-source/.../worldgen/loader/climate/ClimateGridJsonLoader.java`
//! Java `loader.climate.ClimateGridJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::climate::climate_noise::Grid;
use crate::climate::direct_grid::direct_grid;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::cell::evaluator::jitter_point_evaluator::JitterPointEvaluator;
use procedural::noise::cell::evaluator::normal_point_evaluator::NormalPointEvaluator;
use procedural::noise::cell::grid_cell_distance_function::GridCellDistanceFunction;
use procedural::noise::cell::jitter::{CellJitter, ConstantCellJitter};

/// Java `ClimateGridJsonLoader.Constants.KEY_SEED`.
pub const KEY_SEED: &str = "Seed";
/// Java `ClimateGridJsonLoader.Constants.KEY_SCALE`.
pub const KEY_SCALE: &str = "Scale";
/// Java `ClimateGridJsonLoader.Constants.KEY_JITTER`.
pub const KEY_JITTER: &str = "Jitter";

/// Java `ClimateGridJsonLoader.Constants.DEFAULT_SCALE`.
pub const DEFAULT_SCALE: f64 = 1.0;
/// Java `ClimateGridJsonLoader.Constants.DEFAULT_JITTER`.
pub const DEFAULT_JITTER: f64 = 0.8;

/// Java `ClimateGridJsonLoader`.
pub struct ClimateGridJsonLoader {
    /// Java `extends JsonLoader<K, ClimateNoise.Grid>`.
    pub base: JsonLoaderBase,
}

impl ClimateGridJsonLoader {
    /// Java `ClimateGridJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    ///
    /// Note: the Java constructor does **not** append a seed suffix.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `ClimateNoise.Grid load()`.
    pub fn load(&self) -> Grid {
        // Java: `if (json == null) return DEFAULT_GRID;` — a JSON `null`
        // element is *not* a Java `null` and therefore takes the regular
        // path.
        if self.base.json.is_none() {
            return default_grid();
        }

        Grid::new(
            self.load_seed(),
            self.load_scale(),
            GridCellDistanceFunction::distance_function(),
            self.load_evaluator(),
        )
    }

    /// Java `int loadSeed()`.
    pub fn load_seed(&self) -> i32 {
        let mut seed_val = self.base.seed.hash_code();
        if self.base.has(KEY_SEED) {
            let overwritten = self
                .base
                .seed
                .append_to_original(&JsonLoaderBase::get_as_string(
                    &self.base.get(KEY_SEED).unwrap(),
                ));
            seed_val = overwritten.hash_code();
        }
        seed_val
    }

    /// Java `double loadScale()`.
    pub fn load_scale(&self) -> f64 {
        self.base.must_get_number(KEY_SCALE, Some(DEFAULT_SCALE))
    }

    /// Java `PointEvaluator loadEvaluator()`.
    pub fn load_evaluator(
        &self,
    ) -> Rc<dyn procedural::noise::cell::evaluator::point_evaluator::PointEvaluator> {
        let point_evaluator = NormalPointEvaluator::euclidean();
        let jitter = self.base.must_get_number(KEY_JITTER, Some(DEFAULT_JITTER));
        let biome_jitter = CellJitter::Constant(ConstantCellJitter::new(jitter, jitter, jitter));
        Rc::new(JitterPointEvaluator::new(point_evaluator, biome_jitter))
    }
}

/// Java `ClimateGridJsonLoader.Constants.DEFAULT_GRID` —
/// `new ClimateNoise.Grid(0, 1.0, DirectGrid.INSTANCE, NormalPointEvaluator.EUCLIDEAN)`.
pub fn default_grid() -> Grid {
    Grid::new(0, 1.0, direct_grid(), NormalPointEvaluator::euclidean())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Option<Value>) -> ClimateGridJsonLoader {
        ClimateGridJsonLoader::new(SeedString::new("0"), Path::new("."), json)
    }

    #[test]
    fn missing_json_yields_the_default_grid() {
        let grid = loader(None).load();
        assert_eq!(grid.seed_offset, 0);
        assert_eq!(grid.scale, 1.0);
    }

    #[test]
    fn scale_and_seed_are_read() {
        let grid = loader(Some(serde_json::json!({ "Scale": 4.0, "Seed": "GRID" }))).load();
        assert_eq!(grid.scale, 4.0);
        // "0" + "GRID" (appendToOriginal) -> the seed string becomes "0GRID".
        let expected = SeedString::new("0").append_to_original("GRID").hash_code();
        assert_eq!(grid.seed_offset, expected);
    }

    #[test]
    fn seed_defaults_to_the_seed_string_hash() {
        let grid = loader(Some(serde_json::json!({}))).load();
        assert_eq!(grid.seed_offset, SeedString::new("0").hash_code());
        assert_eq!(grid.scale, DEFAULT_SCALE);
    }
}
