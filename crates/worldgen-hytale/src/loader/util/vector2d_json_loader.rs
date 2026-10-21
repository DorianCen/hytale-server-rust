// Ported from: `.hytale-source/.../worldgen/loader/util/Vector2dJsonLoader.java`
//! Java `loader.util.Vector2dJsonLoader` — note that an invalid (primitive)
//! definition returns the zero vector instead of throwing.

use std::path::Path;

use serde_json::Value;

use crate::loader::util::Vector2d;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `Vector2dJsonLoader.Constants`.
pub mod constants {
    pub const KEY_X: &str = "X";
    pub const KEY_Y: &str = "Y";
}

/// Java `Vector2dJsonLoader`.
pub struct Vector2dJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, Vector2d>`.
    pub base: JsonLoaderBase,
}

impl Vector2dJsonLoader {
    /// Java `Vector2dJsonLoader(SeedString seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `Vector2d load()`.
    pub fn load(&self) -> Vector2d {
        let Some(json) = self.base.json.as_ref() else {
            return Vector2d::ZERO;
        };
        if json.is_null() {
            return Vector2d::ZERO;
        }
        if let Some(array) = json.as_array() {
            return match array.len() {
                0 => Vector2d::ZERO,
                1 => {
                    let value = JsonLoaderBase::get_as_double(&array[0]);
                    Vector2d::new(value, value)
                }
                _ => Vector2d::new(
                    JsonLoaderBase::get_as_double(&array[0]),
                    JsonLoaderBase::get_as_double(&array[1]),
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
            return Vector2d::new(x, y);
        }
        Vector2d::ZERO
    }
}
