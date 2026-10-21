// Ported from: `.hytale-source/.../worldgen/cave/shape/distorted/*.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.distorted` package —
//! the distorted shape family.
//!
//! | Java class / interface     | Rust module                                          | State |
//! |----------------------------|------------------------------------------------------|-------|
//! | `DistortedShape`           | [`distorted_shape`]                                  | ✅    |
//! | `AbstractDistortedShape`   | [`abstract_distorted_shape`]                         | ✅    |
//! | `AbstractDistortedExtrusion`| [`abstract_distorted_extrusion`]                    | ✅    |
//! | `AbstractDistortedBody`    | [`abstract_distorted_body`]                          | ✅    |
//! | `DistortedShapes`          | [`distorted_shapes`]                                 | ✅    |
//! | `DistortedCylinderShape`   | [`distorted_cylinder_shape`]                         | ✅    |
//! | `DistortedPipeShape`       | [`distorted_pipe_shape`]                             | ✅    |
//! | `DistortedEllipsoidShape`  | [`distorted_ellipsoid_shape`]                        | ✅    |
//! | `ShapeDistortion`          | [`crate::cave::shape::shape_distortion`]             | ✅    |
//!
//! A distorted shape is a [`DistortedShape`] — a geometry the cave node
//! carves into the chunk through
//! [`DistortedCaveNodeShape`](super::distorted_cave_node_shape::DistortedCaveNodeShape).
//! There are two families:
//!
//! * the **extrusions** ([`DistortedCylinderShape`](distorted_cylinder_shape::DistortedCylinderShape)
//!   and [`DistortedPipeShape`](distorted_pipe_shape::DistortedPipeShape)) —
//!   a cross-section swept along a segment, and
//! * the **bodies** ([`DistortedEllipsoidShape`](distorted_ellipsoid_shape::DistortedEllipsoidShape))
//!   — a blob centred on its origin.
//!
//! # Java → Rust
//!
//! Rust has no class inheritance, so the Java `abstract class` chain becomes
//! composition:
//!
//! ```text
//! AbstractDistortedShape            (bounds)
//!   ├── AbstractDistortedExtrusion  (segment + interpolation)
//!   │     └── DistortedCylinderShape
//!   │           └── DistortedPipeShape
//!   └── AbstractDistortedBody       (origin + rotation)
//!         └── DistortedEllipsoidShape
//! ```
//!
//! * [`AbstractDistortedExtrusion`] and [`AbstractDistortedBody`] are owned by
//!   the concrete shapes.
//! * The two abstract methods of the extrusion are the internal
//!   [`DistortedExtrusion`] trait; the shared `getHeightAtProjection` body is
//!   the free [`get_height_at_projection`] function.
//! * The Java `Factory` interfaces are the
//!   [`DistortedShapeFactory`](distorted_shapes::DistortedShapeFactory) trait
//!   and the three `'static` factories in
//!   [`DistortedShapes`](distorted_shapes::DistortedShapes).
//!
//! # Seams
//!
//! None — the whole package is ported.
//!
//! Note: [`ShapeDistortion`](crate::cave::shape::shape_distortion::ShapeDistortion)
//! lives one level up (`shape::shape_distortion`) although Java declares it in
//! this package; see that module for the reason.

pub mod abstract_distorted_body;
pub mod abstract_distorted_extrusion;
pub mod abstract_distorted_shape;
pub mod distorted_cylinder_shape;
pub mod distorted_ellipsoid_shape;
pub mod distorted_pipe_shape;
pub mod distorted_shape;
pub mod distorted_shapes;

pub use abstract_distorted_body::AbstractDistortedBody;
pub use abstract_distorted_extrusion::{
    get_height_at_projection, AbstractDistortedExtrusion, DistortedExtrusion,
};
pub use abstract_distorted_shape::AbstractDistortedShape;
pub use distorted_cylinder_shape::{DistortedCylinderShape, DistortedCylinderShapeFactory};
pub use distorted_ellipsoid_shape::{DistortedEllipsoidShape, DistortedEllipsoidShapeFactory};
pub use distorted_pipe_shape::{DistortedPipeShape, DistortedPipeShapeFactory};
pub use distorted_shape::{DistortedShape, DEFAULT_INTERPOLATION};
pub use distorted_shapes::{DistortedShapeFactory, DistortedShapes};
