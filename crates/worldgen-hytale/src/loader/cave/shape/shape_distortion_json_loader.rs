// Ported from: `.hytale-source/.../worldgen/loader/cave/shape/ShapeDistortionJsonLoader.java`
//! Java `loader.cave.shape.ShapeDistortionJsonLoader` — the width / floor /
//! ceiling noises that distort a cave node shape.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::cave::shape::shape_distortion::ShapeDistortion;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::support::NoiseProperty;

/// Java `ShapeDistortionJsonLoader.Constants`.
pub mod constants {
    pub const KEY_WIDTH: &str = "Width";
    pub const KEY_FLOOR: &str = "Floor";
    pub const KEY_CEILING: &str = "Ceiling";
}

/// Java `loader.cave.shape.ShapeDistortionJsonLoader`.
pub struct ShapeDistortionJsonLoader {
    /// Java `extends JsonLoader<K, ShapeDistortion>`.
    pub base: JsonLoaderBase,
}

impl ShapeDistortionJsonLoader {
    /// Java `ShapeDistortionJsonLoader(SeedString<K> seed, Path dataFolder,
    /// JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".ShapeDistortion"), data_folder, json),
        }
    }

    /// Java `ShapeDistortion load()`.
    pub fn load(&self) -> ShapeDistortion {
        ShapeDistortion::of(self.load_width(), self.load_floor(), self.load_ceiling())
    }

    /// Java `NoiseProperty loadWidth()`.
    fn load_width(&self) -> Option<Rc<dyn NoiseProperty>> {
        self.load_noise(constants::KEY_WIDTH)
    }

    /// Java `NoiseProperty loadFloor()`.
    fn load_floor(&self) -> Option<Rc<dyn NoiseProperty>> {
        self.load_noise(constants::KEY_FLOOR)
    }

    /// Java `NoiseProperty loadCeiling()`.
    fn load_ceiling(&self) -> Option<Rc<dyn NoiseProperty>> {
        self.load_noise(constants::KEY_CEILING)
    }

    /// The shared body of the three noise getters.
    fn load_noise(&self, key: &str) -> Option<Rc<dyn NoiseProperty>> {
        if !self.base.has(key) {
            return None;
        }
        Some(
            NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(key),
            )
            .load(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Option<Value>) -> ShapeDistortionJsonLoader {
        ShapeDistortionJsonLoader::new(SeedString::new("0"), Path::new("."), json)
    }

    #[test]
    fn the_seed_appends_the_shape_distortion_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.ShapeDistortion");
    }

    #[test]
    fn an_absent_distortion_is_the_default_one() {
        let distortion = loader(None).load();
        assert_eq!(distortion.get_width_factor(0, 1.0, 2.0), 1.0);
        assert_eq!(distortion.get_floor_factor(0, 1.0, 2.0), 1.0);
        assert_eq!(distortion.get_ceiling_factor(0, 1.0, 2.0), 1.0);
    }

    #[test]
    fn a_width_noise_replaces_the_default_channel() {
        let distortion = loader(Some(serde_json::json!({
            "Width": {"NoiseType": "CONSTANT", "Value": 0.25}
        })))
        .load();
        // The constant noise is remapped from `[-1, 1]` to `[0, 1]`.
        assert!((distortion.get_width_factor(0, 0.0, 0.0) - 0.625).abs() < 1e-6);
        // The other channels keep the default.
        assert_eq!(distortion.get_floor_factor(0, 0.0, 0.0), 1.0);
        assert_eq!(distortion.get_ceiling_factor(0, 0.0, 0.0), 1.0);
    }
}
