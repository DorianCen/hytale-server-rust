// Ported from: `.hytale-source/.../worldgen/loader/cave/shape/DistortedCaveNodeShapeGeneratorJsonLoader.java`
//! Java `loader.cave.shape.DistortedCaveNodeShapeGeneratorJsonLoader`.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::shape::distorted::distorted_shapes::DistortedShapeFactory;
use crate::cave::shape::distorted_cave_node_shape::DistortedCaveNodeShapeGenerator;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::supplier::IDoubleRange;
use procedural::noise::InterpolationMode;

/// Java `DistortedCaveNodeShapeGeneratorJsonLoader.Constants`.
pub mod constants {
    pub const KEY_SHAPE: &str = "Shape";
    pub const KEY_WIDTH: &str = "Width";
    pub const KEY_HEIGHT: &str = "Height";
    pub const KEY_MID_WIDTH: &str = "MiddleWidth";
    pub const KEY_MID_HEIGHT: &str = "MiddleHeight";
    pub const KEY_LENGTH: &str = "Length";
    pub const KEY_RADIUS_X: &str = "RadiusX";
    pub const KEY_RADIUS_Y: &str = "RadiusY";
    pub const KEY_RADIUS_Z: &str = "RadiusZ";
    pub const KEY_INHERIT_PARENT_RADIUS: &str = "InheritParentRadius";
    pub const KEY_DISTORTION: &str = "Distortion";
    pub const KEY_INTERPOLATION: &str = "Interpolation";
}

/// Java `loader.cave.shape.DistortedCaveNodeShapeGeneratorJsonLoader`.
pub struct DistortedCaveNodeShapeGeneratorJsonLoader {
    /// Java `extends CaveNodeShapeGeneratorJsonLoader`.
    pub base: JsonLoaderBase,
}

impl DistortedCaveNodeShapeGeneratorJsonLoader {
    /// Java `DistortedCaveNodeShapeGeneratorJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".DistortedCaveNodeShape"), data_folder, json),
        }
    }

    /// Java `CaveNodeShapeEnum.CaveNodeShapeGenerator load()`.
    pub fn load(&self) -> DistortedCaveNodeShapeGenerator {
        DistortedCaveNodeShapeGenerator::new(
            self.load_shape(),
            self.load_width(),
            self.load_height(),
            self.load_mid_width(),
            self.load_mid_height(),
            self.load_length(),
            self.load_inherit_parent_radius(),
            self.load_shape_distortion(),
            Some(self.load_interpolation()),
        )
    }

    /// Java `DistortedShape.Factory loadShape()`.
    fn load_shape(&self) -> &'static dyn DistortedShapeFactory {
        if self.base.has(constants::KEY_SHAPE) {
            let name = JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_SHAPE).unwrap());
            if let Some(shape) =
                crate::cave::shape::distorted::distorted_shapes::DistortedShapes::get_by_name(&name)
            {
                return shape;
            }
        }
        // Java `Constants.DEFAULT_SHAPE` — the pipe.
        crate::cave::shape::distorted::distorted_shapes::DistortedShapes::get_default()
    }

    /// Java `IDoubleRange loadWidth()`.
    fn load_width(&self) -> Rc<dyn IDoubleRange> {
        if self.base.has(constants::KEY_RADIUS_X) {
            return self.load_range(constants::KEY_RADIUS_X, 3.0, 3.0);
        }
        self.load_range(constants::KEY_WIDTH, 3.0, 3.0)
    }

    /// Java `IDoubleRange loadHeight()`.
    fn load_height(&self) -> Rc<dyn IDoubleRange> {
        if self.base.has(constants::KEY_RADIUS_Y) {
            return self.load_range(constants::KEY_RADIUS_Y, 3.0, 3.0);
        }
        self.load_range(constants::KEY_HEIGHT, 3.0, 3.0)
    }

    /// Java `IDoubleRange loadMidWidth()`.
    fn load_mid_width(&self) -> Option<Rc<dyn IDoubleRange>> {
        if !self.base.has(constants::KEY_MID_WIDTH) {
            return None;
        }
        Some(self.load_range(constants::KEY_MID_WIDTH, 0.0, 0.0))
    }

    /// Java `IDoubleRange loadMidHeight()`.
    fn load_mid_height(&self) -> Option<Rc<dyn IDoubleRange>> {
        if !self.base.has(constants::KEY_MID_HEIGHT) {
            return None;
        }
        Some(self.load_range(constants::KEY_MID_HEIGHT, 0.0, 0.0))
    }

    /// Java `IDoubleRange loadLength()`.
    fn load_length(&self) -> Option<Rc<dyn IDoubleRange>> {
        if self.base.has(constants::KEY_RADIUS_Z) {
            return Some(self.load_range(constants::KEY_RADIUS_Z, 3.0, 3.0));
        }
        if self.base.has(constants::KEY_LENGTH) {
            return Some(self.load_range(constants::KEY_LENGTH, 5.0, 15.0));
        }
        None
    }

    /// The shared body of the range getters.
    fn load_range(&self, key: &str, default1: f64, default2: f64) -> Rc<dyn IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1_and_default2(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(key),
                default1,
                default2,
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

    /// Java `ShapeDistortion loadShapeDistortion()`.
    fn load_shape_distortion(&self) -> ShapeDistortion {
        if !self.base.has(constants::KEY_DISTORTION) {
            return ShapeDistortion::default();
        }
        super::shape_distortion_json_loader::ShapeDistortionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_DISTORTION),
        )
        .load()
    }

    /// Java `GeneralNoise.InterpolationFunction loadInterpolation()`.
    fn load_interpolation(&self) -> InterpolationMode {
        if !self.base.has(constants::KEY_INTERPOLATION) {
            // Java `Constants.DEFAULT_INTERPOLATION` — the linear function.
            return InterpolationMode::Linear;
        }
        let name =
            JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_INTERPOLATION).unwrap());
        match name.as_str() {
            "LINEAR" => InterpolationMode::Linear,
            "HERMITE" => InterpolationMode::Hermite,
            "QUINTIC" => InterpolationMode::Quintic,
            other => panic!(
                "No enum constant com.hypixel.hytale.procedurallib.logic.GeneralNoise.InterpolationMode.{other}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
    use crate::cave::shape::distorted::distorted_shapes::DistortedShapes;
    use crate::util::bounds::IWorldBounds;

    fn loader(json: Option<Value>) -> DistortedCaveNodeShapeGeneratorJsonLoader {
        DistortedCaveNodeShapeGeneratorJsonLoader::new(SeedString::new("0"), Path::new("."), json)
    }

    #[test]
    fn the_seed_appends_the_distorted_shape_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.DistortedCaveNodeShape");
    }

    #[test]
    fn the_default_shape_is_the_pipe() {
        let generator = loader(Some(serde_json::json!({"Length": [10.0]}))).load();
        // The pipe is the default factory.
        let mut random = hytale_math::fast_random::FastRandom::new(1);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &crate::cave::test_stubs::cave_type("cave"),
            None,
            Some(&crate::cave::test_stubs::child_entry(
                crate::loader::util::Vector3d::ZERO,
            )),
            crate::loader::util::Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            0.0,
        );
        assert!(shape.get_bounds().get_high_bound_y() >= 50);
    }

    #[test]
    fn the_shape_name_selects_the_factory() {
        let generator = loader(Some(serde_json::json!({"Shape": "CYLINDER"}))).load();
        let mut random = hytale_math::fast_random::FastRandom::new(1);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &crate::cave::test_stubs::cave_type("cave"),
            None,
            Some(&crate::cave::test_stubs::child_entry(
                crate::loader::util::Vector3d::ZERO,
            )),
            crate::loader::util::Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            0.0,
        );
        // The cylinder has no length by default -> a zero length shape.
        assert_eq!(shape.get_start(), shape.get_end());
    }

    #[test]
    fn an_unknown_shape_name_falls_back_to_the_default() {
        // The Java loader falls back to the default shape for an unknown
        // name (it does not fail).
        let generator = loader(Some(
            serde_json::json!({"Shape": "SPHERE", "Length": [10.0]}),
        ))
        .load();
        let mut random = hytale_math::fast_random::FastRandom::new(1);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &crate::cave::test_stubs::cave_type("cave"),
            None,
            Some(&crate::cave::test_stubs::child_entry(
                crate::loader::util::Vector3d::ZERO,
            )),
            crate::loader::util::Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            0.0,
        );
        assert!(shape.get_bounds().get_high_bound_y() >= 50);
    }

    #[test]
    fn the_default_factory_is_the_pipe() {
        // The registry knows the three built-in names; the default is the
        // pipe (see the `distorted_shapes` module test for the identity
        // comparison).
        assert!(DistortedShapes::get_by_name("PIPE").is_some());
        assert!(DistortedShapes::get_by_name("CYLINDER").is_some());
        assert!(DistortedShapes::get_by_name("ELLIPSOID").is_some());
        assert!(DistortedShapes::get_by_name("SPHERE").is_none());
    }

    #[test]
    fn an_unknown_interpolation_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({"Interpolation": "SMOOTH"}))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "No enum constant com.hypixel.hytale.procedurallib.logic.GeneralNoise.InterpolationMode.SMOOTH"
        );
    }
}
