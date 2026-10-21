// Ported from: `.hytale-source/.../worldgen/cave/shape/CaveNodeShapeEnum.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.CaveNodeShapeEnum` —
//! the shape kinds a cave node can have.
//!
//! The nested [`CaveNodeShapeGenerator`] interface is the factory the cave
//! node types use to build their shape. Its Java implementations are the
//! concrete shape generators:
//!
//! | Java generator                   | Rust generator                                                                |
//! |----------------------------------|-------------------------------------------------------------------------------|
//! | `PipeCaveNodeShapeGenerator`      | [`PipeCaveNodeShapeGenerator`](super::pipe_cave_node_shape::PipeCaveNodeShapeGenerator) |
//! | `CylinderCaveNodeShapeGenerator`  | [`CylinderCaveNodeShapeGenerator`](super::cylinder_cave_node_shape::CylinderCaveNodeShapeGenerator) |
//! | `EllipsoidCaveNodeShapeGenerator` | [`EllipsoidCaveNodeShapeGenerator`](super::ellipsoid_cave_node_shape::EllipsoidCaveNodeShapeGenerator) |
//! | `EmptyLineCaveNodeShapeGenerator` | [`EmptyLineCaveNodeShapeGenerator`](super::empty_line_cave_node_shape::EmptyLineCaveNodeShapeGenerator) |
//! | `PrefabCaveNodeShapeGenerator`    | [`PrefabCaveNodeShapeGenerator`](super::prefab_cave_node_shape::PrefabCaveNodeShapeGenerator) |
//! | `DistortedCaveNodeShapeGenerator` | [`DistortedCaveNodeShapeGenerator`](super::distorted_cave_node_shape::DistortedCaveNodeShapeGenerator) |
//!
//! The Java enum itself has no other members; the JSON loader that maps the
//! config onto the generators is Phase 4e.

use hytale_math::fast_random::FastRandom;

use crate::cave::cave_node_type::CaveNodeChildEntry;
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::loader::util::Vector3d;

/// Java `enum CaveNodeShapeEnum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaveNodeShapeEnum {
    /// Java `PIPE`.
    Pipe,
    /// Java `CYLINDER`.
    Cylinder,
    /// Java `PREFAB`.
    Prefab,
    /// Java `EMPTY_LINE`.
    EmptyLine,
    /// Java `ELLIPSOID`.
    Ellipsoid,
    /// Java `DISTORTED`.
    Distorted,
}

/// Java `interface CaveNodeShapeEnum.CaveNodeShapeGenerator`.
pub trait CaveNodeShapeGenerator {
    /// Java `CaveNodeShape generateCaveNodeShape(Random random, CaveType
    /// caveType, CaveNode parentNode, CaveNodeType.CaveNodeChildEntry
    /// childEntry, Vector3d origin, float yaw, float pitch)`.
    #[allow(clippy::too_many_arguments)]
    fn generate_cave_node_shape(
        &self,
        random: &mut FastRandom,
        cave_type: &CaveType,
        parent_node: Option<&CaveNode>,
        child_entry: Option<&CaveNodeChildEntry>,
        origin: Vector3d,
        yaw: f32,
        pitch: f32,
    ) -> Box<dyn CaveNodeShape>;
}
