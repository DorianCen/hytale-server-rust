// Ported from: `.hytale-source/.../worldgen/cave/element/CaveNode.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.element.CaveNode` — one
//! segment of a cave: a shape, its orientation and the prefabs that were
//! spawned inside it.

use std::cell::RefCell;
use std::rc::Rc;

use hytale_math::chunk_util;
use hytale_math::math_util;

use crate::cave::cave_node_type::CaveNodeType;
use crate::cave::element::cave_element::CaveElement;
use crate::cave::element::cave_prefab::CavePrefab;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::loader::util::Vector3d;
use crate::util::bounds::{IChunkBounds, WorldBounds};

/// Java `class CaveNode implements CaveElement`.
pub struct CaveNode {
    /// Java `private final CaveNodeType caveNodeType`.
    pub cave_node_type: Rc<CaveNodeType>,
    /// Java `@Nonnull private final CaveNodeShape shape`.
    pub shape: Rc<dyn CaveNodeShape>,
    /// Java `@Nonnull private final WorldBounds bounds`.
    ///
    /// The Java bounds grow when a prefab is added to the node — which can
    /// happen after the node was added to the cave — so the Rust port keeps
    /// them behind a [`RefCell`].
    pub bounds: RefCell<WorldBounds>,
    /// Java `private final int seedOffset`.
    pub seed_offset: i32,
    /// Java `private final float pitch`.
    pub pitch: f32,
    /// Java `private final float yaw`.
    pub yaw: f32,
    /// Java `@Nullable private List<CavePrefab> rawCavePrefabs` — `None`
    /// after [`CaveNode::compile`].
    pub raw_cave_prefabs: RefCell<Option<Vec<Rc<CavePrefab>>>>,
    /// Java `private CavePrefab[] cavePrefabs`.
    pub cave_prefabs: RefCell<Vec<Rc<CavePrefab>>>,
}

impl CaveNode {
    /// Java `CaveNode(int seedOffset, CaveNodeType caveNodeType, CaveNodeShape
    /// shape, float yaw, float pitch)`.
    pub fn new(
        seed_offset: i32,
        cave_node_type: Rc<CaveNodeType>,
        shape: Rc<dyn CaveNodeShape>,
        yaw: f32,
        pitch: f32,
    ) -> Self {
        // Java: `this.bounds = new WorldBounds(shape.getBounds())`.
        let bounds = WorldBounds::from_bounds(shape.get_bounds());
        Self {
            cave_node_type,
            shape,
            bounds: RefCell::new(bounds),
            seed_offset,
            pitch,
            yaw,
            raw_cave_prefabs: RefCell::new(Some(Vec::new())),
            cave_prefabs: RefCell::new(Vec::new()),
        }
    }

    /// Java `int getSeedOffset()`.
    pub fn get_seed_offset(&self) -> i32 {
        self.seed_offset
    }

    /// Java `CaveNodeType getCaveNodeType()`.
    pub fn get_cave_node_type(&self) -> &Rc<CaveNodeType> {
        &self.cave_node_type
    }

    /// Java `@Nonnull CaveNodeShape getShape()`.
    pub fn get_shape(&self) -> &Rc<dyn CaveNodeShape> {
        &self.shape
    }

    /// Java `CavePrefab[] getCavePrefabs()`.
    ///
    /// Deviation: the Java array is `null` before [`CaveNode::compile`]; the
    /// Rust port hands out an empty list instead.
    pub fn get_cave_prefabs(&self) -> Vec<Rc<CavePrefab>> {
        self.cave_prefabs.borrow().clone()
    }

    /// Java `float getYaw()`.
    pub fn get_yaw(&self) -> f32 {
        self.yaw
    }

    /// Java `float getPitch()`.
    pub fn get_pitch(&self) -> f32 {
        self.pitch
    }

    /// Java `Vector3d getEnd()`.
    pub fn get_end(&self) -> Vector3d {
        self.shape.get_end()
    }

    /// Java `void addPrefab(CavePrefab prefab)`.
    ///
    /// Panics when the node was already compiled — the Java code throws a
    /// `NullPointerException` in the same case.
    pub fn add_prefab(&self, prefab: Rc<CavePrefab>) {
        {
            let mut raw = self.raw_cave_prefabs.borrow_mut();
            let list = raw.as_mut().expect(
                "CaveNode.addPrefab: the node was already compiled (Java: NullPointerException)",
            );
            list.push(Rc::clone(&prefab));
        }
        self.bounds
            .borrow_mut()
            .include_bounds(&prefab.get_bounds());
    }

    /// Java `int getFloorPosition(int seed, double x, double z)`.
    pub fn get_floor_position(&self, seed: i32, x: f64, z: f64) -> i32 {
        math_util::floor(self.shape.get_floor_position(seed, x, z))
    }

    /// Java `int getCeilingPosition(int seed, double x, double z)`.
    pub fn get_ceiling_position(&self, seed: i32, x: f64, z: f64) -> i32 {
        math_util::floor(self.shape.get_ceiling_position(seed, x, z))
    }

    /// Java `void forEachChunk(LongConsumer consumer)` — every chunk the
    /// bounds of the node touch.
    pub fn for_each_chunk(&self, consumer: &mut dyn FnMut(i64)) {
        let bounds = *self.bounds.borrow();
        let low_z = bounds.get_low_chunk_z();
        let high_x = bounds.get_high_chunk_x();
        let high_z = bounds.get_high_chunk_z();
        for x in bounds.get_low_chunk_x()..=high_x {
            for z in low_z..=high_z {
                consumer(chunk_util::index_chunk(x, z));
            }
        }
    }

    /// Java `void compile()` — freezes the prefab list.
    ///
    /// Panics when the node is compiled twice — the Java code throws a
    /// `NullPointerException` in the same case.
    pub fn compile(&self) {
        let raw =
            self.raw_cave_prefabs.borrow_mut().take().expect(
                "CaveNode.compile: the node was already compiled (Java: NullPointerException)",
            );
        *self.cave_prefabs.borrow_mut() = raw;
    }
}

impl CaveElement for CaveNode {
    /// Java `IWorldBounds getBounds()`.
    fn get_bounds(&self) -> WorldBounds {
        *self.bounds.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::element::cave_prefab::CavePrefab;
    use crate::cave::test_stubs::{cave_node_type, StubPrefabSupplier, StubShape};
    use crate::prefab::prefab_rotation::PrefabRotation;
    use crate::util::condition::BlockMaskCondition;
    use procedural::noise::condition::AlwaysTrueIntCondition;

    /// A node with the given bounds, floor and ceiling.
    fn node(bounds: WorldBounds, floor: f64, ceiling: f64) -> Rc<CaveNode> {
        Rc::new(CaveNode::new(
            17,
            cave_node_type(3),
            Rc::new(StubShape::new(bounds, floor, ceiling)),
            1.5,
            -2.5,
        ))
    }

    /// A prefab that spans one block.
    fn prefab(x: i32, y: i32, z: i32) -> Rc<CavePrefab> {
        Rc::new(CavePrefab::new(
            Rc::new(StubPrefabSupplier {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 0,
                max_y: 0,
                max_z: 0,
            }),
            PrefabRotation::Rotation0,
            Rc::new(AlwaysTrueIntCondition),
            BlockMaskCondition::new(),
            x,
            y,
            z,
        ))
    }

    #[test]
    fn the_accessors_return_the_stored_values() {
        let node = node(WorldBounds::with_bounds(0, 0, 0, 10, 10, 10), 1.0, 9.0);
        assert_eq!(node.get_seed_offset(), 17);
        assert_eq!(node.get_yaw(), 1.5);
        assert_eq!(node.get_pitch(), -2.5);
        assert_eq!(node.get_cave_node_type().get_priority(), 3);
        assert_eq!(
            node.get_bounds(),
            WorldBounds::with_bounds(0, 0, 0, 10, 10, 10)
        );
    }

    #[test]
    fn the_floor_and_the_ceiling_are_floored() {
        let node = node(WorldBounds::with_bounds(0, 0, 0, 10, 10, 10), 1.7, 9.2);
        assert_eq!(node.get_floor_position(0, 0.0, 0.0), 1);
        assert_eq!(node.get_ceiling_position(0, 0.0, 0.0), 9);
    }

    #[test]
    fn the_node_is_filed_under_every_chunk_it_touches() {
        let node = node(WorldBounds::with_bounds(0, 0, 0, 63, 0, 0), 0.0, 0.0);
        let mut chunks = Vec::new();
        node.for_each_chunk(&mut |chunk| chunks.push(chunk));
        assert_eq!(
            chunks,
            vec![
                hytale_math::chunk_util::index_chunk(0, 0),
                hytale_math::chunk_util::index_chunk(1, 0)
            ]
        );
    }

    #[test]
    fn the_prefabs_are_frozen_by_the_compile() {
        let node = node(WorldBounds::with_bounds(0, 0, 0, 10, 10, 10), 0.0, 0.0);
        node.add_prefab(prefab(1, 2, 3));
        assert!(node.get_cave_prefabs().is_empty(), "not compiled yet");

        node.compile();
        assert_eq!(node.get_cave_prefabs().len(), 1);
        let prefab = &node.get_cave_prefabs()[0];
        assert_eq!(prefab.get_x(), 1);
        assert_eq!(prefab.get_y(), 2);
        assert_eq!(prefab.get_z(), 3);
    }

    #[test]
    fn a_prefab_grows_the_bounds_of_the_node() {
        let node = node(WorldBounds::with_bounds(0, 0, 0, 10, 10, 10), 0.0, 0.0);
        node.add_prefab(prefab(-5, -5, -5));
        let bounds = node.get_bounds();
        assert_eq!(bounds.min_x, -5);
        assert_eq!(bounds.min_y, -5);
        assert_eq!(bounds.min_z, -5);
    }

    #[test]
    #[should_panic(expected = "already compiled")]
    fn compiling_twice_panics_like_the_java_null_pointer() {
        let node = node(WorldBounds::with_bounds(0, 0, 0, 10, 10, 10), 0.0, 0.0);
        node.compile();
        node.compile();
    }

    #[test]
    fn the_prefab_carries_its_configuration() {
        let prefab = prefab(1, 2, 3);
        assert_eq!(prefab.get_rotation(), PrefabRotation::Rotation0);
        assert!(prefab.get_biome_mask().eval(5));
        assert!(
            prefab.get_configuration().eval(0, 0, 1, 0),
            "the default mask replaces an empty block"
        );
        assert_eq!(prefab.get_bounds(), WorldBounds::from_point(1, 2, 3));
    }
}
