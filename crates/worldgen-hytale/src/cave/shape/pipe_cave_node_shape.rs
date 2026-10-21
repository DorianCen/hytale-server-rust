// Ported from: `.hytale-source/.../worldgen/cave/shape/PipeCaveNodeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.PipeCaveNodeShape` —
//! a cave node that is a tube of a varying radius along a straight segment.
//!
//! The tube is capped with a *sphere* at both ends (the Java shape uses the
//! end radius for the whole cap), so the shape is slightly longer than the
//! segment `o → o + v`.
//!
//! # Differences to the cylinder
//!
//! The pipe and the cylinder share their geometry; they only differ in the
//! `shouldReplace` test: the cylinder rejects everything outside of the
//! segment (`t < 0 || t > 1`), the pipe extends the caps with the end
//! radius.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::math_util;
use hytale_math::trig_math_util;
use procedural::noise::supplier::IDoubleRange;

use crate::cave::cave_node_type::CaveNodeChildEntry;
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::abstract_cave_node_shape;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::cave::shape::cave_node_shape_utils::{
    get_end_radius, get_offset, get_pipe_anchor, WidthHeightSelector,
};
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::util::bounds::{IWorldBounds, WorldBounds};

/// Java `public class PipeCaveNodeShape extends AbstractCaveNodeShape
/// implements IWorldBounds`.
pub struct PipeCaveNodeShape {
    /// Java `private final CaveType caveType`.
    cave_type: Rc<CaveType>,
    /// Java `@Nonnull private final Vector3d o` — the start of the segment.
    o: Vector3d,
    /// Java `@Nonnull private final Vector3d v` — the direction of the
    /// segment.
    v: Vector3d,
    /// Java `private final int lowBoundX, lowBoundY, lowBoundZ, highBoundX,
    /// highBoundY, highBoundZ` — the shape *is* its own `IWorldBounds`.
    bounds: WorldBounds,
    /// Java `private final double radius1` — the radius at the start.
    radius1: f64,
    /// Java `private final double radius2` — the radius at the end.
    radius2: f64,
    /// Java `private final double middleRadius` — the radius in the middle.
    middle_radius: f64,
}

impl PipeCaveNodeShape {
    /// Java `public PipeCaveNodeShape(CaveType caveType, @Nonnull Vector3d o,
    /// @Nonnull Vector3d v, double radius1, double radius2, double
    /// middleRadius)`.
    pub fn new(
        cave_type: Rc<CaveType>,
        o: Vector3d,
        v: Vector3d,
        radius1: f64,
        radius2: f64,
        middle_radius: f64,
    ) -> Self {
        // Java:
        // ```java
        // lowBoundX = MathUtil.floor(Math.min(o.x, o.x + v.x) - Math.max(radius1, radius2));
        // …
        // highBoundX = MathUtil.ceil(Math.max(o.x, o.x + v.x) + Math.max(radius1, radius2));
        // ```
        let max_radius = radius1.max(radius2);
        let bounds = WorldBounds::with_bounds(
            math_util::floor(o.x.min(o.x + v.x) - max_radius),
            math_util::floor(o.y.min(o.y + v.y) - max_radius),
            math_util::floor(o.z.min(o.z + v.z) - max_radius),
            math_util::ceil(o.x.max(o.x + v.x) + max_radius),
            math_util::ceil(o.y.max(o.y + v.y) + max_radius),
            math_util::ceil(o.z.max(o.z + v.z) + max_radius),
        );

        Self {
            cave_type,
            o,
            v,
            bounds,
            radius1,
            radius2,
            middle_radius,
        }
    }

    /// Java `public double getRadius1()`.
    pub fn get_radius1(&self) -> f64 {
        self.radius1
    }

    /// Java `public double getRadius2()`.
    pub fn get_radius2(&self) -> f64 {
        self.radius2
    }

    /// Java `private double getRadiusAt(double t)`.
    pub fn get_radius_at(&self, t: f64) -> f64 {
        if t < 0.5 {
            (self.middle_radius - self.radius1) * 2.0 * t + self.radius1
        } else {
            (self.radius2 - self.middle_radius) * 2.0 * (t - 0.5) + self.middle_radius
        }
    }

    /// Java `private double projectPointOnNode(double px, double py, double
    /// pz)`.
    ///
    /// "Projects a given point on the direction vector. If the point is not
    /// orthogonal to the direction vector the method returns `Double.NaN` to
    /// signal a non-valid point input."
    pub fn project_point_on_node(&self, px: f64, py: f64, pz: f64) -> f64 {
        let mut t =
            (px - self.o.x) * self.v.x + (py - self.o.y) * self.v.y + (pz - self.o.z) * self.v.z;
        t /= self.v.x * self.v.x + self.v.y * self.v.y + self.v.z * self.v.z;
        t
    }
}

impl CaveNodeShape for PipeCaveNodeShape {
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
    fn get_anchor(&self, vector: Vector3d, t: f64, tv: f64, th: f64) -> Vector3d {
        let radius = self.get_radius_at(t);
        get_pipe_anchor(vector, self.o, self.v, radius, radius, radius, t, tv, th)
    }

    /// Java `@Nonnull @Override public IWorldBounds getBounds()`.
    fn get_bounds(&self) -> &WorldBounds {
        &self.bounds
    }

    /// Java `@Override public boolean shouldReplace(int seed, double x, double
    /// z, int y)`.
    fn should_replace(&self, seed: i32, x: f64, z: f64, y: i32) -> bool {
        let t = self.project_point_on_node(x, y as f64, z);
        let (dx, dy, dz, mut r);
        if t < 0.0 {
            dx = self.o.x;
            dy = self.o.y;
            dz = self.o.z;
            r = self.radius1;
        } else if t > 1.0 {
            dx = self.o.x + self.v.x;
            dy = self.o.y + self.v.y;
            dz = self.o.z + self.v.z;
            r = self.radius2;
        } else {
            dx = self.o.x + self.v.x * t;
            dy = self.o.y + self.v.y * t;
            dz = self.o.z + self.v.z * t;
            r = self.get_radius_at(t);
        }
        r *= self
            .cave_type
            .get_height_radius_factor(seed, x, z, math_util::floor(dy)) as f64;
        let dx = dx - x;
        let dy = dy - y as f64;
        let dz = dz - z;
        dx * dx + dy * dy + dz * dz < r * r
    }

    /// Java `@Override public double getFloorPosition(int seed, double x,
    /// double z)`.
    fn get_floor_position(&self, seed: i32, x: f64, z: f64) -> f64 {
        let mut y = self.bounds.get_low_bound_y();
        while y < self.bounds.get_high_bound_y() {
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
        while y > self.bounds.get_low_bound_y() {
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

    /// Java `CaveNodeShapeUtils.getEndRadius(shape, MIN)` — the pipe exposes
    /// a single radius, so every selector returns the end radius.
    fn get_end_radius(&self, _selector: WidthHeightSelector) -> f64 {
        self.radius2
    }
}

/// Java `public static class PipeCaveNodeShapeGenerator implements
/// CaveNodeShapeEnum.CaveNodeShapeGenerator`.
pub struct PipeCaveNodeShapeGenerator {
    /// Java `private final IDoubleRange radius`.
    radius: Rc<dyn IDoubleRange>,
    /// Java `private final IDoubleRange middleRadius` — `null` when the
    /// config does not define one.
    middle_radius: Option<Rc<dyn IDoubleRange>>,
    /// Java `private final IDoubleRange length`.
    length: Rc<dyn IDoubleRange>,
    /// Java `private final boolean inheritParentRadius`.
    inherit_parent_radius: bool,
}

impl PipeCaveNodeShapeGenerator {
    /// Java `public PipeCaveNodeShapeGenerator(IDoubleRange radius, IDoubleRange
    /// middleRadius, IDoubleRange length, boolean inheritParentRadius)`.
    pub fn new(
        radius: Rc<dyn IDoubleRange>,
        middle_radius: Option<Rc<dyn IDoubleRange>>,
        length: Rc<dyn IDoubleRange>,
        inherit_parent_radius: bool,
    ) -> Self {
        Self {
            radius,
            middle_radius,
            length,
            inherit_parent_radius,
        }
    }
}

impl CaveNodeShapeGenerator for PipeCaveNodeShapeGenerator {
    /// Java `@Nonnull @Override public CaveNodeShape
    /// generateCaveNodeShape(@Nonnull Random random, CaveType caveType,
    /// CaveNode parentNode, @Nonnull CaveNodeType.CaveNodeChildEntry
    /// childEntry, @Nonnull Vector3d origin, float yaw, float pitch)`.
    fn generate_cave_node_shape(
        &self,
        random: &mut FastRandom,
        cave_type: &CaveType,
        parent_node: Option<&CaveNode>,
        child_entry: Option<&CaveNodeChildEntry>,
        origin: Vector3d,
        yaw: f32,
        pitch: f32,
    ) -> Box<dyn CaveNodeShape> {
        // Java: `final double l = length.getValue(random.nextDouble());` —
        // the double is drawn *before* the range maps it, so even a
        // `Constant` range consumes a draw here.
        let l = self.length.get_value(random.next_double());

        // Java:
        // ```java
        // final Vector3d direction = new Vector3d(
        //     sin(pitch) * cos(yaw),
        //     cos(pitch),
        //     sin(pitch) * sin(yaw)
        // ).mul(l);
        // ```
        // The trigonometry is `float` precision (the Java `TrigMathUtil`
        // table lookup), the multiplication by `l` is `double`.
        let sin_pitch = trig_math_util::sin(pitch);
        let cos_pitch = trig_math_util::cos(pitch);
        let sin_yaw = trig_math_util::sin(yaw);
        let cos_yaw = trig_math_util::cos(yaw);
        let direction = Vector3d::new(
            (sin_pitch * cos_yaw) as f64 * l,
            cos_pitch as f64 * l,
            (sin_pitch * sin_yaw) as f64 * l,
        );

        let radius1 = if self.inherit_parent_radius {
            // Java: `CaveNodeShapeUtils.getEndRadius(parentNode, radius,
            // random)` — the `Random` overload of the range, which does
            // *not* consume a draw for a `Constant` range.
            get_end_radius(parent_node, self.radius.as_ref(), random)
        } else {
            // Java: `radius.getValue(random.nextDouble())`.
            self.radius.get_value(random.next_double())
        };
        let radius2 = self.radius.get_value(random.next_double());

        let mut radius3 = (radius2 - radius1) * 0.5 + radius1;
        if let Some(middle_radius) = &self.middle_radius {
            // Java: `radius3 = middleRadius.getValue(random.nextDouble());` —
            // only drawn when the config defines a middle radius.
            radius3 = middle_radius.get_value(random.next_double());
        }

        // Java: `var offset = CaveNodeShapeUtils.getOffset(parentNode,
        // childEntry); origin.add(offset);`
        let offset = get_offset(parent_node, child_entry);
        let origin = Vector3d::new(
            origin.x + offset.x,
            origin.y + offset.y,
            origin.z + offset.z,
        );

        Box::new(PipeCaveNodeShape::new(
            Rc::new(cave_type.clone()),
            origin,
            direction,
            radius1,
            radius2,
            radius3,
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
        cave_type_with_height_factor("pipe", 1.0)
    }

    /// A pipe from `(0, 40, 0)` to `(16, 40, 0)` with a constant radius.
    fn pipe(radius: f64) -> PipeCaveNodeShape {
        PipeCaveNodeShape::new(
            test_cave_type(),
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(16.0, 0.0, 0.0),
            radius,
            radius,
            radius,
        )
    }

    #[test]
    fn the_bounds_surround_the_segment_with_the_radius() {
        let shape = pipe(3.0);
        assert_eq!(shape.get_bounds().get_low_bound_x(), -3);
        assert_eq!(shape.get_bounds().get_high_bound_x(), 19);
        assert_eq!(shape.get_bounds().get_low_bound_y(), 37);
        assert_eq!(shape.get_bounds().get_high_bound_y(), 43);
        assert_eq!(shape.get_bounds().get_low_bound_z(), -3);
        assert_eq!(shape.get_bounds().get_high_bound_z(), 3);
    }

    #[test]
    fn the_start_and_the_end_are_the_segment_ends() {
        let shape = pipe(3.0);
        assert_eq!(shape.get_start(), Vector3d::new(0.0, 40.0, 0.0));
        assert_eq!(shape.get_end(), Vector3d::new(16.0, 40.0, 0.0));
    }

    #[test]
    fn the_shape_replaces_the_blocks_inside_the_tube() {
        let shape = pipe(3.0);
        // The centre of the tube.
        assert!(shape.should_replace(0, 8.0, 0.0, 40));
        // Just outside of the radius.
        assert!(!shape.should_replace(0, 8.0, 0.0, 44));
        assert!(!shape.should_replace(0, 8.0, 4.0, 40));
    }

    #[test]
    fn the_pipe_caps_its_ends_with_the_end_radius() {
        let shape = pipe(3.0);
        // Before the start of the segment the cap is still there (the
        // cylinder would reject the block).
        assert!(shape.should_replace(0, -2.0, 0.0, 40));
        assert!(!shape.should_replace(0, -5.0, 0.0, 40));
        // Behind the end of the segment.
        assert!(shape.should_replace(0, 18.0, 0.0, 40));
        assert!(!shape.should_replace(0, 21.0, 0.0, 40));
    }

    #[test]
    fn the_radius_interpolates_through_the_middle_radius() {
        let shape = PipeCaveNodeShape::new(
            test_cave_type(),
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(10.0, 0.0, 0.0),
            2.0,
            4.0,
            6.0,
        );
        assert_eq!(shape.get_radius_at(0.0), 2.0);
        assert_eq!(shape.get_radius_at(0.5), 6.0);
        assert_eq!(shape.get_radius_at(1.0), 4.0);
        // The first half interpolates from the start radius to the middle
        // radius.
        assert_eq!(shape.get_radius_at(0.25), 4.0);
    }

    #[test]
    fn the_floor_and_the_ceiling_are_the_block_below_and_above_the_tube() {
        let shape = pipe(3.0);
        // The tube reaches from y = 38 to y = 42 (the radius is 3 and the
        // test is exclusive).
        assert_eq!(shape.get_floor_position(0, 8.0, 0.0), 37.0);
        assert_eq!(shape.get_ceiling_position(0, 8.0, 0.0), 43.0);
        // Outside of the tube there is neither a floor nor a ceiling.
        assert_eq!(shape.get_floor_position(0, 8.0, 40.0), -1.0);
        assert_eq!(shape.get_ceiling_position(0, 8.0, 40.0), -1.0);
    }

    #[test]
    fn the_end_radius_is_the_radius_at_the_end() {
        let shape = PipeCaveNodeShape::new(
            test_cave_type(),
            Vector3d::new(0.0, 40.0, 0.0),
            Vector3d::new(10.0, 0.0, 0.0),
            2.0,
            4.0,
            6.0,
        );
        assert_eq!(shape.get_radius1(), 2.0);
        assert_eq!(shape.get_radius2(), 4.0);
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Min), 4.0);
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Left), 4.0);
    }

    #[test]
    fn the_generator_draws_in_the_java_order() {
        // Java: `length.getValue(random.nextDouble())` → one draw, then
        // `radius.getValue(random.nextDouble())` twice, and no draw for the
        // middle radius when it is not configured.
        let generator = PipeCaveNodeShapeGenerator::new(
            Rc::new(procedural::noise::supplier::DoubleRange::constant(4.0)),
            None,
            Rc::new(procedural::noise::supplier::DoubleRange::constant(10.0)),
            false,
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

        // The reference: the same draws, done by hand.
        let mut reference = FastRandom::new(1234);
        let l = reference.next_double();
        let _radius1 = reference.next_double();
        let _radius2 = reference.next_double();
        assert_eq!(
            random.next_double(),
            reference.next_double(),
            "the generator consumed exactly three draws"
        );
        assert!(l > 0.0);

        // The direction is the yaw/pitch vector scaled by the length: with a
        // yaw and a pitch of zero the tube points straight up.
        assert_eq!(shape.get_end().x, 0.0);
        assert_eq!(shape.get_end().y, 50.0);
        assert_eq!(shape.get_end().z, 0.0);
    }

    #[test]
    fn the_generator_draws_the_middle_radius_only_when_it_is_configured() {
        let generator = PipeCaveNodeShapeGenerator::new(
            Rc::new(procedural::noise::supplier::DoubleRange::constant(4.0)),
            Some(Rc::new(procedural::noise::supplier::DoubleRange::constant(
                8.0,
            ))),
            Rc::new(procedural::noise::supplier::DoubleRange::constant(10.0)),
            false,
        );
        let mut random = FastRandom::new(1234);
        let _ = generator.generate_cave_node_shape(
            &mut random,
            &test_cave_type(),
            None,
            Some(&child_entry(Vector3d::ZERO)),
            Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            0.0,
        );

        let mut reference = FastRandom::new(1234);
        let _ = reference.next_double();
        let _ = reference.next_double();
        let _ = reference.next_double();
        let _ = reference.next_double();
        assert_eq!(random.next_double(), reference.next_double());
    }
}
