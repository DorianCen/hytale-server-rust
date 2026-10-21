// Ported from: `.hytale-source/.../worldgen/loader/util/Vector3dJsonLoader.java`
//! Java `loader.util.Vector3dJsonLoader` — an invalid (primitive) definition
//! throws `Error("No valid definition for Vector3d found!")`.

use std::path::Path;

use serde_json::Value;

use crate::loader::util::Vector3d;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `Vector3dJsonLoader.Constants`.
pub mod constants {
    pub const KEY_X: &str = "X";
    pub const KEY_Y: &str = "Y";
    pub const KEY_Z: &str = "Z";
}

/// Java `Vector3dJsonLoader`.
pub struct Vector3dJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, Vector3d>`.
    pub base: JsonLoaderBase,
}

impl Vector3dJsonLoader {
    /// Java `Vector3dJsonLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `Vector3d load()`.
    pub fn load(&self) -> Vector3d {
        let Some(json) = self.base.json.as_ref() else {
            return Vector3d::ZERO;
        };
        if json.is_null() {
            return Vector3d::ZERO;
        }
        if let Some(array) = json.as_array() {
            return match array.len() {
                0 => Vector3d::ZERO,
                1 => {
                    let value = JsonLoaderBase::get_as_double(&array[0]);
                    Vector3d::new(value, value, value)
                }
                _ => Vector3d::new(
                    JsonLoaderBase::get_as_double(&array[0]),
                    JsonLoaderBase::get_as_double(&array[1]),
                    JsonLoaderBase::get_as_double(&array[2]),
                ),
            };
        }
        if let Some(object) = json.as_object() {
            let x = JsonLoaderBase::get_as_double(
                object
                    .get(constants::KEY_X)
                    .unwrap_or_else(|| panic!("Missing property '{}'", constants::KEY_X)),
            );
            let y = JsonLoaderBase::get_as_double(
                object
                    .get(constants::KEY_Y)
                    .unwrap_or_else(|| panic!("Missing property '{}'", constants::KEY_Y)),
            );
            let z = JsonLoaderBase::get_as_double(
                object
                    .get(constants::KEY_Z)
                    .unwrap_or_else(|| panic!("Missing property '{}'", constants::KEY_Z)),
            );
            return Vector3d::new(x, y, z);
        }
        panic!("No valid definition for Vector3d found!");
    }
}
