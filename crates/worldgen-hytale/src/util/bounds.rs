// Ported from: `.hytale-source/.../worldgen/util/bounds/*.java`
//! Java `com.hypixel.hytale.server.worldgen.util.bounds` package — axis aligned
//! bounds for chunks and for the world (chunk bounds plus a vertical range).
//!
//! | Java class     | Rust item        |
//! |----------------|------------------|
//! | `IChunkBounds` | [`IChunkBounds`] |
//! | `ChunkBounds`  | [`ChunkBounds`]  |
//! | `IWorldBounds` | [`IWorldBounds`] |
//! | `WorldBounds`  | [`WorldBounds`]  |
//!
//! ⚠ **Not ported**: the four `PrefabRotation` aware default methods
//! (`getLowBoundX(PrefabRotation)`, `getLowBoundZ(PrefabRotation)`,
//! `getHighBoundX(PrefabRotation)`, `getHighBoundZ(PrefabRotation)`) are
//! ported as `get_low_bound_x_rotated` & co. (the prefab package, Phase 5a).

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;

use crate::prefab::prefab_rotation::PrefabRotation;

/// Java `interface IChunkBounds`.
pub trait IChunkBounds {
    /// Java `int getLowBoundX()`.
    fn get_low_bound_x(&self) -> i32;

    /// Java `int getLowBoundZ()`.
    fn get_low_bound_z(&self) -> i32;

    /// Java `int getHighBoundX()`.
    fn get_high_bound_x(&self) -> i32;

    /// Java `int getHighBoundZ()`.
    fn get_high_bound_z(&self) -> i32;

    /// Java `default boolean intersectsChunk(long chunkIndex)`.
    fn intersects_chunk_index(&self, chunk_index: i64) -> bool {
        self.intersects_chunk(
            chunk_util::x_of_chunk_index(chunk_index),
            chunk_util::z_of_chunk_index(chunk_index),
        )
    }

    /// Java `default boolean intersectsChunk(int chunkX, int chunkZ)`.
    fn intersects_chunk(&self, chunk_x: i32, chunk_z: i32) -> bool {
        chunk_util::max_block(chunk_x) >= self.get_low_bound_x()
            && chunk_util::min_block(chunk_x) <= self.get_high_bound_x()
            && chunk_util::max_block(chunk_z) >= self.get_low_bound_z()
            && chunk_util::min_block(chunk_z) <= self.get_high_bound_z()
    }

    /// Java `default int randomX(Random random)`.
    fn random_x(&self, random: &mut FastRandom) -> i32 {
        get_random_offset(self.get_low_bound_x(), self.get_high_bound_x(), random)
    }

    /// Java `default int randomZ(Random random)`.
    fn random_z(&self, random: &mut FastRandom) -> i32 {
        get_random_offset(self.get_low_bound_z(), self.get_high_bound_z(), random)
    }

    /// Java `default double fractionX(double d)`.
    fn fraction_x(&self, d: f64) -> f64 {
        // Java: (getHighBoundX() - getLowBoundX()) * d + getLowBoundX();
        // the subtraction is an `int` subtraction in Java and wraps on
        // overflow.
        (self.get_high_bound_x().wrapping_sub(self.get_low_bound_x())) as f64 * d
            + self.get_low_bound_x() as f64
    }

    /// Java `default double fractionZ(double d)`.
    fn fraction_z(&self, d: f64) -> f64 {
        (self.get_high_bound_z().wrapping_sub(self.get_low_bound_z())) as f64 * d
            + self.get_low_bound_z() as f64
    }

    /// Java `default int getLowChunkX()`.
    fn get_low_chunk_x(&self) -> i32 {
        chunk_util::chunk_coordinate_i32(self.get_low_bound_x())
    }

    /// Java `default int getLowChunkZ()`.
    fn get_low_chunk_z(&self) -> i32 {
        chunk_util::chunk_coordinate_i32(self.get_low_bound_z())
    }

    /// Java `default int getHighChunkX()`.
    fn get_high_chunk_x(&self) -> i32 {
        chunk_util::chunk_coordinate_i32(self.get_high_bound_x())
    }

    /// Java `default int getHighChunkZ()`.
    fn get_high_chunk_z(&self) -> i32 {
        chunk_util::chunk_coordinate_i32(self.get_high_bound_z())
    }

    /// Java `default boolean isValid()`.
    fn is_valid(&self) -> bool {
        self.get_high_bound_x() > self.get_low_bound_x()
            && self.get_high_bound_z() > self.get_low_bound_z()
    }

    /// Java `default int getLowBoundX(@Nonnull PrefabRotation rotation)`.
    ///
    /// The rotated bounds of the box: the corners are rotated and the
    /// smallest / largest of them is reported.
    fn get_low_bound_x_rotated(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_x(self.get_low_bound_x(), self.get_low_bound_z())
            .min(rotation.get_x(self.get_high_bound_x(), self.get_high_bound_z()))
    }

    /// Java `default int getLowBoundZ(@Nonnull PrefabRotation rotation)`.
    fn get_low_bound_z_rotated(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_z(self.get_low_bound_x(), self.get_low_bound_z())
            .min(rotation.get_z(self.get_high_bound_x(), self.get_high_bound_z()))
    }

    /// Java `default int getHighBoundX(@Nonnull PrefabRotation rotation)`.
    fn get_high_bound_x_rotated(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_x(self.get_low_bound_x(), self.get_low_bound_z())
            .max(rotation.get_x(self.get_high_bound_x(), self.get_high_bound_z()))
    }

    /// Java `default int getHighBoundZ(@Nonnull PrefabRotation rotation)`.
    fn get_high_bound_z_rotated(&self, rotation: PrefabRotation) -> i32 {
        rotation
            .get_z(self.get_low_bound_x(), self.get_low_bound_z())
            .max(rotation.get_z(self.get_high_bound_x(), self.get_high_bound_z()))
    }

    /// Java: the `bounds instanceof IWorldBounds` check performed by
    /// `WorldBounds.include(IChunkBounds)`.
    ///
    /// Rust has no `instanceof`; the concrete implementations override this
    /// method so that a world bound can be recognised through an
    /// `IChunkBounds` reference.
    fn as_world_bounds(&self) -> Option<&dyn IWorldBounds> {
        None
    }
}

/// Java `static int IChunkBounds.getRandomOffset(int min, int max, Random random)`.
pub fn get_random_offset(min: i32, max: i32, random: &mut FastRandom) -> i32 {
    // Java: assert max > min : "Invalid bounds: " + min + " to " + max;
    debug_assert!(max > min, "Invalid bounds: {min} to {max}");

    // Java: return max <= min ? min : random.nextInt(max - min) + min;
    if max <= min {
        min
    } else {
        random.next_int_bound(max - min) + min
    }
}

/// Java `class ChunkBounds implements IChunkBounds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkBounds {
    /// Java `protected int minX`.
    pub min_x: i32,
    /// Java `protected int minZ`.
    pub min_z: i32,
    /// Java `protected int maxX`.
    pub max_x: i32,
    /// Java `protected int maxZ`.
    pub max_z: i32,
}

impl ChunkBounds {
    /// Java `ChunkBounds()` — the empty (inverted) bounds.
    pub const fn new() -> Self {
        Self {
            min_x: i32::MAX,
            min_z: i32::MAX,
            max_x: i32::MIN,
            max_z: i32::MIN,
        }
    }

    /// Java `ChunkBounds(IChunkBounds bounds)`.
    pub fn from_bounds(bounds: &dyn IChunkBounds) -> Self {
        Self {
            min_x: bounds.get_low_bound_x(),
            min_z: bounds.get_low_bound_z(),
            max_x: bounds.get_high_bound_x(),
            max_z: bounds.get_high_bound_z(),
        }
    }

    /// Java `ChunkBounds(int minX, int minZ, int maxX, int maxZ)`.
    pub const fn with_bounds(min_x: i32, min_z: i32, max_x: i32, max_z: i32) -> Self {
        Self {
            min_x,
            min_z,
            max_x,
            max_z,
        }
    }

    /// Java `ChunkBounds(int x, int z)` — a single point.
    pub const fn from_point(x: i32, z: i32) -> Self {
        Self {
            min_x: x,
            min_z: z,
            max_x: x,
            max_z: z,
        }
    }

    /// Java `void expandNegative(int x, int z)`.
    pub fn expand_negative_i32(&mut self, x: i32, z: i32) {
        self.min_x = self.min_x.wrapping_add(x);
        self.min_z = self.min_z.wrapping_add(z);
    }

    /// Java `void expandPositive(int x, int z)`.
    pub fn expand_positive_i32(&mut self, x: i32, z: i32) {
        self.max_x = self.max_x.wrapping_add(x);
        self.max_z = self.max_z.wrapping_add(z);
    }

    /// Java `void expandNegative(double x, double z)`.
    pub fn expand_negative_f64(&mut self, x: f64, z: f64) {
        self.min_x = hytale_math::math_util::floor(self.min_x as f64 + x);
        self.min_z = hytale_math::math_util::floor(self.min_z as f64 + z);
    }

    /// Java `void expandPositive(double x, double z)`.
    pub fn expand_positive_f64(&mut self, x: f64, z: f64) {
        self.max_x = hytale_math::math_util::ceil(self.max_x as f64 + x);
        self.max_z = hytale_math::math_util::ceil(self.max_z as f64 + z);
    }

    /// Java `void include(int minX, int minZ, int maxX, int maxZ)`.
    pub fn include(&mut self, min_x: i32, min_z: i32, max_x: i32, max_z: i32) {
        if self.min_x > min_x {
            self.min_x = min_x;
        }
        if self.min_z > min_z {
            self.min_z = min_z;
        }
        if self.max_x < max_x {
            self.max_x = max_x;
        }
        if self.max_z < max_z {
            self.max_z = max_z;
        }
    }

    /// Java `void include(int x, int z)`.
    ///
    /// Note the Java `else if` structure: a coordinate that is *inside* the
    /// current bounds but beyond the opposite bound does **not** extend the
    /// bounds.
    pub fn include_point(&mut self, x: i32, z: i32) {
        if self.min_x > x {
            self.min_x = x;
        } else if self.max_x < x {
            self.max_x = x;
        }
        if self.min_z > z {
            self.min_z = z;
        } else if self.max_z < z {
            self.max_z = z;
        }
    }

    /// Java `void include(IChunkBounds box)`.
    pub fn include_bounds(&mut self, box_bounds: &dyn IChunkBounds) {
        if self.min_x > box_bounds.get_low_bound_x() {
            self.min_x = box_bounds.get_low_bound_x();
        }
        if self.min_z > box_bounds.get_low_bound_z() {
            self.min_z = box_bounds.get_low_bound_z();
        }
        if self.max_x < box_bounds.get_high_bound_x() {
            self.max_x = box_bounds.get_high_bound_x();
        }
        if self.max_z < box_bounds.get_high_bound_z() {
            self.max_z = box_bounds.get_high_bound_z();
        }
    }
}

impl Default for ChunkBounds {
    fn default() -> Self {
        Self::new()
    }
}

impl IChunkBounds for ChunkBounds {
    fn get_low_bound_x(&self) -> i32 {
        self.min_x
    }

    fn get_low_bound_z(&self) -> i32 {
        self.min_z
    }

    fn get_high_bound_x(&self) -> i32 {
        self.max_x
    }

    fn get_high_bound_z(&self) -> i32 {
        self.max_z
    }
}

/// Java `interface IWorldBounds extends IChunkBounds`.
pub trait IWorldBounds: IChunkBounds {
    /// Java `int getLowBoundY()`.
    fn get_low_bound_y(&self) -> i32;

    /// Java `int getHighBoundY()`.
    fn get_high_bound_y(&self) -> i32;

    /// Java `default int randomY(Random random)`.
    fn random_y(&self, random: &mut FastRandom) -> i32 {
        get_random_offset(self.get_low_bound_y(), self.get_high_bound_y(), random)
    }

    /// Java `default double fractionY(double d)`.
    fn fraction_y(&self, d: f64) -> f64 {
        (self.get_high_bound_y().wrapping_sub(self.get_low_bound_y())) as f64 * d
            + self.get_low_bound_y() as f64
    }

    /// Java `default boolean isValid()` — the chunk bounds plus the vertical
    /// range.
    fn is_valid(&self) -> bool {
        IChunkBounds::is_valid(self) && self.get_high_bound_y() > self.get_low_bound_y()
    }
}

/// Java `class WorldBounds extends ChunkBounds implements IWorldBounds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldBounds {
    /// Java `protected int minX`.
    pub min_x: i32,
    /// Java `protected int minY`.
    pub min_y: i32,
    /// Java `protected int minZ`.
    pub min_z: i32,
    /// Java `protected int maxX`.
    pub max_x: i32,
    /// Java `protected int maxY`.
    pub max_y: i32,
    /// Java `protected int maxZ`.
    pub max_z: i32,
}

impl WorldBounds {
    /// Java `WorldBounds()` — the empty (inverted) bounds.
    pub const fn new() -> Self {
        Self {
            min_x: i32::MAX,
            min_y: i32::MAX,
            min_z: i32::MAX,
            max_x: i32::MIN,
            max_y: i32::MIN,
            max_z: i32::MIN,
        }
    }

    /// Java `WorldBounds(IWorldBounds bounds)`.
    pub fn from_bounds(bounds: &dyn IWorldBounds) -> Self {
        Self {
            min_x: bounds.get_low_bound_x(),
            min_y: bounds.get_low_bound_y(),
            min_z: bounds.get_low_bound_z(),
            max_x: bounds.get_high_bound_x(),
            max_y: bounds.get_high_bound_y(),
            max_z: bounds.get_high_bound_z(),
        }
    }

    /// Java `WorldBounds(int minX, int minY, int minZ, int maxX, int maxY, int maxZ)`.
    pub const fn with_bounds(
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> Self {
        Self {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
        }
    }

    /// Java `WorldBounds(int x, int y, int z)` — a single point.
    pub const fn from_point(x: i32, y: i32, z: i32) -> Self {
        Self {
            min_x: x,
            min_y: y,
            min_z: z,
            max_x: x,
            max_y: y,
            max_z: z,
        }
    }

    /// Java `IWorldBounds.isValid()` — the chunk bounds plus the vertical
    /// range.
    ///
    /// Deviation: `WorldBounds` implements both `IChunkBounds` and
    /// `IWorldBounds`, and both traits declare `is_valid`. Java resolves this
    /// through the interface override (`IWorldBounds.isValid()` wins); Rust
    /// needs the inherent method to pick the same winner — an inherent method
    /// takes precedence over the trait methods.
    pub fn is_valid(&self) -> bool {
        IWorldBounds::is_valid(self)
    }

    /// Java `void expandNegative(int x, int z)`.
    pub fn expand_negative_i32(&mut self, x: i32, z: i32) {
        self.min_x = self.min_x.wrapping_add(x);
        self.min_z = self.min_z.wrapping_add(z);
    }

    /// Java `void expandPositive(int x, int z)`.
    pub fn expand_positive_i32(&mut self, x: i32, z: i32) {
        self.max_x = self.max_x.wrapping_add(x);
        self.max_z = self.max_z.wrapping_add(z);
    }

    /// Java `void expandNegative(double x, double y, double z)`.
    pub fn expand_negative_f64(&mut self, x: f64, y: f64, z: f64) {
        self.expand_negative_f64_xz(x, z);
        self.min_y = hytale_math::math_util::floor(self.min_y as f64 + y);
    }

    /// Java `void expandPositive(double x, double y, double z)`.
    pub fn expand_positive_f64(&mut self, x: f64, y: f64, z: f64) {
        self.expand_positive_f64_xz(x, z);
        self.max_y = hytale_math::math_util::ceil(self.max_y as f64 + y);
    }

    /// Java `ChunkBounds.expandNegative(double x, double z)`.
    fn expand_negative_f64_xz(&mut self, x: f64, z: f64) {
        self.min_x = hytale_math::math_util::floor(self.min_x as f64 + x);
        self.min_z = hytale_math::math_util::floor(self.min_z as f64 + z);
    }

    /// Java `ChunkBounds.expandPositive(double x, double z)`.
    fn expand_positive_f64_xz(&mut self, x: f64, z: f64) {
        self.max_x = hytale_math::math_util::ceil(self.max_x as f64 + x);
        self.max_z = hytale_math::math_util::ceil(self.max_z as f64 + z);
    }

    /// Java `void include(int minX, int minZ, int maxX, int maxZ)`.
    pub fn include(&mut self, min_x: i32, min_z: i32, max_x: i32, max_z: i32) {
        if self.min_x > min_x {
            self.min_x = min_x;
        }
        if self.min_z > min_z {
            self.min_z = min_z;
        }
        if self.max_x < max_x {
            self.max_x = max_x;
        }
        if self.max_z < max_z {
            self.max_z = max_z;
        }
    }

    /// Java `void include(int x, int z)`.
    pub fn include_point(&mut self, x: i32, z: i32) {
        if self.min_x > x {
            self.min_x = x;
        } else if self.max_x < x {
            self.max_x = x;
        }
        if self.min_z > z {
            self.min_z = z;
        } else if self.max_z < z {
            self.max_z = z;
        }
    }

    /// Java `void include(IChunkBounds bounds)` — the vertical range is merged
    /// as well when the given bounds are world bounds.
    pub fn include_bounds(&mut self, bounds: &dyn IChunkBounds) {
        if self.min_x > bounds.get_low_bound_x() {
            self.min_x = bounds.get_low_bound_x();
        }
        if self.min_z > bounds.get_low_bound_z() {
            self.min_z = bounds.get_low_bound_z();
        }
        if self.max_x < bounds.get_high_bound_x() {
            self.max_x = bounds.get_high_bound_x();
        }
        if self.max_z < bounds.get_high_bound_z() {
            self.max_z = bounds.get_high_bound_z();
        }
        // Java: if (bounds instanceof IWorldBounds worldBounds) { ... }
        if let Some(world_bounds) = bounds.as_world_bounds() {
            if self.min_y > world_bounds.get_low_bound_y() {
                self.min_y = world_bounds.get_low_bound_y();
            }
            if self.max_y < world_bounds.get_high_bound_y() {
                self.max_y = world_bounds.get_high_bound_y();
            }
        }
    }
}

impl Default for WorldBounds {
    fn default() -> Self {
        Self::new()
    }
}

impl IChunkBounds for WorldBounds {
    fn get_low_bound_x(&self) -> i32 {
        self.min_x
    }

    fn get_low_bound_z(&self) -> i32 {
        self.min_z
    }

    fn get_high_bound_x(&self) -> i32 {
        self.max_x
    }

    fn get_high_bound_z(&self) -> i32 {
        self.max_z
    }

    fn as_world_bounds(&self) -> Option<&dyn IWorldBounds> {
        Some(self)
    }
}

impl IWorldBounds for WorldBounds {
    fn get_low_bound_y(&self) -> i32 {
        self.min_y
    }

    fn get_high_bound_y(&self) -> i32 {
        self.max_y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bounds_are_inverted_and_invalid() {
        let bounds = ChunkBounds::new();
        assert_eq!(bounds.min_x, i32::MAX);
        assert_eq!(bounds.max_x, i32::MIN);
        assert!(!bounds.is_valid());
    }

    #[test]
    fn intersects_chunk_uses_block_ranges() {
        // Chunk 0 spans blocks 0..=31, chunk 1 spans 32..=63.
        let bounds = ChunkBounds::with_bounds(20, 20, 40, 40);
        assert!(bounds.intersects_chunk(0, 0));
        assert!(bounds.intersects_chunk(1, 1));
        assert!(!bounds.intersects_chunk(2, 2));
        assert!(!bounds.intersects_chunk(-1, 0));
    }

    #[test]
    fn intersects_chunk_index_uses_the_packed_index() {
        let bounds = ChunkBounds::with_bounds(0, 0, 31, 31);
        let index = chunk_util::index_chunk(0, 0);
        assert!(bounds.intersects_chunk_index(index));
        let other = chunk_util::index_chunk(5, 5);
        assert!(!bounds.intersects_chunk_index(other));
    }

    #[test]
    fn include_range_grows_the_bounds() {
        let mut bounds = ChunkBounds::with_bounds(0, 0, 10, 10);
        bounds.include(-5, -5, 5, 5);
        assert_eq!(bounds.min_x, -5);
        assert_eq!(bounds.min_z, -5);
        assert_eq!(bounds.max_x, 10);
        assert_eq!(bounds.max_z, 10);
    }

    #[test]
    fn include_point_keeps_java_else_if_semantics() {
        // A point inside the bounds does not extend them.
        let mut bounds = ChunkBounds::with_bounds(0, 0, 10, 10);
        bounds.include_point(5, 5);
        assert_eq!(bounds.min_x, 0);
        assert_eq!(bounds.max_x, 10);
        // A point below the minimum extends the minimum only.
        let mut bounds = ChunkBounds::with_bounds(0, 0, 10, 10);
        bounds.include_point(-1, -1);
        assert_eq!(bounds.min_x, -1);
        assert_eq!(bounds.max_x, 10);
        // A point above the maximum extends the maximum only.
        let mut bounds = ChunkBounds::with_bounds(0, 0, 10, 10);
        bounds.include_point(11, 11);
        assert_eq!(bounds.min_x, 0);
        assert_eq!(bounds.max_x, 11);
    }

    #[test]
    fn include_bounds_merges_another_box() {
        let mut bounds = ChunkBounds::with_bounds(0, 0, 10, 10);
        bounds.include_bounds(&ChunkBounds::with_bounds(-5, -5, 5, 5));
        assert_eq!(bounds.min_x, -5);
        assert_eq!(bounds.max_x, 10);
    }

    #[test]
    fn expand_methods_use_java_arithmetic() {
        let mut bounds = ChunkBounds::with_bounds(0, 0, 10, 10);
        bounds.expand_negative_i32(-2, -3);
        assert_eq!(bounds.min_x, -2);
        assert_eq!(bounds.min_z, -3);
        bounds.expand_positive_i32(2, 3);
        assert_eq!(bounds.max_x, 12);
        assert_eq!(bounds.max_z, 13);

        let mut bounds = ChunkBounds::with_bounds(0, 0, 10, 10);
        bounds.expand_negative_f64(-0.5, -1.5);
        assert_eq!(bounds.min_x, -1);
        assert_eq!(bounds.min_z, -2);
        bounds.expand_positive_f64(0.5, 1.5);
        assert_eq!(bounds.max_x, 11);
        assert_eq!(bounds.max_z, 12);
    }

    #[test]
    fn fraction_uses_the_java_formula() {
        let bounds = ChunkBounds::with_bounds(10, 20, 20, 40);
        assert_eq!(bounds.fraction_x(0.0), 10.0);
        assert_eq!(bounds.fraction_x(0.5), 15.0);
        assert_eq!(bounds.fraction_x(1.0), 20.0);
        assert_eq!(bounds.fraction_z(0.5), 30.0);
    }

    #[test]
    fn random_offset_stays_inside_the_range() {
        let mut random = FastRandom::new(1234);
        for _ in 0..100 {
            let v = get_random_offset(4, 8, &mut random);
            assert!((4..8).contains(&v), "out of range: {v}");
        }
    }

    #[test]
    #[should_panic(expected = "Invalid bounds")]
    fn random_offset_asserts_on_invalid_bounds() {
        // Java: `assert max > min : "Invalid bounds: " + min + " to " + max;`
        // The assertion is only enabled in debug builds (Java: only with
        // `-ea`), so the `max <= min ? min` branch below is only reachable in
        // release builds.
        let mut random = FastRandom::new(1);
        let _ = get_random_offset(5, 5, &mut random);
    }

    #[test]
    fn chunk_coordinates_are_derived_from_block_coordinates() {
        let bounds = ChunkBounds::with_bounds(33, 65, 100, 200);
        assert_eq!(bounds.get_low_chunk_x(), 1);
        assert_eq!(bounds.get_low_chunk_z(), 2);
        assert_eq!(bounds.get_high_chunk_x(), 3);
        assert_eq!(bounds.get_high_chunk_z(), 6);
    }

    #[test]
    fn world_bounds_add_the_vertical_range() {
        let bounds = WorldBounds::with_bounds(0, 0, 0, 10, 10, 10);
        assert!(bounds.is_valid());
        assert_eq!(bounds.fraction_y(0.5), 5.0);
        assert_eq!(bounds.get_low_chunk_x(), 0);
    }

    #[test]
    fn world_bounds_are_invalid_without_a_vertical_range() {
        let bounds = WorldBounds::with_bounds(0, 5, 0, 10, 5, 10);
        assert!(!bounds.is_valid(), "high y must be greater than low y");
    }

    #[test]
    fn world_bounds_include_merges_the_vertical_range() {
        let mut bounds = WorldBounds::with_bounds(0, 0, 0, 10, 10, 10);
        bounds.include_bounds(&WorldBounds::with_bounds(-5, -5, -5, 5, 5, 5));
        assert_eq!(bounds.min_x, -5);
        assert_eq!(bounds.min_y, -5);
        assert_eq!(bounds.max_y, 10);
    }

    #[test]
    fn world_bounds_include_of_chunk_bounds_keeps_the_vertical_range() {
        let mut bounds = WorldBounds::with_bounds(0, 0, 0, 10, 10, 10);
        bounds.include_bounds(&ChunkBounds::with_bounds(-5, -5, 5, 5));
        assert_eq!(bounds.min_x, -5);
        assert_eq!(bounds.min_y, 0, "chunk bounds carry no vertical range");
    }

    #[test]
    fn world_bounds_expand_with_floors_and_ceils() {
        let mut bounds = WorldBounds::with_bounds(0, 0, 0, 10, 10, 10);
        bounds.expand_negative_f64(-0.5, -1.5, -0.5);
        assert_eq!(bounds.min_x, -1);
        assert_eq!(bounds.min_y, -2);
        assert_eq!(bounds.min_z, -1);
        bounds.expand_positive_f64(0.5, 1.5, 0.5);
        assert_eq!(bounds.max_x, 11);
        assert_eq!(bounds.max_y, 12);
        assert_eq!(bounds.max_z, 11);
    }
}
