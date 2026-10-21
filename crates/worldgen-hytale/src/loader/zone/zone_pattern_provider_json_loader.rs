// Ported from: `.hytale-source/.../worldgen/loader/zone/ZonePatternProviderJsonLoader.java`
//! Java `loader.zone.ZonePatternProviderJsonLoader` — the per-world zone
//! pattern provider: the grid generator, the unique-zone candidates, the mask
//! provider and the colour mapping.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use serde_json::Value;

use crate::chunk::mask_provider::MaskProvider;
use crate::climate::climate_color::ClimateColor;
use crate::climate::climate_type::ClimateType;
use crate::loader::zone::unique_zone_entry_json_loader::UniqueZoneEntryJsonLoader;
use crate::loader::zone::zone_color_mapping_json_loader::ZoneColorMappingJsonLoader;
use crate::loader::zone::zone_requirement_json_loader::ZoneRequirementJsonLoader;
use crate::zone::unique::UniqueCandidate;
use crate::zone::zone::Zone;
use crate::zone::zone_color_mapping::ZoneColorMapping;
use crate::zone::zone_pattern_provider::ZonePatternProvider;
use procedural::json::point_generator_json_loader::PointGeneratorJsonLoader;
use procedural::json::{JsonLoaderBase, SeedString};
use procedural::noise::point::IPointGenerator;

/// Java `ZonePatternProviderJsonLoader.Constants`.
pub mod constants {
    pub const KEY_GRID_GENERATOR: &str = "GridGenerator";
    pub const KEY_UNIQUE_ZONES: &str = "UniqueZones";
    pub const KEY_MASK_MAPPING: &str = "MaskMapping";

    pub const ERROR_UNMAPPED_COLOR: &str = "Mask image contains unmapped color! #%s";
    pub const ERROR_NO_MAPPING: &str =
        "Could not find mappings for colors in mask file. Keyword: MaskMapping";

    pub const UNIQUE_ZONE_CANDIDATE_POS_LIMIT: usize = 100;
}

/// Java `loader.zone.ZonePatternProviderJsonLoader`.
pub struct ZonePatternProviderJsonLoader {
    /// Java `extends JsonLoader<SeedStringResource, ZonePatternProvider>`.
    pub base: JsonLoaderBase,
    /// Java `protected final MaskProvider maskProvider`.
    pub mask_provider: MaskProvider,
    /// Java `protected Zone[] zones`.
    pub zones: Vec<Rc<Zone>>,
    /// Java `protected Map<String, Zone> zoneLookup`.
    pub zone_lookup: HashMap<String, Rc<Zone>>,
}

impl ZonePatternProviderJsonLoader {
    /// Java `ZonePatternProviderJsonLoader(SeedString<K> seed, Path dataFolder, JsonElement json, MaskProvider maskProvider)`.
    pub fn new(
        seed: SeedString,
        data_folder: &Path,
        json: Option<Value>,
        mask_provider: MaskProvider,
    ) -> Self {
        Self {
            base: JsonLoaderBase::new(seed.append(".ZonePatternGenerator"), data_folder, json),
            mask_provider,
            zones: Vec::new(),
            zone_lookup: HashMap::new(),
        }
    }

    /// Java `void setZones(Zone[] zones)`.
    pub fn set_zones(&mut self, zones: Vec<Rc<Zone>>) {
        self.zones = zones;
        self.zone_lookup = self
            .zones
            .iter()
            .map(|zone| (zone.name.clone(), Rc::clone(zone)))
            .collect();
    }

    /// Java `ZonePatternProvider load()`.
    pub fn load(&self) -> ZonePatternProvider {
        ZonePatternProvider::new(
            self.load_grid_generator(),
            self.zones.clone(),
            self.load_unique_zone_candidates(),
            self.mask_provider.clone(),
            self.load_color_mapping(),
        )
    }

    /// Java `@Nullable IPointGenerator loadGridGenerator()`.
    pub fn load_grid_generator(&self) -> Rc<dyn IPointGenerator> {
        PointGeneratorJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_GRID_GENERATOR),
        )
        .load()
    }

    /// Java `ZoneColorMapping loadColorMapping()`.
    pub fn load_color_mapping(&self) -> ZoneColorMapping {
        if !self.base.has(constants::KEY_MASK_MAPPING) {
            panic!("{}", constants::ERROR_NO_MAPPING);
        }
        let color_mapping = ZoneColorMappingJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.get(constants::KEY_MASK_MAPPING),
            Rc::new(self.zone_lookup.clone()),
        )
        .load();
        self.ensure_mask_integrity(&color_mapping);
        color_mapping
    }

    /// Java `Set<String> loadZoneRequirement()`.
    pub fn load_zone_requirement(&self) -> HashSet<String> {
        ZoneRequirementJsonLoader::new(
            self.base.seed.clone(),
            &self.base.data_folder,
            self.base.json.clone(),
        )
        .load()
    }

    /// Java `void ensureMaskIntegrity(ZoneColorMapping zoneColorMapping)`.
    pub fn ensure_mask_integrity(&self, zone_color_mapping: &ZoneColorMapping) {
        match &self.mask_provider {
            MaskProvider::Climate(provider) => {
                for parent in provider.get_graph().get_parents() {
                    if parent.children.is_empty() {
                        Self::validate_mapping(
                            Some(parent.as_ref()),
                            parent,
                            &parent.color,
                            zone_color_mapping,
                            "",
                        );
                        Self::validate_mapping(
                            Some(parent.as_ref()),
                            parent,
                            &parent.island,
                            zone_color_mapping,
                            "Island",
                        );
                    } else {
                        for child in &parent.children {
                            Self::validate_mapping(
                                Some(parent.as_ref()),
                                child,
                                &child.color,
                                zone_color_mapping,
                                "",
                            );
                            Self::validate_mapping(
                                Some(parent.as_ref()),
                                child,
                                &child.island,
                                zone_color_mapping,
                                "Island.",
                            );
                        }
                    }
                }
            }
            MaskProvider::Image(_) => {
                for rgb in self
                    .mask_provider
                    .get_fuzzy_zoom()
                    .get_exact_zoom()
                    .get_distance_provider()
                    .get_colors()
                {
                    if zone_color_mapping.get(*rgb).is_none() {
                        // Java `throw new NullPointerException(Integer.toHexString(rgb))`.
                        panic!("{rgb:x}");
                    }
                }
            }
        }
    }

    /// Java `Zone.UniqueCandidate[] loadUniqueZoneCandidates()`.
    pub fn load_unique_zone_candidates(&self) -> Vec<UniqueCandidate> {
        match &self.mask_provider {
            MaskProvider::Climate(provider) => {
                provider.get_unique_zone_candidates(&self.zone_lookup)
            }
            MaskProvider::Image(_) => {
                let unique_zones = UniqueZoneEntryJsonLoader::new(
                    self.base.seed.clone(),
                    &self.base.data_folder,
                    self.base.get(constants::KEY_UNIQUE_ZONES),
                    Rc::new(self.zone_lookup.clone()),
                )
                .load();

                self.mask_provider.generate_unique_zone_candidates(
                    &unique_zones,
                    constants::UNIQUE_ZONE_CANDIDATE_POS_LIMIT,
                )
            }
        }
    }

    /// Java `static void validateMapping(ClimateType parent, ClimateType type, ClimateColor color, ZoneColorMapping mapping, String prefix)`.
    fn validate_mapping(
        parent: Option<&ClimateType>,
        climate_type: &ClimateType,
        color: &ClimateColor,
        mapping: &ZoneColorMapping,
        prefix: &str,
    ) {
        let name = ClimateType::display_name(parent, climate_type);

        if mapping.get(color.land).is_none() {
            panic!("{prefix}Color is not mapped in climate type: {name}");
        }
        if mapping.get(color.shore).is_none() {
            panic!("{prefix}Shore is not mapped in climate type: {name}");
        }
        if mapping.get(color.ocean).is_none() {
            panic!("{prefix}Ocean is not mapped in climate type: {name}");
        }
        if mapping.get(color.shallow_ocean).is_none() {
            panic!("{prefix}ShallowOcean is not mapped in climate type: {name}");
        }
    }
}
