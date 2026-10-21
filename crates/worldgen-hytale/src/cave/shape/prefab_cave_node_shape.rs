// Ported from: `.hytale-source/.../worldgen/cave/shape/PrefabCaveNodeShape.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.shape.PrefabCaveNodeShape` —
//! a cave node that *is* a prefab (a dungeon).
//!
//! The prefab package (Phase 5a) landed: the prefab buffer, the prefab
//! store and the paste are ported, so
//!
//! * [`PrefabCaveNodeShape::get_floor_position`] /
//!   [`PrefabCaveNodeShape::get_ceiling_position`] read the per-column
//!   height map of the prefab (`IPrefabBuffer.getMinYAt` / `getMaxYAt`),
//! * [`PrefabCaveNodeShape::populate_chunk`] pastes the prefab
//!   (`PrefabPasteUtil.generate`).
//!
//! Note that the Java class does *not* extend `AbstractCaveNodeShape` — it
//! implements `CaveNodeShape` directly and has its own `populateChunk`.

use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::fast_random::FastRandom;
use hytale_math::hash_util;
use hytale_math::math_util;

use crate::cave::cave::Cave;
use crate::cave::cave_node_type::CaveNodeChildEntry;
use crate::cave::cave_type::CaveType;
use crate::cave::element::cave_node::CaveNode;
use crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::cave::shape::cave_node_shape_utils::get_box_anchor;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::prefab::prefab_paste_util::PrefabPasteBuffer;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::bounds::{IChunkBounds, WorldBounds};
use crate::util::condition::BlockMaskCondition;

/// Java `Double.doubleToLongBits(double)` — the raw bit pattern of the
/// double.
fn f64_bits(value: f64) -> i64 {
    value.to_bits() as i64
}

/// Java `public class PrefabCaveNodeShape implements CaveNodeShape,
/// IWorldBounds`.
pub struct PrefabCaveNodeShape {
    /// Java `private final CaveType caveType`.
    cave_type: Rc<CaveType>,
    /// Java `@Nonnull private final Vector3dc o` — the position of the
    /// prefab.
    o: Vector3d,
    /// Java `private final Vector3dc e` — the (unused) extent of the node.
    e: Vector3d,
    /// Java `@Nonnull private final WorldGenPrefabSupplier prefabSupplier`.
    prefab_supplier: Rc<dyn CavePrefabSupplier>,
    /// Java `@Nonnull private final PrefabRotation rotation`.
    rotation: PrefabRotation,
    /// Java `private final BlockMaskCondition configuration`.
    configuration: BlockMaskCondition,
    /// Java `private final int lowBoundX, lowBoundY, lowBoundZ, highBoundX,
    /// highBoundY, highBoundZ` — the shape *is* its own `IWorldBounds`.
    bounds: WorldBounds,
}

impl PrefabCaveNodeShape {
    /// Java `public PrefabCaveNodeShape(CaveType caveType, @Nonnull Vector3dc
    /// o, @Nonnull Vector3dc e, @Nonnull WorldGenPrefabSupplier
    /// prefabSupplier, @Nonnull PrefabRotation rotation, BlockMaskCondition
    /// configuration)`.
    pub fn new(
        cave_type: Rc<CaveType>,
        o: Vector3d,
        e: Vector3d,
        prefab_supplier: Rc<dyn CavePrefabSupplier>,
        rotation: PrefabRotation,
        configuration: BlockMaskCondition,
    ) -> Self {
        // Java:
        // ```java
        // final var prefab = prefabSupplier.get();
        // final var bounds = prefabSupplier.getBounds(prefab);
        // lowBoundX = MathUtil.floor(o.x() + bounds.getLowBoundX(rotation));
        // lowBoundY = MathUtil.floor(o.y() + prefab.getMinY());
        // lowBoundZ = MathUtil.floor(o.z() + bounds.getLowBoundZ(rotation));
        // highBoundX = MathUtil.ceil(o.x() + bounds.getHighBoundX(rotation));
        // highBoundY = MathUtil.ceil(o.y() + prefab.getMaxY());
        // highBoundZ = MathUtil.ceil(o.z() + bounds.getHighBoundZ(rotation));
        // ```
        //
        // The X/Z bounds are the *rotated chunk bounds* of the prefab (they
        // include the child prefabs), the Y bounds come from the prefab
        // buffer itself.
        let chunk_bounds = prefab_supplier.get_bounds();
        let min_y = prefab_supplier.get_min_y();
        let max_y = prefab_supplier.get_max_y();
        let bounds = WorldBounds::with_bounds(
            math_util::floor(o.x + chunk_bounds.get_low_bound_x_rotated(rotation) as f64),
            math_util::floor(o.y + min_y as f64),
            math_util::floor(o.z + chunk_bounds.get_low_bound_z_rotated(rotation) as f64),
            math_util::ceil(o.x + chunk_bounds.get_high_bound_x_rotated(rotation) as f64),
            math_util::ceil(o.y + max_y as f64),
            math_util::ceil(o.z + chunk_bounds.get_high_bound_z_rotated(rotation) as f64),
        );

        Self {
            cave_type,
            o,
            e,
            prefab_supplier,
            rotation,
            configuration,
            bounds,
        }
    }

    /// Java `public CaveType getCaveType()`.
    pub fn get_cave_type(&self) -> &Rc<CaveType> {
        &self.cave_type
    }

    /// Java `@Nonnull public PrefabRotation getPrefabRotation()`.
    pub fn get_prefab_rotation(&self) -> PrefabRotation {
        self.rotation
    }

    /// Java `@Nonnull public Vector3dc getO()`.
    pub fn get_o(&self) -> Vector3d {
        self.o
    }

    /// Java `private final BlockMaskCondition configuration` — the block mask
    /// the prefab paste applies.
    pub fn get_configuration(&self) -> &BlockMaskCondition {
        &self.configuration
    }
}

impl CaveNodeShape for PrefabCaveNodeShape {
    /// Java `@Nonnull @Override public Vector3d getStart()`.
    fn get_start(&self) -> Vector3d {
        self.o
    }

    /// Java `@Nonnull @Override public Vector3d getEnd()`.
    fn get_end(&self) -> Vector3d {
        Vector3d::new(
            self.o.x + self.e.x,
            self.o.y + self.e.y,
            self.o.z + self.e.z,
        )
    }

    /// Java `@Nonnull @Override public Vector3d getAnchor(@Nonnull Vector3d
    /// vector, double tx, double ty, double tz)`.
    fn get_anchor(&self, vector: Vector3d, tx: f64, ty: f64, tz: f64) -> Vector3d {
        let anchor = get_box_anchor(vector, &self.bounds, tx, ty, tz);
        // Move anchor to center of nearest block
        let x = math_util::floor(anchor.x) as f64 + 0.5;
        let y = math_util::floor(anchor.y) as f64 + 0.5;
        let z = math_util::floor(anchor.z) as f64 + 0.5;
        Vector3d::new(x, y, z)
    }

    /// Java `@Nonnull @Override public IWorldBounds getBounds()`.
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
    fn get_floor_position(&self, _seed: i32, x: f64, z: f64) -> f64 {
        // Java:
        // ```java
        // x -= o.x();
        // z -= o.z();
        // return prefabSupplier.get().getMinYAt(rotation, (int) x, (int) z);
        // ```
        let x = x - self.o.x;
        let z = z - self.o.z;
        let Some(prefab) = self.prefab_supplier.get_buffer() else {
            return -1.0;
        };
        prefab
            .new_access()
            .get_min_y_at(self.rotation, x as i32, z as i32) as f64
    }

    /// Java `@Override public double getCeilingPosition(int seed, double x,
    /// double z)`.
    fn get_ceiling_position(&self, _seed: i32, x: f64, z: f64) -> f64 {
        // Java:
        // ```java
        // x -= o.x();
        // z -= o.z();
        // return prefabSupplier.get().getMaxYAt(rotation, (int) x, (int) z);
        // ```
        let x = x - self.o.x;
        let z = z - self.o.z;
        let Some(prefab) = self.prefab_supplier.get_buffer() else {
            return -1.0;
        };
        prefab
            .new_access()
            .get_max_y_at(self.rotation, x as i32, z as i32) as f64
    }

    /// Java `@Override public void populateChunk(int seed, @Nonnull
    /// ChunkGeneratorExecution execution, @Nonnull Cave cave, @Nonnull CaveNode
    /// node, Random random)`.
    fn populate_chunk(
        &self,
        seed: i32,
        execution: &mut ChunkGeneratorExecution,
        _cave: &Cave,
        node: &CaveNode,
        _random: &mut FastRandom,
    ) {
        // Java:
        // ```java
        // final int x = MathUtil.floor(o.x());
        // final int y = MathUtil.floor(o.y());
        // final int z = MathUtil.floor(o.z());
        // final int cx = x - minBlock(execution.getX());
        // final int cz = z - minBlock(execution.getZ());
        // final long externalSeed = hash(Double.doubleToLongBits(o.x()), Double.doubleToLongBits(o.z())) * 0x53d9fac9;
        // var buffer = ChunkGenerator.getResource().prefabBuffer;
        // buffer.setSeed(seed, externalSeed);
        // buffer.execution = execution;
        // buffer.blockMask = configuration;
        // buffer.environmentId = node.getCaveNodeType().hasEnvironment() ? node.getCaveNodeType().getEnvironment() : caveType.getEnvironment();
        // buffer.priority = BlockPriorityChunk.PREFAB_CAVE;
        // PrefabPasteUtil.generate(buffer, rotation, prefabSupplier, x, y, z, cx, cz);
        // ```
        let x = math_util::floor(self.o.x);
        let y = math_util::floor(self.o.y);
        let z = math_util::floor(self.o.z);
        let cx = x - chunk_util::min_block(execution.get_x());
        let cz = z - chunk_util::min_block(execution.get_z());
        let external_seed =
            hash_util::hash2(f64_bits(self.o.x), f64_bits(self.o.z)).wrapping_mul(0x53d9_fac9);

        let Some(supplier) = self.prefab_supplier.as_paste_supplier() else {
            return;
        };

        let environment_id = if node.get_cave_node_type().has_environment() {
            node.get_cave_node_type().get_environment()
        } else {
            self.cave_type.get_environment()
        };

        let buffer = PrefabPasteBuffer::new();
        buffer.set_seed(seed, external_seed);
        *buffer.execution.borrow_mut() = Some(execution);
        *buffer.block_mask.borrow_mut() = Some(self.configuration.clone());
        buffer.environment_id.set(environment_id);
        buffer
            .priority
            .set(worldgen_model::block_priority::PREFAB_CAVE);

        crate::prefab::prefab_paste_util::generate(
            &buffer,
            self.rotation,
            supplier,
            x,
            y,
            z,
            cx,
            cz,
        );
    }

    /// Java `CaveNodeShapeUtils.getEndRadius(shape, …)` — the prefab shape
    /// does not expose a radius.
    fn get_end_radius(
        &self,
        _selector: crate::cave::shape::cave_node_shape_utils::WidthHeightSelector,
    ) -> f64 {
        -1.0
    }

    /// Java `shape instanceof PrefabCaveNodeShape` → the rotation of the
    /// prefab.
    fn as_prefab_rotation(&self) -> Option<PrefabRotation> {
        Some(self.rotation)
    }
}

/// Java `public static class PrefabCaveNodeShapeGenerator implements
/// CaveNodeShapeEnum.CaveNodeShapeGenerator`.
pub struct PrefabCaveNodeShapeGenerator {
    /// Java `private final List<WorldGenPrefabSupplier> prefabs`.
    prefabs: Vec<Rc<dyn CavePrefabSupplier>>,
    /// Java `private final BlockMaskCondition configuration`.
    configuration: BlockMaskCondition,
}

impl PrefabCaveNodeShapeGenerator {
    /// Java `public PrefabCaveNodeShapeGenerator(List<WorldGenPrefabSupplier>
    /// prefabs, BlockMaskCondition configuration)`.
    pub fn new(
        prefabs: Vec<Rc<dyn CavePrefabSupplier>>,
        configuration: BlockMaskCondition,
    ) -> Self {
        Self {
            prefabs,
            configuration,
        }
    }
}

impl CaveNodeShapeGenerator for PrefabCaveNodeShapeGenerator {
    /// Java `@Nonnull @Override public CaveNodeShape
    /// generateCaveNodeShape(@Nonnull Random random, CaveType caveType,
    /// @Nullable CaveNode parentNode, @Nonnull CaveNodeType.CaveNodeChildEntry
    /// childEntry, @Nonnull Vector3d origin, float yaw, float pitch)`.
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
        assert!(
            !self.prefabs.is_empty(),
            "PrefabCaveNodeShapeGenerator: the prefab list is empty (Java: IndexOutOfBoundsException)"
        );
        // Java: `prefabs.get(random.nextInt(prefabs.size()))` — a plain list
        // pick (not a weighted map).
        let prefab =
            self.prefabs[random.next_int_bound(self.prefabs.len() as i32) as usize].clone();

        if parent_node.is_none() {
            // Java:
            // ```java
            // final PrefabRotation rotation = PrefabRotation.VALUES[random.nextInt(PrefabRotation.VALUES.length)];
            // return new PrefabCaveNodeShape(caveType, origin, Vector3dUtil.ZERO, prefab, rotation, configuration);
            // ```
            let rotation = PrefabRotation::VALUES
                [random.next_int_bound(PrefabRotation::VALUES.len() as i32) as usize];
            return Box::new(PrefabCaveNodeShape::new(
                Rc::new(cave_type.clone()),
                origin,
                Vector3d::ZERO,
                prefab,
                rotation,
                self.configuration.clone(),
            ));
        }

        // Java:
        // ```java
        // final Vector3d offset = new Vector3d(childEntry.getOffset());
        // PrefabRotation rotation = childEntry.getRotation(random);
        // if (parentNode.getShape() instanceof PrefabCaveNodeShape parentShape) {
        //     final PrefabRotation parentRotation = parentShape.getPrefabRotation();
        //     parentRotation.rotate(offset);
        //     rotation = rotation.add(parentRotation);
        // }
        // origin.add(offset);
        // ```
        let child_entry = child_entry.expect(
            "PrefabCaveNodeShapeGenerator: the child entry is null (Java: NullPointerException)",
        );
        let mut offset = child_entry.get_offset();
        let mut rotation = child_entry.get_rotation(random);
        if let Some(parent_rotation) =
            parent_node.and_then(|parent| parent.get_shape().as_prefab_rotation())
        {
            parent_rotation.rotate(&mut offset);
            rotation = rotation.add(parent_rotation);
        }
        let origin = Vector3d::new(
            origin.x + offset.x,
            origin.y + offset.y,
            origin.z + offset.z,
        );

        Box::new(PrefabCaveNodeShape::new(
            Rc::new(cave_type.clone()),
            origin,
            Vector3d::ZERO,
            prefab,
            rotation,
            self.configuration.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::test_stubs::{cave_type, StubPrefabSupplier};
    use crate::util::bounds::{IChunkBounds, IWorldBounds};

    /// A prefab supplier with the given extents.
    fn supplier(
        min_x: i32,
        min_y: i32,
        min_z: i32,
        max_x: i32,
        max_y: i32,
        max_z: i32,
    ) -> Rc<dyn CavePrefabSupplier> {
        Rc::new(StubPrefabSupplier {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
        })
    }

    /// A prefab node at `(10, 20, 30)`.
    fn prefab_shape(rotation: PrefabRotation) -> PrefabCaveNodeShape {
        PrefabCaveNodeShape::new(
            cave_type("prefab"),
            Vector3d::new(10.0, 20.0, 30.0),
            Vector3d::ZERO,
            supplier(0, 0, 0, 4, 5, 6),
            rotation,
            BlockMaskCondition::new(),
        )
    }

    #[test]
    fn the_bounds_follow_the_prefab_extents() {
        let shape = prefab_shape(PrefabRotation::Rotation0);
        let bounds = shape.get_bounds();
        assert_eq!(bounds.get_low_bound_x(), 10);
        assert_eq!(bounds.get_high_bound_x(), 14);
        assert_eq!(bounds.get_low_bound_y(), 20);
        assert_eq!(bounds.get_high_bound_y(), 25);
        assert_eq!(bounds.get_low_bound_z(), 30);
        assert_eq!(bounds.get_high_bound_z(), 36);
    }

    #[test]
    fn the_start_is_the_position_and_the_end_is_the_position_plus_the_extent() {
        let shape = prefab_shape(PrefabRotation::Rotation0);
        assert_eq!(shape.get_start(), Vector3d::new(10.0, 20.0, 30.0));
        assert_eq!(shape.get_end(), Vector3d::new(10.0, 20.0, 30.0));
        assert_eq!(shape.get_o(), Vector3d::new(10.0, 20.0, 30.0));
    }

    #[test]
    fn the_shape_never_replaces_a_block() {
        let shape = prefab_shape(PrefabRotation::Rotation0);
        assert!(!shape.should_replace(0, 10.0, 30.0, 20));
    }

    #[test]
    fn the_rotation_is_reported_to_the_cave_generator() {
        let shape = prefab_shape(PrefabRotation::Rotation90);
        assert_eq!(
            CaveNodeShape::as_prefab_rotation(&shape),
            Some(PrefabRotation::Rotation90)
        );
        assert_eq!(shape.get_prefab_rotation(), PrefabRotation::Rotation90);
    }

    #[test]
    fn the_anchor_is_moved_to_the_centre_of_the_nearest_block() {
        let shape = prefab_shape(PrefabRotation::Rotation0);
        let anchor = shape.get_anchor(Vector3d::ZERO, 0.5, 0.5, 0.5);
        assert_eq!(anchor.x, 12.5);
        assert_eq!(anchor.y, 22.5);
        assert_eq!(anchor.z, 33.5);
    }

    #[test]
    fn the_generator_picks_a_prefab_and_a_rotation() {
        let generator = PrefabCaveNodeShapeGenerator::new(
            vec![supplier(0, 0, 0, 1, 1, 1), supplier(0, 0, 0, 2, 2, 2)],
            BlockMaskCondition::new(),
        );
        let mut random = FastRandom::new(1234);
        let shape = generator.generate_cave_node_shape(
            &mut random,
            &cave_type("prefab"),
            None,
            None,
            Vector3d::new(0.0, 0.0, 0.0),
            0.0,
            0.0,
        );

        // The reference: one `nextInt(prefabs.size())` and one
        // `nextInt(PrefabRotation.VALUES.length)`.
        let mut reference = FastRandom::new(1234);
        let prefab_index = reference.next_int_bound(2);
        let rotation_index = reference.next_int_bound(4);
        assert_eq!(random.next_double(), reference.next_double());

        assert_eq!(
            shape.as_prefab_rotation(),
            Some(PrefabRotation::VALUES[rotation_index as usize])
        );
        // The picked prefab is the first one (its bounds are 0..=1).
        assert_eq!(shape.get_bounds().get_high_bound_x(), prefab_index + 1);
    }

    #[test]
    fn the_floor_and_the_ceiling_come_from_the_prefab_buffer() {
        // The stub supplier has no prefab buffer, so the Java
        // `getMinYAt` / `getMaxYAt` of a missing column report `-1`.
        let shape = prefab_shape(PrefabRotation::Rotation0);
        assert_eq!(shape.get_floor_position(0, 10.0, 30.0), -1.0);
        assert_eq!(shape.get_ceiling_position(0, 10.0, 30.0), -1.0);
    }
}
