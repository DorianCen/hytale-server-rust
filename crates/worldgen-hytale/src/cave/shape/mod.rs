// Ported from: `.hytale-source/.../worldgen/cave/shape/*.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape` package.
//!
//! | Java class / interface         | Rust module                  | State        |
//! |--------------------------------|------------------------------|--------------|
//! | `CaveNodeShape`                | [`cave_node_shape`]          | ✅           |
//! | `CaveNodeShapeEnum`            | [`cave_node_shape_enum`]     | ✅           |
//! | `AbstractCaveNodeShape`        | [`abstract_cave_node_shape`] | ✅           |
//! | `CaveNodeShapeUtils`           | [`cave_node_shape_utils`]    | ✅           |
//! | `PipeCaveNodeShape`            | [`pipe_cave_node_shape`]     | ✅           |
//! | `CylinderCaveNodeShape`        | [`cylinder_cave_node_shape`] | ✅           |
//! | `EllipsoidCaveNodeShape`       | [`ellipsoid_cave_node_shape`]| ✅           |
//! | `EmptyLineCaveNodeShape`       | [`empty_line_cave_node_shape`]| ✅          |
//! | `PrefabCaveNodeShape`          | [`prefab_cave_node_shape`]   | partial (5)  |
//! | `DistortedCaveNodeShape`       | [`distorted_cave_node_shape`]| ✅           |
//! | `TetrahedronCaveNodeShape`     | —                            | not in the enum |
//!
//! # Phase 4c — the concrete shapes
//!
//! The pipe, the cylinder, the ellipsoid and the empty line share the Java
//! base class [`abstract_cave_node_shape`], which carves the shape into the
//! chunk and places the covers. The prefab shape has its own (structure
//! only) implementation.
//!
//! # Phase 4d — the distorted family
//!
//! [`distorted_cave_node_shape`] ports the Java `DistortedCaveNodeShape` (it
//! has its own `populateChunk`) and the [`distorted`] package ports the
//! distorted shapes themselves: the pipe, the cylinder and the ellipsoid,
//! their factories ([`distorted::distorted_shapes`]) and the
//! [`shape_distortion`] noise triple.
//!
//! # Seams
//!
//! * The prefab buffers and the prefab paste (`PrefabPasteUtil`) are
//!   ported (Phase 5a) — the prefab cave node pastes its prefab.
//! * The prefab *populator* (the biome prefab placement, Phase 5b) and the
//!   *unique* prefabs (Phase 5c) are ported, too.

pub mod abstract_cave_node_shape;
pub mod cave_node_shape;
pub mod cave_node_shape_enum;
pub mod cave_node_shape_utils;
pub mod cylinder_cave_node_shape;
pub mod distorted;
pub mod distorted_cave_node_shape;
pub mod ellipsoid_cave_node_shape;
pub mod empty_line_cave_node_shape;
pub mod pipe_cave_node_shape;
pub mod prefab_cave_node_shape;
pub mod shape_distortion;

pub use cave_node_shape::CaveNodeShape;
pub use cave_node_shape_enum::{CaveNodeShapeEnum, CaveNodeShapeGenerator};
pub use cave_node_shape_utils::WidthHeightSelector;
pub use cylinder_cave_node_shape::{CylinderCaveNodeShape, CylinderCaveNodeShapeGenerator};
pub use distorted_cave_node_shape::{DistortedCaveNodeShape, DistortedCaveNodeShapeGenerator};
pub use ellipsoid_cave_node_shape::{EllipsoidCaveNodeShape, EllipsoidCaveNodeShapeGenerator};
pub use empty_line_cave_node_shape::{EmptyLineCaveNodeShape, EmptyLineCaveNodeShapeGenerator};
pub use pipe_cave_node_shape::{PipeCaveNodeShape, PipeCaveNodeShapeGenerator};
pub use prefab_cave_node_shape::{PrefabCaveNodeShape, PrefabCaveNodeShapeGenerator};
pub use shape_distortion::ShapeDistortion;
