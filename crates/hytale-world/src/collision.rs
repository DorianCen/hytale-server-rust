//! Block collision shapes and world collision queries.
//!
//! Ported from `CoreServer/.../asset/type/blockhitbox/BlockBoundingBoxes`,
//! `modules/collision/BlockDataProvider`, `BlockCollisionProvider` and
//! `util/FillerBlockUtil`.
//!
//! A collider is a world-space [`Aabb`]. Blocks are queried through
//! [`CollisionWorld`], which yields the rotated, world-space boxes of the block
//! at one coordinate. `hytale-entities::movement` performs the sweep; this
//! module only owns the asset-derived shapes and the loaded-chunk lookup.
//!
//! Java's collision material rules are reduced to the one the player body needs:
//! `BlockMaterial.Solid` collides, everything else does not. Fluid fill levels,
//! trigger/damage boxes and entity collisions are later gameplay work (T52/T55).

use hytale_assets::AssetRegistry;
use hytale_protocol::generated::common::BlockMaterial;
use serde_json::Value;

use crate::world::World;

/// A 3D vector used by collision and movement code.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vec3 {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
    /// Z coordinate.
    pub z: f64,
}

impl Vec3 {
    /// Construct a vector.
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    /// Whether every component is finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    /// Component-wise sum.
    pub fn plus(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }

    /// Component-wise difference.
    pub fn minus(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }

    /// Multiply every component.
    pub fn scale(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor, self.z * factor)
    }

    /// Squared length.
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// The component for one axis (0 = X, 1 = Y, 2 = Z).
    pub fn axis(self, axis: usize) -> f64 {
        match axis {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }

    /// Set one axis component.
    pub fn with_axis(self, axis: usize, value: f64) -> Self {
        match axis {
            0 => Self::new(value, self.y, self.z),
            1 => Self::new(self.x, value, self.z),
            _ => Self::new(self.x, self.y, value),
        }
    }
}

/// An axis-aligned bounding box in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl Aabb {
    /// Construct an AABB from its corners.
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// The Java `BlockBoundingBoxes.UNIT_BOX` (`0..1` on every axis).
    pub const fn unit() -> Self {
        Self::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 1.0))
    }

    /// The player body box with its feet at `position`.
    pub fn player(position: Vec3, half_width: f64, height: f64) -> Self {
        Self::new(
            Vec3::new(position.x - half_width, position.y, position.z - half_width),
            Vec3::new(
                position.x + half_width,
                position.y + height,
                position.z + half_width,
            ),
        )
    }

    /// Translate this box.
    pub fn translated(self, offset: Vec3) -> Self {
        Self::new(self.min.plus(offset), self.max.plus(offset))
    }

    /// Whether two boxes overlap on every axis.
    ///
    /// Touching faces are not an overlap, matching the contact resolution in
    /// `CollisionMath.isDisjoint` for a zero-thickness intersection.
    pub fn intersects(self, other: Self) -> bool {
        self.max.x > other.min.x
            && self.min.x < other.max.x
            && self.max.y > other.min.y
            && self.min.y < other.max.y
            && self.max.z > other.min.z
            && self.min.z < other.max.z
    }
}

/// One block type's collision boxes before rotation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BlockShape {
    /// Local boxes in block coordinates (`0..1` for the default full block).
    pub boxes: Vec<Aabb>,
}

impl BlockShape {
    /// A full unit cube.
    pub fn unit() -> Self {
        Self {
            boxes: vec![Aabb::unit()],
        }
    }

    /// Whether this block participates in player collision.
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// Rotate the local boxes by a Java `RotationTuple` index and translate them
    /// to the world block origin.
    ///
    /// Java stores `(roll * 16) + (pitch * 4) + yaw` (`RotationTuple.index`).
    pub fn world_boxes(&self, rotation: i32, origin: (i32, i32, i32), out: &mut Vec<Aabb>) {
        let rotation = rotation.rem_euclid(64) as u8;
        let yaw = rotation % 4;
        let pitch = (rotation / 4) % 4;
        let roll = rotation / 16;
        let offset = Vec3::new(
            f64::from(origin.0),
            f64::from(origin.1),
            f64::from(origin.2),
        );
        for base in &self.boxes {
            let mut min = base.min;
            let mut max = base.max;
            rotate_box(&mut min, &mut max, yaw, pitch, roll);
            normalize(&mut min, &mut max);
            let local = Aabb::new(min, max);
            out.push(local.translated(offset));
        }
    }
}

/// Java `Box.normalize`: keep every minimum below its maximum after rotation.
fn normalize(min: &mut Vec3, max: &mut Vec3) {
    if min.x > max.x {
        std::mem::swap(&mut min.x, &mut max.x);
    }
    if min.y > max.y {
        std::mem::swap(&mut min.y, &mut max.y);
    }
    if min.z > max.z {
        std::mem::swap(&mut min.z, &mut max.z);
    }
}

/// Rotate one box exactly like `BlockBoundingBoxes.rotate`.
fn rotate_box(min: &mut Vec3, max: &mut Vec3, yaw: u8, pitch: u8, roll: u8) {
    match roll {
        0 => {}
        1 => rotate_270_z(min, max),
        2 => rotate_180_z(min, max),
        3 => rotate_90_z(min, max),
        _ => {}
    }
    match pitch {
        0 => {}
        1 => rotate_90_x(min, max),
        2 => rotate_180_x(min, max),
        3 => rotate_270_x(min, max),
        _ => {}
    }
    match yaw {
        0 => {}
        1 => rotate_90_y(min, max),
        2 => rotate_180_y(min, max),
        3 => rotate_270_y(min, max),
        _ => {}
    }
}

fn rotate_90_x(min: &mut Vec3, max: &mut Vec3) {
    let t = min.z;
    min.z = min.y;
    min.y = 1.0 - t;
    let t = max.z;
    max.z = max.y;
    max.y = 1.0 - t;
}

fn rotate_180_x(min: &mut Vec3, max: &mut Vec3) {
    min.z = 1.0 - min.z;
    min.y = 1.0 - min.y;
    max.z = 1.0 - max.z;
    max.y = 1.0 - max.y;
}

fn rotate_270_x(min: &mut Vec3, max: &mut Vec3) {
    let t = min.z;
    min.z = 1.0 - min.y;
    min.y = t;
    let t = max.z;
    max.z = 1.0 - max.y;
    max.y = t;
}

fn rotate_90_y(min: &mut Vec3, max: &mut Vec3) {
    let t = min.x;
    min.x = min.z;
    min.z = 1.0 - t;
    let t = max.x;
    max.x = max.z;
    max.z = 1.0 - t;
}

fn rotate_180_y(min: &mut Vec3, max: &mut Vec3) {
    min.x = 1.0 - min.x;
    min.z = 1.0 - min.z;
    max.x = 1.0 - max.x;
    max.z = 1.0 - max.z;
}

fn rotate_270_y(min: &mut Vec3, max: &mut Vec3) {
    let t = min.x;
    min.x = 1.0 - min.z;
    min.z = t;
    let t = max.x;
    max.x = 1.0 - max.z;
    max.z = t;
}

fn rotate_90_z(min: &mut Vec3, max: &mut Vec3) {
    let t = min.x;
    min.x = min.y;
    min.y = 1.0 - t;
    let t = max.x;
    max.x = max.y;
    max.y = 1.0 - t;
}

fn rotate_180_z(min: &mut Vec3, max: &mut Vec3) {
    min.x = 1.0 - min.x;
    min.y = 1.0 - min.y;
    max.x = 1.0 - max.x;
    max.y = 1.0 - max.y;
}

fn rotate_270_z(min: &mut Vec3, max: &mut Vec3) {
    let t = min.x;
    min.x = 1.0 - min.y;
    min.y = t;
    let t = max.x;
    max.x = 1.0 - max.y;
    max.y = t;
}

/// Collision shapes indexed by the server's `BlockType` ids.
#[derive(Debug, Clone, Default)]
pub struct BlockCollisionTable {
    shapes: Vec<BlockShape>,
}

impl BlockCollisionTable {
    /// Build the table from the registry's block materials and hitboxes.
    ///
    /// Missing `HitboxType` names fall back to the preloaded `Full` box, like
    /// Java's `BlockType.processConfig`.
    pub fn from_assets(assets: &AssetRegistry) -> Self {
        let highest = assets
            .block_types
            .iter()
            .filter_map(|record| record.id)
            .max()
            .unwrap_or(0)
            .max(0) as usize;
        let mut shapes = vec![BlockShape::default(); highest + 1];
        for record in assets.block_types.iter() {
            let Some(id) = record.id else {
                continue;
            };
            let index = id as usize;
            if index >= shapes.len() {
                continue;
            }
            if record.value.material != BlockMaterial::Solid {
                continue;
            }
            let hitbox = record
                .value
                .json
                .get("HitboxType")
                .and_then(Value::as_str)
                .unwrap_or("Full");
            shapes[index] = block_shape(assets, hitbox);
        }
        Self { shapes }
    }

    /// The shape for a block id, or an empty shape for air/unknown.
    pub fn shape(&self, block_id: i32) -> Option<&BlockShape> {
        usize::try_from(block_id)
            .ok()
            .and_then(|index| self.shapes.get(index))
            .filter(|shape| !shape.is_empty())
    }

    /// Number of indexed block ids.
    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    /// Whether the table has no block ids.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }
}

fn block_shape(assets: &AssetRegistry, hitbox_name: &str) -> BlockShape {
    let Some(record) = assets.block_bounding_boxes.get(hitbox_name) else {
        return BlockShape::unit();
    };
    let Some(boxes) = record.value.get("Boxes").and_then(Value::as_array) else {
        return BlockShape::unit();
    };
    let mut parsed = Vec::with_capacity(boxes.len());
    for entry in boxes {
        let min = entry.get("Min");
        let max = entry.get("Max");
        let component = |vector: Option<&Value>, name: &str| -> f64 {
            vector
                .and_then(|value| value.get(name))
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        };
        parsed.push(Aabb::new(
            Vec3::new(
                component(min, "X"),
                component(min, "Y"),
                component(min, "Z"),
            ),
            Vec3::new(
                component(max, "X"),
                component(max, "Y"),
                component(max, "Z"),
            ),
        ));
    }
    if parsed.is_empty() {
        BlockShape::unit()
    } else {
        BlockShape { boxes: parsed }
    }
}

/// A source of world-space block collision boxes.
///
/// Implementations append every solid box for the block at `(x, y, z)` to
/// `out`. Callers clear `out` before each query.
pub trait CollisionWorld {
    /// Collect the solid boxes of one block.
    fn collect_block_boxes(&self, x: i32, y: i32, z: i32, out: &mut Vec<Aabb>);
}

/// A [`World`] plus its asset-derived collision table.
#[derive(Debug, Clone, Copy)]
pub struct WorldCollision<'a> {
    /// Runtime chunks.
    pub world: &'a World,
    /// Block shapes.
    pub table: &'a BlockCollisionTable,
}

impl<'a> WorldCollision<'a> {
    /// Construct a collision query.
    pub fn new(world: &'a World, table: &'a BlockCollisionTable) -> Self {
        Self { world, table }
    }
}

impl CollisionWorld for WorldCollision<'_> {
    fn collect_block_boxes(&self, x: i32, y: i32, z: i32, out: &mut Vec<Aabb>) {
        let Some(column) = self.world.chunk(x >> 5, z >> 5) else {
            // Unloaded chunks are treated as air for movement. Java treats them
            // as `UNKNOWN` (solid); section 10 of the task records why this
            // differs until chunk demand can block the movement path.
            return;
        };
        let filler = column.get_filler(x & 31, y, z & 31);
        let (origin_x, origin_y, origin_z) = filler_origin(x, y, z, filler);
        let origin_column = self
            .world
            .chunk(origin_x >> 5, origin_z >> 5)
            .unwrap_or(column);
        let block_id = origin_column.get_block(origin_x & 31, origin_y, origin_z & 31);
        let Some(shape) = self.table.shape(block_id) else {
            return;
        };
        let rotation = origin_column.get_rotation(origin_x & 31, origin_y, origin_z & 31);
        shape.world_boxes(rotation, (origin_x, origin_y, origin_z), out);
    }
}

/// Java `FillerBlockUtil.unpackX/Y/Z`.
fn filler_origin(x: i32, y: i32, z: i32, filler: i32) -> (i32, i32, i32) {
    if filler == 0 {
        return (x, y, z);
    }
    const BITS: u32 = 5;
    const MASK: i32 = (1 << BITS) - 1;
    const INVERT: i32 = (!0) << BITS;
    let unpack = |value: i32| {
        let mut result = value & MASK;
        if result & (1 << (BITS - 1)) != 0 {
            result |= INVERT;
        }
        result
    };
    (
        x - unpack(filler),
        y - unpack(filler >> (BITS * 2)),
        z - unpack(filler >> BITS),
    )
}

/// Convenience: all solid boxes overlapping one player AABB at `position`.
///
/// Used by tests and by callers that need a boolean overlap test without a
/// movement sweep.
pub fn aabb_has_collision(
    world: &dyn CollisionWorld,
    boxed: Aabb,
    scratch: &mut Vec<Aabb>,
) -> bool {
    let min_x = boxed.min.x.floor() as i32;
    let max_x = (boxed.max.x - f64::EPSILON).floor() as i32;
    let min_y = boxed.min.y.floor() as i32;
    let max_y = (boxed.max.y - f64::EPSILON).floor() as i32;
    let min_z = boxed.min.z.floor() as i32;
    let max_z = (boxed.max.z - f64::EPSILON).floor() as i32;
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                scratch.clear();
                world.collect_block_boxes(x, y, z, scratch);
                if scratch.iter().any(|shape| shape.intersects(boxed)) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_yaw_rotation_moves_the_unit_box() {
        let mut boxes = Vec::new();
        BlockShape::unit().world_boxes(1, (0, 0, 0), &mut boxes);
        // rotate90Y: min.x = min.z, min.z = 1 - old min.x.
        assert_eq!(boxes[0], Aabb::unit());

        let mut boxes = Vec::new();
        let shape = BlockShape {
            boxes: vec![Aabb::new(
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.5, 1.0, 1.0),
            )],
        };
        shape.world_boxes(1, (2, 3, 4), &mut boxes);
        // A half-width box rotated 90° around Y keeps its extents but swaps
        // the X/Z footprint.
        assert_eq!(
            boxes[0],
            Aabb::new(Vec3::new(2.0, 3.0, 4.5), Vec3::new(3.0, 4.0, 5.0))
        );
    }

    #[test]
    fn filler_offsets_unpack_like_java() {
        assert_eq!(filler_origin(10, 20, 30, 0), (10, 20, 30));
        // pack(1, 0, -1) = (1 & 31) | ((-1 & 31) << 5) | (0 << 10).
        let packed = 1 | (31 << 5);
        assert_eq!(filler_origin(10, 20, 30, packed), (9, 20, 31));
    }
}
