// Ported from: `.hytale-source/.../procedurallib/json/PointGeneratorJsonLoader.java`
//! Java `PointGeneratorJsonLoader`.
//!
//! ⚠ NOTE: Java builds a `SeedResourcePointGenerator` (a `PointGenerator`
//! subclass that only reroutes the thread-local scratch buffers and bounds to
//! the `SeedResource`); the Rust port allocates per-call buffers, so the
//! plain [`PointGenerator`] is the faithful equivalent. The
//! `seed.get().reportSeeds(...)` calls are not reproduced (seed reporting is
//! a no-op logging hook; see [`crate::json::seed_string`]).

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::json::cell_distance_function_json_loader::CellDistanceFunctionJsonLoader;
use crate::json::cell_noise_json_loader::constants as cell_noise;
use crate::json::coordinate_randomizer_json_loader::CoordinateRandomizerJsonLoader;
use crate::json::coordinate_rotator_json_loader::{CoordinateRotatorJsonLoader, LoadedRotator};
use crate::json::point_evaluator_json_loader::PointEvaluatorJsonLoader;
use crate::json::seed_string::SeedString;
use crate::json::JsonLoaderBase;
use crate::noise::cell::cell_distance_function::CellDistanceFunction;
use crate::noise::cell::evaluator::point_evaluator::PointEvaluator;
use crate::noise::cell::point_distance_function::PointDistanceFunction;
use crate::noise::point::{
    DistortedPointGenerator, IPointGenerator, OffsetPointGenerator, PointGenerator,
    ScaledPointGenerator,
};
use crate::noise::random::ICoordinateRandomizer;

/// Java `PointGeneratorJsonLoader.Constants`.
mod constants {
    pub const KEY_SEED: &str = "Seed";
    pub const KEY_SCALE: &str = "Scale";
    pub const KEY_RANDOMIZER: &str = "Randomizer";
    pub const KEY_OFFSET: &str = "Offset";
    pub const KEY_OFFSET_X: &str = "OffsetX";
    pub const KEY_OFFSET_Y: &str = "OffsetY";
    pub const KEY_OFFSET_Z: &str = "OffsetZ";
}

/// Java `CoordinateRotatorJsonLoader.Constants.KEY_ROTATE` (static-imported).
const KEY_ROTATE: &str = "Rotate";

/// Java `PointGeneratorJsonLoader`.
pub struct PointGeneratorJsonLoader {
    /// Java `extends JsonLoader<K, IPointGenerator>` — note: the seed is
    /// passed through **without** a suffix append.
    pub base: JsonLoaderBase,
}

impl PointGeneratorJsonLoader {
    /// Java `PointGeneratorJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json)`.
    pub fn new(seed: SeedString, data_folder: &Path, json: Option<Value>) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
        }
    }

    /// Java `IPointGenerator load()`.
    pub fn load(&self) -> Rc<dyn IPointGenerator> {
        let point_generator =
            self.new_point_generator(self.load_seed(), self.load_cell_distance_function());
        let mut generator: Rc<dyn IPointGenerator> = if self.base.has(constants::KEY_SCALE) {
            Rc::new(ScaledPointGenerator::new(
                point_generator,
                JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_SCALE).unwrap()),
            ))
        } else {
            Rc::new(point_generator)
        };
        if self.base.has(constants::KEY_RANDOMIZER) {
            generator = Rc::new(DistortedPointGenerator::new(
                generator,
                CoordinateRandomizerJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_RANDOMIZER),
                )
                .load(),
            ));
        }
        let mut offset_x = f64::NEG_INFINITY;
        let mut offset_y = f64::NEG_INFINITY;
        let mut offset_z = f64::NEG_INFINITY;
        if self.base.has(constants::KEY_OFFSET) {
            offset_x =
                JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_OFFSET).unwrap());
            offset_y = offset_x;
            offset_z = offset_x;
        }
        if self.base.has(constants::KEY_OFFSET_X) {
            offset_x =
                JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_OFFSET_X).unwrap());
        }
        if self.base.has(constants::KEY_OFFSET_Y) {
            offset_y =
                JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_OFFSET_Y).unwrap());
        }
        if self.base.has(constants::KEY_OFFSET_Z) {
            offset_z =
                JsonLoaderBase::get_as_double(&self.base.get(constants::KEY_OFFSET_Z).unwrap());
        }
        if offset_x != f64::NEG_INFINITY
            || offset_y != f64::NEG_INFINITY
            || offset_z != f64::NEG_INFINITY
        {
            if offset_x == f64::NEG_INFINITY {
                offset_x = 0.0;
            }
            if offset_y == f64::NEG_INFINITY {
                offset_y = 0.0;
            }
            if offset_z == f64::NEG_INFINITY {
                offset_z = 0.0;
            }
            generator = Rc::new(OffsetPointGenerator::new(
                generator, offset_x, offset_y, offset_z,
            ));
        }
        if self.base.has(KEY_ROTATE) {
            let rotation = CoordinateRotatorJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(KEY_ROTATE),
            )
            .load();

            // Java: `if (rotation != CoordinateRotator.NONE)`. Unlike the
            // noise-property rotate path, the randomizer consumers call the
            // virtual `randomDoubleX/...` methods, so an origin rotator's
            // translation IS applied here.
            if !matches!(rotation, LoadedRotator::None) {
                let rotation: Rc<dyn ICoordinateRandomizer> = match rotation {
                    LoadedRotator::Plain(rotator) => Rc::new(rotator),
                    LoadedRotator::Origin(rotator) => Rc::new(rotator),
                    LoadedRotator::None => unreachable!("guarded by the matches! check above"),
                };
                generator = Rc::new(DistortedPointGenerator::new(generator, rotation));
            }
        }
        generator
    }

    /// Java `int loadSeed()` — the `Seed` keyword overrides the seed-string
    /// tree with `appendToOriginal`. (Java additionally reports the derived
    /// seeds through `seed.get().reportSeeds(...)`; that logging hook is not
    /// reproduced — see [`crate::json::seed_string`].)
    pub fn load_seed(&self) -> i32 {
        let mut seed_val = self.base.seed.hash_code();
        if self.base.has(constants::KEY_SEED) {
            let overwritten = self
                .base
                .seed
                .append_to_original(&JsonLoaderBase::get_as_string(
                    &self.base.get(constants::KEY_SEED).unwrap(),
                ));
            seed_val = overwritten.hash_code();
        }
        seed_val
    }

    /// Java `PointGenerator newPointGenerator(int seedOffset,
    /// CellDistanceFunction cellDistanceFunction)` (Java constructs the
    /// `SeedResourcePointGenerator` subclass — see the module note).
    pub fn new_point_generator(
        &self,
        seed_offset: i32,
        cell_distance_function: Rc<dyn CellDistanceFunction>,
    ) -> PointGenerator {
        let point_evaluator = self.load_point_evaluator();
        PointGenerator::new(seed_offset, cell_distance_function, point_evaluator)
    }

    /// Java `@Nullable CellDistanceFunction loadCellDistanceFunction()`
    /// (never `null` in practice).
    pub fn load_cell_distance_function(&self) -> Rc<dyn CellDistanceFunction> {
        CellDistanceFunctionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
            Some(self.load_point_distance_function()),
        )
        .load()
    }

    /// Java `@Nullable PointEvaluator loadPointEvaluator()` (never `null` in
    /// practice).
    pub fn load_point_evaluator(&self) -> Rc<dyn PointEvaluator> {
        PointEvaluatorJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
        )
        .load()
    }

    /// Java `PointDistanceFunction loadPointDistanceFunction()`.
    pub fn load_point_distance_function(&self) -> Rc<dyn PointDistanceFunction> {
        let mut distance_calculation_mode = cell_noise::DEFAULT_DISTANCE_MODE;
        if self.base.has(cell_noise::KEY_DISTANCE_MODE) {
            distance_calculation_mode =
                PointEvaluatorJsonLoader::distance_calculation_mode_value_of(
                    &JsonLoaderBase::get_as_string(
                        &self.base.get(cell_noise::KEY_DISTANCE_MODE).unwrap(),
                    ),
                );
        }
        Rc::new(distance_calculation_mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loader(json: Value) -> PointGeneratorJsonLoader {
        PointGeneratorJsonLoader::new(
            SeedString::new("test"),
            std::path::Path::new("."),
            Some(json),
        )
    }

    #[test]
    fn load_seed_uses_the_seed_override() {
        let plain = loader(serde_json::json!({}));
        let overridden = loader(serde_json::json!({ "Seed": "MySeed" }));

        assert_eq!(plain.load_seed(), plain.base.seed.hash_code());
        assert_eq!(
            overridden.load_seed(),
            plain.base.seed.append_to_original("MySeed").hash_code()
        );
        assert_ne!(plain.load_seed(), overridden.load_seed());
    }

    #[test]
    fn loads_a_deterministic_generator() {
        let l = loader(serde_json::json!({
            "CellType": "HEX",
            "DistanceMode": "NATURAL",
            "Jitter": 0.7,
            "Scale": 2.0,
            "Offset": 5.0,
            "Randomizer": { "Generators": [ { "NoiseType": "CONSTANT", "Amplitude": 0.25 } ] }
        }));
        let generator = l.load();

        let a = generator.nearest_2d(11, 1.5, -2.25);
        let b = generator.nearest_2d(11, 1.5, -2.25);
        assert_eq!(a.distance.to_bits(), b.distance.to_bits());
        assert!(a.distance.is_finite());

        let transition = generator.transition_2d(11, 1.5, -2.25);
        assert!(transition.distance <= transition.distance2);
    }

    #[test]
    fn scale_and_offset_are_applied() {
        // A scale of 2 halves the reported interval.
        let scaled = loader(serde_json::json!({ "Scale": 2.0 })).load();
        assert_eq!(IPointGenerator::get_interval(scaled.as_ref()), 0.5);

        // A plain load reports the unit interval.
        let plain = loader(serde_json::json!({})).load();
        assert_eq!(IPointGenerator::get_interval(plain.as_ref()), 1.0);

        // The offset decorator shifts the query: nearest(x) == nearest_plain(x + offset).
        let base_generator = loader(serde_json::json!({})).load();
        let offset = loader(serde_json::json!({ "OffsetX": 10.0, "OffsetZ": 3.0 })).load();
        let shifted = offset.nearest_2d(42, 1.5, -2.25);
        let direct = base_generator.nearest_2d(42, 11.5, -2.25);
        assert_eq!(shifted.distance.to_bits(), direct.distance.to_bits());
    }
}
