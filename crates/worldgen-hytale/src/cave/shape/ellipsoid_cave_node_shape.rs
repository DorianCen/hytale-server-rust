// Ported from: `.hytale-source/.../worldgen/cave/shape/EllipsoidCaveNodeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.EllipsoidCaveNodeShape` —
//! a cave node that is an ellipsoid (a blob).

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::math_util;
use procedural::noise::supplier::IDoubleRange;

use crate::cave::cave_node_type::CaveNodeChildEntry;
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::abstract_cave_node_shape;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::cave::shape::cave_node_shape_utils::{get_offset, get_sphere_anchor};
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::util::bounds::{IWorldBounds, WorldBounds};

/// Java `public class EllipsoidCaveNodeShape extends AbstractCaveNodeShape
/// implements IWorldBounds`.
pub struct EllipsoidCaveNodeShape {
    /// Java `private final CaveType caveType`.
    cave_type: Rc<CaveType>,
    /// Java `@Nonnull private final Vector3d o` — the centre of the
    /// ellipsoid.
    o: Vector3d,
    /// Java `private final double rx, ry, rz` — the radii.
    rx: f64,
    /// The y radius.
    ry: f64,
    /// The z radius.
    rz: f64,
    /// Java `private final int lowBoundX, lowBoundY, lowBoundZ, highBoundX,
    /// highBoundY, highBoundZ` — the shape *is* its own `IWorldBounds`.
    bounds: WorldBounds,
}

impl EllipsoidCaveNodeShape {
    /// Java `public EllipsoidCaveNodeShape(CaveType caveType, @Nonnull
    /// Vector3d o, double rx, double ry, double rz)`.
    pub fn new(cave_type: Rc<CaveType>, o: Vector3d, rx: f64, ry: f64, rz: f64) -> Self {
        // Java:
        // ```java
        // lowBoundX = MathUtil.floor(Math.min(o.x - rx, o.x + rx));
        // …
        // highBoundX = MathUtil.ceil(Math.max(o.x - rx, o.x + rx));
        // ```
        let bounds = WorldBounds::with_bounds(
            math_util::floor((o.x - rx).min(o.x + rx)),
            math_util::floor((o.y - ry).min(o.y + ry)),
            math_util::floor((o.z - rz).min(o.z + rz)),
            math_util::ceil((o.x - rx).max(o.x + rx)),
            math_util::ceil((o.y - ry).max(o.y + ry)),
            math_util::ceil((o.z - rz).max(o.z + rz)),
        );

        Self {
            cave_type,
            o,
            rx,
            ry,
            rz,
            bounds,
        }
    }
}

impl CaveNodeShape for EllipsoidCaveNodeShape {
    /// Java `@Nonnull @Override public Vector3d getStart()`.
    fn get_start(&self) -> Vector3d {
        self.o
    }

    /// Java `@Nonnull @Override public Vector3d getEnd()`.
    fn get_end(&self) -> Vector3d {
        Vector3d::new(self.o.x, self.bounds.get_low_bound_y() as f64, self.o.z)
    }

    /// Java `@Nonnull @Override public Vector3d getAnchor(@Nonnull Vector3d
    /// vector, double tx, double ty, double tz)`.
    fn get_anchor(&self, vector: Vector3d, tx: f64, ty: f64, tz: f64) -> Vector3d {
        get_sphere_anchor(vector, self.o, self.rx, self.ry, self.rz, tx, ty, tz)
    }

    /// Java `@Nonnull @Override public IWorldBounds getBounds()`.
    fn get_bounds(&self) -> &WorldBounds {
        &self.bounds
    }

    /// Java `@Override public boolean shouldReplace(int seed, double x, double
    /// z, int y)`.
    fn should_replace(&self, seed: i32, x: f64, z: f64, y: i32) -> bool {
        let mut fx = x;
        let mut fy = y as f64;
        let mut fz = z;

        fx -= self.o.x;
        fy -= self.o.y;
        fz -= self.o.z;

        fx /= self.rx;
        fy /= self.ry;
        fz /= self.rz;

        let t = self.cave_type.get_height_radius_factor(seed, x, z, y) as f64;
        fx * fx + fy * fy + fz * fz <= t * t
    }

    /// Java `@Override public double getFloorPosition(int seed, double x,
    /// double z)`.
    fn get_floor_position(&self, seed: i32, x: f64, z: f64) -> f64 {
        // Java: `for (y = getLowBoundY(); y < o.y; y++)` — the comparison is
        // against the *double* centre of the ellipsoid.
        let mut y = self.bounds.get_low_bound_y();
        while (y as f64) < self.o.y {
            if self.should_replace(seed, x, z, y) {
                return (y - 1) as f64;
            }
            y += 1;
        }
        -1.0
    }

    /// Java `@Override public double getCeilingPosition(int seed, double x,
    /// double z)`.
    fn get_ceiling_position(&self, seed: i32, x: f64, z: f64) -> f64 {
        let mut y = self.bounds.get_high_bound_y();
        while (y as f64) > self.o.y {
            if self.should_replace(seed, x, z, y) {
                return (y + 1) as f64;
            }
            y -= 1;
        }
        -1.0
    }

    /// Java `@Override public void populateChunk(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, @Nonnull Cave cave, @Nonnull CaveNode
    /// node, @Nonnull Random random)`.
    fn populate_chunk(
        &self,
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cave: &crate::cave::cave::Cave,
        node: &CaveNode,
        random: &mut FastRandom,
    ) {
        abstract_cave_node_shape::populate_chunk(self, seed, execution, cave, node, random);
    }
}

/// Java `public static class EllipsoidCaveNodeShapeGenerator implements
/// CaveNodeShapeEnum.CaveNodeShapeGenerator`.
pub struct EllipsoidCaveNodeShapeGenerator {
    /// Java `private final IDoubleRange radiusX`.
    radius_x: Rc<dyn IDoubleRange>,
    /// Java `private final IDoubleRange radiusY`.
    radius_y: Rc<dyn IDoubleRange>,
    /// Java `private final IDoubleRange radiusZ`.
    radius_z: Rc<dyn IDoubleRange>,
}

impl EllipsoidCaveNodeShapeGenerator {
    /// Java `public EllipsoidCaveNodeShapeGenerator(IDoubleRange radiusX,
    /// IDoubleRange radiusY, IDoubleRange radiusZ)`.
    pub fn new(
        radius_x: Rc<dyn IDoubleRange>,
        radius_y: Rc<dyn IDoubleRange>,
        radius_z: Rc<dyn IDoubleRange>,
    ) -> Self {
        Self {
            radius_x,
            radius_y,
            radius_z,
        }
    }
}

impl CaveNodeShapeGenerator for EllipsoidCaveNodeShapeGenerator {
    /// Java `@Nonnull @Override public CaveNodeShape
    /// generateCaveNodeShape(Random random, CaveType caveType, CaveNode
    /// parentNode, @Nonnull CaveNodeType.CaveNodeChildEntry childEntry, @Nonnull
    /// Vector3d origin, float yaw, float pitch)`.
    fn generate_cave_node_shape(
        &self,
        random: &mut FastRandom,
        cave_type: &CaveType,
        parent_node: Option<&CaveNode>,
        child_entry: Option<&CaveNodeChildEntry>,
        origin: Vector3d,
        _yaw: f32,
        _pitch: f32,
    ) -> Box<dyn CaveNodeShape> {
        // Java: `radiusX.getValue(random)` — the `Random` overload, which a
        // `Constant` range answers *without* consuming a draw.
        let rx = self.radius_x.get_value_random(random);
        let ry = self.radius_y.get_value_random(random);
        let rz = self.radius_z.get_value_random(random);

        // Java: `var offset = CaveNodeShapeUtils.getOffset(parentNode,
        // childEntry); origin.add(offset);`
        let offset = get_offset(parent_node, child_entry);
        let origin = Vector3d::new(
            origin.x + offset.x,
            origin.y + offset.y,
            origin.z + offset.z,
        );

        Box::new(EllipsoidCaveNodeShape::new(
            Rc::new(cave_type.clone()),
            origin,
            rx,
            ry,
            rz,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::test_stubs::{cave_type_with_height_factor, child_entry};
    use crate::util::bounds::IChunkBounds;

    /// A cave type whose height radius factor is always 1.
    fn test_cave_type() -> Rc<CaveType> {
        cave_type_with_height_factor("ellipsoid", 1.0)
    }

    /// An ellipsoid with the centre `(0, 40, 0)` and the radii `(4, 6, 8)`.
    fn ellipsoid() -> EllipsoidCaveNodeShape {
        EllipsoidCaveNodeShape::new(
            test_cave_type(),
            Vector3d::new(0.0, 40.0, 0.0),
            4.0,
            6.0,
            8.0,
        )
    }

    #[test]
    fn the_bounds_surround_the_ellipsoid() {
        let shape = ellipsoid();
        assert_eq!(shape.get_bounds().get_low_bound_x(), -4);
        assert_eq!(shape.get_bounds().get_high_bound_x(), 4);
        assert_eq!(shape.get_bounds().get_low_bound_y(), 34);
        assert_eq!(shape.get_bounds().get_high_bound_y(), 46);
        assert_eq!(shape.get_bounds().get_low_bound_z(), -8);
        assert_eq!(shape.get_bounds().get_high_bound_z(), 8);
    }

    #[test]
    fn the_start_is_the_centre_and_the_end_is_below_it() {
        let shape = ellipsoid();
        assert_eq!(shape.get_start(), Vector3d::new(0.0, 40.0, 0.0));
        assert_eq!(shape.get_end(), Vector3d::new(0.0, 34.0, 0.0));
    }

    #[test]
    fn the_shape_replaces_the_blocks_inside_the_ellipsoid() {
        let shape = ellipsoid();
        assert!(shape.should_replace(0, 0.0, 0.0, 40));
        assert!(shape.should_replace(0, 0.0, 0.0, 34));
        // On the surface of the ellipsoid (the test is inclusive).
        assert!(shape.should_replace(0, 4.0, 0.0, 40));
        // Outside of it.
        assert!(!shape.should_replace(0, 5.0, 0.0, 40));
        assert!(!shape.should_replace(0, 0.0, 9.0, 40));
        assert!(!shape.should_replace(0, 0.0, 0.0, 47));
    }

    #[test]
    fn the_floor_and_the_ceiling_are_the_block_below_and_above_the_blob() {
        let shape = ellipsoid();
        assert_eq!(shape.get_floor_position(0, 0.0, 0.0), 33.0);
        assert_eq!(shape.get_ceiling_position(0, 0.0, 0.0), 47.0);
        assert_eq!(shape.get_floor_position(0, 30.0, 0.0), -1.0);
    }

    #[test]
    fn the_generator_uses_the_random_overload_of_the_ranges() {
        let generator = EllipsoidCaveNodeShapeGenerator::new(
            Rc::new(procedural::noise::supplier::DoubleRange::constant(4.0)),
            Rc::new(procedural::noise::supplier::DoubleRange::constant(6.0)),
            Rc::new(procedural::noise::supplier::DoubleRange::constant(8.0)),
        );
        let mut random = FastRandom::new(1234);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &test_cave_type(),
            None,
            Some(&child_entry(Vector3d::ZERO)),
            Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            0.0,
        );

        // A `Constant` range does not consume a draw in the `Random`
        // overload.
        let mut reference = FastRandom::new(1234);
        assert_eq!(random.next_double(), reference.next_double());

        assert_eq!(shape.get_bounds().get_low_bound_x(), -4);
        assert_eq!(shape.get_bounds().get_high_bound_y(), 46);
    }
}
