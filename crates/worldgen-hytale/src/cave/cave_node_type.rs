// Ported from: `.hytale-source/.../worldgen/cave/CaveNodeType.java`
//! Java `com.hypixel.hytale.server.worldgen.cave.CaveNodeType` — the type of
//! a cave node: the shape it grows into, the children it may spawn, the
//! covers it places and the fillings it fills its volume with.

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::noise::condition::{
    IBlockFluidCondition, ICoordinateCondition, ICoordinateRndCondition,
};
use procedural::noise::supplier::IDoubleRange;

use crate::cave::cave_yaw_mode::CaveYawMode;
use crate::cave::prefab::cave_prefab_container::CavePrefabContainer;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::loader::util::Vector3d;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::list_pool::ListPool;
use crate::weighted_map::IWeightedMap;

thread_local! {
    /// Java `public static final ListPool<CaveNodeCoverEntry> COVER_POOL` =
    /// `new ListPool<>(10, CaveNodeCoverEntry.EMPTY_ARRAY)`.
    ///
    /// Deviation: Java's pool is a process-wide static. The Rust entries
    /// hold `Rc<dyn …>` trait objects, which are not `Send`, so a global
    /// `static` cannot be `Sync` (a `static` requires `Sync`). The pool is
    /// therefore declared `thread_local`, matching the "one scratch pool per
    /// worker thread" model of `PORTING.md`.
    pub static COVER_POOL: &'static ListPool<CaveNodeCoverEntry> =
        Box::leak(Box::new(ListPool::new(10)));

    /// Java `public static final ListPool<CaveNodeChildEntry> CHILD_POOL` =
    /// `new ListPool<>(10, CaveNodeChildEntry.EMPTY_ARRAY)`.
    ///
    /// Deviation: see [`COVER_POOL`].
    pub static CHILD_POOL: &'static ListPool<CaveNodeChildEntry> =
        Box::leak(Box::new(ListPool::new(10)));
}

/// Java `class CaveNodeType`.
pub struct CaveNodeType {
    /// Java `@Nonnull private final String name`.
    pub name: String,
    /// Java `@Nullable private final CavePrefabContainer prefabContainer`.
    pub prefab_container: Option<Rc<CavePrefabContainer>>,
    /// Java `@Nonnull private final IWeightedMap<BlockFluidEntry> fillings`.
    pub fillings: Rc<dyn IWeightedMap<BlockFluidEntry>>,
    /// Java `@Nonnull private final CaveNodeShapeEnum.CaveNodeShapeGenerator
    /// shapeGenerator`.
    pub shape_generator: Rc<dyn CaveNodeShapeGenerator>,
    /// Java `@Nonnull private final ICoordinateCondition heightCondition`.
    pub height_condition: Rc<dyn ICoordinateCondition>,
    /// Java `@Nullable private final IDoubleRange childrenCountBounds`.
    pub children_count_bounds: Option<Rc<dyn IDoubleRange>>,
    /// Java `@Nonnull private final CaveNodeCoverEntry[] covers`.
    pub covers: Vec<CaveNodeCoverEntry>,
    /// Java `private final int priority`.
    pub priority: i32,
    /// Java `private final int environment`.
    pub environment: i32,
    /// Java `private CaveNodeChildEntry[] children`.
    ///
    /// Deviation: the Java array is filled *after* the node type has been
    /// created (`caveNodeType.setChildren(loadChildren())`), because the
    /// children may reference the node type itself (the node-type graph is
    /// cyclic). The Rust port therefore stores the children behind a
    /// [`RefCell`] so they can be set through the shared handle.
    ///
    /// The cycle (the node type → its children → the node type) keeps the
    /// node types alive until the process ends — the same effective
    /// lifetime as the loaded world.
    pub children: std::cell::RefCell<Vec<CaveNodeChildEntry>>,
}

impl CaveNodeType {
    /// Java `CaveNodeType(String name, CavePrefabContainer prefabContainer,
    /// IWeightedMap<BlockFluidEntry> fillings, CaveNodeShapeGenerator
    /// shapeGenerator, ICoordinateCondition heightCondition, IDoubleRange
    /// childrenCountBounds, CaveNodeCoverEntry[] covers, int priority, int
    /// environment)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        prefab_container: Option<Rc<CavePrefabContainer>>,
        fillings: Rc<dyn IWeightedMap<BlockFluidEntry>>,
        shape_generator: Rc<dyn CaveNodeShapeGenerator>,
        height_condition: Rc<dyn ICoordinateCondition>,
        children_count_bounds: Option<Rc<dyn IDoubleRange>>,
        covers: Vec<CaveNodeCoverEntry>,
        priority: i32,
        environment: i32,
    ) -> Self {
        Self {
            name,
            prefab_container,
            fillings,
            shape_generator,
            height_condition,
            children_count_bounds,
            covers,
            priority,
            environment,
            // Java leaves `children` unset until `setChildren` is called.
            children: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Java `@Nonnull String getName()`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Java `@Nullable CavePrefabContainer getPrefabContainer()`.
    pub fn get_prefab_container(&self) -> Option<&Rc<CavePrefabContainer>> {
        self.prefab_container.as_ref()
    }

    /// Java `void setChildren(CaveNodeChildEntry[] children)`.
    ///
    /// Deviation: the Java method takes a mutable node type; the Rust port
    /// needs to set the children *through* the shared handle (the children
    /// may reference the node type itself), so the method takes `&self` and
    /// writes through the [`RefCell`].
    pub fn set_children(&self, children: Vec<CaveNodeChildEntry>) {
        *self.children.borrow_mut() = children;
    }

    /// Java `@Nonnull ICoordinateCondition getHeightCondition()`.
    pub fn get_height_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.height_condition
    }

    /// Java `@Nullable IDoubleRange getChildrenCountBounds()`.
    pub fn get_children_count_bounds(&self) -> Option<&Rc<dyn IDoubleRange>> {
        self.children_count_bounds.as_ref()
    }

    /// Java `@Nullable BlockFluidEntry getFilling(Random random)` —
    /// `fillings.get(random)`.
    pub fn get_filling(&self, random: &mut FastRandom) -> Option<&BlockFluidEntry> {
        self.fillings.get_random(random)
    }

    /// Java `@Nonnull CaveNodeShape generateCaveNodeShape(Random random,
    /// CaveType caveType, CaveNode parentNode, CaveNodeChildEntry childEntry,
    /// Vector3d origin, float yaw, float pitch)`.
    #[allow(clippy::too_many_arguments)]
    pub fn generate_cave_node_shape(
        &self,
        random: &mut FastRandom,
        cave_type: &crate::cave::cave_type::CaveType,
        parent_node: Option<&crate::cave::element::cave_node::CaveNode>,
        child_entry: Option<&CaveNodeChildEntry>,
        origin: Vector3d,
        yaw: f32,
        pitch: f32,
    ) -> Box<dyn CaveNodeShape> {
        self.shape_generator.generate_cave_node_shape(
            random,
            cave_type,
            parent_node,
            child_entry,
            origin,
            yaw,
            pitch,
        )
    }

    /// Java `@Nonnull CaveNodeCoverEntry[] getCovers()`.
    pub fn get_covers(&self) -> &[CaveNodeCoverEntry] {
        &self.covers
    }

    /// Java `@Nonnull CaveNodeChildEntry[] getChildren()`.
    ///
    /// Deviation: the Java array is `null` until `setChildren` is called; the
    /// Rust port starts with an empty list. The borrow guard is returned so
    /// the caller cannot hold the borrow across a `setChildren` call.
    pub fn get_children(&self) -> std::cell::Ref<'_, [CaveNodeChildEntry]> {
        std::cell::Ref::map(self.children.borrow(), |children| children.as_slice())
    }

    /// Java `int getPriority()`.
    pub fn get_priority(&self) -> i32 {
        self.priority
    }

    /// Java `boolean hasEnvironment()` —
    /// `environment != WorldGenConstants.ENVIRONMENT_NOT_SET`.
    pub fn has_environment(&self) -> bool {
        self.environment != worldgen_model::chunk::ENVIRONMENT_NOT_SET
    }

    /// Java `int getEnvironment()`.
    pub fn get_environment(&self) -> i32 {
        self.environment
    }
}

/// Java `static class CaveNodeType.CaveNodeChildEntry`.
///
/// `Clone` models the Java `System.arraycopy` of
/// `CaveGenerator.getChildEntriesRandomized` (the Java copy is shallow; every
/// field is a reference or a value).
#[derive(Clone)]
pub struct CaveNodeChildEntry {
    /// Java `@Nonnull private final IWeightedMap<CaveNodeType> types`.
    pub types: Rc<dyn IWeightedMap<Rc<CaveNodeType>>>,
    /// Java `@Nonnull private final Vector3d anchor`.
    pub anchor: Vector3d,
    /// Java `@Nonnull private final Vector3d offset`.
    pub offset: Vector3d,
    /// Java `@Nonnull private final PrefabRotation[] rotation`.
    pub rotation: Vec<PrefabRotation>,
    /// Java `@Nullable private final IDoubleRange childrenLimit`.
    pub children_limit: Option<Rc<dyn IDoubleRange>>,
    /// Java `@Nonnull private final IDoubleRange repeat`.
    pub repeat: Rc<dyn IDoubleRange>,
    /// Java `@Nonnull private final OrientationModifier pitchModifier`.
    pub pitch_modifier: OrientationModifier,
    /// Java `@Nonnull private final OrientationModifier yawModifier`.
    pub yaw_modifier: OrientationModifier,
    /// Java `private final double chance`.
    pub chance: f64,
    /// Java `@Nonnull private final CaveYawMode yawMode`.
    pub yaw_mode: CaveYawMode,
}

impl CaveNodeChildEntry {
    /// Java `CaveNodeChildEntry(IWeightedMap<CaveNodeType> types, Vector3d
    /// anchor, Vector3d offset, PrefabRotation[] rotation, IDoubleRange
    /// childrenLimit, IDoubleRange repeat, OrientationModifier pitchModifier,
    /// OrientationModifier yawModifier, double chance, CaveYawMode yawMode)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        types: Rc<dyn IWeightedMap<Rc<CaveNodeType>>>,
        anchor: Vector3d,
        offset: Vector3d,
        rotation: Vec<PrefabRotation>,
        children_limit: Option<Rc<dyn IDoubleRange>>,
        repeat: Rc<dyn IDoubleRange>,
        pitch_modifier: OrientationModifier,
        yaw_modifier: OrientationModifier,
        chance: f64,
        yaw_mode: CaveYawMode,
    ) -> Self {
        Self {
            types,
            anchor,
            offset,
            rotation,
            children_limit,
            repeat,
            pitch_modifier,
            yaw_modifier,
            chance,
            yaw_mode,
        }
    }

    /// Java `@Nonnull IWeightedMap<CaveNodeType> getTypes()`.
    pub fn get_types(&self) -> &Rc<dyn IWeightedMap<Rc<CaveNodeType>>> {
        &self.types
    }

    /// Java `@Nonnull Vector3d getAnchor()`.
    pub fn get_anchor(&self) -> Vector3d {
        self.anchor
    }

    /// Java `@Nonnull Vector3d getOffset()`.
    pub fn get_offset(&self) -> Vector3d {
        self.offset
    }

    /// Java `@Nonnull PrefabRotation getRotation(Random random)` —
    /// `rotation.length == 1 ? rotation[0] : rotation[random.nextInt(rotation.
    /// length)]`.
    ///
    /// Panics when the rotation list is empty — the Java code throws an
    /// `ArrayIndexOutOfBoundsException` in the same case.
    pub fn get_rotation(&self, random: &mut FastRandom) -> PrefabRotation {
        assert!(
            !self.rotation.is_empty(),
            "CaveNodeChildEntry.getRotation: the rotation list is empty (Java: ArrayIndexOutOfBoundsException)"
        );
        if self.rotation.len() == 1 {
            self.rotation[0]
        } else {
            self.rotation[random.next_int_bound(self.rotation.len() as i32) as usize]
        }
    }

    /// Java `@Nullable IDoubleRange getChildrenLimit()`.
    pub fn get_children_limit(&self) -> Option<&Rc<dyn IDoubleRange>> {
        self.children_limit.as_ref()
    }

    /// Java `@Nonnull IDoubleRange getRepeat()`.
    pub fn get_repeat(&self) -> &Rc<dyn IDoubleRange> {
        &self.repeat
    }

    /// Java `@Nonnull OrientationModifier getPitchModifier()`.
    pub fn get_pitch_modifier(&self) -> &OrientationModifier {
        &self.pitch_modifier
    }

    /// Java `@Nonnull OrientationModifier getYawModifier()`.
    pub fn get_yaw_modifier(&self) -> &OrientationModifier {
        &self.yaw_modifier
    }

    /// Java `double getChance()`.
    pub fn get_chance(&self) -> f64 {
        self.chance
    }

    /// Java `@Nonnull CaveYawMode getYawMode()`.
    pub fn get_yaw_mode(&self) -> CaveYawMode {
        self.yaw_mode
    }
}

/// Java `@FunctionalInterface interface
/// CaveNodeType.CaveNodeChildEntry.OrientationModifier` —
/// `float calc(float current, Random random)`.
pub type OrientationModifier = Rc<dyn Fn(f32, &mut FastRandom) -> f32>;

/// Java `static class CaveNodeType.CaveNodeCoverEntry`.
pub struct CaveNodeCoverEntry {
    /// Java `@Nonnull protected final IWeightedMap<Entry> entries`.
    pub entries: Rc<dyn IWeightedMap<CaveNodeCoverEntryEntry>>,
    /// Java `@Nonnull protected final ICoordinateRndCondition heightCondition`.
    pub height_condition: Rc<dyn ICoordinateRndCondition>,
    /// Java `@Nonnull protected final ICoordinateCondition mapCondition`.
    pub map_condition: Rc<dyn ICoordinateCondition>,
    /// Java `@Nonnull protected final ICoordinateCondition densityCondition`.
    pub density_condition: Rc<dyn ICoordinateCondition>,
    /// Java `@Nonnull protected final IBlockFluidCondition parentCondition`.
    pub parent_condition: Rc<dyn IBlockFluidCondition>,
    /// Java `@Nonnull protected final CaveNodeCoverType type`.
    pub cover_type: CaveNodeCoverType,
}

impl CaveNodeCoverEntry {
    /// Java `CaveNodeCoverEntry(IWeightedMap<Entry> entries,
    /// ICoordinateRndCondition heightCondition, ICoordinateCondition
    /// mapCondition, ICoordinateCondition densityCondition, IBlockFluidCondition
    /// parentCondition, CaveNodeCoverType type)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        entries: Rc<dyn IWeightedMap<CaveNodeCoverEntryEntry>>,
        height_condition: Rc<dyn ICoordinateRndCondition>,
        map_condition: Rc<dyn ICoordinateCondition>,
        density_condition: Rc<dyn ICoordinateCondition>,
        parent_condition: Rc<dyn IBlockFluidCondition>,
        cover_type: CaveNodeCoverType,
    ) -> Self {
        Self {
            entries,
            height_condition,
            map_condition,
            density_condition,
            parent_condition,
            cover_type,
        }
    }

    /// Java `@Nullable Entry get(Random random)` — `entries.get(random)`.
    pub fn get(&self, random: &mut FastRandom) -> Option<&CaveNodeCoverEntryEntry> {
        self.entries.get_random(random)
    }

    /// Java `@Nonnull ICoordinateRndCondition getHeightCondition()`.
    pub fn get_height_condition(&self) -> &Rc<dyn ICoordinateRndCondition> {
        &self.height_condition
    }

    /// Java `@Nonnull ICoordinateCondition getMapCondition()`.
    pub fn get_map_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.map_condition
    }

    /// Java `@Nonnull ICoordinateCondition getDensityCondition()`.
    pub fn get_density_condition(&self) -> &Rc<dyn ICoordinateCondition> {
        &self.density_condition
    }

    /// Java `@Nonnull IBlockFluidCondition getParentCondition()`.
    pub fn get_parent_condition(&self) -> &Rc<dyn IBlockFluidCondition> {
        &self.parent_condition
    }

    /// Java `@Nonnull CaveNodeCoverType getType()`.
    pub fn get_type(&self) -> CaveNodeCoverType {
        self.cover_type
    }
}

/// Java `static class CaveNodeType.CaveNodeCoverEntry.Entry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaveNodeCoverEntryEntry {
    /// Java `protected final BlockFluidEntry entry`.
    pub entry: BlockFluidEntry,
    /// Java `protected final int offset`.
    pub offset: i32,
}

impl CaveNodeCoverEntryEntry {
    /// Java `Entry(BlockFluidEntry entry, int offset)`.
    pub const fn new(entry: BlockFluidEntry, offset: i32) -> Self {
        Self { entry, offset }
    }

    /// Java `int getOffset()`.
    pub fn get_offset(&self) -> i32 {
        self.offset
    }

    /// Java `BlockFluidEntry getEntry()`.
    pub fn get_entry(&self) -> BlockFluidEntry {
        self.entry
    }
}

/// Java `enum CaveNodeType.CaveNodeCoverType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaveNodeCoverType {
    /// Java `FLOOR(-1)`.
    Floor,
    /// Java `CEILING(+1)`.
    Ceiling,
}

impl CaveNodeCoverType {
    /// Java `public final int parentOffset`.
    pub const fn parent_offset(self) -> i32 {
        match self {
            CaveNodeCoverType::Floor => -1,
            CaveNodeCoverType::Ceiling => 1,
        }
    }
}

impl Hash for CaveNodeType {
    /// Java has no `hashCode` override, so the Java hash is the *identity* of
    /// the node type.
    ///
    /// Deviation: the Rust port hashes the name, the priority and the
    /// identity of the shape generator — the identity of the node type
    /// itself cannot be expressed for a plain (non-`Rc`) struct.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        state.write_i32(self.priority);
        state.write_usize(shape_generator_identity(&self.shape_generator));
    }
}

impl PartialEq for CaveNodeType {
    /// Java has no `equals` override, so the Java equality is the *identity*
    /// of the node type.
    ///
    /// Deviation: see [`Hash`].
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.priority == other.priority
            && shape_generator_identity(&self.shape_generator)
                == shape_generator_identity(&other.shape_generator)
    }
}

impl Eq for CaveNodeType {}

/// The identity (the address) of the shape generator of a node type.
fn shape_generator_identity(generator: &Rc<dyn CaveNodeShapeGenerator>) -> usize {
    Rc::as_ptr(generator) as *const () as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cave::cave_yaw_mode::CaveYawMode;
    use crate::prefab::prefab_rotation::PrefabRotation;
    use crate::util::block_fluid_entry::BlockFluidEntry;
    use crate::weighted_map::WeightedMap;
    use procedural::noise::condition::{
        ConstantBlockFluidCondition, DefaultCoordinateCondition, DefaultCoordinateRndCondition,
    };
    use procedural::noise::supplier::DoubleRange;

    /// A weighted map with a single entry.
    fn single<T: Clone + Eq + std::hash::Hash + 'static>(value: T) -> Rc<dyn IWeightedMap<T>> {
        let mut builder = WeightedMap::<T>::builder();
        builder.put(value, 1.0);
        Rc::new(builder.build())
    }

    /// A cave node type with the given priority and environment.
    fn node_type(priority: i32, environment: i32) -> CaveNodeType {
        CaveNodeType::new(
            "node".to_string(),
            None,
            single(BlockFluidEntry::EMPTY),
            Rc::new(crate::cave::test_stubs::StubShapeGenerator {
                bounds: crate::util::bounds::WorldBounds::new(),
            }),
            Rc::new(DefaultCoordinateCondition::True),
            None,
            Vec::new(),
            priority,
            environment,
        )
    }

    /// A child entry with the given rotations.
    fn child_entry(rotations: Vec<PrefabRotation>) -> CaveNodeChildEntry {
        CaveNodeChildEntry::new(
            single(Rc::new(node_type(1, 0))),
            Vector3d::new(1.0, 2.0, 3.0),
            Vector3d::new(4.0, 5.0, 6.0),
            rotations,
            None,
            Rc::new(DoubleRange::constant(1.0)),
            Rc::new(|current: f32, _random: &mut FastRandom| current),
            Rc::new(|current: f32, _random: &mut FastRandom| current),
            0.5,
            CaveYawMode::Node,
        )
    }

    #[test]
    fn the_accessors_return_the_stored_values() {
        let node_type = node_type(4, 9);
        assert_eq!(node_type.get_name(), "node");
        assert_eq!(node_type.get_priority(), 4);
        assert_eq!(node_type.get_environment(), 9);
        assert!(node_type.has_environment());
        assert!(node_type.get_prefab_container().is_none());
        assert!(node_type.get_children_count_bounds().is_none());
        assert!(node_type.get_children().is_empty());
        assert!(node_type.get_covers().is_empty());
    }

    #[test]
    fn an_unset_environment_is_reported() {
        let node_type = node_type(0, worldgen_model::chunk::ENVIRONMENT_NOT_SET);
        assert!(!node_type.has_environment());
    }

    #[test]
    fn the_filling_is_picked_from_the_weighted_map() {
        let node_type = node_type(0, 0);
        let mut random = FastRandom::new(7);
        assert_eq!(
            node_type.get_filling(&mut random),
            Some(&BlockFluidEntry::EMPTY)
        );
    }

    #[test]
    fn the_children_can_be_replaced() {
        let node_type = node_type(0, 0);
        assert!(node_type.get_children().is_empty());
        node_type.set_children(vec![child_entry(vec![PrefabRotation::Rotation0])]);
        assert_eq!(node_type.get_children().len(), 1);
    }

    #[test]
    fn a_single_rotation_is_always_picked() {
        let entry = child_entry(vec![PrefabRotation::Rotation90]);
        let mut random = FastRandom::new(3);
        assert_eq!(entry.get_rotation(&mut random), PrefabRotation::Rotation90);
        assert_eq!(entry.get_rotation(&mut random), PrefabRotation::Rotation90);
    }

    #[test]
    fn several_rotations_are_picked_at_random() {
        let entry = child_entry(vec![
            PrefabRotation::Rotation0,
            PrefabRotation::Rotation90,
            PrefabRotation::Rotation180,
        ]);
        let mut random = FastRandom::new(3);
        let picked = entry.get_rotation(&mut random);
        assert!(matches!(
            picked,
            PrefabRotation::Rotation0 | PrefabRotation::Rotation90 | PrefabRotation::Rotation180
        ));
    }

    #[test]
    #[should_panic(expected = "rotation list is empty")]
    fn an_empty_rotation_list_panics_like_the_java_array_index() {
        let entry = child_entry(Vec::new());
        let mut random = FastRandom::new(3);
        let _ = entry.get_rotation(&mut random);
    }

    #[test]
    fn the_child_entry_accessors_return_the_stored_values() {
        let entry = child_entry(vec![PrefabRotation::Rotation0]);
        assert_eq!(entry.get_anchor(), Vector3d::new(1.0, 2.0, 3.0));
        assert_eq!(entry.get_offset(), Vector3d::new(4.0, 5.0, 6.0));
        assert_eq!(entry.get_chance(), 0.5);
        assert_eq!(entry.get_yaw_mode(), CaveYawMode::Node);
        assert!(entry.get_children_limit().is_none());
        assert_eq!(entry.get_repeat().get_value(0.0), 1.0);
    }

    #[test]
    fn the_cover_entry_picks_a_filling() {
        let cover = CaveNodeCoverEntry::new(
            single(CaveNodeCoverEntryEntry::new(BlockFluidEntry::EMPTY, 3)),
            Rc::new(DefaultCoordinateRndCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(ConstantBlockFluidCondition::True),
            CaveNodeCoverType::Floor,
        );
        let mut random = FastRandom::new(11);
        let entry = cover.get(&mut random).expect("the cover has an entry");
        assert_eq!(entry.get_offset(), 3);
        assert_eq!(entry.get_entry(), BlockFluidEntry::EMPTY);
        assert_eq!(cover.get_type(), CaveNodeCoverType::Floor);
        assert_eq!(cover.get_type().parent_offset(), -1);
        assert_eq!(CaveNodeCoverType::Ceiling.parent_offset(), 1);
    }

    #[test]
    fn the_cover_conditions_are_exposed() {
        let cover = CaveNodeCoverEntry::new(
            single(CaveNodeCoverEntryEntry::new(BlockFluidEntry::EMPTY, 0)),
            Rc::new(DefaultCoordinateRndCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(DefaultCoordinateCondition::True),
            Rc::new(ConstantBlockFluidCondition::True),
            CaveNodeCoverType::Ceiling,
        );
        assert!(cover.get_map_condition().eval2(0, 0, 0));
        assert!(cover.get_density_condition().eval3(0, 0, 0, 0));
        assert!(cover.get_parent_condition().eval(1, 0));
    }
}
