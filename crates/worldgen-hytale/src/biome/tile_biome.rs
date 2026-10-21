// Ported from: `.hytale-source/.../worldgen/biome/TileBiome.java`
//! Java `biome.TileBiome` — a grid-pattern biome selected by the weighted
//! tile map.
//!
//! ⚠ Equality note: Java overrides `Biome.hashCode()` to `id` but NOT
//! `equals` (so `equals` stays object identity). Within a single world biome
//! ids are unique, therefore id-based `PartialEq`/`Hash` are semantically
//! equivalent for every real input; required because
//! `WeightedMap<TileBiome>` (Java `IWeightedMap<TileBiome>`) uses a key set.
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use procedural::noise::condition::IHeightThresholdInterpreter;
use procedural::noise::property::NoiseProperty;

use crate::biome::biome::Biome;
use crate::biome::biome_interpolation::BiomeInterpolation;
use crate::container::{
    CoverContainer, EnvironmentContainer, FadeContainer, LayerContainer, PrefabContainer,
    TintContainer, WaterContainer,
};

/// Java `TileBiome extends Biome`.
#[derive(Clone)]
pub struct TileBiome {
    /// Java `protected final int id` (declared in `Biome`).
    id: i32,
    /// Java `protected final String name`.
    name: String,
    /// Java `protected final BiomeInterpolation interpolation`.
    interpolation: Rc<BiomeInterpolation>,
    /// Java `@Nonnull protected final IHeightThresholdInterpreter heightmapInterpreter`.
    heightmap_interpreter: Rc<dyn IHeightThresholdInterpreter>,
    /// Java `protected final CoverContainer coverContainer`.
    cover_container: Rc<CoverContainer>,
    /// Java `protected final LayerContainer layerContainer`.
    layer_container: Rc<LayerContainer>,
    /// Java `@Nullable protected final PrefabContainer prefabContainer`.
    prefab_container: Option<Rc<PrefabContainer>>,
    /// Java `protected final TintContainer tintContainer`.
    tint_container: Rc<TintContainer>,
    /// Java `protected final EnvironmentContainer environmentContainer`.
    environment_container: Rc<EnvironmentContainer>,
    /// Java `protected final WaterContainer waterContainer`.
    water_container: Rc<WaterContainer>,
    /// Java `protected final FadeContainer fadeContainer`.
    fade_container: Rc<FadeContainer>,
    /// Java `protected final NoiseProperty heightmapNoise`.
    heightmap_noise: Rc<dyn NoiseProperty>,
    /// Java `protected final double weight`.
    weight: f64,
    /// Java `protected final double sizeModifier`.
    size_modifier: f64,
    /// Java `protected final int mapColor`.
    map_color: i32,
}

impl TileBiome {
    /// Java `TileBiome.TileBiome(int id, String name, BiomeInterpolation
    /// interpolation, IHeightThresholdInterpreter heightmapInterpreter,
    /// CoverContainer coverContainer, LayerContainer layerContainer,
    /// PrefabContainer prefabContainer, TintContainer tintContainer,
    /// EnvironmentContainer environmentContainer, WaterContainer
    /// waterContainer, FadeContainer fadeContainer, NoiseProperty
    /// heightmapNoise, double weight, double sizeModifier, int mapColor)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
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
        weight: f64,
        size_modifier: f64,
        map_color: i32,
    ) -> Self {
        Self {
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
            weight,
            size_modifier,
            map_color,
        }
    }

    /// Java `TileBiome[] EMPTY_ARRAY` — an empty array of tile biomes.
    pub fn empty_array() -> Vec<Rc<TileBiome>> {
        Vec::new()
    }

    /// Java `double getWeight()`.
    pub fn get_weight(&self) -> f64 {
        self.weight
    }

    /// Java `double getSizeModifier()`.
    pub fn get_size_modifier(&self) -> f64 {
        self.size_modifier
    }
}

impl Biome for TileBiome {
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

impl PartialEq for TileBiome {
    /// Java: `Biome.hashCode()` is overridden to `id` while `equals` stays
    /// object identity. Within a single world biome ids are unique, so
    /// id-based equality is semantically equivalent for every real input
    /// (see the module-level equality note).
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TileBiome {}

impl Hash for TileBiome {
    /// Java `Biome.hashCode()` → `id`.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
