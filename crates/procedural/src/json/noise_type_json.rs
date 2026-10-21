// Ported from: `.hytale-source/.../procedurallib/json/NoiseTypeJson.java`
//! Java `procedurallib.json.NoiseTypeJson` — the `NoiseType` keyword → loader
//! mapping. Java resolves the loader class reflectively; the Rust enum calls
//! the ported loader structs directly.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::branch_noise_json_loader::BranchNoiseJsonLoader;
use crate::json::cell_noise_json_loader::CellNoiseJsonLoader;
use crate::json::constant_noise_json_loader::ConstantNoiseJsonLoader;
use crate::json::distance_noise_json_loader::DistanceNoiseJsonLoader;
use crate::json::grid_noise_json_loader::GridNoiseJsonLoader;
use crate::json::mesh_noise_json_loader::MeshNoiseJsonLoader;
use crate::json::old_simplex_noise_json_loader::OldSimplexNoiseJsonLoader;
use crate::json::perlin_noise_json_loader::PerlinNoiseJsonLoader;
use crate::json::point_noise_json_loader::PointNoiseJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::simplex_noise_json_loader::SimplexNoiseJsonLoader;
use crate::json::value_noise_json_loader::ValueNoiseJsonLoader;
use crate::json::NoiseFunctionObj;
use crate::noise::function::NoiseType;

/// Java `NoiseTypeJson` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseTypeJson {
    Cell,
    Constant,
    Distance,
    Perlin,
    Simplex,
    OldSimplex,
    Value,
    Mesh,
    Grid,
    Branch,
    Point,
}

impl NoiseTypeJson {
    /// Java `NoiseTypeJson.valueOf(String)` (enum-name parse, exact names).
    pub fn value_of(name: &str) -> Self {
        match name {
            "CELL" => Self::Cell,
            "CONSTANT" => Self::Constant,
            "DISTANCE" => Self::Distance,
            "PERLIN" => Self::Perlin,
            "SIMPLEX" => Self::Simplex,
            "OLD_SIMPLEX" => Self::OldSimplex,
            "VALUE" => Self::Value,
            "MESH" => Self::Mesh,
            "GRID" => Self::Grid,
            "BRANCH" => Self::Branch,
            "POINT" => Self::Point,
            other => panic!("No enum constant NoiseTypeJson.{other}"),
        }
    }

    /// Java `NoiseType noiseType()` — the mapped generator enum.
    pub fn noise_type(&self) -> NoiseType {
        match self {
            Self::Cell => NoiseType::Cell,
            Self::Constant => NoiseType::Constant,
            Self::Distance => NoiseType::Distance,
            Self::Perlin => NoiseType::Perlin,
            Self::Simplex => NoiseType::Simplex,
            Self::OldSimplex => NoiseType::OldSimplex,
            Self::Value => NoiseType::Value,
            Self::Mesh => NoiseType::Mesh,
            Self::Grid => NoiseType::Grid,
            Self::Branch => NoiseType::Branch,
            Self::Point => NoiseType::Point,
        }
    }

    /// Java `JsonLoader<K, NoiseFunction> newLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`
    /// (reflection replaced by direct dispatch; loaders append their own seed
    /// suffixes).
    pub fn new_loader(
        &self,
        seed: &SeedString,
        data_folder: &Path,
        json: Option<Value>,
    ) -> NoiseFunctionObj {
        match self {
            Self::Simplex => {
                Rc::new(SimplexNoiseJsonLoader::new(seed.clone(), data_folder, json).load())
            }
            Self::OldSimplex => {
                Rc::new(OldSimplexNoiseJsonLoader::new(seed.clone(), data_folder, json).load())
            }
            Self::Perlin => {
                Rc::new(PerlinNoiseJsonLoader::new(seed.clone(), data_folder, json).load())
            }
            Self::Value => {
                Rc::new(ValueNoiseJsonLoader::new(seed.clone(), data_folder, json).load())
            }
            Self::Constant => {
                Rc::new(ConstantNoiseJsonLoader::new(seed.clone(), data_folder, json).load())
            }
            Self::Point => {
                Rc::new(PointNoiseJsonLoader::new(seed.clone(), data_folder, json).load())
            }
            Self::Cell => CellNoiseJsonLoader::new(seed.clone(), data_folder, json).load(),
            Self::Distance => DistanceNoiseJsonLoader::new(seed.clone(), data_folder, json).load(),
            Self::Mesh => MeshNoiseJsonLoader::new(seed.clone(), data_folder, json).load(),
            Self::Grid => Rc::new(GridNoiseJsonLoader::new(seed.clone(), data_folder, json).load()),
            Self::Branch => {
                Rc::new(BranchNoiseJsonLoader::new(seed.clone(), data_folder, json).load())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::noise::function::NoiseFunction2d;

    fn dispatch(type_name: &str, json: Value) -> NoiseFunctionObj {
        NoiseTypeJson::value_of(type_name).new_loader(
            &SeedString::new("test"),
            Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn enum_names_parse_verbatim() {
        assert_eq!(NoiseTypeJson::value_of("CELL"), NoiseTypeJson::Cell);
        assert_eq!(NoiseTypeJson::value_of("DISTANCE"), NoiseTypeJson::Distance);
        assert_eq!(NoiseTypeJson::value_of("MESH"), NoiseTypeJson::Mesh);
        assert_eq!(NoiseTypeJson::value_of("GRID"), NoiseTypeJson::Grid);
        assert_eq!(NoiseTypeJson::value_of("BRANCH"), NoiseTypeJson::Branch);
        assert_eq!(NoiseTypeJson::value_of("POINT"), NoiseTypeJson::Point);
    }

    #[test]
    fn simple_dispatches_are_distinct() {
        // Simplex is a stateless singleton — the sample ignores the seed.
        let simplex = dispatch("SIMPLEX", serde_json::json!({}));
        assert_eq!(
            NoiseFunction2d::get(simplex.as_ref(), 1, 2, 0.5, 0.5).to_bits(),
            NoiseFunction2d::get(simplex.as_ref(), 999, 2, 0.5, 0.5).to_bits()
        );

        // Cell honours the offset seed — the samples differ (the CELL_VALUE
        // cell function hashes the offset seed with the cell indices).
        let cell = dispatch("CELL", serde_json::json!({ "CellMode": "CELL_VALUE" }));
        assert_ne!(
            NoiseFunction2d::get(cell.as_ref(), 1, 2, 0.5, 0.5).to_bits(),
            NoiseFunction2d::get(cell.as_ref(), 1, 999, 0.5, 0.5).to_bits()
        );
    }

    #[test]
    fn cell_distance_mesh_grid_branch_dispatch_resolves() {
        // All five previously-panicking arms now load (smoke).
        let _ = dispatch("CELL", serde_json::json!({}));
        let _ = dispatch("DISTANCE", serde_json::json!({ "Thickness": 0.1 }));
        let _ = dispatch("MESH", serde_json::json!({ "Thickness": 0.1 }));
        let _ = dispatch("GRID", serde_json::json!({ "Thickness": 0.1 }));
        let _ = dispatch("BRANCH", serde_json::json!({}));
    }

    #[test]
    fn grid_dispatch_uses_the_parsed_thickness() {
        // GridNoise is deterministic and periodic; sample through the
        // dispatched trait object.
        let grid = dispatch("GRID", serde_json::json!({ "Thickness": 0.5 }));
        assert_eq!(
            NoiseFunction2d::get(grid.as_ref(), 1, 2, 0.5, 0.5),
            1.0 // cell centre → far edge
        );
    }

    #[test]
    #[should_panic(expected = "No enum constant NoiseTypeJson.NOISE")]
    fn bad_type_name_panics_verbatim() {
        NoiseTypeJson::value_of("NOISE");
    }
}
