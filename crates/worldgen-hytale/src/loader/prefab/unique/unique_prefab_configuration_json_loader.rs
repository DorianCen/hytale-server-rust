// Ported from: `.hytale-source/.../worldgen/loader/prefab/unique/UniquePrefabConfigurationJsonLoader.java`
//! Java `loader.prefab.unique.UniquePrefabConfigurationJsonLoader` — the
//! configuration of one unique prefab generator.
//!
//! The loader reads the *exclusion radius*, the *zone-border exclusion*,
//! the *anchor*, the *spawn offset* / *spawn flag*, the *max distance*, the
//! *max attempts*, the *rotations*, the *biome mask*, the *noise mask*, the
//! *block mask* and the *parent* block set of the `"Config"` object of a
//! unique prefab entry.
//!
//! # Seed chain
//!
//! The Java loader does **not** append a suffix to the seed — the
//! configuration shares the seed of its generator
//! (`…UniquePrefabContainer-0.UniquePrefabGenerator`). The *sub-loaders*
//! append their own suffixes:
//!
//! | Sub-loader                        | Seed suffix              |
//! |-----------------------------------|--------------------------|
//! | `ResolvedBlockArrayJsonLoader`    | `ResolvedBlockArray`     |
//! | `HeightThresholdInterpreterJsonLoader` | `.HeightThresholdInterpreter` |
//! | `BiomeMaskJsonLoader`             | `.BiomeMask-UniquePrefab`|
//! | `NoiseMaskConditionJsonLoader`    | `.NoiseMaskCondition`    |
//! | `BlockPlacementMaskJsonLoader`    | `.BlockMaskCondition`    |
//! | `Vector2dJsonLoader` / `Vector3dJsonLoader` | *(none)*      |

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::prefab::block_placement_mask_json_loader::BlockPlacementMaskJsonLoader;
use crate::loader::prefab::prefab_pattern_generator_json_loader::PrefabPatternGeneratorJsonLoader;
use crate::loader::util::resolved_block_array_json_loader::ResolvedBlockArrayJsonLoader;
use crate::loader::util::vector2d_json_loader::Vector2dJsonLoader;
use crate::loader::util::vector3d_json_loader::Vector3dJsonLoader;
use crate::prefab::prefab_rotation::PrefabRotation;
use crate::prefab::unique::unique_prefab_configuration::UniquePrefabConfiguration;
use crate::util::condition::HashSetBlockFluidCondition;
use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};

/// Java `UniquePrefabConfigurationJsonLoader.Constants`.
pub mod constants {
    pub const KEY_PARENT: &str = "Parent";
    pub const KEY_HEIGHT_THRESHOLD: &str = "HeightThreshold";
    pub const KEY_BIOME_MASK: &str = "BiomeMask";
    pub const KEY_NOISE_MASK: &str = "NoiseMask";
    pub const KEY_MASK: &str = "Mask";
    pub const KEY_ANCHOR: &str = "Anchor";
    pub const KEY_FIT_HEIGHTMAP: &str = "FitHeightmap";
    pub const KEY_SUBMERGE: &str = "Submerge";
    pub const KEY_ENVIRONMENT: &str = "Environment";
    pub const KEY_ON_WATER: &str = "OnWater";
    pub const KEY_MAX_DISTANCE: &str = "MaxDistance";
    pub const KEY_MAX_ATTEMPTS: &str = "MaxAttempts";
    pub const KEY_EXCLUSION_RADIUS: &str = "ExclusionRadius";
    pub const KEY_IS_SPAWN: &str = "IsSpawn";
    pub const KEY_SPAWN_OFFSET: &str = "SpawnOffset";
    pub const KEY_BORDER_EXCLUSION: &str = "BorderExclusion";
    pub const KEY_SHOW_ON_MAP: &str = "ShowOnMap";

    /// Java `PrefabPatternGeneratorJsonLoader.Constants.KEY_ROTATIONS`.
    pub const KEY_ROTATIONS: &str = "Rotations";

    pub const SEED_STRING_BIOME_MASK_TYPE: &str = "UniquePrefab";

    pub const ERROR_BIOME_ERROR_MASK: &str =
        "Could not find tile / custom biome \"%s\" for biome mask. Typo or disabled biome?";
    pub const ERROR_NO_ANCHOR: &str = "Could not find anchor for Unique prefab generator";
    pub const ERROR_LOADING_ENVIRONMENT: &str = "Error while looking up environment \"%s\"!";

    pub const DEFAULT_MAX_DISTANCE: f64 = 100.0;
    pub const DEFAULT_MAX_ATTEMPTS: i32 = 5000;
    pub const DEFAULT_EXCLUSION_RADIUS: f64 = 50.0;
    pub const DEFAULT_ZONE_BORDER_EXCLUSION: f64 = 25.0;

    pub const DEFAULT_SUBMERGE: bool = false;

    /// Java `AssetMapWithIndexes.NOT_FOUND` = `Integer.MIN_VALUE`.
    pub const NOT_FOUND: i32 = i32::MIN;

    /// Java `WorldGenConstants.ENVIRONMENT_NOT_SET` = `Integer.MIN_VALUE`.
    pub const ENVIRONMENT_NOT_SET: i32 = i32::MIN;
}

/// Java `loader.prefab.unique.UniquePrefabConfigurationJsonLoader`.
pub struct UniquePrefabConfigurationJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, UniquePrefabConfiguration>`.
    pub base: JsonLoaderBase,
    /// Java `protected final ZoneFileContext zoneContext`.
    pub zone_context: Rc<ZoneFileContext>,
    /// The resolved block / fluid / environment registries.
    pub registries: Rc<Registries>,
}

impl UniquePrefabConfigurationJsonLoader {
    /// Java `UniquePrefabConfigurationJsonLoader(SeedString seed, Path
    /// dataFolder, JsonElement json, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed, data_folder, json),
            zone_context,
            registries,
        }
    }

    /// Java `@Nonnull UniquePrefabConfiguration load()`.
    ///
    /// The Java evaluates the constructor arguments left to right — the
    /// order of the *loads* is therefore part of the contract (a failing
    /// sub-loader aborts the load with its own message).
    pub fn load(&self) -> UniquePrefabConfiguration {
        let height_thresholds = self.load_height_thresholds();
        let mask = self.load_mask();
        let rotations = self.load_rotations();
        let biome_mask = self.load_biome_mask();
        let map_condition = self.load_map_condition();
        let parent = self.load_parent();
        let anchor = self.load_anchor();
        let spawn_offset = self.load_spawn_offset();
        let max_distance = self.load_max_distance();
        let fit_heightmap = self.load_fit_heightmap();
        let submerge = self.load_submerge();
        let on_water = self.load_on_water();
        let environment = self.load_environment();
        let max_attempts = self.load_max_attempts();
        let exclusion_radius = self.load_exclusion_radius();
        let is_spawn = self.load_is_spawn();
        let zone_border_exclusion = self.load_zone_border_exclusion();
        let show_on_map = self.load_show_on_map();

        UniquePrefabConfiguration::new(
            height_thresholds,
            mask,
            rotations,
            biome_mask,
            map_condition,
            parent,
            anchor,
            spawn_offset,
            max_distance,
            fit_heightmap,
            submerge,
            on_water,
            environment,
            max_attempts,
            exclusion_radius,
            is_spawn,
            zone_border_exclusion,
            show_on_map,
        )
    }

    /// Java `@Nonnull IBlockFluidCondition loadParent()`.
    fn load_parent(&self) -> Rc<dyn procedural::noise::condition::IBlockFluidCondition> {
        if !self.base.has(constants::KEY_PARENT) {
            // Java: `ConstantBlockFluidCondition.DEFAULT_TRUE`.
            return Rc::new(procedural::noise::condition::ConstantBlockFluidCondition::True);
        }
        let block_array = ResolvedBlockArrayJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_PARENT),
            Rc::clone(&self.registries),
        )
        .load();
        Rc::new(HashSetBlockFluidCondition::new(
            block_array.get_entry_set().clone(),
        ))
    }

    /// Java `@Nullable ICoordinateRndCondition loadHeightThresholds()`.
    fn load_height_thresholds(
        &self,
    ) -> Option<Rc<dyn procedural::noise::condition::ICoordinateRndCondition>> {
        if !self.base.has(constants::KEY_HEIGHT_THRESHOLD) {
            return None;
        }
        let height_threshold_object = self
            .base
            .get(constants::KEY_HEIGHT_THRESHOLD)
            .expect("the height threshold key is present");
        let interpreter = HeightThresholdInterpreterJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            Some(height_threshold_object),
            Self::world_height(),
        )
        .load();
        Some(Rc::new(procedural::noise::condition::HeightCondition::new(
            interpreter,
        )))
    }

    /// Java `ChunkUtil.HEIGHT` — the legacy chunk height.
    #[allow(deprecated)]
    fn world_height() -> i32 {
        hytale_math::chunk_util::HEIGHT
    }

    /// Java `@Nullable IIntCondition loadBiomeMask()`.
    fn load_biome_mask(&self) -> Rc<dyn procedural::noise::support::IIntCondition> {
        if !self.base.has(constants::KEY_BIOME_MASK) {
            // Java: `ConstantIntCondition.DEFAULT_TRUE`.
            return Rc::new(procedural::noise::condition::ConstantIntCondition::DEFAULT_TRUE);
        }
        let mask = crate::loader::biome::biome_mask_json_loader::BiomeMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_BIOME_MASK),
            constants::SEED_STRING_BIOME_MASK_TYPE,
            Rc::clone(&self.zone_context),
        )
        .load();
        Rc::new(mask)
    }

    /// Java `@Nullable PrefabRotation[] loadRotations()`.
    fn load_rotations(&self) -> Option<Vec<PrefabRotation>> {
        if !self.base.has(constants::KEY_ROTATIONS) {
            return None;
        }
        PrefabPatternGeneratorJsonLoader::load_rotations_element(
            self.base.get(constants::KEY_ROTATIONS),
        )
    }

    /// Java `@Nonnull ICoordinateCondition loadMapCondition()`.
    fn load_map_condition(&self) -> Rc<dyn procedural::noise::condition::ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE_MASK),
        )
        .load()
    }

    /// Java `@Nullable BlockMaskCondition loadMask()`.
    fn load_mask(&self) -> crate::util::condition::BlockMaskCondition {
        if !self.base.has(constants::KEY_MASK) {
            // Java: `BlockMaskCondition.DEFAULT_TRUE`.
            return crate::util::condition::BlockMaskCondition::new();
        }
        BlockPlacementMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get_raw(constants::KEY_MASK),
            Rc::clone(&self.registries),
        )
        .load()
    }

    /// Java `@Nonnull Vector2d loadAnchor()`.
    fn load_anchor(&self) -> crate::loader::util::Vector2d {
        if !self.base.has(constants::KEY_ANCHOR) {
            panic!("{}", constants::ERROR_NO_ANCHOR);
        }
        Vector2dJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_ANCHOR),
        )
        .load()
    }

    /// Java `@Nonnull Vector3d loadSpawnOffset()`.
    fn load_spawn_offset(&self) -> crate::loader::util::Vector3d {
        // We do it very negative here to signal there is no offset and it has
        // to figure out one by itself.
        if !self.base.has(constants::KEY_SPAWN_OFFSET) {
            return crate::loader::util::Vector3d::new(0.0, -5000.0, 0.0);
        }
        Vector3dJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_SPAWN_OFFSET),
        )
        .load()
    }

    /// Java `protected int loadEnvironment()`.
    fn load_environment(&self) -> i32 {
        if !self.base.has(constants::KEY_ENVIRONMENT) {
            return constants::ENVIRONMENT_NOT_SET;
        }
        let environment_element = self
            .base
            .get(constants::KEY_ENVIRONMENT)
            .expect("the environment key is present");
        let environment_id = JsonLoaderBase::get_as_string(&environment_element);
        match self.registries.environments.index_of(&environment_id) {
            Some(index) => index,
            None => panic!(
                "{}",
                constants::ERROR_LOADING_ENVIRONMENT.replace("%s", &environment_id)
            ),
        }
    }

    /// Java `protected boolean loadFitHeightmap()`.
    fn load_fit_heightmap(&self) -> bool {
        self.base.has(constants::KEY_FIT_HEIGHTMAP)
            && JsonLoaderBase::get_as_bool(
                &self
                    .base
                    .get(constants::KEY_FIT_HEIGHTMAP)
                    .expect("the fit heightmap key is present"),
            )
    }

    /// Java `protected boolean loadSubmerge()`.
    fn load_submerge(&self) -> bool {
        self.base
            .must_get_bool(constants::KEY_SUBMERGE, Some(constants::DEFAULT_SUBMERGE))
    }

    /// Java `protected boolean loadOnWater()`.
    fn load_on_water(&self) -> bool {
        self.base.has(constants::KEY_ON_WATER)
            && JsonLoaderBase::get_as_bool(
                &self
                    .base
                    .get(constants::KEY_ON_WATER)
                    .expect("the on water key is present"),
            )
    }

    /// Java `protected double loadMaxDistance()`.
    fn load_max_distance(&self) -> f64 {
        if self.base.has(constants::KEY_MAX_DISTANCE) {
            JsonLoaderBase::get_as_double(
                &self
                    .base
                    .get(constants::KEY_MAX_DISTANCE)
                    .expect("the max distance key is present"),
            )
        } else {
            constants::DEFAULT_MAX_DISTANCE
        }
    }

    /// Java `protected int loadMaxAttempts()`.
    fn load_max_attempts(&self) -> i32 {
        if self.base.has(constants::KEY_MAX_ATTEMPTS) {
            JsonLoaderBase::get_as_int(
                &self
                    .base
                    .get(constants::KEY_MAX_ATTEMPTS)
                    .expect("the max attempts key is present"),
            )
        } else {
            constants::DEFAULT_MAX_ATTEMPTS
        }
    }

    /// Java `protected double loadExclusionRadius()`.
    fn load_exclusion_radius(&self) -> f64 {
        if self.base.has(constants::KEY_EXCLUSION_RADIUS) {
            JsonLoaderBase::get_as_double(
                &self
                    .base
                    .get(constants::KEY_EXCLUSION_RADIUS)
                    .expect("the exclusion radius key is present"),
            )
        } else {
            constants::DEFAULT_EXCLUSION_RADIUS
        }
    }

    /// Java `protected boolean loadIsSpawn()`.
    fn load_is_spawn(&self) -> bool {
        self.base.has(constants::KEY_IS_SPAWN)
            && JsonLoaderBase::get_as_bool(
                &self
                    .base
                    .get(constants::KEY_IS_SPAWN)
                    .expect("the is spawn key is present"),
            )
    }

    /// Java `protected double loadZoneBorderExclusion()`.
    fn load_zone_border_exclusion(&self) -> f64 {
        if self.base.has(constants::KEY_BORDER_EXCLUSION) {
            JsonLoaderBase::get_as_double(
                &self
                    .base
                    .get(constants::KEY_BORDER_EXCLUSION)
                    .expect("the border exclusion key is present"),
            )
        } else {
            constants::DEFAULT_ZONE_BORDER_EXCLUSION
        }
    }

    /// Java `protected boolean loadShowOnMap()`.
    fn load_show_on_map(&self) -> bool {
        self.base.has(constants::KEY_SHOW_ON_MAP)
            && JsonLoaderBase::get_as_bool(
                &self
                    .base
                    .get(constants::KEY_SHOW_ON_MAP)
                    .expect("the show on map key is present"),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;
    use std::path::PathBuf;

    /// A zone context with a single tile biome.
    fn zone_context() -> Rc<ZoneFileContext> {
        let loading = FileLoadingContext::new("test", PathBuf::from("."));
        loading.create_zone("Zone", PathBuf::from("Zones/Zone"))
    }

    /// A loader over the given configuration JSON.
    fn loader(json: Option<Value>) -> UniquePrefabConfigurationJsonLoader {
        UniquePrefabConfigurationJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            zone_context(),
            Rc::new(Registries::load()),
        )
    }

    /// The panic message of a panicking loader.
    fn panic_message<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> String {
        let result = std::panic::catch_unwind(f);
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        crate::loader::panic_message(payload)
    }

    #[test]
    fn the_defaults_match_the_java_constants() {
        let configuration = loader(Some(serde_json::json!({"Anchor": [0, 0]}))).load();
        assert_eq!(configuration.get_max_distance(), 100.0);
        assert_eq!(configuration.get_max_attempts(), 5000);
        assert_eq!(configuration.get_exclusion_radius_squared(), 2500.0);
        assert_eq!(configuration.get_zone_border_exclusion(), 25.0);
        assert!(!configuration.is_fit_heightmap());
        assert!(!configuration.is_submerge());
        assert!(!configuration.is_on_water());
        assert!(!configuration.is_spawn_location());
        assert!(!configuration.is_show_on_map());
        assert_eq!(configuration.get_environment_id(), i32::MIN);
        // The spawn offset defaults to a very negative value.
        assert_eq!(
            configuration.get_spawn_offset(),
            crate::loader::util::Vector3d::new(0.0, -5000.0, 0.0)
        );
        assert!(configuration.get_height_condition().is_none());
        assert!(configuration.get_rotations().is_none());
    }

    #[test]
    fn a_missing_anchor_panics_verbatim() {
        let message = panic_message(|| {
            loader(Some(serde_json::json!({}))).load();
        });
        assert_eq!(message, "Could not find anchor for Unique prefab generator");
    }

    #[test]
    fn the_anchor_and_the_exclusions_are_read() {
        let configuration = loader(Some(serde_json::json!({
            "Anchor": [12.5, -3.0],
            "ExclusionRadius": 300.0,
            "BorderExclusion": 30.0,
            "MaxDistance": 30.0,
            "MaxAttempts": 42,
            "IsSpawn": true,
            "SpawnOffset": [0, 3, 16],
            "Rotations": ["ROTATION_270"],
            "FitHeightmap": true,
            "Submerge": true,
            "OnWater": true,
            "ShowOnMap": true
        })))
        .load();
        assert_eq!(
            configuration.get_anchor(),
            crate::loader::util::Vector2d::new(12.5, -3.0)
        );
        assert_eq!(configuration.get_exclusion_radius_squared(), 90000.0);
        assert_eq!(configuration.get_zone_border_exclusion(), 30.0);
        assert_eq!(configuration.get_max_distance(), 30.0);
        assert_eq!(configuration.get_max_attempts(), 42);
        assert!(configuration.is_spawn_location());
        assert_eq!(
            configuration.get_spawn_offset(),
            crate::loader::util::Vector3d::new(0.0, 3.0, 16.0)
        );
        assert_eq!(
            configuration.get_rotations(),
            Some(vec![PrefabRotation::Rotation270].as_slice())
        );
        assert!(configuration.is_fit_heightmap());
        assert!(configuration.is_submerge());
        assert!(configuration.is_on_water());
        assert!(configuration.is_show_on_map());
    }

    #[test]
    fn an_unknown_environment_panics_verbatim() {
        let message = panic_message(|| {
            loader(Some(serde_json::json!({
                "Anchor": [0, 0],
                "Environment": "NotAnEnvironment"
            })))
            .load();
        });
        assert_eq!(
            message,
            "Error while looking up environment \"NotAnEnvironment\"!"
        );
    }

    #[test]
    fn a_known_environment_is_resolved() {
        let registries = Rc::new(Registries::load());
        let name = registries
            .environments
            .names()
            .iter()
            .find(|name| !name.is_empty())
            .cloned()
            .expect("the golden registries carry environments");
        let index = registries.environments.index_of(&name).unwrap();
        let loader = UniquePrefabConfigurationJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            Some(serde_json::json!({"Anchor": [0, 0], "Environment": name})),
            zone_context(),
            registries,
        );
        assert_eq!(loader.load().get_environment_id(), index);
    }

    #[test]
    fn the_parent_blocks_become_a_block_fluid_condition() {
        let configuration = loader(Some(serde_json::json!({
            "Anchor": [0, 0],
            "Parent": ["Rock_Stone"]
        })))
        .load();
        // The parent condition accepts the resolved block and nothing else.
        let registries = Rc::new(Registries::load());
        let index = registries
            .blocks
            .index_of("Rock_Stone")
            .expect("Rock_Stone is in the golden registries");
        assert!(configuration.is_valid_parent_block(index, 0));
        assert!(!configuration.is_valid_parent_block(index + 1, 0));
    }
}
