// Ported from: `.hytale-source/.../worldgen/loader/context/FileContextLoader.java`
//! Java `loader.context.FileContextLoader` — scans the `Zones/` folder and
//! builds the [`FileLoadingContext`] (zone + biome file contexts with their
//! id assignment order).
//!
//! Java lists through the multi-root asset filesystem (`FileIO.list`) with
//! dedup + disable-marker handling; the world config path uses a single
//! on-disk root, so the port lists the directory flat and keeps only the
//! disable-marker skip (a `"!"`-prefixed folder/file is disabled). The order
//! comparator (`AssetPath.COMPARATOR`) is the component-wise Java string
//! comparison, and biome files sort by type ordinal (`Tile` < `Custom`)
//! before the path comparison.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;

use super::biome_file_context::{BiomeFileContext, BiomeType};
use super::file_loading_context::FileLoadingContext;
use super::zone_file_context::ZoneFileContext;
use crate::prefab::prefab_category::PrefabCategory;

/// Java `ZonesJsonLoader.Constants.PATH_ZONES`.
const PATH_ZONES: &str = "Zones";
/// Java `ZonesJsonLoader.Constants.FILE_ZONE_MAIN_FILE`.
const FILE_ZONE_MAIN_FILE: &str = "Zone.json";
/// Java `FileContextLoader.Constants.IDENTIFIER_DISABLE`.
const IDENTIFIER_DISABLE: &str = "!";

/// Java `Comparator<AssetPath> COMPARATOR` — component-wise Java string
/// comparison, then by component count.
fn java_path_compare(a: &Path, b: &Path) -> std::cmp::Ordering {
    let names = |p: &Path| {
        p.components()
            .filter(|c| {
                !matches!(
                    c,
                    std::path::Component::RootDir | std::path::Component::CurDir
                )
            })
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
    };
    let (a_names, b_names) = (names(a), names(b));
    let max = a_names.len().min(b_names.len());
    for i in 0..max {
        let comp = java_string_compare(&a_names[i], &b_names[i]);
        if comp != std::cmp::Ordering::Equal {
            return comp;
        }
    }
    a_names.len().cmp(&b_names.len())
}

/// Java `String.compareTo` — UTF-16 code-unit-wise comparison.
fn java_string_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let mut a_units = a.encode_utf16();
    let mut b_units = b.encode_utf16();
    loop {
        match (a_units.next(), b_units.next()) {
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
            }
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
        }
    }
}

/// Java `FileContextLoader`.
pub struct FileContextLoader {
    /// Java `private final String name`.
    name: String,
    /// Java `private final Path dataFolder`.
    data_folder: PathBuf,
    /// Java `private final Set<String> zoneRequirement`.
    zone_requirement: HashSet<String>,
}

impl FileContextLoader {
    /// Java `FileContextLoader(String name, Path dataFolder, Set<String> zoneRequirement)`.
    pub fn new(name: &str, data_folder: &Path, zone_requirement: HashSet<String>) -> Self {
        Self {
            name: name.to_string(),
            data_folder: data_folder.to_path_buf(),
            zone_requirement,
        }
    }

    /// Java `FileLoadingContext load()`.
    pub fn load(&self) -> Rc<FileLoadingContext> {
        let context = FileLoadingContext::new(&self.name, self.data_folder.clone());
        let zones_folder = self.data_folder.join(PATH_ZONES);

        // Java catches the IOException and logs SEVERE (the world then fails
        // zone validation); here a missing/broken folder yields no zones and
        // validation fails with the same Java error.
        let mut files = Self::list_zone_files(&zones_folder);
        files.sort_by(|a, b| java_path_compare(a, b));

        for path in files {
            let zone_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if !self.zone_requirement.contains(&zone_name) {
                continue;
            }

            let zone = Self::load_zone_context(&zone_name, &path, &context);
            context.get_zones().borrow_mut().register(&zone_name, zone);
        }

        Self::validate_zones(&context, &self.zone_requirement);

        Self::load_prefab_categories(&self.data_folder, &context);

        context
    }

    /// Java `static void loadPrefabCategories(Path folder, FileLoadingContext context)`.
    fn load_prefab_categories(folder: &Path, context: &Rc<FileLoadingContext>) {
        let path = folder.join(PrefabCategory::FILENAME);
        if !path.exists() {
            return;
        }

        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to open Categories.json: {e}"));
        let json: Value = serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("Failed to open Categories.json: {e}"));
        let mut categories = Vec::new();
        PrefabCategory::parse(Some(&json), |name, category| {
            categories.push((name.to_string(), category));
        });
        for (name, category) in categories {
            // Registry::register panics on duplicates (Java Error).
            context
                .get_prefab_categories()
                .borrow_mut()
                .register(&name, Rc::new(category));
        }
    }

    /// Java `static ZoneFileContext loadZoneContext(String name, Path folder, FileLoadingContext context)`.
    fn load_zone_context(
        name: &str,
        folder: &Path,
        context: &Rc<FileLoadingContext>,
    ) -> Rc<ZoneFileContext> {
        let zone = context.create_zone(name, folder.to_path_buf());
        let mut files = Self::list_biome_files(folder);
        // Java BIOME_ORDER: `comparing(BiomeFileContext::getBiomeType)` (enum
        // ordinal: Tile < Custom) `.thenComparing(AssetPath.COMPARATOR)`.
        files.sort_by(|a, b| {
            let type_a = Self::path_type(a);
            let type_b = Self::path_type(b);
            let ord = (type_a as usize).cmp(&(type_b as usize));
            if ord != std::cmp::Ordering::Equal {
                return ord;
            }
            java_path_compare(a, b)
        });

        for path in files {
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let biome_type = Self::path_type(&path);
            let biome_name = Self::parse_name(&file_name, biome_type);
            let biome = zone.create_biome(&biome_name, path, biome_type);
            zone.get_biomes(biome_type)
                .borrow_mut()
                .register(&biome_name, biome);
        }

        zone
    }

    fn path_type(path: &Path) -> BiomeType {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        BiomeFileContext::get_biome_type(&name)
    }

    /// Java `static void validateZones(FileLoadingContext context, Set<String> zoneRequirement)`.
    fn validate_zones(context: &Rc<FileLoadingContext>, zone_requirement: &HashSet<String>) {
        for key in zone_requirement {
            // Throws an error if the key is missing
            context.get_zones().borrow().get(key);
        }
    }

    /// Java `static String parseName(AssetPath path, Type type)`.
    fn parse_name(file_name: &str, biome_type: BiomeType) -> String {
        let start = biome_type.get_prefix().len();
        let end = file_name.len() - biome_type.get_suffix().len();
        file_name[start..end].to_string()
    }

    // ------------------------------------------------------------------
    // Java FileIO.list equivalents (single root, flat)
    // ------------------------------------------------------------------

    /// Java `FileIO.list(zonesFolder, ZONE_FILE_MATCHER, DISABLED_FILE)` —
    /// directories containing `Zone.json`, `"!"`-prefixed names disabled.
    fn list_zone_files(zones_folder: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(zones_folder) {
            Ok(entries) => entries,
            // Java logs SEVERE and continues (zone validation then fails).
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(IDENTIFIER_DISABLE) {
                continue; // disabled
            }
            let is_zone = path.is_dir() && path.join(FILE_ZONE_MAIN_FILE).exists();
            if is_zone {
                out.push(path);
            }
        }
        out
    }

    /// Java `FileIO.list(folder, BIOME_FILE_MATCHER, DISABLED_FILE)` — files
    /// with a `Tile.` / `Custom.` prefix and `.json` suffix.
    fn list_biome_files(folder: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(folder)
            .unwrap_or_else(|e| panic!("Failed to list files in: {}: {e}", folder.display()))
        {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(IDENTIFIER_DISABLE) {
                continue; // disabled
            }
            if path.is_dir() {
                continue;
            }
            let is_biome = [BiomeType::Tile, BiomeType::Custom]
                .iter()
                .any(|t| name.starts_with(t.get_prefix()) && name.ends_with(t.get_suffix()));
            if is_biome {
                out.push(path);
            }
        }
        out
    }
}
