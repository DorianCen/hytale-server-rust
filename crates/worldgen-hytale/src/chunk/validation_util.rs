// Ported from: `.hytale-source/.../worldgen/chunk/ValidationUtil.java`
//! Java `com.hypixel.hytale.server.worldgen.chunk.ValidationUtil` — the
//! *validation* of a loaded world: every prefab (and every prefab child) of
//! the world must be loadable.
//!
//! The Java walks the zone pattern provider and collects the *trace* of the
//! prefab that fails to load (`Zone["X"].UniquePrefabs["Y"].Prefabs[0].{name}`);
//! the trace is only used for the log messages.
//!
//! # Deviations
//!
//! * The Java runs the validation asynchronously on the chunk generator's
//!   executor (`CompletableFuture.supplyAsync(…).join()`); the Rust port
//!   validates synchronously (the result is the same).
//! * The Java catches `Throwable` around the prefab loads; the Rust port
//!   catches the panics of the prefab store with
//!   [`std::panic::catch_unwind`].
//! * The Java logs the *path* of a prefab (`supplier.getName()`); the Rust
//!   port logs the *prefab name* of the child prefabs (the `PrefabSupplier`
//!   trait does not expose the path).

use std::collections::HashSet;
use std::rc::Rc;

use crate::biome::biome::Biome;
use crate::cave::cave_node_type::CaveNodeType;
use crate::cave::prefab::cave_prefab_supplier::CavePrefabKey;
use crate::container::unique_prefab_container::UniquePrefabContainer;
use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::prefab_paste_util::PrefabSupplier;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::util::log_util;
use crate::weighted_map::IWeightedMap;
use crate::zone::zone::Zone;
use crate::zone::zone_pattern_provider::ZonePatternProvider;

/// Java `class ValidationUtil`.
pub struct ValidationUtil;

/// The *trace* of the validation — the Java `Deque<String>` (an `ArrayDeque`
/// used as a stack).
///
/// The Java joins the trace with `String.join(".", trace)`; the `ArrayDeque`
/// iterates from the head to the tail, and the head is the *last* pushed
/// element — so the join is the **reverse** of the push order.
#[derive(Debug, Default)]
struct Trace {
    entries: Vec<String>,
}

impl Trace {
    /// A new (empty) trace.
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Java `trace.push(…)`.
    fn push(&mut self, entry: String) {
        self.entries.push(entry);
    }

    /// Java `trace.pop()`.
    fn pop(&mut self) {
        self.entries.pop();
    }

    /// Java `String.join(".", trace)`.
    fn join(&self) -> String {
        self.entries
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(".")
    }
}

impl ValidationUtil {
    /// Java `public static boolean isInvalid(@Nonnull ZonePatternProvider
    /// zonePatternProvider, @Nonnull Executor executor)`.
    pub fn is_invalid(zone_pattern_provider: &ZonePatternProvider) -> bool {
        let mut trace = Trace::new();
        let mut invalid = false;
        for zone in zone_pattern_provider.get_zones() {
            trace.push(format!("Zone[\"{}\"]", zone.name));
            invalid |= Self::is_zone_invalid(zone, &mut trace);
            trace.pop();
        }
        invalid
    }

    /// Java `private static boolean isZoneInvalid(@Nonnull Zone zone, @Nonnull
    /// Deque<String> trace)`.
    fn is_zone_invalid(zone: &Zone, trace: &mut Trace) -> bool {
        let mut invalid = false;

        for generator in zone.unique_prefab_container.get_generators() {
            trace.push(format!("UniquePrefabs[\"{}\"]", generator.get_name()));
            invalid |= Self::are_prefabs_invalid(generator.get_prefabs().as_ref(), trace);
            trace.pop();
        }

        for biome in zone.biome_pattern_generator.get_biomes() {
            trace.push(format!("Biome[\"{}\"]", biome.get_name()));
            invalid |= Self::is_biome_invalid(biome.as_ref(), trace);
            trace.pop();
        }

        if let Some(cave_generator) = &zone.cave_generator {
            for cave_type in cave_generator.get_cave_types() {
                trace.push(format!("Cave[\"{}\"].Entry", cave_type.get_name()));
                let mut encountered_nodes: HashSet<String> = HashSet::new();
                invalid |= Self::is_cave_node_invalid(
                    cave_type.get_entry_node(),
                    &mut encountered_nodes,
                    trace,
                );
                trace.pop();
            }
        }

        invalid
    }

    /// Java `private static boolean isBiomeInvalid(@Nonnull Biome biome,
    /// @Nonnull Deque<String> trace)`.
    fn is_biome_invalid(biome: &dyn Biome, trace: &mut Trace) -> bool {
        let mut invalid = false;
        if let Some(container) = biome.get_prefab_container() {
            for (index, entry) in container.get_entries().iter().enumerate() {
                trace.push(format!("Prefabs[{index}]"));
                invalid |= Self::are_prefabs_invalid(entry.get_prefabs().as_ref(), trace);
                trace.pop();
            }
        }
        invalid
    }

    /// Java `private static boolean isCaveNodeInvalid(@Nonnull CaveNodeType
    /// caveNodeType, @Nonnull Set<String> encounteredNodes, @Nonnull Deque<String>
    /// trace)`.
    fn is_cave_node_invalid(
        cave_node_type: &Rc<CaveNodeType>,
        encountered_nodes: &mut HashSet<String>,
        trace: &mut Trace,
    ) -> bool {
        // Ensure we only check the node once
        if !encountered_nodes.insert(cave_node_type.get_name().to_string()) {
            return false;
        }

        let mut invalid = false;
        if let Some(container) = cave_node_type.get_prefab_container() {
            for (index, entry) in container.get_entries().iter().enumerate() {
                trace.push(format!("Prefabs[{index}]"));
                invalid |= Self::are_cave_prefabs_invalid(entry.get_prefabs().as_ref(), trace);
                trace.pop();
            }
        }

        // The children are cloned out of the `RefCell` so that the recursive
        // call can borrow the node types again.
        let children = cave_node_type.get_children().to_vec();
        for (index, child) in children.iter().enumerate() {
            let nodes = child.get_types().internal_keys().to_vec();
            for (node_index, node) in nodes.iter().enumerate() {
                trace.push(format!("Children[{index}].Node[{node_index}]"));
                invalid |= Self::is_cave_node_invalid(node, encountered_nodes, trace);
                trace.pop();
            }
        }
        invalid
    }

    /// Java `private static boolean arePrefabsInvalid(@Nonnull
    /// IWeightedMap<WorldGenPrefabSupplier> prefabs, @Nonnull Deque<String>
    /// trace)`.
    fn are_prefabs_invalid(
        prefabs: &dyn IWeightedMap<WorldGenPrefabSupplier>,
        trace: &mut Trace,
    ) -> bool {
        let mut invalid = false;
        for (index, supplier) in prefabs.internal_keys().iter().enumerate() {
            trace.push(format!("Prefabs[{index}]"));
            invalid |= Self::is_supplier_invalid(supplier, trace);
            trace.pop();
        }
        invalid
    }

    /// Java `private static boolean arePrefabsInvalid(@Nonnull
    /// IWeightedMap<WorldGenPrefabSupplier> prefabs, @Nonnull Deque<String>
    /// trace)` — the *cave* prefab variant (the cave prefab suppliers are
    /// reached through the [`crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier`]
    /// seam).
    fn are_cave_prefabs_invalid(
        prefabs: &dyn IWeightedMap<CavePrefabKey>,
        trace: &mut Trace,
    ) -> bool {
        let mut invalid = false;
        for (index, key) in prefabs.internal_keys().iter().enumerate() {
            trace.push(format!("Prefabs[{index}]"));
            let Some(paste) = key.supplier.as_paste_supplier() else {
                trace.pop();
                continue;
            };
            invalid |= Self::is_paste_supplier_invalid(&paste, &paste.get_prefab_name(), trace);
            trace.pop();
        }
        invalid
    }

    /// The body of the Java `arePrefabsInvalid` loop for one supplier.
    fn is_supplier_invalid(supplier: &WorldGenPrefabSupplier, trace: &mut Trace) -> bool {
        let name = supplier.get_name();
        let prefab = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| supplier.get()))
        {
            Ok(prefab) => prefab,
            Err(_) => {
                log_util::error(&format!(
                    "Failed to load prefab: {name} loaded from {}",
                    Self::join_trace(trace)
                ));
                return true;
            }
        };

        trace.push(format!("{{{name}}}"));
        let invalid = Self::are_children_invalid(&prefab, supplier, trace);
        trace.pop();
        invalid
    }

    /// The body of the Java `arePrefabsInvalid` loop for one *cave* prefab
    /// supplier.
    fn is_paste_supplier_invalid(
        supplier: &Rc<dyn PrefabSupplier>,
        name: &str,
        trace: &mut Trace,
    ) -> bool {
        let prefab = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| supplier.get()))
        {
            Ok(Some(prefab)) => prefab,
            Ok(None) => {
                log_util::error(&format!(
                    "Failed to load prefab: {name} loaded from {}",
                    Self::join_trace(trace)
                ));
                return true;
            }
            Err(_) => {
                log_util::error(&format!(
                    "Failed to load prefab: {name} loaded from {}",
                    Self::join_trace(trace)
                ));
                return true;
            }
        };

        trace.push(format!("{{{name}}}"));
        let invalid = Self::are_children_invalid_generic(&prefab, supplier, trace);
        trace.pop();
        invalid
    }

    /// Java `for (var childMarker : prefab.getChildPrefabs()) { … }` — the
    /// worldgen variant (the children are resolved through the supplier's
    /// store).
    fn are_children_invalid(
        prefab: &Rc<PrefabBuffer>,
        supplier: &WorldGenPrefabSupplier,
        trace: &mut Trace,
    ) -> bool {
        let mut invalid = false;
        let access = prefab.new_access();
        for child in access.get_child_prefabs() {
            trace.push(format!("({},{},{})", child.x, child.y, child.z));
            invalid |= Self::is_child_prefab_invalid(&child.path, supplier, trace);
            trace.pop();
        }
        invalid
    }

    /// The child-prefab walk of [`Self::are_children_invalid`] for the
    /// generic paste supplier (the cave prefabs).
    fn are_children_invalid_generic(
        prefab: &Rc<PrefabBuffer>,
        supplier: &Rc<dyn PrefabSupplier>,
        trace: &mut Trace,
    ) -> bool {
        let mut invalid = false;
        let access = prefab.new_access();
        for child in access.get_child_prefabs() {
            trace.push(format!("({},{},{})", child.x, child.y, child.z));
            let children = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                supplier.get_child_suppliers(&child.path)
            })) {
                Ok(children) => children,
                Err(_) => {
                    log_util::error(&format!(
                        "Failed to resolve child prefab: {} loaded from {}",
                        child.path,
                        Self::join_trace(trace)
                    ));
                    trace.pop();
                    invalid = true;
                    continue;
                }
            };
            for child_supplier in children {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| child_supplier.get()))
                    .is_err()
                {
                    log_util::error(&format!(
                        "Failed to load child prefab: {} loaded from {}",
                        child_supplier.get_prefab_name(),
                        Self::join_trace(trace)
                    ));
                    invalid = true;
                }
            }
            trace.pop();
        }
        invalid
    }

    /// Java `private static boolean isChildPrefabInvalid(@Nonnull
    /// PrefabBuffer.ChildPrefab childMarker, @Nonnull WorldGenPrefabLoader
    /// loader, @Nonnull Deque<String> trace)`.
    fn is_child_prefab_invalid(
        path: &str,
        supplier: &WorldGenPrefabSupplier,
        trace: &mut Trace,
    ) -> bool {
        let children = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            supplier.get_child_suppliers(path)
        })) {
            Ok(children) => children,
            Err(_) => {
                log_util::error(&format!(
                    "Failed to resolve child prefab: {path} loaded from {}",
                    Self::join_trace(trace)
                ));
                return true;
            }
        };

        let mut invalid = false;
        for child_supplier in children {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| child_supplier.get()))
                .is_err()
            {
                log_util::error(&format!(
                    "Failed to load child prefab: {} loaded from {}",
                    child_supplier.get_prefab_name(),
                    Self::join_trace(trace)
                ));
                invalid = true;
            }
        }
        invalid
    }

    /// Java `String.join(".", trace)` — the trace is an `ArrayDeque` used as
    /// a stack, so the iteration order is the *reverse* of the push order.
    fn join_trace(trace: &Trace) -> String {
        trace.join()
    }

    /// The unique prefab container of a zone — the Java accessor
    /// `zone.uniquePrefabContainer()`.
    pub fn unique_prefab_container(zone: &Zone) -> &Rc<UniquePrefabContainer> {
        &zone.unique_prefab_container
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use procedural::noise::random::empty_randomizer;
    use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::chunk::mask_provider::MaskProvider;
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::loader::test_prefab_store::test_prefab_loader;
    use crate::prefab::prefab_category::PrefabCategory;
    use crate::prefab::prefab_store::PrefabStore;
    use crate::prefab::prefab_store_root::PrefabStoreRoot;
    use crate::prefab::unique::unique_prefab_configuration::UniquePrefabConfiguration;
    use crate::weighted_map::{IWeightedMap, WeightedMap};
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    /// Java `PointGenerator.nearest2D` with a fixed cell result.
    struct FixedPointGenerator;

    impl procedural::noise::point::IPointGenerator for FixedPointGenerator {
        fn nearest_2d(&self, _seed: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            ResultBuffer2d::new()
        }

        fn nearest_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by these tests")
        }

        fn transition_2d(&self, _s: i32, _x: f64, _y: f64) -> ResultBuffer2d {
            unimplemented!("not exercised by these tests")
        }

        fn transition_3d(&self, _s: i32, _x: f64, _y: f64, _z: f64) -> ResultBuffer3d {
            unimplemented!("not exercised by these tests")
        }

        fn collect(
            &self,
            _seed: i32,
            _min_x: f64,
            _min_y: f64,
            _max_x: f64,
            _max_y: f64,
            _consumer: &mut dyn FnMut(f64, f64),
        ) {
            unimplemented!("not exercised by these tests")
        }

        fn get_interval(&self) -> f64 {
            unimplemented!("not exercised by these tests")
        }
    }

    /// A tile biome with a constant threshold.
    fn tile_biome(id: i32) -> Rc<TileBiome> {
        let interpolation = Rc::new(BiomeInterpolation::create(
            crate::chunk::MAX_RADIUS,
            std::collections::HashMap::new(),
        ));
        let interpreter: Rc<dyn procedural::noise::condition::IHeightThresholdInterpreter> =
            Rc::new(
                crate::chunk::chunk_generator::test_stubs::StubInterpreter::new(0.75, 0.0, 0, 63),
            );

        let mut tint_builder = WeightedMap::<i32>::builder();
        tint_builder.put(0x112233, 1.0);
        let tint: Rc<dyn IWeightedMap<i32>> = Rc::new(tint_builder.build());
        let tint = Rc::new(TintContainer::new(
            DefaultTintContainerEntry::new(
                tint,
                Rc::new(crate::chunk::chunk_generator::test_stubs::ConstantNoise(
                    0.0,
                )),
            ),
            Vec::new(),
        ));

        let mut environment_builder = WeightedMap::<i32>::builder();
        environment_builder.put(5, 1.0);
        let environment: Rc<dyn IWeightedMap<i32>> = Rc::new(environment_builder.build());
        let environment = Rc::new(EnvironmentContainer::new(
            DefaultEnvironmentContainerEntry::new(
                environment,
                Rc::new(crate::chunk::chunk_generator::test_stubs::ConstantNoise(
                    0.0,
                )),
            ),
            Vec::new(),
        ));

        Rc::new(TileBiome::new(
            id,
            format!("biome-{id}"),
            interpolation,
            interpreter,
            Rc::new(CoverContainer::new(Vec::new())),
            Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
            None,
            tint,
            environment,
            Rc::new(WaterContainer::new(Vec::new())),
            Rc::new(FadeContainer::new(
                0.0,
                1.0,
                0.0,
                1.0,
                FadeContainer::NO_FADE_HEIGHTMAP,
            )),
            Rc::new(crate::chunk::chunk_generator::test_stubs::ConstantNoise(
                0.5,
            )),
            1.0,
            1.0,
            0,
        ))
    }

    /// A zone whose unique prefab container holds the given prefab map.
    fn zone_with_prefabs(prefabs: Rc<dyn IWeightedMap<WorldGenPrefabSupplier>>) -> Rc<Zone> {
        let biome = tile_biome(7);
        let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
        builder.put(biome, 1.0);
        let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());

        let configuration = UniquePrefabConfiguration::new(
            None,
            crate::util::condition::BlockMaskCondition::new(),
            None,
            Rc::new(procedural::noise::support::AlwaysTrueIntCondition),
            Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True),
            Rc::new(procedural::noise::condition::ConstantBlockFluidCondition::True),
            crate::loader::util::Vector2d::new(0.0, 0.0),
            crate::loader::util::Vector3d::new(0.0, 0.0, 0.0),
            100.0,
            false,
            false,
            false,
            worldgen_model::chunk::ENVIRONMENT_NOT_SET,
            5000,
            50.0,
            false,
            25.0,
            false,
        );

        let generator = crate::container::unique_prefab_container::UniquePrefabGenerator::new(
            "Spawn".to_string(),
            PrefabCategory::unique(),
            prefabs,
            configuration,
            3,
        );

        Rc::new(Zone {
            id: 3,
            name: "zone-3".to_string(),
            discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
            cave_generator: None,
            unique_prefab_container: Rc::new(UniquePrefabContainer::new(0, vec![generator])),
            biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
                Rc::new(FixedPointGenerator),
                tile_biomes,
                Vec::new(),
            )),
        })
    }

    /// A zone pattern provider over a single zone.
    fn provider(zone: Rc<Zone>) -> ZonePatternProvider {
        let mut mapping = ZoneColorMapping::new();
        mapping.add(0x00FF00, Rc::clone(&zone));
        ZonePatternProvider::new(
            Rc::new(FixedPointGenerator),
            vec![zone],
            Vec::new(),
            MaskProvider::new(FuzzyZoom::new(
                empty_randomizer(),
                PixelProvider::from_pixels(4, 4, vec![0x00FF00; 16]),
                1.0,
                1.0,
                0,
                0,
            )),
            mapping,
        )
    }

    /// A weighted map with a single *loadable* prefab.
    fn loadable_prefab_map() -> Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
        let loader = test_prefab_loader();
        let mut suppliers = loader.get("A");
        assert_eq!(suppliers.len(), 1);
        let mut builder = WeightedMap::<WorldGenPrefabSupplier>::builder();
        builder.put(suppliers.remove(0), 1.0);
        Rc::new(builder.build())
    }

    /// A weighted map with a single *broken* prefab (the file is missing).
    fn broken_prefab_map() -> Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
        let root = std::env::temp_dir().join("worldgen-prefab-store-test/Prefabs");
        let store = Rc::new(PrefabStore::new(
            root.clone(),
            Rc::new(worldgen_model::registry::Registries::empty()),
        ));
        let mut builder = WeightedMap::<WorldGenPrefabSupplier>::builder();
        builder.put(
            WorldGenPrefabSupplier::new(store, "Missing", root.join("Missing.prefab.json")),
            1.0,
        );
        Rc::new(builder.build())
    }

    #[test]
    fn a_world_with_loadable_prefabs_is_valid() {
        let zone = zone_with_prefabs(loadable_prefab_map());
        let provider = provider(zone);
        assert!(
            !ValidationUtil::is_invalid(&provider),
            "every prefab of the world loads"
        );
    }

    #[test]
    fn a_world_with_a_broken_prefab_is_invalid() {
        let zone = zone_with_prefabs(broken_prefab_map());
        let provider = provider(zone);
        assert!(
            ValidationUtil::is_invalid(&provider),
            "the broken prefab is reported"
        );
    }

    #[test]
    fn a_world_without_prefabs_is_valid() {
        let zone = zone_with_prefabs(loadable_prefab_map());
        let provider = provider(zone);
        // A world whose zones carry no prefab at all is valid, too.
        let empty = provider.get_zones()[0]
            .unique_prefab_container
            .get_generators()
            .len();
        assert_eq!(empty, 1);
        assert!(!ValidationUtil::is_invalid(&provider));
    }

    #[test]
    fn the_prefab_store_root_of_the_test_loader_is_the_world_gen_root() {
        // The Java `PrefabStoreRoot.ASSETS` is the world-gen prefab folder.
        assert_eq!(PrefabStoreRoot::WorldGen, PrefabStoreRoot::WorldGen);
    }
}
