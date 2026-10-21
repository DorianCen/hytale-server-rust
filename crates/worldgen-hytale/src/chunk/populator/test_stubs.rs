//! Shared test doubles for the populator tests.
//!
//! The populators need a [`ChunkGeneratorExecution`] whose columns resolve to
//! a *known* biome, so the tests build a tiny world: one zone, one tile
//! biome, and a fixed point generator that never scatters.
//!
//! The prefab stage (Phase 5b) adds the prefab test doubles: the recording
//! point generator (the lattice of the pattern generator), the test prefab
//! files and their suppliers, the prefab buffers, the weighted prefab maps
//! and the prefab pattern generators.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use procedural::noise::point::IPointGenerator;
use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
use procedural::noise::support::NoiseProperty;

use crate::biome::biome_interpolation::BiomeInterpolation;
use crate::biome::biome_pattern_generator::BiomePatternGenerator;
use crate::biome::tile_biome::TileBiome;
use crate::chunk::chunk_generator::test_stubs::{ConstantNoise, StubInterpreter};
use crate::chunk::chunk_generator::ChunkGenerator;
use crate::chunk::chunk_generator_execution::ChunkGeneratorExecution;
use crate::chunk::mask_provider::MaskProvider;
use crate::container::cover_container::CoverContainer;
use crate::container::environment_container::{
    DefaultEnvironmentContainerEntry, EnvironmentContainer,
};
use crate::container::fade_container::FadeContainer;
use crate::container::layer_container::LayerContainer;
use crate::container::prefab_container::PrefabContainer;
use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
use crate::container::water_container::WaterContainer;
use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::buffer::prefab_buffer_block_entry::PrefabBufferBlockEntry;
use crate::prefab::prefab_base_check::PrefabBaseCheck;
use crate::prefab::prefab_pattern_generator::PrefabPatternGenerator;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::prefab::prefab_store::PrefabStore;
use crate::prefab::prefab_supplier::WorldGenPrefabSupplier;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use crate::zone::discovery_config::ZoneDiscoveryConfig;
use crate::zone::zone::Zone;
use crate::zone::zone_color_mapping::ZoneColorMapping;
use crate::zone::zone_pattern_provider::ZonePatternProvider;
use crate::zoom::fuzzy_zoom::FuzzyZoom;
use crate::zoom::pixel_provider::PixelProvider;
use worldgen_model::chunk::GeneratedChunk;

/// A point generator that never scatters — every column resolves to the same
/// tile biome.
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

/// The environment id the test biomes use.
pub(crate) const TEST_ENVIRONMENT: i32 = 5;

/// A tint container with a single colour.
pub(crate) fn tint_container(color: i32) -> Rc<TintContainer> {
    let mut builder = WeightedMap::<i32>::builder();
    builder.put(color, 1.0);
    let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
    Rc::new(TintContainer::new(
        DefaultTintContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
        Vec::new(),
    ))
}

/// An environment container with a single environment.
pub(crate) fn environment_container(environment: i32) -> Rc<EnvironmentContainer> {
    let mut builder = WeightedMap::<i32>::builder();
    builder.put(environment, 1.0);
    let map: Rc<dyn IWeightedMap<i32>> = Rc::new(builder.build());
    Rc::new(EnvironmentContainer::new(
        DefaultEnvironmentContainerEntry::new(map, Rc::new(ConstantNoise(0.0))),
        Vec::new(),
    ))
}

/// The fade container of the test biomes.
pub(crate) fn fade_container() -> Rc<FadeContainer> {
    Rc::new(FadeContainer::new(
        0.0,
        1.0,
        0.0,
        1.0,
        FadeContainer::NO_FADE_HEIGHTMAP,
    ))
}

/// The heightmap noise of the test biomes — a constant `0.5`.
pub(crate) fn heightmap_noise() -> Rc<dyn NoiseProperty> {
    Rc::new(ConstantNoise(0.5))
}

/// A tile biome whose height threshold is a constant `0.75`.
///
/// The threshold is above the height noise (`0.5`), so the whole
/// `[0, highestNonZero]` band is filled by the block populator.
pub(crate) fn tile_biome(
    layer_container: Rc<LayerContainer>,
    cover_container: Rc<CoverContainer>,
    water_container: Rc<WaterContainer>,
) -> Rc<TileBiome> {
    Rc::new(TileBiome::new(
        7,
        "biome-7".to_string(),
        Rc::new(BiomeInterpolation::create(
            crate::chunk::MAX_RADIUS,
            HashMap::new(),
        )),
        Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
        cover_container,
        layer_container,
        None,
        tint_container(0x112233),
        environment_container(TEST_ENVIRONMENT),
        water_container,
        fade_container(),
        heightmap_noise(),
        1.0,
        1.0,
        0,
    ))
}

/// A world whose every column resolves to `biome`.
pub(crate) fn generator_for_biome(biome: Rc<TileBiome>) -> Rc<ChunkGenerator> {
    let mut builder = WeightedMap::<Rc<TileBiome>>::builder();
    builder.put(Rc::clone(&biome), 1.0);
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
    ChunkGenerator::new(provider, None)
}

/* ============================================================================================================ \\
||                                                                                                               ||
||                                        the prefab test doubles                                               ||
||                                                                                                               ||
\\ ============================================================================================================ */

/// A point generator that records the rectangle it was asked to collect and
/// reports a fixed set of lattice points.
///
/// The prefab populator collects its lattice through the *grid generator* of
/// the pattern generator; the stub records the rectangle so that the tests
/// can pin the "the chunk padded by the extents of the entry" rule.
pub(crate) struct RecordingPointGenerator {
    /// The rectangle of the last `collect` call.
    pub(crate) bounds: std::cell::RefCell<Option<(f64, f64, f64, f64)>>,
    /// The points the generator reports.
    pub(crate) points: Vec<(f64, f64)>,
}

impl RecordingPointGenerator {
    /// A generator that reports the given points.
    pub(crate) fn new(points: Vec<(f64, f64)>) -> Self {
        Self {
            bounds: std::cell::RefCell::new(None),
            points,
        }
    }
}

impl IPointGenerator for RecordingPointGenerator {
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
        *self.bounds.borrow_mut() = Some((min_x, min_y, max_x, max_y));
        for (x, z) in &self.points {
            consumer(*x, *z);
        }
    }

    fn get_interval(&self) -> f64 {
        1.0
    }
}

/// The folder of the test prefabs.
pub(crate) fn prefab_folder() -> PathBuf {
    let folder = std::env::temp_dir().join("worldgen-prefab-populator-test");
    std::fs::create_dir_all(&folder).expect("the test prefab folder");
    folder
}

/// Writes a prefab file with the given blocks and returns its path.
pub(crate) fn write_prefab(name: &str, blocks: &[(i32, i32, i32)]) -> PathBuf {
    let path = prefab_folder().join(format!("{name}.prefab.json"));
    let block_values: Vec<serde_json::Value> = blocks
        .iter()
        .map(|(x, y, z)| serde_json::json!({"x": x, "y": y, "z": z, "name": "Rock_Stone"}))
        .collect();
    let document = serde_json::json!({
        "version": 8,
        "blockIdVersion": 11,
        "anchorX": 0,
        "anchorY": 0,
        "anchorZ": 0,
        "blocks": block_values
    });
    std::fs::write(&path, document.to_string()).expect("the test prefab file");
    path
}

/// A prefab supplier over a test prefab file.
pub(crate) fn test_supplier(name: &str, blocks: &[(i32, i32, i32)]) -> WorldGenPrefabSupplier {
    let path = write_prefab(name, blocks);
    let store = Rc::new(PrefabStore::new(
        prefab_folder(),
        Rc::new(worldgen_model::registry::Registries::load()),
    ));
    WorldGenPrefabSupplier::new(store, name, path)
}

/// A prefab buffer with the given blocks (the in-memory form).
pub(crate) fn test_buffer(blocks: &[(i32, i32, i32)]) -> Rc<PrefabBuffer> {
    let mut builder = PrefabBuffer::new_builder();
    builder.set_anchor((0, 0, 0));
    let mut columns: std::collections::BTreeMap<(i32, i32), Vec<PrefabBufferBlockEntry>> =
        std::collections::BTreeMap::new();
    for (x, y, z) in blocks {
        columns
            .entry((*x, *z))
            .or_default()
            .push(PrefabBufferBlockEntry::new(*y, 1, "Rock_Stone"));
    }
    for ((x, z), mut entries) in columns {
        entries.sort_by_key(|entry| entry.y);
        builder.add_column(x, z, entries);
    }
    Rc::new(builder.build())
}

/// A weighted map of the given prefab suppliers.
pub(crate) fn supplier_map(
    suppliers: Vec<WorldGenPrefabSupplier>,
) -> Rc<dyn IWeightedMap<WorldGenPrefabSupplier>> {
    let mut builder = WeightedMap::<WorldGenPrefabSupplier>::builder();
    for supplier in suppliers {
        builder.put(supplier, 1.0);
    }
    Rc::new(builder.build())
}

/// A prefab pattern generator with the given grid generator and rotations.
pub(crate) fn pattern_generator(
    grid: Option<Rc<dyn IPointGenerator>>,
    rotations: Option<Vec<PrefabRotation>>,
    category_priority: i32,
    base_checks: Vec<PrefabBaseCheck>,
) -> Rc<PrefabPatternGenerator> {
    Rc::new(PrefabPatternGenerator::new(
        0,
        crate::prefab::prefab_category::PrefabCategory {
            name: "Test".to_string(),
            priority: category_priority,
        },
        grid,
        Rc::new(procedural::noise::condition::DefaultCoordinateRndCondition::True),
        None,
        crate::util::condition::BlockMaskCondition::new(),
        Rc::new(procedural::noise::condition::DefaultCoordinateCondition::True),
        Rc::new(procedural::noise::condition::ConstantBlockFluidCondition::True),
        rotations,
        Rc::new(crate::util::function::ConstantCoordinateDoubleSupplier::DEFAULT_ZERO),
        false,
        false,
        false,
        false,
        16,
        0,
        base_checks,
    ))
}

/// A tile biome with the given prefab container (the other containers are
/// the default ones of the test world).
pub(crate) fn tile_biome_with_prefabs(
    id: i32,
    container: Option<Rc<PrefabContainer>>,
) -> Rc<TileBiome> {
    Rc::new(TileBiome::new(
        id,
        format!("biome-{id}"),
        Rc::new(BiomeInterpolation::create(
            crate::chunk::MAX_RADIUS,
            HashMap::new(),
        )),
        Rc::new(StubInterpreter::new(0.75, 0.0, 0, 63)),
        Rc::new(CoverContainer::new(Vec::new())),
        Rc::new(LayerContainer::new(0, 0, Vec::new(), Vec::new())),
        container,
        tint_container(0x112233),
        environment_container(TEST_ENVIRONMENT),
        Rc::new(WaterContainer::new(Vec::new())),
        fade_container(),
        heightmap_noise(),
        1.0,
        1.0,
        0,
    ))
}

/// An execution of the chunk (0, 0) over the given generator.
pub(crate) fn execution(generator: Rc<ChunkGenerator>) -> ChunkGeneratorExecution {
    let chunk = GeneratedChunk::with_coordinates(0, 0, 0);
    ChunkGeneratorExecution::new(0, generator, chunk)
}
