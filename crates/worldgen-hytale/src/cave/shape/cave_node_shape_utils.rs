// Ported from: `.hytale-source/.../worldgen/cave/shape/CaveNodeShapeUtils.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.CaveNodeShapeUtils` —
//! the helpers the cave node shapes share.
//!
//! # Scope of this port (Phase 4c / 4d)
//!
//! The module ports the whole Java class:
//!
//! * the anchor projections ([`get_box_anchor`], [`get_line_anchor`],
//!   [`get_sphere_anchor`], [`get_pipe_anchor`]),
//! * the child offset / end radius helpers ([`get_offset`],
//!   [`get_end_radius`], [`get_end_width`], [`get_end_height`]),
//! * and the shared population helpers ([`get_filling_block`],
//!   [`get_cover_height`], [`is_cover_matching_parent`],
//!   [`invalidate_cover`]).
//!
//! # The end radius of a distorted shape
//!
//! Phase 4d completed the `DistortedCaveNodeShape` branch of the Java
//! `getEndRadius`: the distorted shape exposes a *width* and a *height* at its
//! end, so the width/height selector matters there. The check itself stays
//! the [`CaveNodeShape::get_end_radius`] method (Rust has no `instanceof`);
//! the distorted shape and the 4c shapes override it.
//!
//! # The block-type support map of `invalidateCover`
//!
//! The Java method asks the block type whether it requires a support below
//! (`BlockFace.DOWN`) or above (`BlockFace.UP`). The Rust port models that
//! through [`worldgen_model::registry::BlockTypeInfo::get_support`].
//!
//! ⚠ Data gap CLOSED: the golden block dump now carries the support data —
//! one `containsKey(DOWN/UP)` entry per Java `RotationTuple` per block (the
//! dump `meta.supportField`), loaded through
//! [`worldgen_model::registry::BlockTypeInfo::get_support`]. A `CAVE_COVER`
//! block whose block type requires a supporting face is invalidated like in
//! Java.
//!
//! # Java → Rust notes
//!
//! * `BiDoubleToDoubleFunction` becomes the [`WidthHeightSelector`] enum
//!   (the Java class has exactly four constants).
//! * `Vector3d.set(...)` (joml, in-place) becomes a returned copy — the
//!   Rust [`Vector3d`] is a `Copy` value type. The `vector` parameter of the
//!   anchor helpers is therefore unused (it is the Java "destination"
//!   argument).

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use procedural::noise::supplier::IDoubleRange;
use worldgen_model::registry::BlockRegistry;

use crate::cave::cave_node_type::{
    CaveNodeChildEntry, CaveNodeCoverEntry, CaveNodeCoverType, CaveNodeType,
};
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::bounds::IWorldBounds;

/// Java `CaveNodeShapeUtils.LEFT` / `RIGHT` / `MIN` / `MAX` — the
/// `BiDoubleToDoubleFunction` that picks between the horizontal and the
/// vertical radius of a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidthHeightSelector {
    /// Java `LEFT` — `(l, r) -> l`.
    Left,
    /// Java `RIGHT` — `(l, r) -> r`.
    Right,
    /// Java `MIN` — `Math::min`.
    Min,
    /// Java `MAX` — `Math::max`.
    Max,
}

impl WidthHeightSelector {
    /// Java `BiDoubleToDoubleFunction.apply(double l, double r)`.
    pub fn apply(self, l: f64, r: f64) -> f64 {
        match self {
            WidthHeightSelector::Left => l,
            WidthHeightSelector::Right => r,
            WidthHeightSelector::Min => java_min(l, r),
            WidthHeightSelector::Max => java_max(l, r),
        }
    }
}

/// Java `Math.min(double, double)` — propagates NaN (unlike Rust's
/// `f64::min`, which returns the non-NaN operand).
fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

/// Java `Math.max(double, double)` — propagates NaN (unlike Rust's
/// `f64::max`, which returns the non-NaN operand).
fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// Java `@Nonnull public static Vector3d getBoxAnchor(@Nonnull Vector3d
/// vector, @Nonnull IWorldBounds bounds, double tx, double ty, double tz)`.
///
/// "Returns the projected point inside a cuboid represented by the
/// `IWorldBounds`."
pub fn get_box_anchor(
    _vector: Vector3d,
    bounds: &dyn IWorldBounds,
    tx: f64,
    ty: f64,
    tz: f64,
) -> Vector3d {
    let x = bounds.fraction_x(tx);
    let y = bounds.fraction_y(ty);
    let z = bounds.fraction_z(tz);
    Vector3d::new(x, y, z)
}

/// Java `@Nonnull public static Vector3d getLineAnchor(@Nonnull Vector3d
/// vector, @Nonnull Vector3d o, @Nonnull Vector3d v, double t)`.
///
/// "Returns the point at the projection 't' on the line `o + v`."
pub fn get_line_anchor(_vector: Vector3d, o: Vector3d, v: Vector3d, t: f64) -> Vector3d {
    let x = o.x + v.x * t;
    let y = o.y + v.y * t;
    let z = o.z + v.z * t;
    Vector3d::new(x, y, z)
}

/// Java `@Nonnull public static Vector3d getSphereAnchor(@Nonnull Vector3d
/// vector, @Nonnull Vector3d origin, double rx, double ry, double rz, double
/// tx, double ty, double tz)`.
///
/// "Returns the projected point inside a sphere/ellipsoid."
#[allow(clippy::too_many_arguments)] // The Java signature has eight parameters.
pub fn get_sphere_anchor(
    _vector: Vector3d,
    origin: Vector3d,
    rx: f64,
    ry: f64,
    rz: f64,
    tx: f64,
    ty: f64,
    tz: f64,
) -> Vector3d {
    // Map to range [-1 to 1]
    let fx = tx * 2.0 - 1.0;
    let fy = ty * 2.0 - 1.0;
    let fz = tz * 2.0 - 1.0;
    get_radial_projection(
        _vector, origin.x, origin.y, origin.z, rx, ry, rz, fx, fy, fz,
    )
}

/// Java `@Nonnull public static Vector3d getPipeAnchor(@Nonnull Vector3d
/// vector, @Nonnull Vector3d o, @Nonnull Vector3d v, double rx, double ry,
/// double rz, double t, double tv, double th)`.
///
/// "Returns the projected point inside a tube/cylinder shape."
#[allow(clippy::too_many_arguments)] // The Java signature has nine parameters.
pub fn get_pipe_anchor(
    _vector: Vector3d,
    o: Vector3d,
    v: Vector3d,
    rx: f64,
    ry: f64,
    rz: f64,
    t: f64,
    tv: f64,
    th: f64,
) -> Vector3d {
    let x = o.x + v.x * t;
    let y = o.y + v.y * t;
    let z = o.z + v.z * t;

    // Normalize direction components
    // Java: `v.length()` — the joml length is `sqrt(x*x + y*y + z*z)`.
    let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
    let nx = v.x / len;
    let ny = v.y / len;
    let nz = v.z / len;

    // Map projections to range [-1 to 1]
    let fv = 2.0 * tv - 1.0;
    let fh = 2.0 * th - 1.0;

    // Rotate normals to point along the plane of cross-section at t, and then
    // scale by the projection mappings
    let fx = (-ny * fv) - (nz * fh); // RotateZ(90°) + RotateY(90°)
    let fy = nx * fv; // RotateZ(90°)
    let fz = nx * fh; // RotateY(90°)

    get_radial_projection(_vector, x, y, z, rx, ry, rz, fx, fy, fz)
}

/// Java `protected static Vector3d getRadialProjection(@Nonnull Vector3d
/// vector, double x, double y, double z, double rx, double ry, double rz,
/// double tx, double ty, double tz)`.
///
/// "Returns the projection point on the radial cross-section around
/// `x, y, z`."
#[allow(clippy::too_many_arguments)] // The Java signature has ten parameters.
fn get_radial_projection(
    _vector: Vector3d,
    x: f64,
    y: f64,
    z: f64,
    rx: f64,
    ry: f64,
    rz: f64,
    tx: f64,
    ty: f64,
    tz: f64,
) -> Vector3d {
    let len2 = (tx * tx) + (ty * ty) + (tz * tz);
    if len2 == 0.0 {
        return Vector3d::new(x, y, z);
    }

    // Calculate distance to circumference
    let inv_len = (1.0 / len2).sqrt();
    let dx = tx.abs() * rx * inv_len;
    let dy = ty.abs() * ry * inv_len;
    let dz = tz.abs() * rz * inv_len;

    // Scale distances by direction components & add to origin
    let x = x + dx * tx;
    let y = y + dy * ty;
    let z = z + dz * tz;

    Vector3d::new(x, y, z)
}

/// Java `@Nonnull public static Vector3d getOffset(@Nullable CaveNode parent,
/// @Nonnull CaveNodeType.CaveNodeChildEntry childEntry)`.
///
/// "Get the origin offset vector for a child cave node, applying rotations
/// if the parent node happens to be a `PrefabCaveNode`."
///
/// Panics when the child entry is missing — the Java code dereferences the
/// `@Nonnull` parameter and throws a `NullPointerException` in that case.
pub fn get_offset(parent: Option<&CaveNode>, child_entry: Option<&CaveNodeChildEntry>) -> Vector3d {
    let child_entry = child_entry.expect(
        "CaveNodeShapeUtils.getOffset: the child entry is null (Java: NullPointerException)",
    );
    let mut offset = child_entry.get_offset();
    // Java: `if (offset == Vector3dUtil.ZERO) return offset;` — a *reference*
    // comparison against the shared zero constant. Rotating the zero vector
    // is the identity, so the value comparison below is equivalent.
    if offset == Vector3d::ZERO {
        return offset;
    }

    if let Some(parent) = parent {
        if let Some(rotation) = parent.get_shape().as_prefab_rotation() {
            // Java copies the offset before rotating it, so the child entry
            // keeps its own vector.
            rotation.rotate(&mut offset);
        }
    }

    offset
}

/// Java `public static double getEndRadius(@Nullable CaveNode node, @Nonnull
/// IDoubleRange range, Random random)`.
///
/// "Get the radius of a `CaveNode` at its end. If the node shape supports
/// independent values for its horizontal and vertical radii, the lower of
/// the two will be selected."
pub fn get_end_radius(
    node: Option<&CaveNode>,
    range: &dyn IDoubleRange,
    random: &mut FastRandom,
) -> f64 {
    if let Some(node) = node {
        let radius = get_shape_end_radius(node.get_shape().as_ref(), WidthHeightSelector::Min);
        if radius != -1.0 {
            return radius;
        }
    }
    // Java: `range.getValue(random)` — the `Random` overload. A
    // `DoubleRange.Constant` does *not* consume a draw there.
    range.get_value_random(random)
}

/// Java `public static double getEndWidth(@Nullable CaveNode node, @Nonnull
/// IDoubleRange range, Random random)`.
///
/// "Get the horizontal radius of a `CaveNode` at its end."
pub fn get_end_width(
    node: Option<&CaveNode>,
    range: &dyn IDoubleRange,
    random: &mut FastRandom,
) -> f64 {
    if let Some(node) = node {
        let radius = get_shape_end_radius(node.get_shape().as_ref(), WidthHeightSelector::Left);
        if radius != -1.0 {
            return radius;
        }
    }
    // Java: `range.getValue(random)` — the `Random` overload.
    range.get_value_random(random)
}

/// Java `public static double getEndHeight(@Nullable CaveNode node, @Nonnull
/// IDoubleRange range, Random random)`.
///
/// "Get the vertical radius of a `CaveNode` at its end."
pub fn get_end_height(
    node: Option<&CaveNode>,
    range: &dyn IDoubleRange,
    random: &mut FastRandom,
) -> f64 {
    if let Some(node) = node {
        let radius = get_shape_end_radius(node.get_shape().as_ref(), WidthHeightSelector::Right);
        if radius != -1.0 {
            return radius;
        }
    }
    // Java: `range.getValue(random)` — the `Random` overload.
    range.get_value_random(random)
}

/// Java `public static double getEndRadius(@Nonnull CaveNodeShape shape, @Nonnull
/// BiDoubleToDoubleFunction widthHeightSelector)`.
///
/// "Get the radius of the `CaveNodeShape` at its end."
///
/// Deviation: the Java method dispatches with a chain of `instanceof`
/// checks; Rust trait objects cannot be downcast, so the check is exposed as
/// the [`CaveNodeShape::get_end_radius`] method (the default answers Java's
/// "unknown shape" value, `-1`).
pub fn get_shape_end_radius(shape: &dyn CaveNodeShape, selector: WidthHeightSelector) -> f64 {
    shape.get_end_radius(selector)
}

/// Java `@Nullable public static BlockFluidEntry getFillingBlock(@Nonnull
/// CaveType cave, @Nonnull CaveNodeType node, int y, @Nonnull Random random)`.
///
/// "Get the filling block for a cave node at a given height."
///
/// Panics when the node type has no filling — the Java code dereferences the
/// `@Nullable` result and throws a `NullPointerException` in that case.
pub fn get_filling_block(
    cave: &CaveType,
    node: &CaveNodeType,
    y: i32,
    random: &mut FastRandom,
) -> BlockFluidEntry {
    if cave.get_fluid_level().get_height() >= y {
        cave.get_fluid_level().get_block_entry()
    } else {
        *node.get_filling(random).expect(
            "CaveNodeShapeUtils.getFillingBlock: the node type has no filling (Java: NullPointerException)",
        )
    }
}

/// Java `protected static int getCoverHeight(int lowest, int lowestPossible,
/// int highest, int highestPossible, boolean heightLimited, @Nonnull
/// CaveNodeType.CaveNodeCoverEntry cover, @Nonnull
/// CaveNodeType.CaveNodeCoverEntry.Entry entry)`.
///
/// "Get the height that the provided cover entry should be placed at in a
/// cave code."
pub fn get_cover_height(
    lowest: i32,
    lowest_possible: i32,
    highest: i32,
    highest_possible: i32,
    height_limited: bool,
    cover: &CaveNodeCoverEntry,
    entry: &crate::cave::cave_node_type::CaveNodeCoverEntryEntry,
) -> i32 {
    match cover.get_type() {
        CaveNodeCoverType::Floor => {
            if lowest == i32::MAX || lowest_possible != lowest {
                return -1;
            }
            lowest.wrapping_sub(1).wrapping_add(entry.get_offset())
        }
        CaveNodeCoverType::Ceiling => {
            // We dont want to have floating ceiling covers so don't place them
            // at the "surface"
            if height_limited {
                return -1;
            }
            if highest == i32::MIN || highest_possible != highest {
                return -1;
            }
            highest.wrapping_add(1).wrapping_sub(entry.get_offset())
        }
    }
}

/// Java `public static boolean isCoverMatchingParent(int cx, int cz, int y,
/// @Nonnull ChunkGeneratorExecution execution, @Nonnull
/// CaveNodeType.CaveNodeCoverEntry cover)`.
///
/// "Test a position within the given chunk to see if a cover can be placed
/// above or below the 'parent' of that position."
///
/// Deviation: the Java identity short-circuits for the two default
/// conditions (`ConstantBlockFluidCondition.DEFAULT_TRUE` /
/// `DEFAULT_FALSE`) are omitted — the port evaluates the condition, which
/// yields the same value (the conditions are pure).
#[allow(deprecated)] // Java `ChunkUtil.HEIGHT_MINUS_1` (deprecated in Java too).
pub fn is_cover_matching_parent(
    cx: i32,
    cz: i32,
    y: i32,
    execution: &ChunkGeneratorExecution,
    cover: &CaveNodeCoverEntry,
) -> bool {
    let parent_y = y.wrapping_add(cover.get_type().parent_offset());
    if !(chunk_util::MIN_Y..=chunk_util::HEIGHT_MINUS_1).contains(&parent_y) {
        return false;
    }

    let parent_condition = cover.get_parent_condition();
    let parent = execution.get_block(cx, parent_y, cz);
    let parent_fluid = execution.get_fluid(cx, parent_y, cz);
    parent_condition.eval(parent, parent_fluid)
}

/// Java `public static boolean invalidateCover(int x, int y, int z, @Nonnull
/// CaveNodeType.CaveNodeCoverType type, @Nonnull ChunkGeneratorExecution
/// execution, @Nonnull BlockTypeAssetMap<String, BlockType> blockTypeMap)`.
///
/// "Test the block at the provided chunk-relative coordinates to see if it is
/// a cover block that requires a supporting block below it."
///
/// The Java method asks the block type whether it requires a support below
/// (`BlockFace.DOWN`) or above (`BlockFace.UP`) — see
/// [`BlockTypeInfo::get_support`](worldgen_model::registry::BlockTypeInfo::get_support).
///
/// Deviation: Java's `blockTypeMap.getAsset(block)` is `@Nonnull` at this
/// point (the block was written by a cave cover), so a registry miss would
/// throw a `NullPointerException`. The Rust registry is a data seam that may
/// not know the block (an empty or incomplete dump), so a miss is treated as
/// "no support map" (`false`) instead.
#[allow(deprecated)] // Java `ChunkUtil.HEIGHT_MINUS_1` (deprecated in Java too).
pub fn invalidate_cover(
    x: i32,
    y: i32,
    z: i32,
    cover_type: CaveNodeCoverType,
    execution: &ChunkGeneratorExecution,
    block_registry: &BlockRegistry,
) -> bool {
    if !(0..=chunk_util::HEIGHT_MINUS_1).contains(&y) {
        return false;
    }

    let priority = execution.get_priority_chunk().get(x, y, z);

    // Caves should always invalidate surface covers
    if priority == worldgen_model::block_priority::COVER {
        return true;
    }

    // Ignore non-cover blocks
    if priority != worldgen_model::block_priority::CAVE_COVER {
        return false;
    }

    // Java: "Check if the cave-cover block type requires a supporting block
    // above/below it"
    let block = execution.get_block(x, y, z);
    let supports = block_registry
        .get(block)
        .and_then(|block_type| block_type.get_support(execution.get_rotation_index(x, y, z)));

    let Some(supports) = supports else {
        return false;
    };

    match cover_type {
        // Java: "Assume that the presence of a downward
        // 'RequiredBlockFaceSupport' means the block requires support below"
        CaveNodeCoverType::Floor => supports.down,
        // Java: "Assume that the presence of an upward
        // 'RequiredBlockFaceSupport' means the block requires support above"
        CaveNodeCoverType::Ceiling => supports.up,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::bounds::WorldBounds;

    #[test]
    fn the_width_height_selector_picks_the_requested_side() {
        assert_eq!(WidthHeightSelector::Left.apply(1.0, 2.0), 1.0);
        assert_eq!(WidthHeightSelector::Right.apply(1.0, 2.0), 2.0);
        assert_eq!(WidthHeightSelector::Min.apply(1.0, 2.0), 1.0);
        assert_eq!(WidthHeightSelector::Max.apply(1.0, 2.0), 2.0);
    }

    #[test]
    fn the_width_height_selector_propagates_nan() {
        assert!(WidthHeightSelector::Min.apply(f64::NAN, 2.0).is_nan());
        assert!(WidthHeightSelector::Max.apply(1.0, f64::NAN).is_nan());
    }

    #[test]
    fn the_box_anchor_uses_the_bounds_fractions() {
        let bounds = WorldBounds::with_bounds(10, 20, 30, 40, 50, 60);
        let anchor = get_box_anchor(Vector3d::ZERO, &bounds, 0.5, 0.5, 0.5);
        assert_eq!(anchor.x, 25.0);
        assert_eq!(anchor.y, 35.0);
        assert_eq!(anchor.z, 45.0);
    }

    #[test]
    fn the_line_anchor_is_the_projection_on_the_line() {
        let anchor = get_line_anchor(
            Vector3d::ZERO,
            Vector3d::new(1.0, 2.0, 3.0),
            Vector3d::new(10.0, 20.0, 30.0),
            0.5,
        );
        assert_eq!(anchor.x, 6.0);
        assert_eq!(anchor.y, 12.0);
        assert_eq!(anchor.z, 18.0);
    }

    #[test]
    fn the_sphere_anchor_maps_the_unit_cube_onto_the_ellipsoid() {
        // The centre of the cube maps onto the centre of the ellipsoid.
        let anchor = get_sphere_anchor(
            Vector3d::ZERO,
            Vector3d::new(5.0, 6.0, 7.0),
            2.0,
            3.0,
            4.0,
            0.5,
            0.5,
            0.5,
        );
        assert_eq!(anchor.x, 5.0);
        assert_eq!(anchor.y, 6.0);
        assert_eq!(anchor.z, 7.0);

        // The corner of the cube maps onto the *radial* projection: the
        // direction (1, 1, 1) is scaled to the surface of the ellipsoid.
        let anchor =
            get_sphere_anchor(Vector3d::ZERO, Vector3d::ZERO, 2.0, 3.0, 4.0, 1.0, 1.0, 1.0);
        let inv_len = (1.0f64 / 3.0).sqrt();
        assert!((anchor.x - 2.0 * inv_len).abs() < 1e-9);
        assert!((anchor.y - 3.0 * inv_len).abs() < 1e-9);
        assert!((anchor.z - 4.0 * inv_len).abs() < 1e-9);
    }

    #[test]
    fn the_pipe_anchor_projects_onto_the_tube() {
        // A tube along the x axis with a radius of 2: the projection of the
        // "up" direction is straight up.
        let anchor = get_pipe_anchor(
            Vector3d::ZERO,
            Vector3d::new(0.0, 0.0, 0.0),
            Vector3d::new(10.0, 0.0, 0.0),
            2.0,
            2.0,
            2.0,
            0.5,
            1.0,
            0.5,
        );
        assert!((anchor.x - 5.0).abs() < 1e-9);
        assert!((anchor.y - 2.0).abs() < 1e-9);
        assert!((anchor.z).abs() < 1e-9);
    }

    #[test]
    fn the_end_width_and_height_come_from_the_distorted_shape() {
        use std::rc::Rc;

        use procedural::noise::InterpolationMode;

        use crate::cave::element::cave_node::CaveNode;
        use crate::cave::shape::distorted::distorted_cylinder_shape::DistortedCylinderShape;
        use crate::cave::shape::distorted_cave_node_shape::DistortedCaveNodeShape;
        use crate::cave::shape::shape_distortion::ShapeDistortion;
        use crate::cave::test_stubs::{cave_node_type, cave_type_with_height_factor};

        // A cylinder whose width grows from 2 to 6 and whose height grows
        // from 3 to 7.
        let shape = DistortedCaveNodeShape::new(
            cave_type_with_height_factor("distorted", 1.0),
            Box::new(DistortedCylinderShape::new(
                Vector3d::new(0.0, 40.0, 0.0),
                Vector3d::new(8.0, 0.0, 0.0),
                2.0,
                3.0,
                4.0,
                5.0,
                6.0,
                7.0,
                InterpolationMode::Linear,
            )),
            ShapeDistortion::default(),
        );
        let node = CaveNode::new(0, cave_node_type(0), Rc::new(shape), 0.0, 0.0);

        let range = procedural::noise::supplier::DoubleRange::constant(99.0);
        let mut random = FastRandom::new(1234);
        let before = random.next_double();

        let mut random = FastRandom::new(1234);
        assert_eq!(get_end_width(Some(&node), &range, &mut random), 6.0);
        assert_eq!(get_end_height(Some(&node), &range, &mut random), 7.0);
        assert_eq!(
            random.next_double(),
            before,
            "the shape answered, so the fallback range was not used"
        );
    }

    #[test]
    fn a_cave_cover_that_needs_support_is_invalidated() {
        use std::rc::Rc;

        use worldgen_model::block_priority;
        use worldgen_model::chunk::GeneratedChunk;
        use worldgen_model::registry::{
            BlockRegistry, BlockSupport, BlockTypeInfo, DrawType, Registries,
        };

        use crate::cave::test_stubs::chunk_generator;
        use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;

        // A block that requires a support *below* it.
        let registry = Registries::new(
            BlockRegistry::new(vec![Some(BlockTypeInfo::new(DrawType::Cube).with_support(
                BlockSupport {
                    down: true,
                    up: false,
                },
            ))]),
            worldgen_model::registry::FluidRegistry::empty(),
        );
        let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
        let mut execution =
            ChunkGeneratorExecution::with_registry(42, chunk_generator(), chunk, Rc::new(registry));

        // A cave cover at (1, 10, 1).
        execution.override_block(1, 10, 1, block_priority::CAVE_COVER, 0);

        assert!(invalidate_cover(
            1,
            10,
            1,
            CaveNodeCoverType::Floor,
            &execution,
            &execution.registry().blocks
        ));
        assert!(!invalidate_cover(
            1,
            10,
            1,
            CaveNodeCoverType::Ceiling,
            &execution,
            &execution.registry().blocks
        ));

        // A surface cover is always invalidated.
        execution.override_block(1, 11, 1, block_priority::COVER, 0);
        assert!(invalidate_cover(
            1,
            11,
            1,
            CaveNodeCoverType::Ceiling,
            &execution,
            &execution.registry().blocks
        ));

        // A block that is not a cover is never invalidated.
        assert!(!invalidate_cover(
            1,
            12,
            1,
            CaveNodeCoverType::Floor,
            &execution,
            &execution.registry().blocks
        ));

        // A cave cover whose block type is unknown has no support map.
        execution.override_block(1, 13, 1, block_priority::CAVE_COVER, 7);
        assert!(!invalidate_cover(
            1,
            13,
            1,
            CaveNodeCoverType::Floor,
            &execution,
            &execution.registry().blocks
        ));
    }
}
