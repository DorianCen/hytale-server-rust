// Ported from: `.hytale-source/.../worldgen/loader/util/*.java`
//! Java `loader.util` package — the JSON loaders for the block-array /
//! vector helpers the biome and container configs reference.
//!
//! Java uses the joml `Vector2d` / `Vector3d` types in the two vector loaders.
//! The world generator only reads the `x` / `y` / `z` components, so the Rust
//! port defines the minimal equivalents here instead of pulling in a math
//! dependency.

pub mod float_range;
pub mod noise_block_array_json_loader;
pub mod path_util;
pub mod resolved_block_array_json_loader;
pub mod resolved_variants_block_array_loader;
pub mod vector2d_json_loader;
pub mod vector3d_json_loader;

/// Minimal `org.joml.Vector2d` equivalent (the fields the generator reads).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector2d {
    /// Java `Vector2d.x`.
    pub x: f64,
    /// Java `Vector2d.y`.
    pub y: f64,
}

impl Vector2d {
    /// Java `new Vector2d()` — the zero vector.
    pub const ZERO: Vector2d = Vector2d { x: 0.0, y: 0.0 };

    /// Java `new Vector2d(double x, double y)`.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Minimal `org.joml.Vector3d` equivalent (the fields the generator reads).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector3d {
    /// Java `Vector3d.x`.
    pub x: f64,
    /// Java `Vector3d.y`.
    pub y: f64,
    /// Java `Vector3d.z`.
    pub z: f64,
}

impl Vector3d {
    /// Java `new Vector3d()` — the zero vector.
    pub const ZERO: Vector3d = Vector3d {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    /// Java `new Vector3d(double x, double y, double z)`.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}
