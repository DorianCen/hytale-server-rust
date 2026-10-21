//! Shared test doubles for the cave package tests.

use std::rc::Rc;

use hytale_math::fast_random::FastRandom;
use procedural::noise::point::IPointGenerator;
use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};

use crate::cave::cave::Cave;
use crate::cave::cave_node_type::{CaveNodeChildEntry, CaveNodeType};
use crate::cave::cave_type::{CaveType, FluidLevel};
use crate::cave::element::cave_node::CaveNode;
use crate::cave::shape::cave_node_shape::CaveNodeShape;
use crate::cave::shape::cave_node_shape_enum::CaveNodeShapeGenerator;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::loader::util::Vector3d;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::util::block_fluid_entry::BlockFluidEntry;
use crate::util::bounds::WorldBounds;
use crate::util::condition::flag::ConstantInt2Flags;
use crate::util::condition::BlockMaskCondition;
use crate::weighted_map::{IWeightedMap, WeightedMap};

/// A point generator that always reports the origin.
pub(crate) struct FixedPointGenerator;

impl IPointGenerator for FixedPointGenerator {
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
        min_x: f64,
        min_y: f64,
        max_x: f64,
        max_y: f64,
        consumer: &mut dyn FnMut(f64, f64),
    ) {
        // The fixed point of the generator is the origin; report it when the
        // query rectangle contains it (the cave populator queries a padded
        // chunk rectangle around the origin).
        if min_x <= 0.0 && 0.0 <= max_x && min_y <= 0.0 && 0.0 <= max_y {
            consumer(0.0, 0.0);
        }
    }

    fn get_interval(&self) -> f64 {
        unimplemented!("not exercised by these tests")
    }
}

/// A cave node shape with a fixed bounding box and fixed floor/ceiling.
pub(crate) struct StubShape {
    pub(crate) bounds: WorldBounds,
    pub(crate) floor: f64,
    pub(crate) ceiling: f64,
}

impl StubShape {
    /// A shape whose bounds are the given block range and whose floor and
    /// ceiling are `floor` / `ceiling`.
    pub(crate) fn new(bounds: WorldBounds, floor: f64, ceiling: f64) -> Self {
        Self {
            bounds,
            floor,
            ceiling,
        }
    }
}

impl CaveNodeShape for StubShape {
    fn get_start(&self) -> Vector3d {
        Vector3d::ZERO
    }

    fn get_end(&self) -> Vector3d {
        Vector3d::ZERO
    }

    fn get_anchor(&self, vector: Vector3d, _tx: f64, _ty: f64, _tz: f64) -> Vector3d {
        vector
    }

    fn get_bounds(&self) -> &WorldBounds {
        &self.bounds
    }

    fn should_replace(&self, _seed: i32, _x: f64, _z: f64, _y: i32) -> bool {
        false
    }

    fn get_floor_position(&self, _seed: i32, _x: f64, _z: f64) -> f64 {
        self.floor
    }

    fn get_ceiling_position(&self, _seed: i32, _x: f64, _z: f64) -> f64 {
        self.ceiling
    }

    fn populate_chunk(
        &self,
        _seed: i32,
        _execution: &mut ChunkGeneratorExecution,
        _cave: &Cave,
        _node: &CaveNode,
        _random: &mut FastRandom,
    ) {
    }
}

/// A shape generator that always produces a [`StubShape`].
pub(crate) struct StubShapeGenerator {
    pub(crate) bounds: WorldBounds,
}

impl CaveNodeShapeGenerator for StubShapeGenerator {
    fn generate_cave_node_shape(
        &self,
        _random: &mut FastRandom,
        _cave_type: &CaveType,
        _parent_node: Option<&CaveNode>,
        _child_entry: Option<&CaveNodeChildEntry>,
        _origin: Vector3d,
        _yaw: f32,
        _pitch: f32,
    ) -> Box<dyn CaveNodeShape> {
        Box::new(StubShape::new(self.bounds, 0.0, 0.0))
    }
}

/// A cave node type with a fixed priority and no children.
pub(crate) fn cave_node_type(priority: i32) -> Rc<CaveNodeType> {
    cave_node_type_with_filling(priority, BlockFluidEntry::EMPTY)
}

/// A cave node type whose filling is the given block.
pub(crate) fn cave_node_type_with_filling(
    priority: i32,
    filling: BlockFluidEntry,
) -> Rc<CaveNodeType> {
    let mut builder = WeightedMap::<BlockFluidEntry>::builder();
    builder.put(filling, 1.0);
    let fillings = Rc::new(builder.build());
    Rc::new(CaveNodeType::new(
        "node".to_string(),
        None,
        fillings,
        Rc::new(StubShapeGenerator {
            bounds: WorldBounds::new(),
        }),
        Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True),
        None,
        Vec::new(),
        priority,
        worldgen_model::chunk::ENVIRONMENT_NOT_SET,
    ))
}

/// A child entry with the given offset and a single rotation.
///
/// The shape generators need a child entry (the Java code dereferences the
/// `@Nonnull` parameter), so the tests hand them one.
pub(crate) fn child_entry(offset: Vector3d) -> CaveNodeChildEntry {
    let mut builder = WeightedMap::<Rc<CaveNodeType>>::builder();
    builder.put(cave_node_type(0), 1.0);
    let types: Rc<dyn IWeightedMap<Rc<CaveNodeType>>> = Rc::new(builder.build());
    CaveNodeChildEntry::new(
        types,
        Vector3d::ZERO,
        offset,
        vec![PrefabRotation::Rotation0],
        None,
        Rc::new(procedural::noise::supplier::DoubleRange::constant(1.0)),
        Rc::new(|current: f32, _random: &mut FastRandom| current),
        Rc::new(|current: f32, _random: &mut FastRandom| current),
        1.0,
        crate::cave::cave_yaw_mode::CaveYawMode::Node,
    )
}

/// A cave type with constant ranges and the default masks.
pub(crate) fn cave_type(name: &str) -> Rc<CaveType> {
    cave_type_with_height_factor(name, 0.5)
}

/// A cave type with constant ranges, the default masks and a constant height
/// radius factor.
///
/// The Java `CaveType.getHeightRadiusFactor` scales the radius of a cave
/// node; the stub interpreter answers with a constant.
pub(crate) fn cave_type_with_height_factor(name: &str, factor: f32) -> Rc<CaveType> {
    Rc::new(CaveType::new(
        name.to_string(),
        cave_node_type(0),
        Rc::new(crate::supplier::FloatRange::constant(0.0)),
        Rc::new(crate::supplier::FloatRange::constant(0.0)),
        Rc::new(procedural::noise::supplier::DoubleRange::constant(1.0)),
        Rc::new(
            crate::chunk::chunk_generator::test_stubs::StubInterpreter::new(factor, 0.0, 0, 63),
        ),
        Rc::new(FixedPointGenerator),
        Rc::new(ConstantInt2Flags::new(7)),
        BlockMaskCondition::new(),
        Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True),
        Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True),
        None,
        None,
        FluidLevel::EMPTY,
        0,
        false,
        false,
        64.0,
    ))
}

/// A prefab supplier with fixed extents.
pub(crate) struct StubPrefabSupplier {
    pub(crate) min_x: i32,
    pub(crate) min_y: i32,
    pub(crate) min_z: i32,
    pub(crate) max_x: i32,
    pub(crate) max_y: i32,
    pub(crate) max_z: i32,
}

impl crate::cave::prefab::cave_prefab_supplier::CavePrefabSupplier for StubPrefabSupplier {
    fn get_min_x(&self, _rotation: crate::prefab::prefab_rotation::PrefabRotation) -> i32 {
        self.min_x
    }
    fn get_min_y(&self) -> i32 {
        self.min_y
    }
    fn get_min_z(&self, _rotation: crate::prefab::prefab_rotation::PrefabRotation) -> i32 {
        self.min_z
    }
    fn get_max_x(&self, _rotation: crate::prefab::prefab_rotation::PrefabRotation) -> i32 {
        self.max_x
    }
    fn get_max_y(&self) -> i32 {
        self.max_y
    }
    fn get_max_z(&self, _rotation: crate::prefab::prefab_rotation::PrefabRotation) -> i32 {
        self.max_z
    }
    fn get_buffer(&self) -> Option<Rc<crate::prefab::buffer::prefab_buffer::PrefabBuffer>> {
        None
    }
    fn as_paste_supplier(
        &self,
    ) -> Option<Rc<dyn crate::prefab::prefab_paste_util::PrefabSupplier>> {
        None
    }
    fn get_bounds(&self) -> crate::util::bounds::ChunkBounds {
        crate::util::bounds::ChunkBounds::with_bounds(
            self.min_x, self.min_z, self.max_x, self.max_z,
        )
    }
}

impl PartialEq for StubPrefabSupplier {
    /// The Java supplier compares by path — the stub compares by value.
    fn eq(&self, other: &Self) -> bool {
        self.min_x == other.min_x
            && self.min_y == other.min_y
            && self.min_z == other.min_z
            && self.max_x == other.max_x
            && self.max_y == other.max_y
            && self.max_z == other.max_z
    }
}

impl Eq for StubPrefabSupplier {}

impl std::hash::Hash for StubPrefabSupplier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.min_x.hash(state);
        self.min_y.hash(state);
        self.min_z.hash(state);
        self.max_x.hash(state);
        self.max_y.hash(state);
        self.max_z.hash(state);
    }
}

/// A cave generator that hands out an empty cave.
pub(crate) struct StubCaveGenerator {
    /// The cave types of the generator.
    pub(crate) cave_types: Vec<Rc<CaveType>>,
}

impl StubCaveGenerator {
    /// A generator with a single cave type.
    pub(crate) fn new(cave_types: Vec<Rc<CaveType>>) -> Self {
        Self { cave_types }
    }
}

impl crate::cave::cave_generator::CaveGenerator for StubCaveGenerator {
    fn get_cave_types(&self) -> &[Rc<CaveType>] {
        &self.cave_types
    }

    fn generate(
        &self,
        _seed: i32,
        _chunk_generator: &crate::chunk::chunk_generator::ChunkGenerator,
        cave_type: &CaveType,
        _x: i32,
        _y: i32,
        _z: i32,
    ) -> Cave {
        let mut cave = Cave::new(Rc::new(cave_type.clone()));
        cave.compile();
        cave
    }
}

/// A chunk generator with a single zone whose every column resolves to the
/// same biome.
pub(crate) fn chunk_generator() -> Rc<crate::chunk::chunk_generator::ChunkGenerator> {
    use crate::biome::biome_interpolation::BiomeInterpolation;
    use crate::biome::biome_pattern_generator::BiomePatternGenerator;
    use crate::biome::tile_biome::TileBiome;
    use crate::chunk::chunk_generator::test_stubs::ConstantNoise;
    use crate::chunk::chunk_generator::test_stubs::StubInterpreter;
    use crate::chunk::mask_provider::MaskProvider;
    use crate::container::cover_container::CoverContainer;
    use crate::container::environment_container::{
        DefaultEnvironmentContainerEntry, EnvironmentContainer,
    };
    use crate::container::fade_container::FadeContainer;
    use crate::container::layer_container::LayerContainer;
    use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
    use crate::container::water_container::WaterContainer;
    use crate::zone::discovery_config::ZoneDiscoveryConfig;
    use crate::zone::zone::Zone;
    use crate::zone::zone_color_mapping::ZoneColorMapping;
    use crate::zone::zone_pattern_provider::ZonePatternProvider;
    use crate::zoom::fuzzy_zoom::FuzzyZoom;
    use crate::zoom::pixel_provider::PixelProvider;

    let mut tint_builder = WeightedMap::<i32>::builder();
    tint_builder.put(0x112233, 1.0);
    let tint_map: Rc<dyn IWeightedMap<i32>> = Rc::new(tint_builder.build());
    let tint_container = Rc::new(TintContainer::new(
        DefaultTintContainerEntry::new(tint_map, Rc::new(ConstantNoise(0.0))),
        Vec::new(),
    ));

    let mut environment_builder = WeightedMap::<i32>::builder();
    environment_builder.put(5, 1.0);
    let environment_map: Rc<dyn IWeightedMap<i32>> = Rc::new(environment_builder.build());
    let environment_container = Rc::new(EnvironmentContainer::new(
        DefaultEnvironmentContainerEntry::new(environment_map, Rc::new(ConstantNoise(0.0))),
        Vec::new(),
    ));

    let fade_container = Rc::new(FadeContainer::new(
        0.0,
        1.0,
        0.0,
        1.0,
        FadeContainer::NO_FADE_HEIGHTMAP,
    ));

    let biome = Rc::new(TileBiome::new(
        7,
        "biome-7".to_string(),
        Rc::new(BiomeInterpolation::create(
            crate::chunk::MAX_RADIUS,
            std::collections::HashMap::new(),
        )),
        Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
        Rc::new(CoverContainer::new(Vec::new())),
        Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
        None,
        tint_container,
        environment_container,
        Rc::new(WaterContainer::new(Vec::new())),
        fade_container,
        Rc::new(ConstantNoise(0.5)),
        1.0,
        1.0,
        0,
    ));

    let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
    builder.put(biome, 1.0);
    let tile_biomes: Rc<dyn IWeightedMap<Rc<TileBiome>>> = Rc::new(builder.build());

    let zone = Rc::new(Zone {
        id: 3,
        name: "zone-3".to_string(),
        discovery_config: Rc::new(ZoneDiscoveryConfig::default()),
        cave_generator: None,
        unique_prefab_container: Rc::new(
            crate::container::unique_prefab_container::UniquePrefabContainer::new(0, Vec::new()),
        ),
        biome_pattern_generator: Rc::new(BiomePatternGenerator::new(
            Rc::new(FixedPointGenerator),
            tile_biomes,
            Vec::new(),
        )),
    });

    let mut mapping = ZoneColorMapping::new();
    mapping.add(0x00FF00, Rc::clone(&zone));
    let provider = ZonePatternProvider::new(
        Rc::new(FixedPointGenerator),
        vec![zone],
        Vec::new(),
        MaskProvider::new(FuzzyZoom::new(
            procedural::noise::random::empty_randomizer(),
            PixelProvider::from_pixels(4, 4, vec![0x00FF00; 16]),
            1.0,
            1.0,
            0,
            0,
        )),
        mapping,
    );
    crate::chunk::chunk_generator::ChunkGenerator::new(provider, None)
}
