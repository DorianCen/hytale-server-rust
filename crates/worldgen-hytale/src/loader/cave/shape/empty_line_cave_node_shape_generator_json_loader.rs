// Ported from: `.hytale-source/.../worldgen/loader/cave/shape/EmptyLineCaveNodeShapeGeneratorJsonLoader.java`
//! Java `loader.cave.shape.EmptyLineCaveNodeShapeGeneratorJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::shape::empty_line_cave_node_shape::EmptyLineCaveNodeShapeGenerator;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::supplier::IDoubleRange;

/// Java `EmptyLineCaveNodeShapeGeneratorJsonLoader.Constants`.
pub mod constants {
    pub const KEY_LENGTH: &str = "Length";
}

/// Java `loader.cave.shape.EmptyLineCaveNodeShapeGeneratorJsonLoader`.
pub struct EmptyLineCaveNodeShapeGeneratorJsonLoader {
    /// Java `extends CaveNodeShapeGeneratorJsonLoader`.
    pub base: JsonLoaderBase,
}

impl EmptyLineCaveNodeShapeGeneratorJsonLoader {
    /// Java `EmptyLineCaveNodeShapeGeneratorJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(".EmptyLineCaveNodeShapeGenerator"),
                data_folder,
                json,
            ),
        }
    }

    /// Java `EmptyLineCaveNodeShape.EmptyLineCaveNodeShapeGenerator load()`.
    pub fn load(&self) -> EmptyLineCaveNodeShapeGenerator {
        EmptyLineCaveNodeShapeGenerator::new(self.load_length())
    }

    /// Java `IDoubleRange loadLength()`.
    fn load_length(&self) -> Rc<dyn IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_LENGTH),
                0.0,
            )
            .load(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;

    fn loader(json: Option<Value>) -> EmptyLineCaveNodeShapeGeneratorJsonLoader {
        EmptyLineCaveNodeShapeGeneratorJsonLoader::new(SeedString::new("0"), Path::new("."), json)
    }

    #[test]
    fn the_seed_appends_the_empty_line_shape_generator_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.EmptyLineCaveNodeShapeGenerator");
    }

    #[test]
    fn the_length_defaults_to_zero() {
        let generator = loader(None).load();
        let mut random = hytale_math::fast_random::FastRandom::new(1);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &crate::cave::test_stubs::cave_type("cave"),
            None,
            None,
            crate::loader::util::Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            0.0,
        );
        assert_eq!(
            shape.get_start(),
            crate::loader::util::Vector3d::new(0.0, 40.0, 0.0)
        );
        assert_eq!(
            shape.get_end(),
            crate::loader::util::Vector3d::new(0.0, 40.0, 0.0)
        );
    }
}
