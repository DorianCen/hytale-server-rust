// Ported from: `.hytale-source/.../procedurallib/json/AbstractCellJitterJsonLoader.java`
//! Java `AbstractCellJitterJsonLoader` — the shared
//! `Jitter`/`JitterX`/`JitterY`/`JitterZ` loading helpers.
//!
//! Java is an abstract class sitting between `JsonLoader` and the
//! `PointEvaluator`/`MeshNoise`/`BranchNoise` loaders; the Rust port is a
//! trait with provided methods plus a [`Self::base`] accessor (Java
//! inheritance → composition, mirroring the other loader ports).

use crate::json::cell_noise_json_loader::constants;
use crate::json::JsonLoaderBase;
use crate::noise::cell::jitter::CellJitter;

/// Java `AbstractCellJitterJsonLoader`.
pub trait AbstractCellJitterJsonLoader {
    /// The composed loader state (Java `this` viewed as `JsonLoader`).
    fn base(&self) -> &JsonLoaderBase;

    /// Java `CellJitter loadJitter()`.
    fn load_jitter(&self) -> CellJitter {
        let default_jitter = self.load_default_jitter();
        let x = self.load_jitter_x(default_jitter);
        let y = self.load_jitter_y(default_jitter);
        let z = self.load_jitter_z(default_jitter);
        CellJitter::of(x, y, z)
    }

    /// Java `double loadDefaultJitter()`.
    fn load_default_jitter(&self) -> f64 {
        load_jitter(
            self.base(),
            constants::KEY_JITTER,
            constants::DEFAULT_JITTER,
        )
    }

    /// Java `double loadJitterX(double defaultJitter)`.
    fn load_jitter_x(&self, default_jitter: f64) -> f64 {
        load_jitter(self.base(), constants::KEY_JITTER_X, default_jitter)
    }

    /// Java `double loadJitterY(double defaultJitter)`.
    fn load_jitter_y(&self, default_jitter: f64) -> f64 {
        load_jitter(self.base(), constants::KEY_JITTER_Y, default_jitter)
    }

    /// Java `double loadJitterZ(double defaultJitter)`.
    fn load_jitter_z(&self, default_jitter: f64) -> f64 {
        load_jitter(self.base(), constants::KEY_JITTER_Z, default_jitter)
    }
}

/// Java `protected static double loadJitter(JsonLoader<?, ?> loader, String
/// key, double defaultJitter)` — Java overloads `loadJitter`; the static form
/// is a free function here.
pub fn load_jitter(loader: &JsonLoaderBase, key: &str, default_jitter: f64) -> f64 {
    if !loader.has(key) {
        return default_jitter;
    }

    JsonLoaderBase::get_as_double(&loader.get(key).unwrap())
}
