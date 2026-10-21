// Ported from: `.hytale-source/.../worldgen/loader/cave/CavePrefabConfigJsonLoader.java`
//! Java `loader.cave.CavePrefabConfigJsonLoader` — the configuration of a
//! cave prefab entry (the rotations, the placement, the masks, the iteration
//! count and the displacement of the prefab).

use std::path::Path;
use std::rc::Rc;

use serde_json::Value;
use worldgen_model::registry::Registries;

use crate::cave::cave_prefab_placement::CavePrefabPlacement;
use crate::cave::prefab::cave_prefab_container::CavePrefabConfig;
use crate::loader::context::zone_file_context::ZoneFileContext;
use crate::loader::prefab::block_placement_mask_json_loader::BlockPlacementMaskJsonLoader;
use crate::util::condition::BlockMaskCondition;
use procedural::json::double_range_json_loader::DoubleRangeJsonLoader;
use procedural::json::height_threshold_interpreter_json_loader::HeightThresholdInterpreterJsonLoader;
use procedural::json::noise_mask_condition_json_loader::NoiseMaskConditionJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::condition::IIntCondition;
use procedural::noise::condition::{
    DefaultCoordinateCondition, DefaultCoordinateRndCondition, HeightCondition,
    ICoordinateCondition, ICoordinateRndCondition,
};
use procedural::noise::supplier::{
    ConstantDoubleCoordinateHashSupplier, DoubleRangeCoordinateHashSupplier,
    IDoubleCoordinateHashSupplier,
};

/// Java `CavePrefabConfigJsonLoader.Constants`.
pub mod constants {
    pub const KEY_ROTATIONS: &str = "Rotations";
    pub const KEY_PLACEMENT: &str = "Placement";
    pub const KEY_BIOME_MASK: &str = "BiomeMask";
    pub const KEY_BLOCK_MASK: &str = "Mask";
    pub const KEY_ITERATIONS: &str = "Iterations";
    pub const KEY_DISPLACEMENT: &str = "Displacement";
    pub const KEY_NOISE_MASK: &str = "NoiseMask";
    pub const KEY_HEIGHT_THRESHOLD: &str = "HeightThreshold";

    pub const SEED_STRING_BIOME_MASK_TYPE: &str = "Prefab";

    pub const ERROR_ROTATIONS_MUST_POSITIVE: &str =
        "Array for rotations must have at least one entry or left away to allow random rotation";
    pub const ERROR_ROTATIONS_UNKOWN: &str =
        "Could not find rotation \"%s\". Allowed: [ROTATION_0, ROTATION_90, ROTATION_180, ROTATION_270]";
    pub const ERROR_ROTATIONS_UNKOWN_TYPE: &str =
        "\"Rotations\" is not an array nor a string, other types are not supported! Given: %s";
}

/// Java `loader.cave.CavePrefabConfigJsonLoader`.
pub struct CavePrefabConfigJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource,
    /// CavePrefabContainer.CavePrefabEntry.CavePrefabConfig>`.
    pub base: JsonLoaderBase,
    /// Java `private final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
    /// The resolved block / fluid registries.
    pub registries: Rc<Registries>,
}

impl CavePrefabConfigJsonLoader {
    /// Java `CavePrefabConfigJsonLoader(SeedString seed, Path dataFolder,
    /// JsonElement json, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        zone_context: Rc<ZoneFileContext>,
        registries: Rc<Registries>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".CavePrefabConfig"), data_folder, json),
            zone_context,
            registries,
        }
    }

    /// Java `CavePrefabConfig load()`.
    pub fn load(&self) -> CavePrefabConfig {
        // Java evaluates the constructor arguments left-to-right.
        let rotations = self.load_rotations();
        let placement = self.load_placement();
        let biome_mask = self.load_biome_mask();
        let block_mask = self.load_block_mask();
        let iterations = self.load_iterations();
        let displacement_supplier = self.load_displacement_supplier();
        let mask_condition = self.load_noise_condition();
        let height_condition = self.load_height_condition();

        CavePrefabConfig::new(
            rotations,
            placement,
            biome_mask,
            block_mask,
            iterations,
            displacement_supplier,
            mask_condition,
            height_condition,
        )
    }

    /// Java `PrefabRotation[] loadRotations()`.
    fn load_rotations(&self) -> Vec<crate::prefab::prefab_rotation::PrefabRotation> {
        if !self.base.has(constants::KEY_ROTATIONS) {
            // Java `PrefabRotation.VALUES`.
            return crate::prefab::prefab_rotation::PrefabRotation::VALUES.to_vec();
        }

        let element = self.base.get(constants::KEY_ROTATIONS).unwrap();
        match &element {
            Value::Array(array) => {
                if array.is_empty() {
                    panic!("{}", constants::ERROR_ROTATIONS_MUST_POSITIVE);
                }
                array
                    .iter()
                    .map(|element| {
                        let name = JsonLoaderBase::get_as_string(element);
                        Self::value_of(&name)
                    })
                    .collect()
            }
            Value::String(name) => vec![Self::value_of(name)],
            Value::Number(_) | Value::Bool(_) => {
                let name = JsonLoaderBase::get_as_string(&element);
                vec![Self::value_of(&name)]
            }
            other => panic!(
                "{}",
                constants::ERROR_ROTATIONS_UNKOWN_TYPE.replace("%s", &other.to_string())
            ),
        }
    }

    /// Java `PrefabRotation.valueOf(String)` — the *exact* enum name (not
    /// the extended form).
    fn value_of(name: &str) -> crate::prefab::prefab_rotation::PrefabRotation {
        crate::prefab::prefab_rotation::PrefabRotation::VALUES
            .into_iter()
            .find(|rotation| rotation.name() == name)
            .unwrap_or_else(|| panic!("{}", constants::ERROR_ROTATIONS_UNKOWN.replace("%s", name)))
    }

    /// Java `CavePrefabPlacement loadPlacement()`.
    fn load_placement(&self) -> CavePrefabPlacement {
        if !self.base.has(constants::KEY_PLACEMENT) {
            return CavePrefabPlacement::Default;
        }
        let name = JsonLoaderBase::get_as_string(&self.base.get(constants::KEY_PLACEMENT).unwrap());
        match name.as_str() {
            "CEILING" => CavePrefabPlacement::Ceiling,
            "FLOOR" => CavePrefabPlacement::Floor,
            "DEFAULT" => CavePrefabPlacement::Default,
            other => panic!(
                "No enum constant com.hypixel.hytale.server.worldgen.cave.CavePrefabPlacement.{other}"
            ),
        }
    }

    /// Java `IIntCondition loadBiomeMask()`.
    fn load_biome_mask(&self) -> Rc<dyn IIntCondition> {
        if !self.base.has(constants::KEY_BIOME_MASK) {
            return Rc::new(procedural::noise::condition::ConstantIntCondition::DEFAULT_TRUE);
        }
        // The biome mask may redirect to a file in a different zone folder.
        let context = self
            .zone_context
            .match_context_json(self.base.json.as_ref(), constants::KEY_BIOME_MASK);
        Rc::new(
            crate::loader::biome::biome_mask_json_loader::BiomeMaskJsonLoader::new(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_BIOME_MASK),
                constants::SEED_STRING_BIOME_MASK_TYPE,
                context,
            )
            .load(),
        )
    }

    /// Java `BlockMaskCondition loadBlockMask()`.
    fn load_block_mask(&self) -> BlockMaskCondition {
        if !self.base.has(constants::KEY_BLOCK_MASK) {
            return BlockMaskCondition::new();
        }
        BlockPlacementMaskJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get_raw(constants::KEY_BLOCK_MASK),
            Rc::clone(&self.registries),
        )
        .load()
    }

    /// Java `IDoubleRange loadIterations()`.
    fn load_iterations(&self) -> Rc<dyn procedural::noise::supplier::IDoubleRange> {
        Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_ITERATIONS),
                5.0,
            )
            .load(),
        )
    }

    /// Java `IDoubleCoordinateHashSupplier loadDisplacementSupplier()`.
    fn load_displacement_supplier(&self) -> Rc<dyn IDoubleCoordinateHashSupplier> {
        if !self.base.has(constants::KEY_DISPLACEMENT) {
            return Rc::new(ConstantDoubleCoordinateHashSupplier::ZERO);
        }
        let range: Rc<dyn procedural::noise::supplier::IDoubleRange> = Rc::new(
            DoubleRangeJsonLoader::with_default1(
                self.base.seed.clone(),
                &self.base.data_folder,
                self.base.get(constants::KEY_DISPLACEMENT),
                0.0,
            )
            .load(),
        );
        Rc::new(DoubleRangeCoordinateHashSupplier::new(range))
    }

    /// Java `ICoordinateCondition loadNoiseCondition()`.
    fn load_noise_condition(&self) -> Rc<dyn ICoordinateCondition> {
        NoiseMaskConditionJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_NOISE_MASK),
        )
        .load()
    }

    /// Java `ICoordinateRndCondition loadHeightCondition()`.
    fn load_height_condition(&self) -> Rc<dyn ICoordinateRndCondition> {
        if !self.base.has(constants::KEY_HEIGHT_THRESHOLD) {
            return Rc::new(DefaultCoordinateRndCondition::True);
        }
        let interpreter = HeightThresholdInterpreterJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_HEIGHT_THRESHOLD),
            #[allow(deprecated)] // deprecated in hytale-math but this is its exact Java value
            hytale_math::chunk_util::HEIGHT,
        )
        .load();
        Rc::new(HeightCondition::new(interpreter))
    }
}

/// The Java `DefaultCoordinateCondition.DEFAULT_TRUE` of the noise condition
/// (kept for the documentation of the default).
#[allow(dead_code)]
fn default_true() -> Rc<dyn ICoordinateCondition> {
    Rc::new(DefaultCoordinateCondition::True)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::context::file_loading_context::FileLoadingContext;

    fn loader(json: Option<Value>) -> CavePrefabConfigJsonLoader {
        let loading = FileLoadingContext::new("test", PathBuf::from("/tmp/world"));
        let zone = loading.create_zone("Oceans", PathBuf::from("/tmp/world/Zones/Oceans"));
        CavePrefabConfigJsonLoader::new(
            SeedString::new("0"),
            Path::new("."),
            json,
            zone,
            Rc::new(Registries::empty()),
        )
    }

    use std::path::PathBuf;

    #[test]
    fn the_seed_appends_the_prefab_config_suffix() {
        let loader = loader(None);
        assert_eq!(loader.base.seed.seed(), "0.CavePrefabConfig");
    }

    #[test]
    fn the_default_rotations_are_all_of_them() {
        let config = loader(None).load();
        assert_eq!(config.rotations.len(), 4);
    }

    #[test]
    fn the_rotations_are_read_from_an_array() {
        let config = loader(Some(serde_json::json!({"Rotations": ["ROTATION_90"]}))).load();
        assert_eq!(config.rotations.len(), 1);
        assert_eq!(
            config.rotations[0],
            crate::prefab::prefab_rotation::PrefabRotation::Rotation90
        );
    }

    #[test]
    fn an_empty_rotation_array_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({"Rotations": []}))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Array for rotations must have at least one entry or left away to allow random rotation"
        );
    }

    #[test]
    fn an_unknown_rotation_panics_verbatim() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({"Rotations": ["ROTATION_45"]}))).load()
        }));
        let payload = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => payload,
        };
        let message = crate::loader::panic_message(payload);
        assert_eq!(
            message,
            "Could not find rotation \"ROTATION_45\". Allowed: [ROTATION_0, ROTATION_90, ROTATION_180, ROTATION_270]"
        );
    }

    #[test]
    fn the_short_rotation_spelling_is_rejected() {
        // The Java loader uses `PrefabRotation.valueOf` (not
        // `valueOfExtended`), so "90" is not a valid rotation name.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader(Some(serde_json::json!({"Rotations": ["90"]}))).load()
        }));
        assert!(result.is_err());
    }

    #[test]
    fn the_placement_is_read() {
        let config = loader(Some(serde_json::json!({"Placement": "FLOOR"}))).load();
        assert_eq!(config.placement, CavePrefabPlacement::Floor);
    }
}
