//! Noise generators — port of `com.hypixel.hytale.procedurallib.logic.**`
//! (noise classes, root noise interfaces, DoubleArray/ResultBuffer helpers,
//! GeneralNoise shared plumbing and the `cell/` cell-distance subpackage).
//!
//! Module layout mirrors the Java class layout 1:1:
//!
//! | Java class (ProceduralLibrary)            | Rust module in this file tree                 |
//! |-------------------------------------------|-----------------------------------------------|
//! | `NoiseFunction`, `NoiseFunction2d`, `NoiseFunction3d`, `NoiseFunctionPair`, `NoiseType` | [`function`] |
//! | `logic/GeneralNoise`                      | [`general_noise`]                             |
//! | `logic/DoubleArray`                       | [`double_array`]                              |
//! | `logic/ResultBuffer`                      | [`result_buffer`]                             |
//! | `logic/SimplexNoise`                      | [`simplex_noise`]                             |
//! | `logic/OldSimplexNoise`                   | [`old_simplex_noise`]                         |
//! | `logic/PerlinNoise`                       | [`perlin_noise`]                              |
//! | `logic/ValueNoise`                        | [`value_noise`]                               |
//! | `logic/ConstantNoise`                     | [`constant_noise`]                            |
//! | `logic/CellularNoise`                     | [`cellular_noise`]                            |
//! | `logic/CellNoise`                         | [`cell_noise`]                                |
//! | `logic/DistanceNoise`                     | [`distance_noise`]                            |
//! | `logic/GridNoise`                         | [`grid_noise`]                                |
//! | `logic/MeshNoise`                         | [`mesh_noise`]                                |
//! | `logic/HexMeshNoise`                      | [`hex_mesh_noise`]                            |
//! | `logic/PointNoise`                        | [`point_noise`]                               |
//! | `logic/BranchNoise`                       | [`branch_noise`]                              |
//! | `logic/cell/**`                           | [`cell`]                                      |
//!
//! Out-of-package support interfaces that the noise code depends on
//! (`condition.IIntCondition` etc.) are provisionally defined in [`support`]
//! because the `condition/`, `supplier/` and `property/` packages are ported
//! separately; small `MathUtil`/`TrigMathUtil` helpers used here are
//! provisionally local in [`math_util`] / [`trig_math_util`].

pub mod branch_noise;
pub mod cell;
pub mod cell_noise;
pub mod cellular_noise;
pub mod constant_noise;
pub mod distance_noise;
pub mod double_array;
pub mod function;
pub mod general_noise;
pub mod grid_noise;
pub mod hex_mesh_noise;
pub mod math_util;
pub mod mesh_noise;
pub mod old_simplex_noise;
pub mod perlin_noise;
pub mod point_noise;
pub mod result_buffer;
pub mod simplex_noise;
pub mod support;
pub mod trig_math_util;
pub mod value_noise;

pub use branch_noise::BranchNoise;
pub use cell_noise::{CellMode, CellNoise, CellFunction};
pub use cellular_noise::{CELL_2D, CELL_3D};
pub use constant_noise::ConstantNoise;
pub use distance_noise::{Distance2Mode, DistanceNoise};
pub use double_array::{Double2, Double3};
pub use function::{NoiseFunction, NoiseFunction2d, NoiseFunction3d, NoiseFunctionPair, NoiseType};
pub use general_noise::{
    fast_ceil, fast_floor, grad_coord_2d, grad_coord_3d, hash2d, hash3d, limit, lerp,
    InterpolationFunction, InterpolationMode, X_PRIME, Y_PRIME, Z_PRIME,
};
pub use grid_noise::GridNoise;
pub use hex_mesh_noise::HexMeshNoise;
pub use mesh_noise::MeshNoise;
pub use old_simplex_noise::OldSimplexNoise;
pub use perlin_noise::PerlinNoise;
pub use point_noise::PointNoise;
pub use result_buffer::{Bounds2d, ResultBuffer2d, ResultBuffer3d};
pub use simplex_noise::SimplexNoise;
pub use value_noise::ValueNoise;
