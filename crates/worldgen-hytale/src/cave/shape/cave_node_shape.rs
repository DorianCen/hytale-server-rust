// Ported from: `.hytale-source/.../worldgen/cave/shape/CaveNodeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.CaveNodeShape` — the
//! geometry of a cave node.
//!
//! The Java interface is implemented by the concrete shapes:
//!
//! | Java shape              | Rust shape                                                          |
//! |-------------------------|---------------------------------------------------------------------|
//! | `PipeCaveNodeShape`     | [`PipeCaveNodeShape`](super::pipe_cave_node_shape::PipeCaveNodeShape) |
//! | `CylinderCaveNodeShape` | [`CylinderCaveNodeShape`](super::cylinder_cave_node_shape::CylinderCaveNodeShape) |
//! | `EllipsoidCaveNodeShape`| [`EllipsoidCaveNodeShape`](super::ellipsoid_cave_node_shape::EllipsoidCaveNodeShape) |
//! | `EmptyLineCaveNodeShape`| [`EmptyLineCaveNodeShape`](super::empty_line_cave_node_shape::EmptyLineCaveNodeShape) |
//! | `PrefabCaveNodeShape`   | [`PrefabCaveNodeShape`](super::prefab_cave_node_shape::PrefabCaveNodeShape) |
//! | `DistortedCaveNodeShape`| [`DistortedCaveNodeShape`](super::distorted_cave_node_shape::DistortedCaveNodeShape) |
//!
//! The geometric shapes share the Java base class
//! [`AbstractCaveNodeShape`](super::abstract_cave_node_shape), which carves
//! the shape into the chunk; the prefab shape has its own implementation.

use hytale_math::fast_random::FastRandom;

use crate::cave::cave::Cave;
use crate::cave::element::cave_node::CaveNode;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::util::bounds::WorldBounds;

/// Java `interface CaveNodeShape`.
pub trait CaveNodeShape {
    /// Java `Vector3d getStart()`.
    fn get_start(&self) -> Vector3d;

    /// Java `Vector3d getEnd()`.
    fn get_end(&self) -> Vector3d;

    /// Java `Vector3d getAnchor(Vector3d vector, double tx, double ty,
    /// double tz)`.
    fn get_anchor(&self, vector: Vector3d, tx: f64, ty: f64, tz: f64) -> Vector3d;

    /// Java `IWorldBounds getBounds()`.
    fn get_bounds(&self) -> &WorldBounds;

    /// Java `boolean shouldReplace(int seed, double x, double z, int y)`.
    fn should_replace(&self, seed: i32, x: f64, z: f64, y: i32) -> bool;

    /// Java `double getFloorPosition(int seed, double x, double z)`.
    fn get_floor_position(&self, seed: i32, x: f64, z: f64) -> f64;

    /// Java `double getCeilingPosition(int seed, double x, double z)`.
    fn get_ceiling_position(&self, seed: i32, x: f64, z: f64) -> f64;

    /// Java `void populateChunk(int seed, ChunkGeneratorExecution execution,
    /// Cave cave, CaveNode node, Random random)`.
    fn populate_chunk(
        &self,
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cave: &Cave,
        node: &CaveNode,
        random: &mut FastRandom,
    );

    /// Java `default boolean hasGeometry()` — `getBounds().isValid()`.
    fn has_geometry(&self) -> bool {
        self.get_bounds().is_valid()
    }

    /// The Rust analogue of the Java `shape instanceof PrefabCaveNodeShape`
    /// check `CaveGenerator.getRotation` performs: the prefab rotation of a
    /// prefab cave node, or `None` for every other shape.
    ///
    /// Deviation: Rust trait objects cannot be downcast without an extra
    /// `Any` bound, so the check is exposed as a default method that
    /// [`PrefabCaveNodeShape`](crate::cave::shape::prefab_cave_node_shape::PrefabCaveNodeShape)
    /// overrides. Every other shape keeps the `None` default — exactly
    /// Java's `instanceof` result.
    fn as_prefab_rotation(&self) -> Option<crate::prefab::prefab_rotation::PrefabRotation> {
        None
    }

    /// Java `CaveNodeShapeUtils.getEndRadius(CaveNodeShape shape,
    /// BiDoubleToDoubleFunction widthHeightSelector)` — the radius of the
    /// shape at its end, or `-1` when the shape does not expose one.
    ///
    /// Deviation: the Java method dispatches with a chain of `instanceof`
    /// checks (the cylinder, the pipe and the distorted shape); Rust trait
    /// objects cannot be downcast, so the check is exposed as a method the
    /// shapes override. The default is Java's "unknown shape" answer, `-1`.
    fn get_end_radius(
        &self,
        _selector: crate::cave::shape::cave_node_shape_utils::WidthHeightSelector,
    ) -> f64 {
        -1.0
    }
}
