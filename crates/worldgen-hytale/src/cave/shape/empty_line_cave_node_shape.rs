// Ported from: `.hytale-source/.../worldgen/cave/shape/EmptyLineCaveNodeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.EmptyLineCaveNodeShape` —
//! a cave node without any geometry.
//!
//! The empty line is the shape the cave *entry* node uses: it has a start
//! and an end (so the children can grow from it) but it does not carve
//! anything. Its bounds are all zero, so
//! [`CaveNodeShape::has_geometry`] is `false` and the node is never added
//! to the cave.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::trig_math_util;
use procedural::noise::supplier::IDoubleRange;

use crate::cave::cave_node_type::CaveNodeChildEntry;
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::cave::shape::cave_node_shape_utils::get_line_anchor;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::util::bounds::WorldBounds;

/// Java `public class EmptyLineCaveNodeShape extends AbstractCaveNodeShape
/// implements IWorldBounds`.
pub struct EmptyLineCaveNodeShape {
    /// Java `private final Vector3d o`.
    o: Vector3d,
    /// Java `private final Vector3d v`.
    v: Vector3d,
    /// Java `implements IWorldBounds` — every bound of the empty line is
    /// zero.
    bounds: WorldBounds,
}

impl EmptyLineCaveNodeShape {
    /// Java `public EmptyLineCaveNodeShape(Vector3d o, Vector3d v)`.
    pub fn new(o: Vector3d, v: Vector3d) -> Self {
        Self {
            o,
            v,
            bounds: WorldBounds::with_bounds(0, 0, 0, 0, 0, 0),
        }
    }
}

impl CaveNodeShape for EmptyLineCaveNodeShape {
    /// Java `@Nonnull @Override public Vector3d getStart()`.
    fn get_start(&self) -> Vector3d {
        self.o
    }

    /// Java `@Nonnull @Override public Vector3d getEnd()`.
    fn get_end(&self) -> Vector3d {
        Vector3d::new(
            self.o.x + self.v.x,
            self.o.y + self.v.y,
            self.o.z + self.v.z,
        )
    }

    /// Java `@Nonnull @Override public Vector3d getAnchor(@Nonnull Vector3d
    /// vector, double t, double tv, double th)`.
    fn get_anchor(&self, vector: Vector3d, t: f64, _tv: f64, _th: f64) -> Vector3d {
        get_line_anchor(vector, self.o, self.v, t)
    }

    /// Java `@Nonnull @Override public IWorldBounds getBounds()`.
    ///
    /// Deviation: the Java shape returns `this` (it implements
    /// `IWorldBounds` itself); the Rust port returns a reference to a
    /// stored copy of the (all-zero) bounds.
    fn get_bounds(&self) -> &WorldBounds {
        &self.bounds
    }

    /// Java `@Override public boolean shouldReplace(int seed, double x, double
    /// z, int y)`.
    fn should_replace(&self, _seed: i32, _x: f64, _z: f64, _y: i32) -> bool {
        false
    }

    /// Java `@Override public double getFloorPosition(int seed, double x,
    /// double z)`.
    fn get_floor_position(&self, _seed: i32, _x: f64, _z: f64) -> f64 {
        -1.0
    }

    /// Java `@Override public double getCeilingPosition(int seed, double x,
    /// double z)`.
    fn get_ceiling_position(&self, _seed: i32, _x: f64, _z: f64) -> f64 {
        -1.0
    }

    /// Java `@Override public void populateChunk(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, @Nonnull Cave cave, @Nonnull CaveNode
    /// node, @Nonnull Random random)`.
    ///
    /// The Java base class implementation runs, but the empty line never
    /// replaces a block, so nothing is written.
    fn populate_chunk(
        &self,
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cave: &crate::cave::cave::Cave,
        node: &CaveNode,
        random: &mut FastRandom,
    ) {
        crate::cave::shape::abstract_cave_node_shape::populate_chunk(
            self, seed, execution, cave, node, random,
        );
    }

    /// Java `@Override public boolean hasGeometry()` — `false`.
    fn has_geometry(&self) -> bool {
        false
    }
}

/// Java `public static class EmptyLineCaveNodeShapeGenerator implements
/// CaveNodeShapeEnum.CaveNodeShapeGenerator`.
pub struct EmptyLineCaveNodeShapeGenerator {
    /// Java `private final IDoubleRange length`.
    length: Rc<dyn IDoubleRange>,
}

impl EmptyLineCaveNodeShapeGenerator {
    /// Java `public EmptyLineCaveNodeShapeGenerator(IDoubleRange length)`.
    pub fn new(length: Rc<dyn IDoubleRange>) -> Self {
        Self { length }
    }
}

impl CaveNodeShapeGenerator for EmptyLineCaveNodeShapeGenerator {
    /// Java `@Nonnull @Override public CaveNodeShape
    /// generateCaveNodeShape(@Nonnull Random random, CaveType caveType, CaveNode
    /// parentNode, CaveNodeType.CaveNodeChildEntry childEntry, Vector3d
    /// origin, float yaw, float pitch)`.
    fn generate_cave_node_shape(
        &self,
        random: &mut FastRandom,
        _cave_type: &CaveType,
        _parent_node: Option<&CaveNode>,
        _child_entry: Option<&CaveNodeChildEntry>,
        origin: Vector3d,
        yaw: f32,
        pitch: f32,
    ) -> Box<dyn CaveNodeShape> {
        // Java: `final double l = length.getValue(random.nextDouble());`
        let l = self.length.get_value(random.next_double());

        // Java:
        // ```java
        // final Vector3d direction = new Vector3d(
        //     sin(pitch) * cos(yaw),
        //     cos(pitch),
        //     sin(pitch) * sin(yaw)
        // ).mul(l);
        // ```
        let sin_pitch = trig_math_util::sin(pitch);
        let cos_pitch = trig_math_util::cos(pitch);
        let sin_yaw = trig_math_util::sin(yaw);
        let cos_yaw = trig_math_util::cos(yaw);
        let direction = Vector3d::new(
            (sin_pitch * cos_yaw) as f64 * l,
            cos_pitch as f64 * l,
            (sin_pitch * sin_yaw) as f64 * l,
        );

        // The Java generator does *not* apply the child offset — the empty
        // line is the shape of the entry node.
        Box::new(EmptyLineCaveNodeShape::new(origin, direction))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::test_stubs::cave_type;
    use crate::util::bounds::{IChunkBounds, IWorldBounds};

    #[test]
    fn the_bounds_are_all_zero() {
        let shape =
            EmptyLineCaveNodeShape::new(Vector3d::new(1.0, 2.0, 3.0), Vector3d::new(4.0, 5.0, 6.0));
        let bounds = shape.get_bounds();
        assert_eq!(bounds.get_low_bound_x(), 0);
        assert_eq!(bounds.get_high_bound_y(), 0);
        assert!(!bounds.is_valid());
    }

    #[test]
    fn the_shape_has_no_geometry() {
        let shape = EmptyLineCaveNodeShape::new(Vector3d::ZERO, Vector3d::new(1.0, 0.0, 0.0));
        assert!(!shape.has_geometry());
    }

    #[test]
    fn the_shape_never_replaces_a_block() {
        let shape = EmptyLineCaveNodeShape::new(Vector3d::ZERO, Vector3d::new(1.0, 0.0, 0.0));
        assert!(!shape.should_replace(0, 0.0, 0.0, 0));
        assert_eq!(shape.get_floor_position(0, 0.0, 0.0), -1.0);
        assert_eq!(shape.get_ceiling_position(0, 0.0, 0.0), -1.0);
    }

    #[test]
    fn the_start_and_the_end_are_the_line_ends() {
        let shape =
            EmptyLineCaveNodeShape::new(Vector3d::new(1.0, 2.0, 3.0), Vector3d::new(4.0, 5.0, 6.0));
        assert_eq!(shape.get_start(), Vector3d::new(1.0, 2.0, 3.0));
        assert_eq!(shape.get_end(), Vector3d::new(5.0, 7.0, 9.0));
    }

    #[test]
    fn the_anchor_is_the_projection_on_the_line() {
        let shape = EmptyLineCaveNodeShape::new(
            Vector3d::new(0.0, 0.0, 0.0),
            Vector3d::new(10.0, 0.0, 0.0),
        );
        let anchor = shape.get_anchor(Vector3d::ZERO, 0.5, 0.0, 0.0);
        assert_eq!(anchor.x, 5.0);
        assert_eq!(anchor.y, 0.0);
        assert_eq!(anchor.z, 0.0);
    }

    #[test]
    fn the_generator_draws_the_length_once() {
        let generator = EmptyLineCaveNodeShapeGenerator::new(Rc::new(
            procedural::noise::supplier::DoubleRange::constant(10.0),
        ));
        let mut random = FastRandom::new(1234);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &cave_type("empty"),
            None,
            None,
            Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            0.0,
        );

        let mut reference = FastRandom::new(1234);
        let _ = reference.next_double();
        assert_eq!(random.next_double(), reference.next_double());

        assert_eq!(shape.get_start(), Vector3d::new(0.0, 40.0, 0.0));
        // With a yaw and a pitch of zero the line points straight up.
        assert_eq!(shape.get_end(), Vector3d::new(0.0, 50.0, 0.0));
    }
}
