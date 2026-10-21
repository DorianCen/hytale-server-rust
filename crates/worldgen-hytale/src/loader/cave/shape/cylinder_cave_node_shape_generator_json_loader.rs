// Ported from: `.hytale-source/.../worldgen/loader/cave/shape/CylinderCaveNodeShapeGeneratorJsonLoader.java`
//! Java `loader.cave.shape.CylinderCaveNodeShapeGeneratorJsonLoader`.
//!
//! The cylinder shares its configuration with the pipe (the Java loader
//! even imports the pipe's constants), so the two loaders behave the same.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::shape::cylinder_cave_node_shape::CylinderCaveNodeShapeGenerator;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::supplier::IDoubleRange;

/// Java `CylinderCaveNodeShapeGeneratorJsonLoader.Constants`.
pub mod constants {
    pub const KEY_RADIUS: &str = "Radius";
    pub const KEY_MIDDLE_RADIUS: &str = "MiddleRadius";
    pub const KEY_LENGTH: &str = "Length";
    pub const KEY_INHERIT_PARENT_RADIUS: &str = "InheritParentRadius";
}

/// Java `loader.cave.shape.CylinderCaveNodeShapeGeneratorJsonLoader`.
pub struct CylinderCaveNodeShapeGeneratorJsonLoader {
    /// Java `extends CaveNodeShapeGeneratorJsonLoader`.
    pub base: JsonLoaderBase,
}

impl CylinderCaveNodeShapeGeneratorJsonLoader {
    /// Java `CylinderCaveNodeShapeGeneratorJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(".CylinderCaveNodeShapeGenerator"),
                data_folder,
                json,
            ),
        }
    }

    /// Java `CylinderCaveNodeShape.CylinderCaveNodeShapeGenerator load()`.
    pub fn load(&self) -> CylinderCaveNodeShapeGenerator {
        CylinderCaveNodeShapeGenerator::new(
            self.load_radius(),
            self.load_middle_radius(),
            self.load_length(),
            self.load_inherit_parent_radius(),
        )
    }

    /// Java `IDoubleRange loadRadius()`.
    fn load_radius(&self) -> Rc<dyn IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_RADIUS),
                3.0,
            )
            .load(),
        )
    }

    /// Java `IDoubleRange loadMiddleRadius()`.
    fn load_middle_radius(&self) -> Option<Rc<dyn IDoubleRange>> {
        if !self.base.has(constants::KEY_MIDDLE_RADIUS) {
            return None;
        }
        Some(Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_MIDDLE_RADIUS),
                0.0,
            )
            .load(),
        ))
    }

    /// Java `IDoubleRange loadLength()`.
    fn load_length(&self) -> Rc<dyn IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1_and_default2(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_LENGTH),
                5.0,
                15.0,
            )
            .load(),
        )
    }

    /// Java `boolean loadInheritParentRadius()`.
    fn load_inherit_parent_radius(&self) -> bool {
        if !self.base.has(constants::KEY_INHERIT_PARENT_RADIUS) {
            return true;
        }
        JsonLoaderBase::get_as_bool(&self.base.get(constants::KEY_INHERIT_PARENT_RADIUS).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
    use crate::util::bounds::IChunkBounds;

    fn loader(json: Option<Value>) -> CylinderCaveNodeShapeGeneratorJsonLoader {
        CylinderCaveNodeShapeGeneratorJsonLoader::new(SeedString::new("0"), Path::new("."), json)
    }

    #[test]
    fn the_seed_appends_the_cylinder_shape_generator_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CylinderCaveNodeShapeGenerator");
    }

    #[test]
    fn the_defaults_are_the_java_ones() {
        let generator = loader(None).load();
        let mut random = hytale_math::fast_random::FastRandom::new(1);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &crate::cave::test_stubs::cave_type("cave"),
            None,
            Some(&crate::cave::test_stubs::child_entry(
                crate::loader::util::Vector3d::ZERO,
            )),
            crate::loader::util::Vector3d::new(0.0, 0.0, 0.0),
            0.0,
            0.0,
        );
        assert_eq!(shape.get_bounds().get_low_bound_x(), -3);
        assert_eq!(shape.get_bounds().get_high_bound_x(), 3);
    }
}
