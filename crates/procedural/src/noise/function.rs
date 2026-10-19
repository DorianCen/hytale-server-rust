// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/ProceduralLibrary/src/main/java/com/hypixel/hytale/procedurallib/NoiseFunction.java`,
//              `.../NoiseFunction2d.java`, `.../NoiseFunction3d.java`,
//              `.../NoiseFunctionPair.java`, `.../NoiseType.java`
//! Root noise interfaces and the function-pair / type enums.
//!
//! Java `@FunctionalInterface NoiseFunction2d` → trait [`NoiseFunction2d`];
//! same for 3d. Java `NoiseFunction extends NoiseFunction2d, NoiseFunction3d`
//! → supertrait [`NoiseFunction`] with a blanket impl.

use std::rc::Rc;

/// Java `NoiseFunction2d` — `double get(int seed, int offsetSeed, double x, double y)`.
pub trait NoiseFunction2d {
    /// Java `NoiseFunction2d.get(int seed, int offsetSeed, double x, double y)`.
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64) -> f64;
}

/// Java `NoiseFunction3d` — `double get(int seed, int offsetSeed, double x, double y, double z)`.
pub trait NoiseFunction3d {
    /// Java `NoiseFunction3d.get(int seed, int offsetSeed, double x, double y, double z)`.
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64;
}

/// Java `NoiseFunction` (extends both `NoiseFunction2d` and `NoiseFunction3d`).
pub trait NoiseFunction: NoiseFunction2d + NoiseFunction3d {}

// Blanket impl: any type implementing both dimension traits is a NoiseFunction.
impl<T: NoiseFunction2d + NoiseFunction3d + ?Sized> NoiseFunction for T {}

/// Java `NoiseFunctionPair` — pairs a 2d and a 3d noise function.
pub struct NoiseFunctionPair {
    /// Java `noiseFunction2d`.
    pub noise_function_2d: Rc<dyn NoiseFunction2d>,
    /// Java `noiseFunction3d`.
    pub noise_function_3d: Rc<dyn NoiseFunction3d>,
}

impl NoiseFunctionPair {
    /// Java `NoiseFunctionPair(NoiseFunction2d, NoiseFunction3d)`.
    pub fn new(
        noise_function_2d: Rc<dyn NoiseFunction2d>,
        noise_function_3d: Rc<dyn NoiseFunction3d>,
    ) -> Self {
        Self {
            noise_function_2d,
            noise_function_3d,
        }
    }

    /// Java `getNoiseFunction2d`.
    pub fn get_noise_function_2d(&self) -> &dyn NoiseFunction2d {
        self.noise_function_2d.as_ref()
    }

    /// Java `getNoiseFunction3d`.
    pub fn get_noise_function_3d(&self) -> &dyn NoiseFunction3d {
        self.noise_function_3d.as_ref()
    }
}

impl NoiseFunction2d for NoiseFunctionPair {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64) -> f64 {
        self.noise_function_2d.get(seed, offset_seed, x, y)
    }
}

impl NoiseFunction3d for NoiseFunctionPair {
    fn get(&self, seed: i32, offset_seed: i32, x: f64, y: f64, z: f64) -> f64 {
        self.noise_function_3d.get(seed, offset_seed, x, y, z)
    }
}

/// Java `NoiseType` — the list of noise kinds known to the noise JSON loaders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoiseType {
    Simplex,
    OldSimplex,
    Value,
    Perlin,
    Cell,
    Distance,
    Constant,
    Grid,
    Mesh,
    Branch,
    Point,
}
