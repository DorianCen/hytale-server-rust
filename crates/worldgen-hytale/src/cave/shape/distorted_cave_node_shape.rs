// Ported from: `.hytale-source/.../worldgen/cave/shape/DistortedCaveNodeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.DistortedCaveNodeShape`
//! — the cave node shape that wraps a
//! [`DistortedShape`](super::distorted::distorted_shape::DistortedShape) and
//! carves it into the chunk.
//!
//! Unlike the 4c shapes (pipe / cylinder / ellipsoid / empty line) this class
//! does **not** extend `AbstractCaveNodeShape`: it has its own `populateChunk`
//! because the whole vertical range of a column comes from the distorted
//! shape's `getFloor` / `getCeiling`, not from a `shouldReplace` test per
//! block. The column loop is otherwise the same (carve, place covers,
//! invalidate unsupported covers).
//!
//! # The height distortion
//!
//! The width distortion lives inside
//! [`DistortedShape::get_height_at_projection`] (it contracts the horizontal
//! radius). The floor / ceiling distortions are applied here, once per
//! column:
//!
//! ```text
//! floor   = max(floor(shape.getFloor(x, z, centerY, height * floorFactor)), minY)
//! ceiling = min(ceil (shape.getCeiling(x, z, centerY, height * ceilingFactor)), maxY)
//! ```
//!
//! # Java → Rust
//!
//! * `DistortedShape` (a Java interface) is a boxed trait object.
//! * Java's `CaveNodeShapeUtils.getEndRadius(shape, MIN)` branch is the
//!   [`CaveNodeShape::get_end_radius`] override.
//! * The `DistortedCaveNodeShapeGenerator` keeps the exact Java random draw
//!   order (see [`DistortedCaveNodeShapeGenerator::generate_cave_node_shape`]).
//! * `@Nullable InterpolationFunction` → `Option<InterpolationMode>`; using a
//!   `None` interpolation panics like the Java dereference would.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use hytale_math::{chunk_util, math_util, trig_math_util};
use procedural::noise::supplier::IDoubleRange;
use procedural::noise::InterpolationMode;
use worldgen_model::block_priority;

use crate::cave::cave::Cave;
use crate::cave::cave_node_type::{CaveNodeChildEntry, CaveNodeCoverType};
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::abstract_cave_node_shape::chunk_height;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::cave::shape::cave_node_shape_utils::{
    get_cover_height, get_end_height, get_end_width, get_filling_block, get_offset,
    invalidate_cover, is_cover_matching_parent, WidthHeightSelector,
};
use crate::cave::shape::distorted::abstract_distorted_shape::AbstractDistortedShape;
use crate::cave::shape::distorted::distorted_shape::DistortedShape;
use crate::cave::shape::distorted::distorted_shapes::DistortedShapeFactory;
use crate::cave::shape::shape_distortion::ShapeDistortion;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::util::bounds::{IChunkBounds, IWorldBounds, WorldBounds};

/// Java `public class DistortedCaveNodeShape implements CaveNodeShape`.
pub struct DistortedCaveNodeShape {
    /// Java `private final CaveType caveType`.
    cave_type: Rc<CaveType>,
    /// Java `private final DistortedShape shape`.
    shape: Box<dyn DistortedShape>,
    /// Java `private final ShapeDistortion distortion`.
    distortion: ShapeDistortion,
}

impl DistortedCaveNodeShape {
    /// Java `public DistortedCaveNodeShape(CaveType caveType, DistortedShape
    /// shape, ShapeDistortion distortion)`.
    pub fn new(
        cave_type: Rc<CaveType>,
        shape: Box<dyn DistortedShape>,
        distortion: ShapeDistortion,
    ) -> Self {
        Self {
            cave_type,
            shape,
            distortion,
        }
    }

    /// Java `public DistortedShape getShape()`.
    pub fn get_shape(&self) -> &dyn DistortedShape {
        self.shape.as_ref()
    }

    /// Java `private int getFloor(int seed, double x, double z, double centerY,
    /// double height, int minY)`.
    fn get_floor(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        center_y: f64,
        mut height: f64,
        min_y: i32,
    ) -> i32 {
        height *= self.distortion.get_floor_factor(seed, x, z);
        let floor_y = self.shape.get_floor(x, z, center_y, height);
        math_util::floor(floor_y).max(min_y)
    }

    /// Java `private int getCeiling(int seed, double x, double z, double
    /// centerY, double height, int maxY)`.
    fn get_ceiling(
        &self,
        seed: i32,
        x: f64,
        z: f64,
        center_y: f64,
        mut height: f64,
        max_y: i32,
    ) -> i32 {
        height *= self.distortion.get_ceiling_factor(seed, x, z);
        let ceiling_y = self.shape.get_ceiling(x, z, center_y, height);
        math_util::ceil(ceiling_y).min(max_y)
    }
}

impl CaveNodeShape for DistortedCaveNodeShape {
    /// Java `@Override public Vector3d getStart()`.
    fn get_start(&self) -> Vector3d {
        self.shape.get_start()
    }

    /// Java `@Override public Vector3d getEnd()`.
    fn get_end(&self) -> Vector3d {
        self.shape.get_end()
    }

    /// Java `@Override public Vector3d getAnchor(Vector3d vector, double tx,
    /// double ty, double tz)`.
    fn get_anchor(&self, vector: Vector3d, tx: f64, ty: f64, tz: f64) -> Vector3d {
        self.shape.get_anchor(vector, tx, ty, tz)
    }

    /// Java `@Override public IWorldBounds getBounds()` — the shape *is* its
    /// own bounds.
    fn get_bounds(&self) -> &WorldBounds {
        self.shape.get_bounds()
    }

    /// Java `@Override public boolean shouldReplace(int seed, double x, double
    /// z, int y)`.
    fn should_replace(&self, seed: i32, x: f64, z: f64, y: i32) -> bool {
        let t = self.shape.get_projection(x, z);
        if self.shape.is_valid_projection(t) {
            let center_y = self.shape.get_y_at(t);
            let shape_height = self.shape.get_height_at_projection(
                seed,
                x,
                z,
                t,
                center_y,
                &self.cave_type,
                &self.distortion,
            );
            if shape_height > 0.0 {
                let min_y = self.get_bounds().get_low_bound_y();
                let floor = self.get_floor(seed, x, z, center_y, shape_height, min_y);
                if y < floor {
                    return false;
                }

                let max_y = self.get_bounds().get_high_bound_y();
                let ceiling = self.get_ceiling(seed, x, z, center_y, shape_height, max_y);
                return y <= ceiling;
            }
        }
        false
    }

    /// Java `@Override public double getFloorPosition(int seed, double x,
    /// double z)`.
    fn get_floor_position(&self, seed: i32, x: f64, z: f64) -> f64 {
        let t = self.shape.get_projection(x, z);
        if self.shape.is_valid_projection(t) {
            let center_y = self.shape.get_y_at(t);
            let shape_height = self.shape.get_height_at_projection(
                seed,
                x,
                z,
                t,
                center_y,
                &self.cave_type,
                &self.distortion,
            );
            if shape_height > 0.0 {
                let min_y = self.get_bounds().get_low_bound_y();
                // Java: "getFloor returns the lowest air block so subtract 1
                // to get the block beneath"
                return (self.get_floor(seed, x, z, center_y, shape_height, min_y) - 1) as f64;
            }
        }
        -1.0
    }

    /// Java `@Override public double getCeilingPosition(int seed, double x,
    /// double z)`.
    fn get_ceiling_position(&self, seed: i32, x: f64, z: f64) -> f64 {
        let t = self.shape.get_projection(x, z);
        if self.shape.is_valid_projection(t) {
            let center_y = self.shape.get_y_at(t);
            let shape_height = self.shape.get_height_at_projection(
                seed,
                x,
                z,
                t,
                center_y,
                &self.cave_type,
                &self.distortion,
            );
            if shape_height > 0.0 {
                let max_y = self.get_bounds().get_high_bound_y();
                // Java: "getCeiling returns the highest air block so add 1 to
                // get the block above"
                return (self.get_ceiling(seed, x, z, center_y, shape_height, max_y) + 1) as f64;
            }
        }
        -1.0
    }

    /// Java `@Override public void populateChunk(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, @Nonnull Cave cave, @Nonnull CaveNode
    /// node, @Nonnull Random random)`.
    ///
    /// The Java class has its own implementation (it does not extend
    /// `AbstractCaveNodeShape`): the `floorY` / `ceilingY` pair is computed
    /// once per column from the distorted shape and the whole range is carved
    /// without a per-block `shouldReplace` test.
    fn populate_chunk(
        &self,
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        cave: &Cave,
        node: &CaveNode,
        random: &mut FastRandom,
    ) {
        let cave_type = cave.get_cave_type();
        let cave_node_type = node.get_cave_node_type();
        let shape_bounds = self.get_bounds();
        let surface_limited = cave_type.is_surface_limited();
        let environment = if cave_node_type.has_environment() {
            cave_node_type.get_environment()
        } else {
            cave_type.get_environment()
        };

        let chunk_low_x = chunk_util::min_block(execution.get_x());
        let chunk_low_z = chunk_util::min_block(execution.get_z());
        let chunk_high_x = chunk_util::max_block(execution.get_x());
        let chunk_high_z = chunk_util::max_block(execution.get_z());

        let min_x = chunk_low_x.max(shape_bounds.get_low_bound_x());
        let min_y = shape_bounds.get_low_bound_y();
        let min_z = chunk_low_z.max(shape_bounds.get_low_bound_z());
        let max_x = chunk_high_x.min(shape_bounds.get_high_bound_x());
        let max_y = shape_bounds.get_high_bound_y();
        let max_z = chunk_high_z.min(shape_bounds.get_high_bound_z());

        // 1. Iterate the xz plane
        // 2. Check if point xz inside the horizontal radius of the shape
        // 3. Calculate the ceiling/floor levels at point xz and carve column
        for x in min_x..=max_x {
            let cx = x - chunk_low_x;
            for z in min_z..=max_z {
                let cz = z - chunk_low_z;

                // Calculate the maximum height to generate the cave at
                let mut maximum_y = max_y;
                let mut height_limited = false;
                if surface_limited {
                    let column_height =
                        chunk_height(execution.get_chunk(), execution.registry(), cx, cz);
                    if maximum_y >= column_height {
                        maximum_y = column_height;
                        height_limited = true;
                    }
                }

                // Lowest block of this node to add covers
                let mut lowest = i32::MAX;
                let mut lowest_possible = i32::MAX;

                // Highest block of this node to remove covers on top of the
                // node
                let mut highest = i32::MIN;
                let mut highest_possible = i32::MIN;

                // Get t on the x/z plane
                let t = self.shape.get_projection(x as f64, z as f64);
                if !self.shape.is_valid_projection(t) {
                    continue;
                }

                // The y-level where point x,z intersects the center/center-line
                // of the shape
                let center_y = self.shape.get_y_at(t);
                let shape_height = self.shape.get_height_at_projection(
                    seed,
                    x as f64,
                    z as f64,
                    t,
                    center_y,
                    cave_type,
                    &self.distortion,
                );
                if shape_height <= 0.0 {
                    continue;
                }

                // The ceiling/floor y-levels of the shape at xz
                let floor_y =
                    self.get_floor(seed, x as f64, z as f64, center_y, shape_height, min_y);
                let ceiling_y =
                    self.get_ceiling(seed, x as f64, z as f64, center_y, shape_height, maximum_y);

                if floor_y < lowest_possible {
                    lowest_possible = floor_y;
                }
                if ceiling_y > highest_possible {
                    highest_possible = ceiling_y;
                }

                for y in floor_y..=ceiling_y {
                    let current = execution.get_block(cx, y, cz);
                    let current_fluid = execution.get_fluid(cx, y, cz);
                    let is_candidate_block = !surface_limited || current != 0;
                    if is_candidate_block {
                        let block_entry = get_filling_block(cave_type, cave_node_type, y, random);
                        if cave_type.get_block_mask().eval(
                            current,
                            current_fluid,
                            block_entry.block_id,
                            block_entry.fluid_id,
                        ) {
                            if execution.set_block_entry_with_environment(
                                cx,
                                y,
                                cz,
                                block_priority::CAVE,
                                &block_entry,
                                environment,
                            ) {
                                if y < lowest {
                                    lowest = y;
                                }
                                if y > highest {
                                    highest = y;
                                }
                            }

                            if execution.set_fluid_with_environment(
                                cx,
                                y,
                                cz,
                                block_priority::CAVE,
                                block_entry.fluid_id,
                                environment,
                            ) {
                                if y < lowest {
                                    lowest = y;
                                }
                                if y > highest {
                                    highest = y;
                                }
                            }
                        }
                    }
                }

                let covers = cave_node_type.get_covers();
                for cover in covers {
                    let entry = cover.get(random).expect(
                        "DistortedCaveNodeShape.populateChunk: the cover has no entry (Java: NullPointerException)",
                    );
                    let y = get_cover_height(
                        lowest,
                        lowest_possible,
                        highest,
                        highest_possible,
                        height_limited,
                        cover,
                        entry,
                    );

                    if y >= 0
                        && cover.get_density_condition().eval2(
                            seed.wrapping_add(node.get_seed_offset()),
                            x,
                            z,
                        )
                        && cover.get_height_condition().eval(seed, x, z, y, random)
                        && cover.get_map_condition().eval2(seed, x, z)
                        && is_cover_matching_parent(cx, cz, y, execution, cover)
                    {
                        let cover_entry = entry.get_entry();
                        execution.set_block_entry_with_environment(
                            cx,
                            y,
                            cz,
                            block_priority::CAVE_COVER,
                            &cover_entry,
                            environment,
                        );
                        execution.set_fluid_with_environment(
                            cx,
                            y,
                            cz,
                            block_priority::CAVE_COVER,
                            cover_entry.fluid_id,
                            environment,
                        );
                    }
                }

                // Remove unsupported ceiling covers where we've intersected the
                // ceiling of another cave from above
                if invalidate_cover(
                    cx,
                    lowest.wrapping_sub(1),
                    cz,
                    CaveNodeCoverType::Ceiling,
                    execution,
                    &execution.registry().blocks,
                ) {
                    let block_entry = get_filling_block(
                        cave_type,
                        cave_node_type,
                        lowest.wrapping_sub(1),
                        random,
                    );
                    execution.override_block_entry(
                        cx,
                        lowest.wrapping_sub(1),
                        cz,
                        block_priority::CAVE,
                        &block_entry,
                    );
                    let fluid_level = execution
                        .registry()
                        .fluids
                        .get_max_fluid_level(block_entry.fluid_id);
                    execution.override_fluid(
                        cx,
                        lowest.wrapping_sub(1),
                        cz,
                        block_priority::CAVE,
                        block_entry.fluid_id,
                        fluid_level,
                    );
                }

                // Remove unsupported floor covers where we've breached surface
                // or intersected the floor of another cave from below
                if invalidate_cover(
                    cx,
                    highest.wrapping_add(1),
                    cz,
                    CaveNodeCoverType::Floor,
                    execution,
                    &execution.registry().blocks,
                ) {
                    let block_entry = get_filling_block(
                        cave_type,
                        cave_node_type,
                        highest.wrapping_add(1),
                        random,
                    );
                    execution.override_block_entry(
                        cx,
                        highest.wrapping_add(1),
                        cz,
                        block_priority::CAVE,
                        &block_entry,
                    );
                    let fluid_level = execution
                        .registry()
                        .fluids
                        .get_max_fluid_level(block_entry.fluid_id);
                    execution.override_fluid(
                        cx,
                        highest.wrapping_add(1),
                        cz,
                        block_priority::CAVE,
                        block_entry.fluid_id,
                        fluid_level,
                    );
                }
            }
        }
    }

    /// Java `CaveNodeShapeUtils.getEndRadius(shape, selector)` — the distorted
    /// shape exposes a width *and* a height, so the selector decides.
    fn get_end_radius(&self, selector: WidthHeightSelector) -> f64 {
        let width = self.shape.get_width_at(1.0);
        let height = self.shape.get_height_at(1.0);
        selector.apply(width, height)
    }
}

/// Java `public static class DistortedCaveNodeShapeGenerator implements
/// CaveNodeShapeEnum.CaveNodeShapeGenerator`.
pub struct DistortedCaveNodeShapeGenerator {
    /// Java `private final DistortedShape.Factory shapeFactory`.
    shape_factory: &'static dyn DistortedShapeFactory,
    /// Java `private final IDoubleRange widthRange`.
    width_range: Rc<dyn IDoubleRange>,
    /// Java `private final IDoubleRange heightRange`.
    height_range: Rc<dyn IDoubleRange>,
    /// Java `@Nullable private final IDoubleRange midWidthRange`.
    mid_width_range: Option<Rc<dyn IDoubleRange>>,
    /// Java `@Nullable private final IDoubleRange midHeightRange`.
    mid_height_range: Option<Rc<dyn IDoubleRange>>,
    /// Java `@Nullable private final IDoubleRange lengthRange`.
    length_range: Option<Rc<dyn IDoubleRange>>,
    /// Java `private final boolean inheritParentRadius`.
    inherit_parent_radius: bool,
    /// Java `private final ShapeDistortion distortion`.
    distortion: ShapeDistortion,
    /// Java `@Nullable private final GeneralNoise.InterpolationFunction
    /// interpolation`.
    interpolation: Option<InterpolationMode>,
}

impl DistortedCaveNodeShapeGenerator {
    /// Java `public DistortedCaveNodeShapeGenerator(DistortedShape.Factory
    /// shapeFactory, IDoubleRange widthRange, IDoubleRange heightRange,
    /// @Nullable IDoubleRange midWidthRange, @Nullable IDoubleRange
    /// midHeightRange, @Nullable IDoubleRange lengthRange, boolean
    /// inheritParentRadius, ShapeDistortion distortion, @Nullable
    /// GeneralNoise.InterpolationFunction interpolation)`.
    #[allow(clippy::too_many_arguments)] // The Java constructor has nine parameters.
    pub fn new(
        shape_factory: &'static dyn DistortedShapeFactory,
        width_range: Rc<dyn IDoubleRange>,
        height_range: Rc<dyn IDoubleRange>,
        mid_width_range: Option<Rc<dyn IDoubleRange>>,
        mid_height_range: Option<Rc<dyn IDoubleRange>>,
        length_range: Option<Rc<dyn IDoubleRange>>,
        inherit_parent_radius: bool,
        distortion: ShapeDistortion,
        interpolation: Option<InterpolationMode>,
    ) -> Self {
        Self {
            shape_factory,
            width_range,
            height_range,
            mid_width_range,
            mid_height_range,
            length_range,
            inherit_parent_radius,
            distortion,
            interpolation,
        }
    }
}

impl CaveNodeShapeGenerator for DistortedCaveNodeShapeGenerator {
    /// Java `@Nonnull @Override public CaveNodeShape
    /// generateCaveNodeShape(@Nonnull Random random, CaveType caveType,
    /// @Nullable CaveNode parentNode, @Nonnull CaveNodeType.CaveNodeChildEntry
    /// childEntry, @Nonnull Vector3d position, float yaw, float pitch)`.
    ///
    /// The draw order is part of the contract (a `DoubleRange.Constant` does
    /// not consume a draw through the `Random` overload):
    ///
    /// 1. `lengthRange.getValue(random)` — only when a length range is set,
    /// 2. `startWidth`, 3. `startHeight`,
    /// 4. `endWidth`, 5. `endHeight`,
    /// 6. `midWidth` — only when a middle-width range is set,
    /// 7. `midHeight` — only when a middle-height range is set.
    ///
    /// The origin / direction calculations in between draw nothing.
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
        // Java: `double length = getLength(lengthRange, random);`
        let length = get_length(self.length_range.as_deref(), random);

        // Java: `Vector3d origin = getOrigin(position, parentNode,
        // childEntry);`
        let origin = get_origin(origin, parent_node, child_entry);

        // Java: `Vector3d direction = getDirection(yaw, pitch, length);`
        let direction = get_direction(yaw, pitch, length);

        // Java: `double startWidth = getStartWidth(inheritParentRadius,
        // parentNode, widthRange, random);`
        let start_width = if self.inherit_parent_radius {
            get_end_width(parent_node, self.width_range.as_ref(), random)
        } else {
            self.width_range.get_value_random(random)
        };
        // Java: `double startHeight = getStartHeight(inheritParentRadius,
        // parentNode, heightRange, random);`
        let start_height = if self.inherit_parent_radius {
            get_end_height(parent_node, self.height_range.as_ref(), random)
        } else {
            self.height_range.get_value_random(random)
        };
        // Java: `double endWidth = widthRange.getValue(random);`
        let end_width = self.width_range.get_value_random(random);
        // Java: `double endHeight = heightRange.getValue(random);`
        let end_height = self.height_range.get_value_random(random);
        // Java: `double midWidth = getMiddleRadius(startWidth, endWidth,
        // midWidthRange, random);`
        let mid_width = get_middle_radius(
            start_width,
            end_width,
            self.mid_width_range.as_deref(),
            random,
        );
        // Java: `double midHeight = getMiddleRadius(startHeight, endHeight,
        // midHeightRange, random);`
        let mid_height = get_middle_radius(
            start_height,
            end_height,
            self.mid_height_range.as_deref(),
            random,
        );

        let interpolation = self.interpolation.expect(
            "DistortedCaveNodeShapeGenerator: the interpolation function is null (Java: NullPointerException)",
        );

        let shape = self.shape_factory.create(
            origin,
            direction,
            length,
            start_width,
            start_height,
            mid_width,
            mid_height,
            end_width,
            end_height,
            interpolation,
        );

        Box::new(DistortedCaveNodeShape::new(
            Rc::new(cave_type.clone()),
            shape,
            self.distortion.clone(),
        ))
    }
}

/// Java `@Nonnull private static Vector3d getOrigin(@Nonnull Vector3d origin,
/// @Nullable CaveNode parentNode, @Nonnull CaveNodeType.CaveNodeChildEntry
/// childEntry)`.
fn get_origin(
    origin: Vector3d,
    parent_node: Option<&CaveNode>,
    child_entry: Option<&CaveNodeChildEntry>,
) -> Vector3d {
    if parent_node.is_none() {
        return origin;
    }

    let offset = get_offset(parent_node, child_entry);

    // Java: `origin.add(offset)` — the return value is what the caller keeps.
    Vector3d::new(
        origin.x + offset.x,
        origin.y + offset.y,
        origin.z + offset.z,
    )
}

/// Java `private static double getLength(@Nullable IDoubleRange lengthRange,
/// Random random)`.
fn get_length(length_range: Option<&dyn IDoubleRange>, random: &mut FastRandom) -> f64 {
    match length_range {
        None => 0.0,
        Some(length_range) => length_range.get_value_random(random),
    }
}

/// Java `@Nonnull private static Vector3d getDirection(double yaw, double
/// pitch, double length)`.
///
/// The trigonometric lookup is `float` precision (`TrigMathUtil.sin(double)`
/// casts to `float`), the multiplication by `length` is `double`.
fn get_direction(yaw: f32, pitch: f32, length: f64) -> Vector3d {
    if length == 0.0 {
        return Vector3d::new(0.0, 0.0, 0.0);
    }

    let pitch = AbstractDistortedShape::clamp_pitch(pitch as f64);

    Vector3d::new(
        (trig_math_util::sin_f64(pitch) * trig_math_util::cos(yaw)) as f64 * length,
        trig_math_util::cos_f64(pitch) as f64 * length,
        (trig_math_util::sin_f64(pitch) * trig_math_util::sin(yaw)) as f64 * length,
    )
}

/// Java `private static double getMiddleRadius(double start, double end,
/// @Nullable IDoubleRange range, Random random)`.
fn get_middle_radius(
    start: f64,
    end: f64,
    range: Option<&dyn IDoubleRange>,
    random: &mut FastRandom,
) -> f64 {
    match range {
        None => (end - start) * 0.5 + start,
        Some(range) => range.get_value_random(random),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use procedural::noise::property::NoiseProperty;
    use procedural::noise::supplier::DoubleRange;

    use crate::cave::shape::distorted::distorted_cylinder_shape::DistortedCylinderShape;
    use crate::cave::shape::distorted::distorted_shape::DEFAULT_INTERPOLATION;
    use crate::cave::shape::distorted::distorted_shapes::DistortedShapes;
    use crate::cave::test_stubs::{
        cave_node_type_with_filling, cave_type_with_height_factor, child_entry, chunk_generator,
    };
    use crate::util::block_fluid_entry::BlockFluidEntry;
    use worldgen_model::chunk::GeneratedChunk;

    /// A noise property that answers a fixed value for every sample.
    struct FixedNoise(f64);

    impl NoiseProperty for FixedNoise {
        fn get(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }

        fn get_3d(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    fn test_cave_type() -> Rc<CaveType> {
        cave_type_with_height_factor("distorted", 1.0)
    }

    /// A horizontal cylinder from `(8, 40, 8)` to `(16, 40, 8)` with a radius
    /// of 3 and a constant cross-section.
    fn cylinder(origin: Vector3d, length: f64, radius: f64) -> Box<dyn DistortedShape> {
        Box::new(DistortedCylinderShape::new(
            origin,
            Vector3d::new(length, 0.0, 0.0),
            radius,
            radius,
            radius,
            radius,
            radius,
            radius,
            InterpolationMode::Linear,
        ))
    }

    fn node_shape(
        shape: Box<dyn DistortedShape>,
        distortion: ShapeDistortion,
    ) -> DistortedCaveNodeShape {
        DistortedCaveNodeShape::new(test_cave_type(), shape, distortion)
    }

    fn node_with_shape(shape: DistortedCaveNodeShape) -> CaveNode {
        let node_type = cave_node_type_with_filling(0, BlockFluidEntry::new(42, 0, 0));
        CaveNode::new(0, node_type, Rc::new(shape), 0.0, 0.0)
    }

    #[test]
    fn the_bounds_come_from_the_distorted_shape() {
        let shape = node_shape(
            cylinder(Vector3d::new(8.0, 40.0, 8.0), 8.0, 3.0),
            ShapeDistortion::default(),
        );
        assert_eq!(shape.get_bounds().get_low_bound_x(), 5);
        assert_eq!(shape.get_bounds().get_high_bound_x(), 19);
        assert_eq!(shape.get_bounds().get_low_bound_y(), 37);
        assert_eq!(shape.get_bounds().get_high_bound_y(), 43);
        assert_eq!(shape.get_bounds().get_low_bound_z(), 5);
        assert_eq!(shape.get_bounds().get_high_bound_z(), 11);
        assert_eq!(shape.get_start(), Vector3d::new(8.0, 40.0, 8.0));
        assert_eq!(shape.get_end(), Vector3d::new(16.0, 40.0, 8.0));
    }

    #[test]
    fn the_column_is_carved_from_the_floor_to_the_ceiling() {
        let shape = node_shape(
            cylinder(Vector3d::new(8.0, 40.0, 8.0), 8.0, 3.0),
            ShapeDistortion::default(),
        );
        // The height component at the centre-line is 1, so the column reaches
        // from 40 - 3 to 40 + 3 (both inclusive).
        assert!(!shape.should_replace(0, 12.0, 8.0, 36));
        assert!(shape.should_replace(0, 12.0, 8.0, 37));
        assert!(shape.should_replace(0, 12.0, 8.0, 40));
        assert!(shape.should_replace(0, 12.0, 8.0, 43));
        assert!(!shape.should_replace(0, 12.0, 8.0, 44));
        // Outside of the radius there is no column at all.
        assert!(!shape.should_replace(0, 12.0, 12.0, 40));
        // Outside of the segment the cylinder has no cap: x = 7 is before the
        // start, x = 17 is past the end.
        assert!(!shape.should_replace(0, 7.0, 8.0, 40));
        assert!(!shape.should_replace(0, 17.0, 8.0, 40));
    }

    #[test]
    fn the_floor_and_ceiling_positions_are_the_blocks_outside_the_column() {
        let shape = node_shape(
            cylinder(Vector3d::new(8.0, 40.0, 8.0), 8.0, 3.0),
            ShapeDistortion::default(),
        );
        assert_eq!(shape.get_floor_position(0, 12.0, 8.0), 36.0);
        assert_eq!(shape.get_ceiling_position(0, 12.0, 8.0), 44.0);
        // Outside of the shape there is neither a floor nor a ceiling.
        assert_eq!(shape.get_floor_position(0, 12.0, 20.0), -1.0);
        assert_eq!(shape.get_ceiling_position(0, 12.0, 20.0), -1.0);
    }

    #[test]
    fn the_width_distortion_contracts_the_horizontal_radius() {
        let shape = node_shape(
            cylinder(Vector3d::new(8.0, 40.0, 8.0), 8.0, 3.0),
            ShapeDistortion::of(Some(Rc::new(FixedNoise(0.5))), None, None),
        );
        // z = 10 is 2 blocks away from the centre-line; the distorted radius
        // is 1.5, so the column disappears.
        assert_eq!(shape.get_floor_position(0, 12.0, 10.0), -1.0);
        assert_eq!(shape.get_ceiling_position(0, 12.0, 10.0), -1.0);
        // The centre is still inside.
        assert!(shape.should_replace(0, 12.0, 8.0, 40));
    }

    #[test]
    fn the_floor_and_ceiling_distortions_scale_the_column_height() {
        let shape = node_shape(
            cylinder(Vector3d::new(8.0, 40.0, 8.0), 8.0, 3.0),
            ShapeDistortion::of(
                None,
                Some(Rc::new(FixedNoise(0.5))),
                Some(Rc::new(FixedNoise(0.25))),
            ),
        );
        // floor: 40 - 3 * 0.5 = 38.5 → floor(38.5) = 38 → cover at 37.
        assert_eq!(shape.get_floor_position(0, 12.0, 8.0), 37.0);
        // ceiling: 40 + 3 * 0.25 = 40.75 → ceil(40.75) = 41 → cover at 42.
        assert_eq!(shape.get_ceiling_position(0, 12.0, 8.0), 42.0);
        assert!(shape.should_replace(0, 12.0, 8.0, 38));
        assert!(!shape.should_replace(0, 12.0, 8.0, 37));
        assert!(shape.should_replace(0, 12.0, 8.0, 41));
        assert!(!shape.should_replace(0, 12.0, 8.0, 42));
    }

    #[test]
    fn the_end_radius_uses_the_width_height_selector() {
        let shape = node_shape(
            Box::new(DistortedCylinderShape::new(
                Vector3d::new(0.0, 40.0, 0.0),
                Vector3d::new(8.0, 0.0, 0.0),
                2.0,
                5.0,
                2.0,
                5.0,
                4.0,
                9.0,
                InterpolationMode::Linear,
            )),
            ShapeDistortion::default(),
        );
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Left), 4.0);
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Right), 9.0);
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Min), 4.0);
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Max), 9.0);
    }

    #[test]
    fn populate_chunk_carves_the_distorted_column() {
        let shape = node_shape(
            cylinder(Vector3d::new(8.0, 40.0, 8.0), 8.0, 3.0),
            ShapeDistortion::default(),
        );
        let node = node_with_shape(shape);
        let cave = Cave::new(Rc::clone(&test_cave_type()));

        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution = ChunkGeneratorExecution::new(42, chunk_generator(), chunk);
        let mut random = FastRandom::new(7);

        node.get_shape()
            .populate_chunk(42, &mut execution, &cave, &node, &mut random);

        // The column at the centre of the shape (12, 8) reaches from y = 37
        // through y = 43.
        for y in 37..=43 {
            assert_eq!(
                execution.get_block(12, y, 8),
                42,
                "the block at y = {y} was not carved"
            );
            assert_eq!(
                execution.get_priority_chunk().get(12, y, 8),
                block_priority::CAVE
            );
        }
        assert_eq!(execution.get_block(12, 36, 8), 0);
        assert_eq!(execution.get_block(12, 44, 8), 0);
        assert_eq!(
            execution.get_priority_chunk().get(12, 36, 8),
            block_priority::NONE
        );
        // Outside of the horizontal radius nothing was written.
        assert_eq!(execution.get_block(12, 40, 12), 0);
    }

    #[test]
    fn the_generator_draws_in_the_java_order() {
        // Every range is a normal range, so every value is exactly one
        // `nextDouble()` draw: length, startWidth, startHeight, endWidth,
        // endHeight, midWidth, midHeight.
        let generator = DistortedCaveNodeShapeGenerator::new(
            DistortedShapes::CYLINDER,
            Rc::new(DoubleRange::normal(0.0, 1.0)),
            Rc::new(DoubleRange::normal(0.0, 1.0)),
            Some(Rc::new(DoubleRange::normal(0.0, 1.0))),
            Some(Rc::new(DoubleRange::normal(0.0, 1.0))),
            Some(Rc::new(DoubleRange::normal(0.0, 1.0))),
            false,
            ShapeDistortion::default(),
            Some(InterpolationMode::Linear),
        );

        let mut random = FastRandom::new(1234);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &test_cave_type(),
            None,
            Some(&child_entry(Vector3d::ZERO)),
            Vector3d::new(0.0, 40.0, 0.0),
            0.0,
            // A horizontal direction: the pitch compensation is zero, so the
            // heights are not scaled by the factory.
            std::f32::consts::FRAC_PI_2,
        );

        let mut reference = FastRandom::new(1234);
        let length = reference.next_double();
        let start_width = reference.next_double();
        let start_height = reference.next_double();
        let end_width = reference.next_double();
        let end_height = reference.next_double();
        let mid_width = reference.next_double();
        let mid_height = reference.next_double();
        assert_eq!(
            random.next_double(),
            reference.next_double(),
            "the generator consumed exactly seven draws"
        );

        // The direction is the yaw/pitch vector scaled by the length. With a
        // pitch of 90° the tube points along +x and its y component is 0.
        let end = shape.get_end();
        assert!((end.y - 40.0).abs() < 1e-9);
        assert!((end.x - length).abs() < 1e-9);

        // The dimensions are the drawn values (linear interpolation through
        // the middle: the midpoint is the middle radius). A horizontal segment
        // exposes its width along z and its height along y through the
        // anchor projection.
        let start = shape.get_start();
        assert_eq!(start, Vector3d::new(0.0, 40.0, 0.0));
        let width_at = |t: f64| shape.get_anchor(Vector3d::ZERO, t, 0.5, 1.0).z - start.z;
        let height_at = |t: f64| shape.get_anchor(Vector3d::ZERO, t, 1.0, 0.5).y - start.y;
        // The width is exact (the anchor is offset by 0 on the z axis).
        assert_eq!(width_at(0.0), start_width);
        assert_eq!(width_at(1.0), end_width);
        // The height is read back from an absolute y coordinate, so the
        // subtraction loses a few bits; the middle dimension is interpolated
        // (`lerpUnclamped(start, mid, 1)`), which is not bit-exact either.
        assert!((height_at(0.0) - start_height).abs() < 1e-12);
        assert!((height_at(1.0) - end_height).abs() < 1e-12);
        assert!((width_at(0.5) - mid_width).abs() < 1e-12);
        assert!((height_at(0.5) - mid_height).abs() < 1e-12);
        // The end radius goes through the same width/height pair.
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Left), end_width);
        assert_eq!(shape.get_end_radius(WidthHeightSelector::Right), end_height);
    }

    #[test]
    fn the_generator_draws_the_optional_ranges_only_when_configured() {
        // No length range and no middle ranges: length is 0 and only the four
        // start/end dimensions consume a draw.
        let generator = DistortedCaveNodeShapeGenerator::new(
            DistortedShapes::CYLINDER,
            Rc::new(DoubleRange::normal(0.0, 1.0)),
            Rc::new(DoubleRange::normal(0.0, 1.0)),
            None,
            None,
            None,
            false,
            ShapeDistortion::default(),
            Some(InterpolationMode::Linear),
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

        let mut reference = FastRandom::new(1234);
        let _ = reference.next_double();
        let _ = reference.next_double();
        let _ = reference.next_double();
        let _ = reference.next_double();
        assert_eq!(
            random.next_double(),
            reference.next_double(),
            "the generator consumed exactly four draws"
        );

        // A length of 0 makes the direction the zero vector.
        assert_eq!(shape.get_end(), shape.get_start());
    }

    #[test]
    fn the_generator_defaults_the_interpolation_when_it_is_missing() {
        // The Java field is @Nullable but the JSON loader always supplies
        // `Constants.DEFAULT_INTERPOLATION`; a null value is only reachable
        // through a hand-built generator, where Java would NPE. The Rust
        // panic is asserted here so the behaviour is explicit.
        let generator = DistortedCaveNodeShapeGenerator::new(
            DistortedShapes::CYLINDER,
            Rc::new(DoubleRange::constant(2.0)),
            Rc::new(DoubleRange::constant(2.0)),
            None,
            None,
            None,
            false,
            ShapeDistortion::default(),
            None,
        );

        let mut random = FastRandom::new(1234);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            generator.generate_cave_node_shape(
                &mut random,
                &test_cave_type(),
                None,
                Some(&child_entry(Vector3d::ZERO)),
                Vector3d::new(0.0, 40.0, 0.0),
                0.0,
                0.0,
            )
        }));
        assert!(result.is_err());
    }

    #[test]
    fn the_default_interpolation_constant_is_linear() {
        assert_eq!(DEFAULT_INTERPOLATION, InterpolationMode::Linear);
    }

    #[test]
    fn get_length_is_zero_without_a_range() {
        let mut random = FastRandom::new(1);
        let before = random.next_double();
        let mut random = FastRandom::new(1);
        assert_eq!(get_length(None, &mut random), 0.0);
        assert_eq!(
            random.next_double(),
            before,
            "a missing length range consumes no draw"
        );
    }
}
