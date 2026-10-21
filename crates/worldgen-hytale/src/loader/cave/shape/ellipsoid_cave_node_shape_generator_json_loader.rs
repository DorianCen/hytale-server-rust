// Ported from: `.hytale-source/.../worldgen/loader/cave/shape/EllipsoidCaveNodeShapeGeneratorJsonLoader.java`
//! Java `loader.cave.shape.EllipsoidCaveNodeShapeGeneratorJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::shape::ellipsoid_cave_node_shape::EllipsoidCaveNodeShapeGenerator;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::supplier::IDoubleRange;

/// Java `EllipsoidCaveNodeShapeGeneratorJsonLoader.Constants`.
pub mod constants {
    pub const KEY_RADIUS: &str = "Radius";
    pub const KEY_RADIUS_X: &str = "RadiusX";
    pub const KEY_RADIUS_Y: &str = "RadiusY";
    pub const KEY_RADIUS_Z: &str = "RadiusZ";

    pub const ERROR_RADIUS_NOT_SET: &str = "%s was not set for Ellipsoid!";
}

/// Java `loader.cave.shape.EllipsoidCaveNodeShapeGeneratorJsonLoader`.
pub struct EllipsoidCaveNodeShapeGeneratorJsonLoader {
    /// Java `extends CaveNodeShapeGeneratorJsonLoader`.
    pub base: JsonLoaderBase,
}

impl EllipsoidCaveNodeShapeGeneratorJsonLoader {
    /// Java `EllipsoidCaveNodeShapeGeneratorJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(".EllipsoidCaveNodeShapeGenerator"),
                data_folder,
                json,
            ),
        }
    }

    /// Java `EllipsoidCaveNodeShape.EllipsoidCaveNodeShapeGenerator load()`.
    pub fn load(&self) -> EllipsoidCaveNodeShapeGenerator {
        let mut radius_x: Option<Rc<dyn IDoubleRange>> = None;
        let mut radius_y: Option<Rc<dyn IDoubleRange>> = None;
        let mut radius_z: Option<Rc<dyn IDoubleRange>> = None;

        if self.base.has(constants::KEY_RADIUS) {
            let radius = self.load_radius(constants::KEY_RADIUS);
            radius_x = Some(radius.clone());
            radius_y = Some(radius.clone());
            radius_z = Some(radius);
        }
        if self.base.has(constants::KEY_RADIUS_X) {
            radius_x = Some(self.load_radius(constants::KEY_RADIUS_X));
        }
        if self.base.has(constants::KEY_RADIUS_Y) {
            radius_y = Some(self.load_radius(constants::KEY_RADIUS_Y));
        }
        if self.base.has(constants::KEY_RADIUS_Z) {
            radius_z = Some(self.load_radius(constants::KEY_RADIUS_Z));
        }

        EllipsoidCaveNodeShapeGenerator::new(
            radius_x.unwrap_or_else(|| {
                panic!(
                    "{}",
                    constants::ERROR_RADIUS_NOT_SET.replace("%s", constants::KEY_RADIUS_X)
                )
            }),
            radius_y.unwrap_or_else(|| {
                panic!(
                    "{}",
                    constants::ERROR_RADIUS_NOT_SET.replace("%s", constants::KEY_RADIUS_Y)
                )
            }),
            radius_z.unwrap_or_else(|| {
                panic!(
                    "{}",
                    constants::ERROR_RADIUS_NOT_SET.replace("%s", constants::KEY_RADIUS_Z)
                )
            }),
        )
    }

    /// Java `new DoubleRangeJsonLoader<>(seed, dataFolder, get(KEY), 5).load()`
    /// — the shared body of the four radius getters.
    fn load_radius(&self, key: &str) -> Rc<dyn IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(key),
                5.0,
            )
            .load(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
    use crate::util::bounds::IChunkBounds;

    fn loader(json: Option<Value>) -> EllipsoidCaveNodeShapeGeneratorJsonLoader {
        EllipsoidCaveNodeShapeGeneratorJsonLoader::new(SeedString::new("0"), Path::new("."), json)
    }

    #[test]
    fn the_seed_appends_the_ellipsoid_shape_generator_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.EllipsoidCaveNodeShapeGenerator");
    }

    #[test]
    #[should_panic(expected = "RadiusX was not set for Ellipsoid!")]
    fn a_missing_radius_x_panics_verbatim() {
        loader(None).load();
    }

    #[test]
    fn the_shared_radius_is_used_for_all_three_axes() {
        let generator = loader(Some(serde_json::json!({"Radius": 6.0}))).load();
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
        assert_eq!(shape.get_bounds().get_low_bound_x(), -6);
        assert_eq!(shape.get_bounds().get_high_bound_z(), 6);
    }
}
