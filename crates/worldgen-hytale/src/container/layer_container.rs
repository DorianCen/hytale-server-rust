// Ported from: `.hytale-source/hytale-shared-source/HytaleServer/WorldGenerator/src/main/java/com/hypixel/hytale/server/worldgen/container/LayerContainer.java`
//! Java `com.hypixel.hytale.server.worldgen.container.LayerContainer` — the
//! static and dynamic block layers of a zone.
//!
//! * A **static layer** contributes a block band whose minimum and maximum
//!   height are given by two double suppliers.
//! * A **dynamic layer** contributes a block whose vertical offset is given
//!   by a double supplier.
//!
//! Both are described by the generic [`Layer`] class; the concrete
//! [`StaticLayer`] / [`DynamicLayer`] classes add the layer-specific state.

use std::ops::Deref;
use std::rc::Rc;

use hytale_math::math_util;
use procedural::noise::condition::ICoordinateCondition;
use procedural::noise::supplier::IDoubleCoordinateSupplier;

use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::list_pool::ListPool;
use crate::util::noise_block_array::NoiseBlockArray;

thread_local! {
    /// Java `public static final ListPool<StaticLayer> STATIC_POOL` =
    /// `new ListPool<>(10, new StaticLayer[0])`.
    ///
    /// Deviation: see [`crate::container::tint_container::ENTRY_POOL`] — the
    /// pooled entries hold `Rc<dyn …>` trait objects (not `Send`), so the
    /// pool is thread-local rather than a process-wide `static`.
    pub static STATIC_POOL: &'static ListPool<StaticLayer> =
        Box::leak(Box::new(ListPool::new(10)));
}

thread_local! {
    /// Java `public static final ListPool<LayerContainer.DynamicLayer>
    /// DYNAMIC_POOL` = `new ListPool<>(10, new DynamicLayer[0])`.
    ///
    /// Deviation: see [`crate::container::tint_container::ENTRY_POOL`].
    pub static DYNAMIC_POOL: &'static ListPool<DynamicLayer> =
        Box::leak(Box::new(ListPool::new(10)));
}

/// Java `class LayerContainer`.
pub struct LayerContainer {
    /// Java `@Nonnull protected final BlockFluidEntry filling`.
    filling: BlockFluidEntry,
    /// Java `protected final int fillingEnvironment`.
    filling_environment: i32,
    /// Java `protected final StaticLayer[] staticLayers`.
    static_layers: Vec<Rc<StaticLayer>>,
    /// Java `protected final DynamicLayer[] dynamicLayers`.
    dynamic_layers: Vec<Rc<DynamicLayer>>,
}

impl LayerContainer {
    /// Java `LayerContainer(int filling, int fillingEnvironment, StaticLayer[]
    /// staticLayers, DynamicLayer[] dynamicLayers)`.
    pub fn new(
        filling: i32,
        filling_environment: i32,
        static_layers: Vec<Rc<StaticLayer>>,
        dynamic_layers: Vec<Rc<DynamicLayer>>,
    ) -> Self {
        // Java: `new BlockFluidEntry(filling, RotationTuple.NONE_INDEX, Fluid.EMPTY_ID)`.
        // CoreServer constants: `RotationTuple.NONE_INDEX` = 0 and
        // `Fluid.EMPTY_ID` = 0.
        Self {
            filling: BlockFluidEntry::new(filling, 0, 0),
            filling_environment,
            static_layers,
            dynamic_layers,
        }
    }

    /// Java `BlockFluidEntry getFilling()`.
    pub const fn get_filling(&self) -> BlockFluidEntry {
        self.filling
    }

    /// Java `int getFillingEnvironment()`.
    pub const fn get_filling_environment(&self) -> i32 {
        self.filling_environment
    }

    /// Java `StaticLayer[] getStaticLayers()`.
    pub fn get_static_layers(&self) -> &[Rc<StaticLayer>] {
        &self.static_layers
    }

    /// Java `DynamicLayer[] getDynamicLayers()`.
    pub fn get_dynamic_layers(&self) -> &[Rc<DynamicLayer>] {
        &self.dynamic_layers
    }

    /// Java `BlockFluidEntry getTopBlockAt(int seed, int x, int z)`.
    pub fn get_top_block_at(&self, seed: i32, x: i32, z: i32) -> BlockFluidEntry {
        for layer in &self.dynamic_layers {
            if let Some(entry) = layer.get_active_entry(seed, x, z) {
                return entry
                    .get_block_array()
                    .get_top_block_at(seed, x as f64, z as f64);
            }
        }
        self.filling
    }
}

/// Java `static class LayerContainer.LayerEntry` (abstract).
///
/// The Java class is the common base of [`StaticLayerEntry`] and
/// [`DynamicLayerEntry`]; in Rust it is modelled as a trait.
pub trait LayerEntry {
    /// Java `boolean isActive(int seed, int x, int z)`.
    fn is_active(&self, seed: i32, x: i32, z: i32) -> bool;

    /// Java `NoiseBlockArray getBlockArray()`.
    fn get_block_array(&self) -> &NoiseBlockArray;
}

/// Java `static class LayerContainer.Layer<T extends LayerEntry>`.
pub struct Layer<T: LayerEntry> {
    /// Java `protected final T[] entries`.
    ///
    /// The Java array holds shared references to the entry objects; the Rust
    /// port uses `Rc` so that cloning a layer never duplicates the
    /// `NoiseBlockArray` bodies.
    pub entries: Vec<Rc<T>>,
    /// Java `protected final ICoordinateCondition mapCondition`.
    pub map_condition: Rc<dyn ICoordinateCondition>,
    /// Java `protected final int environmentId`.
    pub environment_id: i32,
}

impl<T: LayerEntry> Layer<T> {
    /// Java `Layer(T[] entries, ICoordinateCondition mapCondition, int
    /// environmentId)`.
    pub fn new(
        entries: Vec<Rc<T>>,
        map_condition: Rc<dyn ICoordinateCondition>,
        environment_id: i32,
    ) -> Self {
        Self {
            entries,
            map_condition,
            environment_id,
        }
    }

    /// Java `int getEnvironmentId()`.
    pub const fn get_environment_id(&self) -> i32 {
        self.environment_id
    }

    /// Java `T[] getEntries()`.
    ///
    /// Deviation: Java has no getter; the Rust port exposes the entries so
    /// the loaders/tests can inspect them.
    pub fn get_entries(&self) -> &[Rc<T>] {
        &self.entries
    }

    /// Java `T getActiveEntry(int seed, int x, int z)`.
    pub fn get_active_entry(&self, seed: i32, x: i32, z: i32) -> Option<&Rc<T>> {
        // Java: mapCondition.eval(seed, x, z) → the 3-argument (2d) overload.
        if !self.map_condition.eval2(seed, x, z) {
            return None;
        }
        self.entries
            .iter()
            .find(|entry| entry.is_active(seed, x, z))
    }
}

/// Java `static class LayerContainer.StaticLayer extends Layer<StaticLayerEntry>`.
pub struct StaticLayer {
    /// Java `super(entries, mapCondition, environmentId)`.
    layer: Layer<StaticLayerEntry>,
}

impl StaticLayer {
    /// Java `StaticLayer(StaticLayerEntry[] entries, ICoordinateCondition
    /// mapCondition, int environmentId)`.
    pub fn new(
        entries: Vec<Rc<StaticLayerEntry>>,
        map_condition: Rc<dyn ICoordinateCondition>,
        environment_id: i32,
    ) -> Self {
        Self {
            layer: Layer::new(entries, map_condition, environment_id),
        }
    }
}

impl Deref for StaticLayer {
    type Target = Layer<StaticLayerEntry>;

    fn deref(&self) -> &Self::Target {
        &self.layer
    }
}

/// Java `static class LayerContainer.DynamicLayer extends Layer<DynamicLayerEntry>`.
pub struct DynamicLayer {
    /// Java `super(entries, mapCondition, environmentId)`.
    layer: Layer<DynamicLayerEntry>,
    /// Java `protected final IDoubleCoordinateSupplier offset`.
    offset: Rc<dyn IDoubleCoordinateSupplier>,
}

impl DynamicLayer {
    /// Java `DynamicLayer(DynamicLayerEntry[] entries, ICoordinateCondition
    /// mapCondition, int environmentId, IDoubleCoordinateSupplier offset)`.
    pub fn new(
        entries: Vec<Rc<DynamicLayerEntry>>,
        map_condition: Rc<dyn ICoordinateCondition>,
        environment_id: i32,
        offset: Rc<dyn IDoubleCoordinateSupplier>,
    ) -> Self {
        Self {
            layer: Layer::new(entries, map_condition, environment_id),
            offset,
        }
    }

    /// Java `int getOffset(int seed, int x, int z)`.
    pub fn get_offset(&self, seed: i32, x: i32, z: i32) -> i32 {
        math_util::floor(self.offset.get2(seed, x as f64, z as f64))
    }

    /// Java `IDoubleCoordinateSupplier getOffset()`.
    ///
    /// Deviation: Java has no getter for the supplier; the Rust port
    /// exposes it so the loaders/tests can inspect it.
    pub fn get_offset_supplier(&self) -> &Rc<dyn IDoubleCoordinateSupplier> {
        &self.offset
    }
}

impl Deref for DynamicLayer {
    type Target = Layer<DynamicLayerEntry>;

    fn deref(&self) -> &Self::Target {
        &self.layer
    }
}

/// Java `static class LayerContainer.StaticLayerEntry extends LayerEntry`.
pub struct StaticLayerEntry {
    /// Java `LayerEntry.blockArray`.
    pub block_array: Rc<NoiseBlockArray>,
    /// Java `LayerEntry.mapCondition`.
    pub map_condition: Rc<dyn ICoordinateCondition>,
    /// Java `StaticLayerEntry.min`.
    pub min: Rc<dyn IDoubleCoordinateSupplier>,
    /// Java `StaticLayerEntry.max`.
    pub max: Rc<dyn IDoubleCoordinateSupplier>,
}

impl StaticLayerEntry {
    /// Java `StaticLayerEntry(NoiseBlockArray blockArray, ICoordinateCondition
    /// mapCondition, IDoubleCoordinateSupplier min, IDoubleCoordinateSupplier
    /// max)`.
    pub fn new(
        block_array: Rc<NoiseBlockArray>,
        map_condition: Rc<dyn ICoordinateCondition>,
        min: Rc<dyn IDoubleCoordinateSupplier>,
        max: Rc<dyn IDoubleCoordinateSupplier>,
    ) -> Self {
        Self {
            block_array,
            map_condition,
            min,
            max,
        }
    }

    /// Java `int getMinInt(int seed, int x, int z)`.
    pub fn get_min_int(&self, seed: i32, x: i32, z: i32) -> i32 {
        math_util::floor(self.get_min_value(seed, x, z))
    }

    /// Java `double getMinValue(int seed, int x, int z)`.
    pub fn get_min_value(&self, seed: i32, x: i32, z: i32) -> f64 {
        self.min.get2(seed, x as f64, z as f64)
    }

    /// Java `int getMaxInt(int seed, int x, int z)`.
    pub fn get_max_int(&self, seed: i32, x: i32, z: i32) -> i32 {
        math_util::floor(self.get_max_value(seed, x, z))
    }

    /// Java `double getMaxValue(int seed, int x, int z)`.
    pub fn get_max_value(&self, seed: i32, x: i32, z: i32) -> f64 {
        self.max.get2(seed, x as f64, z as f64)
    }
}

impl LayerEntry for StaticLayerEntry {
    fn is_active(&self, seed: i32, x: i32, z: i32) -> bool {
        // Java: mapCondition.eval(seed, x, z) → the 3-argument (2d) overload.
        self.map_condition.eval2(seed, x, z)
    }

    fn get_block_array(&self) -> &NoiseBlockArray {
        &self.block_array
    }
}

/// Java `static class LayerContainer.DynamicLayerEntry extends LayerEntry`.
pub struct DynamicLayerEntry {
    /// Java `LayerEntry.blockArray`.
    pub block_array: Rc<NoiseBlockArray>,
    /// Java `LayerEntry.mapCondition`.
    pub map_condition: Rc<dyn ICoordinateCondition>,
}

impl DynamicLayerEntry {
    /// Java `DynamicLayerEntry(NoiseBlockArray blockArray, ICoordinateCondition
    /// mapCondition)`.
    pub fn new(
        block_array: Rc<NoiseBlockArray>,
        map_condition: Rc<dyn ICoordinateCondition>,
    ) -> Self {
        Self {
            block_array,
            map_condition,
        }
    }
}

impl LayerEntry for DynamicLayerEntry {
    fn is_active(&self, seed: i32, x: i32, z: i32) -> bool {
        // Java: mapCondition.eval(seed, x, z) → the 3-argument (2d) overload.
        self.map_condition.eval2(seed, x, z)
    }

    fn get_block_array(&self) -> &NoiseBlockArray {
        &self.block_array
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::constant_noise_property::default_zero;
    use procedural::noise::condition::DefaultCoordinateCondition;
    use procedural::noise::supplier::DoubleRange;

    /// Test stub: a constant double supplier.
    struct ConstantSupplier(f64);

    impl IDoubleCoordinateSupplier for ConstantSupplier {
        fn get2(&self, _seed: i32, _x: f64, _y: f64) -> f64 {
            self.0
        }
        fn get3(&self, _seed: i32, _x: f64, _y: f64, _z: f64) -> f64 {
            self.0
        }
    }

    /// Test stub: matches when `x` is strictly greater than the threshold.
    struct XGreaterThan(i32);

    impl ICoordinateCondition for XGreaterThan {
        fn eval2(&self, _seed: i32, x: i32, _y: i32) -> bool {
            x > self.0
        }
        fn eval3(&self, _seed: i32, x: i32, _y: i32, _z: i32) -> bool {
            x > self.0
        }
    }

    /// A block array with a single always-present block.
    fn block_array(block_id: i32) -> Rc<NoiseBlockArray> {
        Rc::new(NoiseBlockArray::new(vec![NoiseBlockArrayEntry::new(
            "stone".to_string(),
            BlockFluidEntry::new(block_id, 0, 0),
            Rc::new(DoubleRange::normal(0.0, 4.0)),
            default_zero(),
        )]))
    }

    use crate::util::noise_block_array::Entry as NoiseBlockArrayEntry;

    fn dynamic_entry(
        block_id: i32,
        condition: Rc<dyn ICoordinateCondition>,
    ) -> Rc<DynamicLayerEntry> {
        Rc::new(DynamicLayerEntry::new(block_array(block_id), condition))
    }

    fn static_entry(
        block_id: i32,
        condition: Rc<dyn ICoordinateCondition>,
        min: f64,
        max: f64,
    ) -> Rc<StaticLayerEntry> {
        Rc::new(StaticLayerEntry::new(
            block_array(block_id),
            condition,
            Rc::new(ConstantSupplier(min)),
            Rc::new(ConstantSupplier(max)),
        ))
    }

    #[test]
    fn the_top_block_comes_from_the_first_active_dynamic_layer() {
        let container = LayerContainer::new(
            99,
            3,
            Vec::new(),
            vec![
                Rc::new(DynamicLayer::new(
                    vec![dynamic_entry(1, Rc::new(XGreaterThan(100)))],
                    Rc::new(DefaultCoordinateCondition::True),
                    0,
                    Rc::new(ConstantSupplier(0.0)),
                )),
                Rc::new(DynamicLayer::new(
                    vec![dynamic_entry(2, Rc::new(XGreaterThan(10)))],
                    Rc::new(DefaultCoordinateCondition::True),
                    0,
                    Rc::new(ConstantSupplier(0.0)),
                )),
            ],
        );
        // x = 50 matches the second layer only.
        assert_eq!(container.get_top_block_at(0, 50, 0).block_id, 2);
        // x = 500 matches the first layer.
        assert_eq!(container.get_top_block_at(0, 500, 0).block_id, 1);
    }

    #[test]
    fn the_filling_is_used_when_no_layer_is_active() {
        let container = LayerContainer::new(
            99,
            3,
            Vec::new(),
            vec![Rc::new(DynamicLayer::new(
                vec![dynamic_entry(1, Rc::new(XGreaterThan(100)))],
                Rc::new(DefaultCoordinateCondition::True),
                0,
                Rc::new(ConstantSupplier(0.0)),
            ))],
        );
        assert_eq!(container.get_top_block_at(0, 0, 0), container.get_filling());
        assert_eq!(container.get_filling(), BlockFluidEntry::new(99, 0, 0));
        assert_eq!(container.get_filling_environment(), 3);
    }

    #[test]
    fn the_layer_map_condition_gates_the_active_entry() {
        let layer = DynamicLayer::new(
            vec![dynamic_entry(1, Rc::new(DefaultCoordinateCondition::True))],
            Rc::new(XGreaterThan(10)),
            7,
            Rc::new(ConstantSupplier(0.0)),
        );
        // The map condition fails → no active entry.
        assert!(layer.get_active_entry(0, 5, 0).is_none());
        assert!(layer.get_active_entry(0, 50, 0).is_some());
        assert_eq!(layer.get_environment_id(), 7);
    }

    #[test]
    fn the_static_layer_min_and_max_are_floored() {
        let entry = static_entry(1, Rc::new(DefaultCoordinateCondition::True), 10.9, 20.2);
        assert_eq!(entry.get_min_value(0, 0, 0), 10.9);
        assert_eq!(entry.get_max_value(0, 0, 0), 20.2);
        assert_eq!(entry.get_min_int(0, 0, 0), 10);
        assert_eq!(entry.get_max_int(0, 0, 0), 20);
    }

    #[test]
    fn the_dynamic_layer_offset_is_floored() {
        let layer = DynamicLayer::new(
            Vec::new(),
            Rc::new(DefaultCoordinateCondition::True),
            0,
            Rc::new(ConstantSupplier(-3.7)),
        );
        assert_eq!(layer.get_offset(0, 0, 0), -4);
    }

    #[test]
    fn the_layer_entries_are_shared_references() {
        let entry = dynamic_entry(1, Rc::new(DefaultCoordinateCondition::True));
        let layer = DynamicLayer::new(
            vec![entry.clone()],
            Rc::new(DefaultCoordinateCondition::True),
            0,
            Rc::new(ConstantSupplier(0.0)),
        );
        assert_eq!(layer.get_entries().len(), 1);
        assert!(std::rc::Rc::ptr_eq(&layer.get_entries()[0], &entry));
    }

    #[test]
    fn the_layer_pools_yield_usable_lists() {
        let mut static_resource = STATIC_POOL.with(|pool| pool.acquire());
        static_resource.push(StaticLayer::new(
            Vec::new(),
            Rc::new(DefaultCoordinateCondition::True),
            0,
        ));
        assert_eq!(static_resource.len(), 1);

        let mut dynamic_resource = DYNAMIC_POOL.with(|pool| pool.acquire());
        dynamic_resource.push(DynamicLayer::new(
            Vec::new(),
            Rc::new(DefaultCoordinateCondition::True),
            0,
            Rc::new(ConstantSupplier(0.0)),
        ));
        assert_eq!(dynamic_resource.len(), 1);
    }
}
