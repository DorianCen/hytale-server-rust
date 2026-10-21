// Ported from: `.hytale-source/.../worldgen/loader/biome/BiomeJsonLoader.java`
//! Java `loader.biome.BiomeJsonLoader` — the abstract base shared by
//! [`super::tile_biome_json_loader::TileBiomeJsonLoader`] and
//! [`super::custom_biome_json_loader::CustomBiomeJsonLoader`].
//!
//! The seven biome containers (covers, layers, prefabs, tint, environment,
//! water, fade) are loaded through the ported `loader.container` classes,
//! which resolve block/fluid/environment names through the registries
//! threaded down from `ChunkGeneratorJsonLoader`. The primitive fields
//! (height-threshold interpreter, heightmap noise, interpolation, map color)
//! are ported faithfully too.

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::biome::biome_interpolation::BiomeInterpolation;
use crate::container::cover_container::CoverContainer;
use crate::container::environment_container::EnvironmentContainer;
use crate::container::fade_container::FadeContainer;
use crate::container::layer_container::LayerContainer;
use crate::container::prefab_container::PrefabContainer;
use crate::container::tint_container::TintContainer;
use crate::container::water_container::WaterContainer;
use crate::loader::biome::biome_interpolation_json_loader::BiomeInterpolationJsonLoader;
use crate::loader::color_util::ColorUtil;
use crate::loader::container::cover_container_json_loader::CoverContainerJsonLoader;
use crate::loader::container::environment_container_json_loader::EnvironmentContainerJsonLoader;
use crate::loader::container::fade_container_json_loader::FadeContainerJsonLoader;
use crate::loader::container::layer_container_json_loader::LayerContainerJsonLoader;
use crate::loader::container::prefab_container_json_loader::PrefabContainerJsonLoader;
use crate::loader::container::tint_container_json_loader::TintContainerJsonLoader;
use crate::loader::container::water_container_json_loader::WaterContainerJsonLoader;
use crate::loader::context::biome_file_context::BiomeFileContext;
use crate::util::constant_noise_property::default_zero;
use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::noise_property_json_loader::NoisePropertyJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::IHeightThresholdInterpreter;
use procedural::noise::property::NoiseProperty;

/// Java `BiomeJsonLoader.Constants`.
pub mod constants {
    pub const KEY_TERRAIN_HEIGHT_THRESHOLD: &str = "TerrainHeightThreshold";
    pub const KEY_COVERS: &str = "Covers";
    pub const KEY_LAYERS: &str = "Layers";
    pub const KEY_PREFABS: &str = "Prefabs";
    pub const KEY_FADE: &str = "Fade";
    pub const KEY_TINT: &str = "Tint";
    pub const KEY_ENVIRONMENT: &str = "Environment";
    pub const KEY_WATER: &str = "Water";
    pub const KEY_HEIGHTMAP_NOISE: &str = "HeightmapNoise";
    pub const KEY_INTERPOLATION: &str = "Interpolation";
    pub const KEY_MAP_COLOR: &str = "MapColor";

    pub const ERROR_NO_LAYER_CONTAINER: &str = "LayerContainer is not defined in Biome!";
    pub const ERROR_COVER_CONTAINER: &str = "Failed to load cover container";
    pub const ERROR_HEIGHT_CONTAINER: &str = "Failed to load height threshold container";
    pub const ERROR_LAYER_CONTAINER: &str = "Failed to load layer container";
    pub const ERROR_WATER_CONTAINER: &str = "Failed to load water container";
    pub const ERROR_TINT_CONTAINER: &str = "Failed to load tint container";
    pub const ERROR_FADE_CONTAINER: &str = "Failed to load fade container";
    pub const ERROR_ENVIRONMENT_CONTAINER: &str = "Failed to load environment container";
    pub const ERROR_PREFAB_CONTAINER: &str = "Failed to load prefab container";

    /// Java `TintContainerJsonLoader.Constants.DEFAULT_TINT_COLOR`.
    pub const DEFAULT_TINT_COLOR: i32 = 0xFF0000;
}

/// Java `loader.biome.BiomeJsonLoader`.
pub struct BiomeJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, Biome>`.
    pub base: JsonLoaderBase,
    /// Java `protected final BiomeFileContext biomeContext`.
    pub biome_context: Rc<BiomeFileContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl BiomeJsonLoader {
    /// Java `BiomeJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, BiomeFileContext biomeContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        biome_context: Rc<BiomeFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            biome_context,
            registries,
        }
    }

    /// Java `IHeightThresholdInterpreter loadTerrainHeightThreshold()`.
    pub fn load_terrain_height_threshold(&self) -> Rc<dyn IHeightThresholdInterpreter> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            HeightThresholdInterpreterJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_TERRAIN_HEIGHT_THRESHOLD),
                #[allow(deprecated)] // deprecated in hytale-math but this is its exact Java value
                hytale_math::chunk_util::HEIGHT,
            )
            .load()
        }));
        match result {
            Ok(interpreter) => interpreter,
            Err(_) => panic!("{}", constants::ERROR_HEIGHT_CONTAINER),
        }
    }

    /// Java `CoverContainer loadCoverContainer()`.
    pub fn load_cover_container(&self) -> Rc<CoverContainer> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            CoverContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_COVERS),
                Rc::clone(&self.biome_context),
                Rc::clone(&self.registries),
            )
            .load()
        }));
        match result {
            Ok(container) => Rc::new(container),
            Err(_) => panic!("{}", constants::ERROR_COVER_CONTAINER),
        }
    }

    /// Java `FadeContainer loadFadeContainer()`.
    pub fn load_fade_container(&self) -> Rc<FadeContainer> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            FadeContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_FADE),
            )
            .load()
        }));
        match result {
            Ok(container) => Rc::new(container),
            Err(_) => panic!("{}", constants::ERROR_FADE_CONTAINER),
        }
    }

    /// Java `LayerContainer loadLayerContainers()`.
    pub fn load_layer_containers(&self) -> Rc<LayerContainer> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if !self.base.has(constants::KEY_LAYERS) {
                panic!("{}", constants::ERROR_NO_LAYER_CONTAINER);
            }
            LayerContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_LAYERS),
                Rc::clone(&self.biome_context),
                Rc::clone(&self.registries),
            )
            .load()
        }));
        match result {
            Ok(container) => Rc::new(container),
            Err(_) => panic!("{}", constants::ERROR_LAYER_CONTAINER),
        }
    }

    /// Java `@Nullable PrefabContainer loadPrefabContainer()`.
    pub fn load_prefab_container(&self) -> Option<Rc<PrefabContainer>> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if self.base.has(constants::KEY_PREFABS) {
                return Some(Rc::new(
                    PrefabContainerJsonLoader::new(
                        self.base.seed.clone(),
                        &self.base.data_folder,
                        self.base.get(constants::KEY_PREFABS),
                        Rc::clone(&self.biome_context),
                        Rc::clone(&self.registries),
                    )
                    .load(),
                ));
            }
            None
        }));
        match result {
            Ok(container) => container,
            Err(_) => panic!("{}", constants::ERROR_PREFAB_CONTAINER),
        }
    }

    /// Java `TintContainer loadTintContainer()`.
    pub fn load_tint_container(&self) -> Rc<TintContainer> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            TintContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_TINT),
                Rc::clone(&self.biome_context),
            )
            .load()
        }));
        match result {
            Ok(container) => Rc::new(container),
            Err(_) => panic!("{}", constants::ERROR_TINT_CONTAINER),
        }
    }

    /// Java `EnvironmentContainer loadEnvironmentContainer()`.
    pub fn load_environment_container(&self) -> Rc<EnvironmentContainer> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            EnvironmentContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ENVIRONMENT),
                Rc::clone(&self.biome_context),
                Rc::clone(&self.registries),
            )
            .load()
        }));
        match result {
            Ok(container) => Rc::new(container),
            Err(_) => panic!("{}", constants::ERROR_ENVIRONMENT_CONTAINER),
        }
    }

    /// Java `WaterContainer loadWaterContainer()`.
    pub fn load_water_container(&self) -> Rc<WaterContainer> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            WaterContainerJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_WATER),
                Rc::clone(&self.biome_context),
                Rc::clone(&self.registries),
            )
            .load()
        }));
        match result {
            Ok(container) => Rc::new(container),
            Err(_) => panic!("{}", constants::ERROR_WATER_CONTAINER),
        }
    }

    /// Java `@Nullable NoiseProperty loadHeightmapNoise()`.
    ///
    /// Deviation: Java may return `null`; the Rust `TileBiome`/`CustomBiome`
    /// require a `Rc<dyn NoiseProperty>`, so an absent key yields the
    /// `ConstantNoiseProperty.DEFAULT_ZERO` remap (`0.5`).
    pub fn load_heightmap_noise(&self) -> Rc<dyn NoiseProperty> {
        if self.base.has(constants::KEY_HEIGHTMAP_NOISE) {
            return NoisePropertyJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_HEIGHTMAP_NOISE),
            )
            .load();
        }
        default_zero()
    }

    /// Java `int loadColor()`.
    pub fn load_color(&self) -> i32 {
        let mut rgb = 0xFF0000;
        if self.base.has(constants::KEY_MAP_COLOR) {
            rgb = Self::get_color(&JsonLoaderBase::get_as_string(
                &self.base.get(constants::KEY_MAP_COLOR).unwrap(),
            ));
        }
        rgb
    }

    /// Java `@Nullable BiomeInterpolation loadInterpolation()`.
    pub fn load_interpolation(&self) -> Rc<BiomeInterpolation> {
        if self.base.has(constants::KEY_INTERPOLATION) {
            return Rc::new(
                BiomeInterpolationJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_INTERPOLATION),
                    self.biome_context.get_parent_context(),
                )
                .load(),
            );
        }
        // Java `BiomeInterpolation.DEFAULT`.
        Rc::new((*crate::biome::biome_interpolation::DEFAULT).clone())
    }

    /// Java `static int getColor(String string)`.
    fn get_color(string: &str) -> i32 {
        ColorUtil::hex_string(string)
    }
}
