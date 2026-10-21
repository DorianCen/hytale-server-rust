// Ported from: `.hytale-source/.../procedurallib/json/CoordinateRotatorJsonLoader.java`
//! Java `CoordinateRotatorJsonLoader`.
//!
//! Java returns a `CoordinateRotator` that may be a `CoordinateOriginRotator`
//! subclass instance. Both consumers (`RotatedCoordinateRandomizer` and
//! `RotateNoiseProperty`) only call the **concrete** `rotateX/rotateY/rotateZ`
//! matrix methods on it, so an origin rotator's translation is effectively
//! dropped at those call sites (Java quirk). The Rust port makes that explicit:
//! [`LoadedRotator`] keeps the loaded kind, and `into_matrix_rotator` collapses
//! it to the plain matrix rotator those call sites observe.

use std::path::Path;

use serde_json::Value;

use hytale_math::trig_math_util::DEG_TO_RAD;

use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::random::{CoordinateOriginRotator, CoordinateRotator};

/// The load result — Java's polymorphic `CoordinateRotator` return value.
pub enum LoadedRotator {
    /// Java `CoordinateRotator.NONE`.
    None,
    /// Java a plain `CoordinateRotator`.
    Plain(CoordinateRotator),
    /// Java a `CoordinateOriginRotator` (subclass instance).
    Origin(CoordinateOriginRotator),
}

impl LoadedRotator {
    /// The rotator as the consumers observe it: the plain matrix (Java quirk —
    /// the origin translation is dropped by the concrete `rotateX/...` calls).
    pub fn into_matrix_rotator(self) -> CoordinateRotator {
        match self {
            Self::None => CoordinateRotator::none(),
            Self::Plain(rotator) => rotator,
            Self::Origin(rotator) => rotator.base,
        }
    }
}

/// Java `CoordinateRotatorJsonLoader`.
pub struct CoordinateRotatorJsonLoader {
    /// Java `extends JsonLoader<K, CoordinateRotator>`.
    pub base: JsonLoaderBase,
}

impl CoordinateRotatorJsonLoader {
    /// Java `CoordinateRotatorJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `CoordinateRotator load()`.
    pub fn load(&self) -> LoadedRotator {
        const KEY_PITCH: &str = "Pitch";
        const KEY_YAW: &str = "Yaw";
        const KEY_ORIGIN_X: &str = "OriginX";
        const KEY_ORIGIN_Y: &str = "OriginY";
        const KEY_ORIGIN_Z: &str = "OriginZ";

        let pitch = if self.base.has(KEY_PITCH) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_PITCH).unwrap()) * DEG_TO_RAD as f64
        } else {
            0.0
        };
        let yaw = if self.base.has(KEY_YAW) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_YAW).unwrap()) * DEG_TO_RAD as f64
        } else {
            0.0
        };

        if pitch == 0.0 && yaw == 0.0 {
            return LoadedRotator::None;
        }

        let origin_x = if self.base.has(KEY_ORIGIN_X) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_ORIGIN_X).unwrap())
        } else {
            0.0
        };
        let origin_y = if self.base.has(KEY_ORIGIN_Y) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_ORIGIN_Y).unwrap())
        } else {
            0.0
        };
        let origin_z = if self.base.has(KEY_ORIGIN_Z) {
            JsonLoaderBase::get_as_double(&self.base.get(KEY_ORIGIN_Z).unwrap())
        } else {
            0.0
        };

        if origin_x == 0.0 && origin_y == 0.0 && origin_z == 0.0 {
            return LoadedRotator::Plain(CoordinateRotator::new(pitch, yaw));
        }

        LoadedRotator::Origin(CoordinateOriginRotator::new(
            pitch, yaw, origin_x, origin_y, origin_z,
        ))
    }
}
