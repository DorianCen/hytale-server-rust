// Ported from: `.hytale-source/.../worldgen/cave/`
//! Java `com.hypixel.hytale.server.worldgen.cave` package — the caves.
//!
//! | Java class / interface                | Rust module                        | State      |
//! |---------------------------------------|------------------------------------|------------|
//! | `Cave`                                | [`cave`]                           | ✅         |
//! | `CaveType`                            | [`cave_type`]                      | ✅         |
//! | `CaveNodeType`                        | [`cave_node_type`]                 | ✅         |
//! | `CaveBiomeMaskFlags`                  | [`cave_biome_mask_flags`]          | ✅         |
//! | `CaveYawMode`                         | [`cave_yaw_mode`]                  | ✅         |
//! | `CavePrefabPlacement`                 | [`cave_prefab_placement`]          | ✅         |
//! | `CaveBlockPriorityModifier`           | [`cave_block_priority_modifier`]   | ✅         |
//! | `cache.CaveGeneratorCache`            | [`cave_generator_cache`]           | ✅         |
//! | `CaveGenerator`                       | [`cave_generator`]                 | ✅ 4b      |
//! | `element.CaveElement`                 | [`element::cave_element`]          | ✅         |
//! | `element.CaveNode`                    | [`element::cave_node`]             | ✅         |
//! | `element.CavePrefab`                  | [`element::cave_prefab`]           | ✅         |
//! | `prefab.CavePrefabContainer`          | [`prefab::cave_prefab_container`]  | ✅         |
//! | `shape.CaveNodeShape`                 | [`shape::cave_node_shape`]         | ✅         |
//! | `shape.CaveNodeShapeEnum`             | [`shape::cave_node_shape_enum`]    | ✅         |
//! | `shape.AbstractCaveNodeShape`         | [`shape::abstract_cave_node_shape`]| ✅ 4c      |
//! | `shape.CaveNodeShapeUtils`            | [`shape::cave_node_shape_utils`]   | ✅         |
//! | `shape.PipeCaveNodeShape`             | [`shape::pipe_cave_node_shape`]    | ✅ 4c      |
//! | `shape.CylinderCaveNodeShape`         | [`shape::cylinder_cave_node_shape`]| ✅ 4c      |
//! | `shape.EllipsoidCaveNodeShape`        | [`shape::ellipsoid_cave_node_shape`]| ✅ 4c     |
//! | `shape.EmptyLineCaveNodeShape`        | [`shape::empty_line_cave_node_shape`]| ✅ 4c    |
//! | `shape.PrefabCaveNodeShape`           | [`shape::prefab_cave_node_shape`]  | partial 5  |
//! | `shape.DistortedCaveNodeShape`        | [`shape::distorted_cave_node_shape`]| ✅ 4d     |
//! | `shape.distorted.*`                   | [`shape::distorted`]               | ✅ 4d      |
//! | `shape.distorted.ShapeDistortion`     | [`shape::shape_distortion`]        | ✅ 4d      |
//!
//! # Phase 4a — the core value types
//!
//! This slice ports the value types of the cave package: the compiled cave
//! ([`cave::Cave`]), its description ([`cave_type::CaveType`]), the node type
//! ([`cave_node_type::CaveNodeType`]), the biome mask flags, the yaw mode,
//! the prefab placement, the cave elements and the two integration points:
//! the [`cave_generator_cache::CaveGeneratorCache`] and the
//! [`cave_block_priority_modifier::CaveBlockPriorityModifier`].
//!
//! # Phase 4b — the recursive growth and the cave stage
//!
//! [`cave_generator::CaveGeneratorImpl`] ports the Java `CaveGenerator`
//! walk (`generate` → `startCave` → `continueNode`): the child-entry shuffle,
//! the repeat/chance/child-count filters, the biome-mask and height checks,
//! the child origin/yaw/pitch/anchor math and the prefab placement pass.
//! [`crate::chunk::populator::cave_populator::CavePopulator`] and the cave
//! stage of [`crate::chunk::chunk_generator_execution::ChunkGeneratorExecution::execute`]
//! are ported with it.
//!
//! # Phase 4c — the concrete cave node shapes
//!
//! [`shape::pipe_cave_node_shape`], [`shape::cylinder_cave_node_shape`],
//! [`shape::ellipsoid_cave_node_shape`] and
//! [`shape::empty_line_cave_node_shape`] port the Java shape classes and
//! their generators; they share the Java base class
//! [`shape::abstract_cave_node_shape`], which carves the shape into the
//! chunk and places the covers.
//! [`shape::prefab_cave_node_shape`] ports the structure of the prefab
//! cave node (the paste itself is Phase 5).
//!
//! # Phase 4d — the distorted shape family
//!
//! [`shape::distorted_cave_node_shape`] ports the Java
//! `DistortedCaveNodeShape` (which has its own `populateChunk`) and
//! [`shape::distorted`] ports the `distorted` package: the
//! `DistortedShape` interface, the abstract base classes, the pipe, the
//! cylinder and the ellipsoid shapes, their factories and the
//! [`shape::shape_distortion`] noise triple. The two distortion-dependent
//! parts of `CaveNodeShapeUtils` (the `DistortedCaveNodeShape` branch of
//! `getEndRadius` and the block-type support map of `invalidateCover`) are
//! ported as well.
//!
//! # Phase 4e — the cave JSON loaders
//!
//! The loaders that build the cave configuration of a zone live in
//! [`crate::loader::cave`] (1:1 with the Java `loader.cave` package): the
//! cave generator, the cave types, the cave node types (and their storage),
//! the child and cover entries, the biome mask, the prefab configuration
//! and the shape generators.
//!
//! # Seams
//!
//! * [`prefab::cave_prefab_supplier::CavePrefabSupplier`] — the prefab
//!   buffers and the prefab paste are ported (Phase 5a): the cave loaders
//!   resolve the prefab *names* through the prefab store and hand the
//!   suppliers of the matching prefab files to the cave package.
//! * [`crate::supplier`] — the float range (`IFloatRange`) and the
//!   coordinate-hash supplier of the procedural library are not ported into
//!   the `procedural` crate yet.
//! * the 4e loaders close the last Phase-4 gaps (the JSON loaders, the
//!   distortion noises, the interpolation functions, the shape factories);
//!   the block-type support *data* of `BlockType.getSupport` is part of the
//!   golden registry dump since the dumper's `supportField` extension (one
//!   entry per Java `RotationTuple`).

#[allow(clippy::module_inception)] // Java class Cave in Java package cave
pub mod cave;
pub mod cave_biome_mask_flags;
pub mod cave_block_priority_modifier;
pub mod cave_generator;
pub mod cave_generator_cache;
pub mod cave_node_type;
pub mod cave_prefab_placement;
pub mod cave_type;
pub mod cave_yaw_mode;
pub mod element;
pub mod prefab;
pub mod shape;

#[cfg(test)]
pub(crate) mod test_stubs;

pub use cave::Cave;
pub use cave_block_priority_modifier::CaveBlockPriorityModifier;
pub use cave_generator::{CaveGenerator, CaveGeneratorImpl};
pub use cave_generator_cache::CaveGeneratorCache;
pub use cave_node_type::{
    CaveNodeChildEntry, CaveNodeCoverEntry, CaveNodeCoverEntryEntry, CaveNodeCoverType,
    CaveNodeType,
};
pub use cave_prefab_placement::CavePrefabPlacement;
pub use cave_type::{CaveType, FluidLevel};
pub use cave_yaw_mode::CaveYawMode;
pub use element::{CaveElement, CaveNode, CavePrefab};
