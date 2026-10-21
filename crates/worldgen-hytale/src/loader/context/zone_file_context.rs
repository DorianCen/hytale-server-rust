// Ported from: `.hytale-source/.../worldgen/loader/context/ZoneFileContext.java`
//! Java `loader.context.ZoneFileContext`.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::Value;

use super::biome_file_context::{BiomeFileContext, BiomeType};
use super::file_context::{FileContext, ParentContext, Registry};
use super::file_loading_context::FileLoadingContext;

/// Java `ZoneFileContext.Constants.ZONE_PREFIX` (`"Zones."`).
const ZONE_PREFIX: &str = "Zones.";
/// Java `JsonLoader.Constants.KEY_FILE`.
const KEY_FILE: &str = "File";
/// Java `JsonLoader.Constants.JSON_FILEPATH_SEPARATOR`.
const JSON_FILEPATH_SEPARATOR: char = '.';

/// Java `ZoneFileContext`.
pub struct ZoneFileContext {
    /// Java `extends FileContext<FileLoadingContext>`.
    pub base: FileContext,
    /// Java `private final Registry<BiomeFileContext> tileBiomes`.
    tile_biomes: RefCell<Registry<BiomeFileContext>>,
    /// Java `private final Registry<BiomeFileContext> customBiomes`.
    custom_biomes: RefCell<Registry<BiomeFileContext>>,
}

impl ZoneFileContext {
    /// Java `ZoneFileContext(int id, String name, Path filepath, FileLoadingContext context)`.
    pub fn new(id: i32, name: &str, filepath: PathBuf, parent: Rc<FileLoadingContext>) -> Rc<Self> {
        Rc::new(Self {
            base: FileContext::new(
                id,
                name.to_string(),
                filepath,
                ParentContext::Loading(parent),
            ),
            tile_biomes: RefCell::new(Registry::new("TileBiome")),
            custom_biomes: RefCell::new(Registry::new("CustomBiome")),
        })
    }

    /// Java `Registry<BiomeFileContext> getTileBiomes()`.
    pub fn get_tile_biomes(&self) -> &RefCell<Registry<BiomeFileContext>> {
        &self.tile_biomes
    }

    /// Java `Registry<BiomeFileContext> getCustomBiomes()`.
    pub fn get_custom_biomes(&self) -> &RefCell<Registry<BiomeFileContext>> {
        &self.custom_biomes
    }

    /// Java `Registry<BiomeFileContext> getBiomes(BiomeFileContext.Type type)`.
    pub fn get_biomes(&self, biome_type: BiomeType) -> &RefCell<Registry<BiomeFileContext>> {
        match biome_type {
            BiomeType::Tile => self.get_tile_biomes(),
            BiomeType::Custom => self.get_custom_biomes(),
        }
    }

    /// Java `ZoneFileContext matchContext(JsonElement json, String key)` —
    /// resolves the zone context for a (possibly file-redirected) keyed
    /// element.
    pub fn match_context_json(
        self: &Rc<Self>,
        json: Option<&Value>,
        key: &str,
    ) -> Rc<ZoneFileContext> {
        let Some(json) = json else {
            return self.clone(); // Root json element is not valid
        };
        let Some(obj) = json.as_object() else {
            return self.clone(); // Root json element is not valid
        };

        let Some(element) = obj.get(key) else {
            return self.clone(); // Not possible for keyed element to have a file redirect
        };
        let Some(element_obj) = element.as_object() else {
            return self.clone();
        };

        let Some(file_path) = element_obj.get(KEY_FILE).and_then(|v| v.as_str()) else {
            return self.clone(); // Element doesn't contain a file redirect
        };

        self.match_context(file_path)
    }

    /// Java `ZoneFileContext matchContext(String filePath)` — resolves the
    /// zone context for a file path below `"Zones."`.
    pub fn match_context(self: &Rc<Self>, file_path: &str) -> Rc<ZoneFileContext> {
        if !file_path.starts_with(ZONE_PREFIX) {
            return self.clone(); // Isn't a zone-specific file
        }

        let name_start = ZONE_PREFIX.len();
        let Some(name_end) = file_path[name_start..]
            .find(JSON_FILEPATH_SEPARATOR)
            .map(|i| i + name_start)
        else {
            return self.clone(); // Path is pointing to a zone folder not a file
        };
        if file_path[name_start..name_end] == self.base.name {
            return self.clone(); // Already in the right context
        }

        let zone_name = &file_path[name_start..name_end];
        let parent_context = self.get_parent_context();
        let zone_registry = parent_context.get_zones().borrow();
        if !zone_registry.contains(zone_name) {
            return self.clone(); // Path isn't pointing inside a zone folder
        }

        zone_registry.get(zone_name)
    }

    /// Java `getParentContext()` — the loading context (typed).
    pub fn get_parent_context(&self) -> Rc<FileLoadingContext> {
        match &self.base.parent {
            ParentContext::Loading(loading) => loading.clone(),
            _ => unreachable!("ZoneFileContext parent is always a FileLoadingContext"),
        }
    }

    /// Java `protected BiomeFileContext createBiome(String name, Path path, Type type)`.
    pub fn create_biome(
        self: &Rc<Self>,
        name: &str,
        path: PathBuf,
        biome_type: BiomeType,
    ) -> Rc<BiomeFileContext> {
        let id = self.get_parent_context().next_biome_id();
        self.create_biome_with_id(id, name, path, biome_type)
    }

    /// Java `protected BiomeFileContext createBiome(int id, String name, Path path, Type type)`.
    pub fn create_biome_with_id(
        self: &Rc<Self>,
        id: i32,
        name: &str,
        path: PathBuf,
        biome_type: BiomeType,
    ) -> Rc<BiomeFileContext> {
        let id = self.get_parent_context().update_biome_id(id);
        BiomeFileContext::new(id, name, path, biome_type, self.clone())
    }
}
