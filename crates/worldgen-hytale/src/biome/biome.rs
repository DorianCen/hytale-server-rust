// Ported from: `.hytale-source/.../worldgen/biome/Biome.java`
//! Java `biome.Biome` — the abstract base of [`TileBiome`] and
//! [`CustomBiome`].
//!
//! Java's abstract class becomes an object-safe trait; the shared field set
//! is carried by every concrete struct and exposed through the trait getters
//! (Java's shape, because the containers/interpreters are shared references).

use std::rc::Rc;

use procedural::noise::condition::IHeightThresholdInterpreter;
use procedural::noise::property::NoiseProperty;

use crate::container::{
    CoverContainer, EnvironmentContainer, FadeContainer, LayerContainer, PrefabContainer,
    TintContainer, WaterContainer,
};

use crate::biome::biome_interpolation::BiomeInterpolation;

/// Java `biome.Biome` (abstract) — object-safe trait implemented by
/// [`TileBiome`] and [`CustomBiome`].
pub trait Biome {
    /// Java `int getId()`.
    fn get_id(&self) -> i32;
    /// Java `String getName()`.
    fn get_name(&self) -> &str;
    /// Java `BiomeInterpolation getInterpolation()`.
    fn get_interpolation(&self) -> Rc<BiomeInterpolation>;
    /// Java `@Nonnull IHeightThresholdInterpreter getHeightmapInterpreter()`.
    fn get_heightmap_interpreter(&self) -> Rc<dyn IHeightThresholdInterpreter>;
    /// Java `CoverContainer getCoverContainer()`.
    fn get_cover_container(&self) -> Rc<CoverContainer>;
    /// Java `LayerContainer getLayerContainer()`.
    fn get_layer_container(&self) -> Rc<LayerContainer>;
    /// Java `@Nullable PrefabContainer getPrefabContainer()`.
    fn get_prefab_container(&self) -> Option<Rc<PrefabContainer>>;
    /// Java `TintContainer getTintContainer()`.
    fn get_tint_container(&self) -> Rc<TintContainer>;
    /// Java `EnvironmentContainer getEnvironmentContainer()`.
    fn get_environment_container(&self) -> Rc<EnvironmentContainer>;
    /// Java `WaterContainer getWaterContainer()`.
    fn get_water_container(&self) -> Rc<WaterContainer>;
    /// Java `FadeContainer getFadeContainer()`.
    fn get_fade_container(&self) -> Rc<FadeContainer>;
    /// Java `NoiseProperty getHeightmapNoise()`.
    fn get_heightmap_noise(&self) -> Rc<dyn NoiseProperty>;
    /// Java `int getMapColor()`.
    fn get_map_color(&self) -> i32;
}
