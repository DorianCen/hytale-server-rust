//! Shared test doubles for the populator tests.
//!
//! The populators need a [`ChunkGeneratorExecution`] whose columns resolve to
//! a *known* biome, so the tests build a tiny world: one zone, one tile
//! biome, and a fixed point generator that never scatters.

use std::collections::HashMap;
use std::rc::Rc;

use procedural::noise::point::IPointGenerator;
use procedural::noise::result_buffer::{ResultBuffer2d, ResultBuffer3d};
use procedural::noise::support::NoiseProperty;

use crate::biome::biome_interpolation::BiomeInterpolation;
use crate::biome::biome_pattern_generator::BiomePatternGenerator;
use crate::biome::tile_biome::TileBiome;
use crate::chunk::chunk_generator::test_stubs::{ConstantNoise, StubInterpreter};
use crate::chunk::chunk_generator::ChunkGenerator;
use crate::chunk::mask_provider::MaskProvider;
use crate::container::cover_container::CoverContainer;
use crate::container::environment_container::{
    DefaultEnvironmentContainerEntry, EnvironmentContainer,
};
use crate::container::fade_container::FadeContainer;
use crate::container::layer_container::LayerContainer;
use crate::container::tint_container::{DefaultTintContainerEntry, TintContainer};
use crate::container::water_container::WaterContainer;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use crate::zone::discovery_config::ZoneDiscoveryConfig;
use crate::zone::zone::Zone;
use crate::zone::zone_color_mapping::ZoneColorMapping;
use crate::zone::zone_pattern_provider::ZonePatternProvider;
use crate::zoom::fuzzy_zoom::FuzzyZoom;
use crate::zoom::pixel_provider::PixelProvider;

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
