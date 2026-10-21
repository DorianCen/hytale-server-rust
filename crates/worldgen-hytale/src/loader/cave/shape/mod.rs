// Ported from: `.hytale-source/.../worldgen/loader/cave/shape/**`
//! Java `loader.cave.shape` package — the loaders of the cave node shapes.
//!
//! | Java class                                    | Rust module                                                    |
//! |-----------------------------------------------|----------------------------------------------------------------|
//! | `CaveNodeShapeGeneratorJsonLoader` (abstract) | — (the base class of the loaders below)                        |
//! | `PipeCaveNodeShapeGeneratorJsonLoader`        | [`pipe_cave_node_shape_generator_json_loader`]                  |
//! | `CylinderCaveNodeShapeGeneratorJsonLoader`    | [`cylinder_cave_node_shape_generator_json_loader`]              |
//! | `EllipsoidCaveNodeShapeGeneratorJsonLoader`   | [`ellipsoid_cave_node_shape_generator_json_loader`]             |
//! | `EmptyLineCaveNodeShapeGeneratorJsonLoader`   | [`empty_line_cave_node_shape_generator_json_loader`]            |
//! | `PrefabCaveNodeShapeGeneratorJsonLoader`      | [`prefab_cave_node_shape_generator_json_loader`]               |
//! | `DistortedCaveNodeShapeGeneratorJsonLoader`   | [`distorted_cave_node_shape_generator_json_loader`]             |
//! | `ShapeDistortionJsonLoader`                   | [`shape_distortion_json_loader`]                                |
//!
//! # Seam
//!
//! The prefab shape resolves its prefab names through the prefab store
//! (Phase 5) — see the module note of
//! [`prefab_cave_node_shape_generator_json_loader`].

pub mod cylinder_cave_node_shape_generator_json_loader;
pub mod distorted_cave_node_shape_generator_json_loader;
pub mod ellipsoid_cave_node_shape_generator_json_loader;
pub mod empty_line_cave_node_shape_generator_json_loader;
pub mod pipe_cave_node_shape_generator_json_loader;
pub mod prefab_cave_node_shape_generator_json_loader;
pub mod shape_distortion_json_loader;
