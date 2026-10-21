// Ported from: `.hytale-source/.../worldgen/loader/biome/BiomeMaskJsonLoader.java`
//! Java `loader.biome.BiomeMaskJsonLoader` — the `"Zone.Biome#Type"` rule
//! strings that build an int condition over biome ids.
//!
//! # Deviation
//!
//! Java `load()` returns `IIntCondition` and `BiomeInterpolationJsonLoader`
//! tests `mask instanceof HashSetIntCondition` to extract the id set. Rust
//! cannot downcast a `dyn IIntCondition`, so the loader returns the concrete
//! [`LoadedBiomeMask`] enum (which implements `IIntCondition`) and the
//! interpolation loader matches on it. The behaviour is identical.

use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::loader::context::biome_file_context::{BiomeFileContext, BiomeType};
use crate::loader::context::file_context::Registry;
use crate::loader::context::zone_file_context::ZoneFileContext;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::support::IIntCondition;

/// Java `BiomeMaskJsonLoader.Constants`.
pub mod constants {
    pub const ZONE_MARKER: char = '.';
    pub const TYPE_MARKER: char = '#';
    pub const NULL_BIOME_ID: i32 = -1;
    pub const WILDCARD_BIOME_NAME: &str = "*";
    pub const BIOME_TYPE_ANY_DISPLAY_NAME: &str = "Biome";
    pub const ERROR_PARSE_RULE: &str =
        "Failed to parse BiomeMask rule '%s'. Unable to find a %s called %s in %s";
}

/// The Java `HashSetIntCondition` / `ConstantIntCondition.DEFAULT_TRUE` split,
/// exposed concretely so the interpolation loader can read the id set.
#[derive(Clone)]
pub enum LoadedBiomeMask {
    /// Java `ConstantIntCondition.DEFAULT_TRUE` (no rules / empty array).
    Default,
    /// Java `HashSetIntCondition`.
    Set(HashSet<i32>),
}

impl LoadedBiomeMask {
    /// Whether the mask is the Java `ConstantIntCondition.DEFAULT_TRUE` — the
    /// loader returns it when the JSON is not an array (or the array is
    /// empty).
    ///
    /// The Java comparison is an *identity* comparison against the constant;
    /// the Rust port compares the values, which is the same thing for the
    /// two shapes the loader produces.
    pub fn is_default_true(&self) -> bool {
        matches!(self, Self::Default)
    }

    /// Whether the mask is the Java `ConstantIntCondition.DEFAULT_FALSE`.
    ///
    /// The Java biome mask loader never produces it (it returns
    /// `DEFAULT_TRUE` or a `HashSetIntCondition`), so the branch is dead in
    /// Java as well.
    pub fn is_default_false(&self) -> bool {
        false
    }
}

impl IIntCondition for LoadedBiomeMask {
    fn eval(&self, value: i32) -> bool {
        match self {
            Self::Default => true,
            Self::Set(set) => set.contains(&value),
        }
    }
}

/// Java `loader.biome.BiomeMaskJsonLoader`.
pub struct BiomeMaskJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, IIntCondition>`.
    pub base: JsonLoaderBase,
    /// Java `private final ZoneFileContext zoneContext`.
    zone_context: Rc<ZoneFileContext>,
}

impl BiomeMaskJsonLoader {
    /// Java `BiomeMaskJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, String maskName, ZoneFileContext zoneContext)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        mask_name: &str,
        zone_context: Rc<ZoneFileContext>,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(
                seed.append(&format!(".BiomeMask-{mask_name}")),
                data_folder,
                json,
            ),
            zone_context,
        }
    }

    /// Java `IIntCondition load()` — see the module note for the return type.
    pub fn load(&self) -> LoadedBiomeMask {
        self.load_mask()
    }

    /// Java `IIntCondition loadMask()`.
    pub fn load_mask(&self) -> LoadedBiomeMask {
        let mut mask = LoadedBiomeMask::Default;

        if let Some(array) = self.base.json.as_ref().and_then(|json| json.as_array()) {
            let mut builder =
                crate::util::condition::IntConditionBuilder::new(constants::NULL_BIOME_ID);
            for element in array {
                let rule = JsonLoaderBase::get_as_string(element);
                self.parse_rule(&rule, &mut builder);
            }
            if let Some(set) = builder.build_hash_set() {
                mask = LoadedBiomeMask::Set(set);
            }
        }

        mask
    }

    /// Java `void parseRule(String rule, IntConditionBuilder builder)`.
    pub fn parse_rule(
        &self,
        rule: &str,
        builder: &mut crate::util::condition::IntConditionBuilder,
    ) {
        let zone_marker = rule
            .find(constants::ZONE_MARKER)
            .map(|i| i as i32)
            .unwrap_or(-1);
        let type_marker = rule
            .find(constants::TYPE_MARKER)
            .map(|i| i as i32)
            .unwrap_or(-1);

        let zone = Self::parse_zone(rule, zone_marker, &self.zone_context);
        let biome_name = Self::parse_biome_name(rule, zone_marker, type_marker);
        let biome_type = Self::parse_biome_type(rule, type_marker + 1);

        let result = match biome_type {
            None => {
                let a = Self::collect_biomes(zone.get_tile_biomes(), &biome_name, builder);
                let b = Self::collect_biomes(zone.get_custom_biomes(), &biome_name, builder);
                a | b
            }
            Some(biome_type) => {
                Self::collect_biomes(zone.get_biomes(biome_type), &biome_name, builder)
            }
        };

        if !result {
            panic!(
                "{}",
                format!(
                    "Failed to parse BiomeMask rule '{}'. Unable to find a {} called {} in {}",
                    rule,
                    Self::get_display_name(biome_type),
                    biome_name,
                    zone.base.get_name()
                )
            );
        }
    }

    /// Java `static boolean collectBiomes(Registry<BiomeFileContext> registry, String biomeName, IntConditionBuilder builder)`.
    fn collect_biomes(
        registry: &std::cell::RefCell<Registry<BiomeFileContext>>,
        biome_name: &str,
        builder: &mut crate::util::condition::IntConditionBuilder,
    ) -> bool {
        let registry = registry.borrow();
        if biome_name == constants::WILDCARD_BIOME_NAME {
            for (_, biome) in registry.iter() {
                builder.add(biome.base.get_id());
            }
            return true;
        }
        if registry.contains(biome_name) {
            let biome = registry.get(biome_name);
            builder.add(biome.base.get_id());
            return true;
        }
        false
    }

    /// Java `static ZoneFileContext parseZone(String rule, int marker, ZoneFileContext context)`.
    fn parse_zone(rule: &str, marker: i32, context: &Rc<ZoneFileContext>) -> Rc<ZoneFileContext> {
        if marker <= 0 {
            return Rc::clone(context);
        }
        let zone_name = &rule[..marker as usize];
        let parent = context.get_parent_context();
        let zones = parent.get_zones().borrow();
        zones.get(zone_name)
    }

    /// Java `static BiomeFileContext.Type parseBiomeType(String rule, int marker)`.
    fn parse_biome_type(rule: &str, marker: i32) -> Option<BiomeType> {
        if marker <= 0 {
            return None;
        }
        let type_name = &rule[marker as usize..];
        Some(match type_name {
            "Tile" => BiomeType::Tile,
            "Custom" => BiomeType::Custom,
            other => panic!("No enum constant BiomeFileContext.Type.{other}"),
        })
    }

    /// Java `static String parseBiomeName(String rule, int zoneMarker, int typeMarker)`.
    fn parse_biome_name(rule: &str, zone_marker: i32, type_marker: i32) -> String {
        let name_start = (zone_marker + 1) as usize;
        let name_end = if type_marker > zone_marker {
            type_marker as usize
        } else {
            rule.len()
        };
        if name_start == name_end {
            return constants::WILDCARD_BIOME_NAME.to_string();
        }
        rule[name_start..name_end].to_string()
    }

    /// Java `static String getDisplayName(BiomeFileContext.Type type)`.
    fn get_display_name(biome_type: Option<BiomeType>) -> &'static str {
        match biome_type {
            None => constants::BIOME_TYPE_ANY_DISPLAY_NAME,
            Some(biome_type) => biome_type.get_display_name(),
        }
    }
}
