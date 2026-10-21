// Ported from: `.hytale-source/.../worldgen/loader/biome/BiomePatternGeneratorJsonLoader.java`
//! Java `loader.biome.BiomePatternGeneratorJsonLoader`.
//!
//! The Java loader wraps the point generator's `PointDistanceFunction` in a
//! `LoadedPointGeneratorDistanceFunction` that multiplies the 2d cell distance
//! by the *parent tile biome's* `sizeModifier`. That wrapper needs the
//! (not-yet-built) `BiomePatternGenerator`, so the Java code builds the
//! generator first and then back-fills the provider. The Rust port keeps the
//! same two-phase construction with an `Rc<RefCell<Option<Rc<BiomePatternGenerator>>>>`
//! provider.
//!
//! ⚠ The resulting object graph has an `Rc` cycle
//! (`generator → point_generator → distance_function → provider → generator`).
//! Java's GC collects it; Rust leaks it for the lifetime of the loaded world
//! (the same acceptable pattern as the loader-context `Rc` cycles).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::biome::biome_pattern_generator::BiomePatternGenerator;
use crate::biome::custom_biome::CustomBiome;
use crate::biome::tile_biome::TileBiome;
use crate::weighted_map::IWeightedMap;
use procedural::json::cell_distance_function_json_loader::CellDistanceFunctionJsonLoader;
use procedural::json::coordinate_randomizer_json_loader::CoordinateRandomizerJsonLoader;
use procedural::json::coordinate_rotator_json_loader::{
    CoordinateRotatorJsonLoader, LoadedRotator,
};
use procedural::json::point_generator_json_loader::PointGeneratorJsonLoader;
use procedural::json::JsonLoaderBase;
use procedural::json::SeedString;
use procedural::noise::cell::point_distance_function::PointDistanceFunction;
use procedural::noise::point::{
    DistortedPointGenerator, IPointGenerator, OffsetPointGenerator, ScaledPointGenerator,
};
use procedural::noise::random::ICoordinateRandomizer;

/// Java `BiomePatternGeneratorJsonLoader.Constants.KEY_GRID_GENERATOR`.
pub const KEY_GRID_GENERATOR: &str = "GridGenerator";

/// Java `PointGeneratorJsonLoader.Constants` (mirrored).
#[allow(dead_code)] // kept for parity with Java Constants (used by a later slice)
const KEY_SEED: &str = "Seed";
const KEY_SCALE: &str = "Scale";
const KEY_RANDOMIZER: &str = "Randomizer";
const KEY_OFFSET: &str = "Offset";
const KEY_OFFSET_X: &str = "OffsetX";
const KEY_OFFSET_Y: &str = "OffsetY";
const KEY_OFFSET_Z: &str = "OffsetZ";

/// Java `CoordinateRotatorJsonLoader.Constants.KEY_ROTATE`.
const KEY_ROTATE: &str = "Rotate";

/// Java `BiomePatternGeneratorJsonLoader`.
pub struct BiomePatternGeneratorJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, BiomePatternGenerator>`.
    pub base: JsonLoaderBase,
    /// Java `protected final IWeightedMap<TileBiome> tileBiomes`.
    pub tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>>,
    /// Java `protected final CustomBiome[] customBiomes`.
    pub custom_biomes: Vec<Rc<CustomBiome>>,
}

impl BiomePatternGeneratorJsonLoader {
    /// Java `BiomePatternGeneratorJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, IWeightedMap<TileBiome> tileBiomes, CustomBiome[] customBiomes)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>>,
        custom_biomes: Vec<Rc<CustomBiome>>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".BiomePatternGenerator"), data_folder, json),
            tile_biomes,
            custom_biomes,
        }
    }

    /// Java `BiomePatternGenerator load()`.
    pub fn load(&self) -> Rc<BiomePatternGenerator> {
        let size_modifier_provider = Rc::new(BiomePatternGeneratorSizeModifierProvider::new());
        let point_generator = self.load_point_generator(Rc::clone(&size_modifier_provider));
        let generator = Rc::new(BiomePatternGenerator::new(
            point_generator,
            self.tile_biomes.clone(),
            self.custom_biomes.clone(),
        ));
        size_modifier_provider.set_generator(generator.clone());
        generator
    }

    /// Java `@Nullable IPointGenerator loadPointGenerator(ISizeModifierProvider sizeModifierProvider)`.
    pub fn load_point_generator(
        &self,
        size_modifier_provider: Rc<BiomePatternGeneratorSizeModifierProvider>,
    ) -> Rc<dyn IPointGenerator> {
        // Java builds the point generator from the `GridGenerator` CHILD of
        // the `BiomeGenerator` element (`get(KEY_GRID_GENERATOR)`), not from
        // the whole element.
        let grid_generator = self.base.get(KEY_GRID_GENERATOR);
        let base_distance = PointGeneratorJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            grid_generator.clone(),
        )
        .load_point_distance_function();
        let distance_function = Rc::new(LoadedPointGeneratorDistanceFunction::new(
            size_modifier_provider,
            base_distance,
        ));
        build_point_generator(
            self.base.seed.clone(),
            &self.base.data_folder,
            grid_generator,
            distance_function,
        )
    }
}

/// Builds the point generator with the given (wrapped) point distance
/// function, mirroring `PointGeneratorJsonLoader.load()`'s decoration chain.
fn build_point_generator(
    seed: SeedString,
    data_folder: &Path,
    json: Option<Value>,
    distance_function: Rc<dyn PointDistanceFunction>,
) -> Rc<dyn IPointGenerator> {
    let has = |key: &str| json.as_ref().is_some_and(|j| j.get(key).is_some());

    let loader = PointGeneratorJsonLoader::new(seed.clone(), data_folder, json.clone());
    let seed_offset = loader.load_seed();
    let cell_distance_function = CellDistanceFunctionJsonLoader::new(
        seed.clone(),
        data_folder,
        json.clone(),
        Some(distance_function),
    )
    .load();
    let point_generator = loader.new_point_generator(seed_offset, cell_distance_function);

    let mut generator: Rc<dyn IPointGenerator> = if has(KEY_SCALE) {
        Rc::new(ScaledPointGenerator::new(
            point_generator,
            JsonLoaderBase::get_as_double(json.as_ref().unwrap().get(KEY_SCALE).unwrap()),
        ))
    } else {
        Rc::new(point_generator)
    };
    if has(KEY_RANDOMIZER) {
        generator = Rc::new(DistortedPointGenerator::new(
            generator,
            CoordinateRandomizerJsonLoader::new(
                seed.clone(),
                data_folder,
                json.as_ref().unwrap().get(KEY_RANDOMIZER).cloned(),
            )
            .load(),
        ));
    }
    let mut offset_x = f64::NEG_INFINITY;
    let mut offset_y = f64::NEG_INFINITY;
    let mut offset_z = f64::NEG_INFINITY;
    if has(KEY_OFFSET) {
        offset_x = JsonLoaderBase::get_as_double(json.as_ref().unwrap().get(KEY_OFFSET).unwrap());
        offset_y = offset_x;
        offset_z = offset_x;
    }
    if has(KEY_OFFSET_X) {
        offset_x = JsonLoaderBase::get_as_double(json.as_ref().unwrap().get(KEY_OFFSET_X).unwrap());
    }
    if has(KEY_OFFSET_Y) {
        offset_y = JsonLoaderBase::get_as_double(json.as_ref().unwrap().get(KEY_OFFSET_Y).unwrap());
    }
    if has(KEY_OFFSET_Z) {
        offset_z = JsonLoaderBase::get_as_double(json.as_ref().unwrap().get(KEY_OFFSET_Z).unwrap());
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
    if has(KEY_ROTATE) {
        let rotation = CoordinateRotatorJsonLoader::new(
            seed.clone(),
            data_folder,
            json.as_ref().unwrap().get(KEY_ROTATE).cloned(),
        )
        .load();

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

/// Java `BiomePatternGeneratorJsonLoader.ISizeModifierProvider`.
pub trait ISizeModifierProvider {
    /// Java `double get(int seed, int x, int y)`.
    fn get(&self, seed: i32, x: i32, y: i32) -> f64;
}

/// Java `BiomePatternGeneratorJsonLoader.LoadedPointGeneratorDistanceFunction`.
struct LoadedPointGeneratorDistanceFunction {
    size_modifier_provider: Rc<BiomePatternGeneratorSizeModifierProvider>,
    distance_function: Rc<dyn PointDistanceFunction>,
}

impl LoadedPointGeneratorDistanceFunction {
    fn new(
        size_modifier_provider: Rc<BiomePatternGeneratorSizeModifierProvider>,
        distance_function: Rc<dyn PointDistanceFunction>,
    ) -> Self {
        Self {
            size_modifier_provider,
            distance_function,
        }
    }
}

impl PointDistanceFunction for LoadedPointGeneratorDistanceFunction {
    fn distance_2d(&self, delta_x: f64, delta_y: f64) -> f64 {
        self.distance_function.distance_2d(delta_x, delta_y)
    }

    fn distance_3d(&self, delta_x: f64, delta_y: f64, delta_z: f64) -> f64 {
        self.distance_function
            .distance_3d(delta_x, delta_y, delta_z)
    }

    #[allow(clippy::too_many_arguments)]
    fn distance_2d_at_cell(
        &self,
        seed: i32,
        cell_x: i32,
        cell_y: i32,
        cell_centre_x: f64,
        cell_centre_y: f64,
        delta_x: f64,
        delta_y: f64,
    ) -> f64 {
        self.distance_function.distance_2d_at_cell(
            seed,
            cell_x,
            cell_y,
            cell_centre_x,
            cell_centre_y,
            delta_x,
            delta_y,
        ) * self.size_modifier_provider.get(seed, cell_x, cell_y)
    }

    #[allow(clippy::too_many_arguments)]
    fn distance_3d_at_cell(
        &self,
        seed: i32,
        cell_x: i32,
        cell_y: i32,
        cell_z: i32,
        cell_centre_x: f64,
        cell_centre_y: f64,
        cell_centre_z: f64,
        delta_x: f64,
        delta_y: f64,
        delta_z: f64,
    ) -> f64 {
        self.distance_function.distance_3d_at_cell(
            seed,
            cell_x,
            cell_y,
            cell_z,
            cell_centre_x,
            cell_centre_y,
            cell_centre_z,
            delta_x,
            delta_y,
            delta_z,
        )
    }
}

/// Java `BiomePatternGeneratorJsonLoader.BiomePatternGeneratorSizeModifierProvider`.
pub struct BiomePatternGeneratorSizeModifierProvider {
    generator: RefCell<Option<Rc<BiomePatternGenerator>>>,
}

impl BiomePatternGeneratorSizeModifierProvider {
    /// Java `BiomePatternGeneratorSizeModifierProvider()`.
    pub fn new() -> Self {
        Self {
            generator: RefCell::new(None),
        }
    }

    /// Java `BiomePatternGeneratorSizeModifierProvider(BiomePatternGenerator)`.
    pub fn with_generator(generator: Rc<BiomePatternGenerator>) -> Self {
        Self {
            generator: RefCell::new(Some(generator)),
        }
    }

    /// Java `void setGenerator(BiomePatternGenerator)`.
    pub fn set_generator(&self, generator: Rc<BiomePatternGenerator>) {
        *self.generator.borrow_mut() = Some(generator);
    }
}

impl ISizeModifierProvider for BiomePatternGeneratorSizeModifierProvider {
    fn get(&self, seed: i32, x: i32, y: i32) -> f64 {
        let generator = self.generator.borrow();
        let generator = generator
            .as_ref()
            .expect("BiomePatternGeneratorSizeModifierProvider.setGenerator was not called");
        let biome = generator
            .get_biome_direct(seed, x as f64, y as f64)
            .expect("getBiomeDirect returned null (Java: NPE)");
        biome.get_size_modifier()
    }
}

impl Default for BiomePatternGeneratorSizeModifierProvider {
    fn default() -> Self {
        Self::new()
    }
}
