// Ported from: `.hytale-source/.../worldgen/biome/CustomBiome.java`
//! Java `biome.CustomBiome` — a noise-threshold-driven biome that can
//! override the sampled tile biome.
//!
//! Equality note: inherits the same Java semantics as [`TileBiome`] —
//! `Biome.hashCode()` overridden to `id`, identity `equals`; id-based
//! equality is equivalent because world biome ids are unique.
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use procedural::noise::condition::IHeightThresholdInterpreter;
use procedural::noise::support::NoiseProperty;

use crate::biome::biome::Biome;
use crate::biome::biome_interpolation::BiomeInterpolation;
use crate::biome::custom_biome_generator::CustomBiomeGenerator;
use crate::container::{
    CoverContainer, EnvironmentContainer, FadeContainer, LayerContainer, PrefabContainer,
    TintContainer, WaterContainer,
};

/// Java `CustomBiome extends Biome`.
#[derive(Clone)]
pub struct CustomBiome {
    /// Java `protected final CustomBiomeGenerator customBiomeGenerator`.
    custom_biome_generator: Rc<CustomBiomeGenerator>,
    /// Java `Biome` base fields (see [`TileBiome`]).
    id: i32,
    name: String,
    interpolation: Rc<BiomeInterpolation>,
    heightmap_interpreter: Rc<dyn IHeightThresholdInterpreter>,
    cover_container: Rc<CoverContainer>,
    layer_container: Rc<LayerContainer>,
    prefab_container: Option<Rc<PrefabContainer>>,
    tint_container: Rc<TintContainer>,
    environment_container: Rc<EnvironmentContainer>,
    water_container: Rc<WaterContainer>,
    fade_container: Rc<FadeContainer>,
    heightmap_noise: Rc<dyn NoiseProperty>,
    map_color: i32,
}

impl CustomBiome {
    /// Java `CustomBiome(int id, String name, BiomeInterpolation
    /// interpolation, CustomBiomeGenerator customBiomeGenerator,
    /// IHeightThresholdInterpreter heightmapInterpreter, CoverContainer
    /// coverContainer, LayerContainer layerContainer, PrefabContainer
    /// prefabContainer, TintContainer tintContainer, EnvironmentContainer
    /// environmentContainer, WaterContainer waterContainer, FadeContainer
    /// fadeContainer, NoiseProperty heightmapNoise, int mapColor)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: i32,
        name: String,
        interpolation: Rc<BiomeInterpolation>,
        custom_biome_generator: Rc<CustomBiomeGenerator>,
        heightmap_interpreter: Rc<dyn IHeightThresholdInterpreter>,
        cover_container: Rc<CoverContainer>,
        layer_container: Rc<LayerContainer>,
        prefab_container: Option<Rc<PrefabContainer>>,
        tint_container: Rc<TintContainer>,
        environment_container: Rc<EnvironmentContainer>,
        water_container: Rc<WaterContainer>,
        fade_container: Rc<FadeContainer>,
        heightmap_noise: Rc<dyn NoiseProperty>,
        map_color: i32,
    ) -> Self {
        Self {
            custom_biome_generator,
            id,
            name,
            interpolation,
            heightmap_interpreter,
            cover_container,
            layer_container,
            prefab_container,
            tint_container,
            environment_container,
            water_container,
            fade_container,
            heightmap_noise,
            map_color,
        }
    }

    /// Java `CustomBiomeGenerator getCustomBiomeGenerator()`.
    pub fn get_custom_biome_generator(&self) -> Rc<CustomBiomeGenerator> {
        Rc::clone(&self.custom_biome_generator)
    }
}

impl Biome for CustomBiome {
    fn get_id(&self) -> i32 {
        self.id
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_interpolation(&self) -> Rc<BiomeInterpolation> {
        Rc::clone(&self.interpolation)
    }

    fn get_heightmap_interpreter(&self) -> Rc<dyn IHeightThresholdInterpreter> {
        Rc::clone(&self.heightmap_interpreter)
    }

    fn get_cover_container(&self) -> Rc<CoverContainer> {
        Rc::clone(&self.cover_container)
    }

    fn get_layer_container(&self) -> Rc<LayerContainer> {
        Rc::clone(&self.layer_container)
    }

    fn get_prefab_container(&self) -> Option<Rc<PrefabContainer>> {
        self.prefab_container.clone()
    }

    fn get_tint_container(&self) -> Rc<TintContainer> {
        Rc::clone(&self.tint_container)
    }

    fn get_environment_container(&self) -> Rc<EnvironmentContainer> {
        Rc::clone(&self.environment_container)
    }

    fn get_water_container(&self) -> Rc<WaterContainer> {
        Rc::clone(&self.water_container)
    }

    fn get_fade_container(&self) -> Rc<FadeContainer> {
        Rc::clone(&self.fade_container)
    }

    fn get_heightmap_noise(&self) -> Rc<dyn NoiseProperty> {
        Rc::clone(&self.heightmap_noise)
    }

    fn get_map_color(&self) -> i32 {
        self.map_color
    }
}

impl PartialEq for CustomBiome {
    /// Java: `Biome.hashCode()` → `id`, identity `equals` (see `TileBiome`).
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for CustomBiome {}

impl Hash for CustomBiome {
    /// Java `Biome.hashCode()` → `id`.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
