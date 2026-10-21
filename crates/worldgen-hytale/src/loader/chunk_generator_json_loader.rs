// Ported from: `.hytale-source/.../worldgen/loader/ChunkGeneratorJsonLoader.java`
//! Java `worldgen.loader.ChunkGeneratorJsonLoader` — loads a world
//! configuration folder (`World.json` + `Mask.json` / `Mask.png`).
//!
//! The full load path is wired: world JSON → mask provider (climate or
//! image) → zone pattern provider → `FileContextLoader` → zones →
//! `ChunkGenerator`. The golden registry dump is loaded before the zones and
//! threaded down so the biome containers resolve block/fluid/environment
//! names. Known seams inside the zone files: the Phase-4 cave package and the
//! Phase-5 unique-prefab containers (see `zone_json_loader`); prefab
//! container entries are likewise a Phase-5 seam.

use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;

use crate::chunk::mask_provider::MaskProvider;
use crate::climate::climate_mask_provider::ClimateMaskProvider;
use crate::loader::climate::climate_mask_json_loader::ClimateMaskJsonLoader;
use crate::loader::context::file_context_loader::FileContextLoader;
use crate::loader::mask_provider_json_loader::MaskProviderJsonLoader;
use crate::loader::seed_string_resource::SeedStringResource;
use crate::loader::world_gen_prefab_loader::WorldGenPrefabLoader;
use crate::loader::zone::zone_pattern_provider_json_loader::ZonePatternProviderJsonLoader;
use crate::loader::zones_json_loader::ZonesJsonLoader;
use crate::prefab::prefab_store_root::PrefabStoreRoot;
use crate::weighted_map::{IWeightedMap, WeightedMap};
use crate::world_gen_config::WorldGenConfig;
use hytale_math::fast_random::FastRandom;
use procedural::json::{JsonLoaderBase, SeedString};
use worldgen_model::registry::Registries;

/// Java `ChunkGeneratorJsonLoader.Constants.KEY_WIDTH`.
pub const KEY_WIDTH: &str = "Width";
/// Java `ChunkGeneratorJsonLoader.Constants.KEY_HEIGHT`.
pub const KEY_HEIGHT: &str = "Height";
/// Java `ChunkGeneratorJsonLoader.Constants.KEY_OFFSET_X`.
pub const KEY_OFFSET_X: &str = "OffsetX";
/// Java `ChunkGeneratorJsonLoader.Constants.KEY_OFFSET_Y`.
pub const KEY_OFFSET_Y: &str = "OffsetY";
/// Java `ChunkGeneratorJsonLoader.Constants.KEY_RANDOMIZER`.
pub const KEY_RANDOMIZER: &str = "Randomizer";
/// Java `ChunkGeneratorJsonLoader.Constants.KEY_MASKS`.
pub const KEY_MASKS: &str = "Masks";
/// Java `ChunkGeneratorJsonLoader.Constants.KEY_PREFAB_STORE`.
pub const KEY_PREFAB_STORE: &str = "PrefabStore";
/// Java `ChunkGeneratorJsonLoader.Constants.OVERRIDE_DATA_FOLDER`.
pub const OVERRIDE_DATA_FOLDER: &str = "OverrideDataFolder";

/// Java `ChunkGeneratorJsonLoader.Constants.FILE_WORLD_JSON`.
pub const FILE_WORLD_JSON: &str = "World.json";
/// Java `ChunkGeneratorJsonLoader.Constants.FILE_ZONES_JSON`.
pub const FILE_ZONES_JSON: &str = "Zones.json";
/// Java `ChunkGeneratorJsonLoader.Constants.FILE_MASK_JSON`.
pub const FILE_MASK_JSON: &str = "Mask.json";
/// Java `ChunkGeneratorJsonLoader.Constants.FILE_MASK_PNG`.
pub const FILE_MASK_PNG: &str = "Mask.png";

/// Java `ChunkGeneratorJsonLoader.Constants.ERROR_WORLD_JSON_CORRUPT`.
pub const ERROR_WORLD_JSON_CORRUPT: &str = "Could not read JSON configuration for world. File: %s";
/// Java `ChunkGeneratorJsonLoader.Constants.ERROR_ZONE_FILE`.
pub const ERROR_ZONE_FILE: &str = "Failed to read zone configuration file! File: %s";

/// The mask provider a world configuration resolved to.
///
/// Java returns either a `ClimateMaskProvider` (a `MaskProvider` subclass,
/// the `Mask.json` branch) or a plain `MaskProvider` (the `Mask.png`
/// branch); the Rust port models the two branches with this enum.
pub enum LoadedMaskProvider {
    /// Java `ClimateMaskProvider` (the `Mask.json` branch).
    Climate(Rc<ClimateMaskProvider>),
    /// Java `MaskProvider` (the `Mask.png` branch).
    Image(MaskProvider),
}

impl LoadedMaskProvider {
    /// The climate mask provider (the `Mask.json` branch).
    pub fn as_climate(&self) -> Option<&Rc<ClimateMaskProvider>> {
        match self {
            Self::Climate(provider) => Some(provider),
            Self::Image(_) => None,
        }
    }

    /// The image mask provider (the `Mask.png` branch).
    pub fn as_image(&self) -> Option<&MaskProvider> {
        match self {
            Self::Image(provider) => Some(provider),
            Self::Climate(_) => None,
        }
    }
}

/// Java `worldgen.loader.ChunkGeneratorJsonLoader`.
pub struct ChunkGeneratorJsonLoader {
    /// Java `protected SeedString<SeedStringResource> seed` (the `Loader`
    /// base class field — **no** seed suffix is appended).
    pub seed: SeedString,
    /// Java `@Nonnull private final WorldGenConfig config`.
    pub config: WorldGenConfig,
    /// Java `protected final Path dataFolder` — the config path.
    pub data_folder: PathBuf,
    /// The root of the extracted base-game asset pack (Java
    /// `AssetUtil.getHytaleAssetsPath()`).
    pub assets_root: PathBuf,
}

impl ChunkGeneratorJsonLoader {
    /// Java `ChunkGeneratorJsonLoader(SeedString<SeedStringResource> seed, WorldGenConfig config)`.
    pub fn new(seed: SeedString, config: WorldGenConfig) -> Self {
        let data_folder = config.path.clone();
        Self {
            seed,
            config,
            data_folder,
            assets_root: PrefabStoreRoot::default_assets_root(),
        }
    }

    /// Java `ChunkGenerator load()`.
    ///
    /// Java's `AssetFileSystem` / `EventHandler` / `SeedStringResource
    /// .setPrefabConfig` boot block is out of loader scope (documented in
    /// NEXT_STEPS Subtask 7): the world config path reads from the on-disk
    /// data folder directly and the prefab config hook is a no-op.
    pub fn load(&self) -> Rc<crate::chunk::chunk_generator::ChunkGenerator> {
        let world_file = absolute_path(&self.data_folder.join(FILE_WORLD_JSON));
        if !world_file.exists() {
            // Java: `throw new IllegalArgumentException(String.valueOf(worldFile))`.
            panic!("{}", world_file.display());
        }
        if !is_readable(&world_file) {
            panic!("{}", world_file.display());
        }

        let world_json = self.load_world_json(&world_file);
        let override_data_folder =
            self.load_override_data_folder_path(&world_json, &self.config.path);
        let config = self.config.with_override(&override_data_folder);

        let world_size = self.load_world_size(&world_json);
        let world_offset = self.load_world_offset(&world_json);
        let loaded_mask = self.load_mask_provider(&world_json, world_size, world_offset);
        let prefab_store = self.load_prefab_store(&world_json);
        let _ = prefab_store; // `seed.get().setPrefabConfig(config, prefabStore)` — no-op hook

        // Java `loadMaskProvider` returns the polymorphic `MaskProvider`
        // (a `ClimateMaskProvider` subclass instance for the Mask.json
        // branch); the Rust enum folds back into the shared wrapper here.
        let mask_provider = match loaded_mask {
            LoadedMaskProvider::Climate(provider) => MaskProvider::climate(provider),
            LoadedMaskProvider::Image(provider) => provider,
        };

        // Java `loadZonePatternGenerator(maskProvider)` — wraps failures with
        // `ERROR_ZONE_FILE`.
        let zone_file = self.data_folder.join(FILE_ZONES_JSON);
        let zone_json = crate::loader::load_json_object(&zone_file);
        let mut zone_pattern_loader = ZonePatternProviderJsonLoader::new(
            self.seed.clone(),
            &self.data_folder,
            Some(zone_json),
            mask_provider,
        );

        let loading_context = FileContextLoader::new(
            &config.name,
            &override_data_folder,
            zone_pattern_loader.load_zone_requirement(),
        )
        .load();

        let registries = Rc::new(Registries::load());
        // The Java `SeedStringResource` — the world-wide unique-name
        // counters (the cave node types use them for their names and
        // therefore for their seeds).
        let resource = Rc::new(SeedStringResource::new());
        // The prefab loader of the world (Java `SeedStringResource.loader`).
        // The Rust port keeps it in the root loading context (which every
        // loader of the chain can reach).
        let prefab_loader = Rc::new(WorldGenPrefabLoader::new(
            prefab_store,
            &config.path,
            &self.assets_root,
            Rc::clone(&registries),
        ));
        loading_context.set_prefab_loader(Rc::clone(&prefab_loader));
        let zones = ZonesJsonLoader::new(
            self.seed.clone(),
            &override_data_folder,
            loading_context,
            registries,
            resource,
        )
        .load();
        zone_pattern_loader.set_zones(zones);

        crate::chunk::chunk_generator::ChunkGenerator::new(
            zone_pattern_loader.load(),
            Some(override_data_folder),
        )
    }

    /// Java `JsonObject loadWorldJson(Path file)`.
    pub fn load_world_json(&self, file: &Path) -> Value {
        // Java: `throw new Error(String.format(ERROR_WORLD_JSON_CORRUPT, file), e)`.
        let corrupt = |cause: &str| {
            format!(
                "{}: {cause}",
                ERROR_WORLD_JSON_CORRUPT.replace("%s", &file.display().to_string())
            )
        };
        match std::fs::read_to_string(file) {
            Ok(text) => match serde_json::from_str::<Value>(&text) {
                Ok(value) if value.is_object() => value,
                Ok(_) => panic!("{}", corrupt("the file is not a JSON object")),
                Err(e) => panic!("{}", corrupt(&e.to_string())),
            },
            Err(e) => panic!("{}", corrupt(&e.to_string())),
        }
    }

    /// Java `Path loadOverrideDataFolderPath(JsonObject worldJson, Path dataFolder)`.
    pub fn load_override_data_folder_path(
        &self,
        world_json: &Value,
        data_folder: &Path,
    ) -> PathBuf {
        if let Some(value) = world_json.get(OVERRIDE_DATA_FOLDER) {
            let override_folder =
                normalize_path(&data_folder.join(JsonLoaderBase::get_as_string(value)));
            // Use parent to allow sibling world gen configurations
            // (e.g. "../OtherConfig")
            let parent = match data_folder.parent() {
                Some(parent) => parent.to_path_buf(),
                // Java: `dataFolder.getParent()` is null -> the
                // `startsWith(parent)` call throws a NullPointerException.
                None => panic!(
                    "Cannot invoke \"java.nio.file.Path.startsWith(java.nio.file.Path)\" because \"parent\" is null"
                ),
            };

            if !override_folder.starts_with(&parent) || !override_folder.exists() {
                panic!(
                    "Override folder '{}' must exist within: '{}'",
                    file_name_of(&override_folder),
                    parent.display()
                );
            }

            return override_folder;
        }
        data_folder.to_path_buf()
    }

    /// Java `Vector2i loadWorldSize(JsonObject worldJson)`.
    pub fn load_world_size(&self, world_json: &Value) -> (i32, i32) {
        let mut width = 0;
        let mut height = 0;
        if let Some(value) = world_json.get(KEY_WIDTH) {
            width = JsonLoaderBase::get_as_int(value);
        }
        if let Some(value) = world_json.get(KEY_HEIGHT) {
            height = JsonLoaderBase::get_as_int(value);
        }
        (width, height)
    }

    /// Java `Vector2i loadWorldOffset(JsonObject worldJson)`.
    pub fn load_world_offset(&self, world_json: &Value) -> (i32, i32) {
        let mut offset_x = 0;
        let mut offset_y = 0;
        if let Some(value) = world_json.get(KEY_OFFSET_X) {
            offset_x = JsonLoaderBase::get_as_int(value);
        }
        if let Some(value) = world_json.get(KEY_OFFSET_Y) {
            offset_y = JsonLoaderBase::get_as_int(value);
        }
        (offset_x, offset_y)
    }

    /// Java `MaskProvider loadMaskProvider(JsonObject worldJson, Vector2i worldSize, Vector2i worldOffset)`.
    pub fn load_mask_provider(
        &self,
        world_json: &Value,
        world_size: (i32, i32),
        world_offset: (i32, i32),
    ) -> LoadedMaskProvider {
        let mut builder = WeightedMap::<String>::builder();
        match world_json.get(KEY_MASKS) {
            None => {
                builder.put(FILE_MASK_PNG.to_string(), 1.0);
            }
            // Java: a JSON `null` is neither a primitive, an array nor an
            // object — no mask is added at all (the weighted map stays empty
            // and the pick below fails).
            Some(Value::Null) => {}
            Some(Value::Array(items)) => {
                if items.is_empty() {
                    builder.put(FILE_MASK_PNG.to_string(), 1.0);
                } else {
                    for item in items.iter() {
                        builder.put(JsonLoaderBase::get_as_string(item), 1.0);
                    }
                }
            }
            Some(Value::Object(object)) => {
                if object.is_empty() {
                    builder.put(FILE_MASK_PNG.to_string(), 1.0);
                } else {
                    for (key, value) in object {
                        builder.put(key.clone(), JsonLoaderBase::get_as_double(value));
                    }
                }
            }
            Some(primitive) => {
                builder.put(JsonLoaderBase::get_as_string(primitive), 1.0);
            }
        }

        let weighted_map = builder.build();
        let mut random = FastRandom::new(self.seed.hash_code() as i64);
        let mask_name = match weighted_map.get_random(&mut random) {
            Some(name) => name.clone(),
            None => panic!(
                "Cannot invoke \"PathUtil.resolvePathWithinDir(Path, String)\" because \"maskName\" is null"
            ),
        };

        let mask_file = match resolve_path_within_dir(&self.data_folder, &mask_name) {
            Some(file) => file,
            None => panic!("Invalid mask file path: {mask_name}"),
        };

        if file_name_of(&mask_file).ends_with(FILE_MASK_JSON) {
            return LoadedMaskProvider::Climate(
                ClimateMaskJsonLoader::new(self.seed.clone(), &self.data_folder, &mask_file).load(),
            );
        }

        LoadedMaskProvider::Image(
            MaskProviderJsonLoader::new(
                self.seed.clone(),
                &self.data_folder,
                world_json.get(KEY_RANDOMIZER).cloned(),
                mask_file,
                world_size,
                world_offset,
            )
            .load(),
        )
    }

    /// Java `PrefabStoreRoot loadPrefabStore(JsonObject worldJson)`.
    pub fn load_prefab_store(&self, world_json: &Value) -> PrefabStoreRoot {
        if let Some(store_json) = world_json.get(KEY_PREFAB_STORE) {
            let is_string = matches!(store_json, Value::String(_));
            if !is_string {
                panic!("Expected 'PrefabStore' to be a string");
            }

            let store = JsonLoaderBase::get_as_string(store_json);
            // Java: `PrefabStoreRoot.valueOf(store)` — an unknown name is an
            // `IllegalArgumentException`, wrapped into
            // `new Error("Invalid PrefabStore name: " + store, e)`.
            return match PrefabStoreRoot::try_value_of(&store) {
                Some(store_root) => store_root,
                None => panic!("Invalid PrefabStore name: {store}"),
            };
        }

        PrefabStoreRoot::DEFAULT
    }
}

/// Java `PathUtil.resolvePathWithinDir(Path directory, String relativePath)`.
fn resolve_path_within_dir(directory: &Path, relative_path: &str) -> Option<PathBuf> {
    let resolved = normalize_path(&directory.join(relative_path));
    if !is_child_of(directory, &resolved) {
        return None;
    }
    Some(resolved)
}

/// Java `PathUtil.isChildOf(Path parent, Path child)`.
fn is_child_of(parent: &Path, child: &Path) -> bool {
    let parent = normalize_path(&absolute_path(parent));
    let child = normalize_path(&absolute_path(child));
    child.starts_with(parent)
}

/// Java `Path.toAbsolutePath()` — the path is made absolute without
/// normalizing it.
fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(dir) => dir.join(path),
            Err(_) => path.to_path_buf(),
        }
    }
}

/// Java `Path.normalize()` — the lexical normalization (`.` and `..` are
/// resolved without touching the file system).
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let pop = matches!(out.components().next_back(), Some(Component::Normal(_)));
                if pop {
                    out.pop();
                } else if !matches!(out.components().next_back(), Some(Component::RootDir)) {
                    out.push(component.as_os_str());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Java `Path.getFileName()` — the last name element (as a string).
fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Java `Files.isReadable(Path)`.
fn is_readable(path: &Path) -> bool {
    std::fs::File::open(path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extracts the message of a caught panic.
    fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
        if let Some(message) = payload.downcast_ref::<String>() {
            return message.clone();
        }
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            return (*message).to_string();
        }
        "<non-string panic>".to_string()
    }

    fn loader(root: &Path) -> ChunkGeneratorJsonLoader {
        ChunkGeneratorJsonLoader::new(
            SeedString::new("0"),
            WorldGenConfig::new(
                root.to_path_buf(),
                "Default".to_string(),
                "0.0.0".to_string(),
            ),
        )
    }

    fn default_root() -> PathBuf {
        PathBuf::from(format!(
            "{}/../../testdata/Default",
            env!("CARGO_MANIFEST_DIR")
        ))
    }

    #[test]
    fn world_json_is_loaded() {
        let root = default_root();
        let loader = loader(&root);
        let world_json = loader.load_world_json(&root.join(FILE_WORLD_JSON));
        assert_eq!(world_json["Width"], serde_json::json!(1));
        assert_eq!(world_json["Height"], serde_json::json!(1));
        assert_eq!(world_json["Masks"], serde_json::json!(["Mask.json"]));
    }

    #[test]
    fn world_size_and_offset_are_read() {
        let root = default_root();
        let loader = loader(&root);
        let world_json = loader.load_world_json(&root.join(FILE_WORLD_JSON));
        assert_eq!(loader.load_world_size(&world_json), (1, 1));
        assert_eq!(loader.load_world_offset(&world_json), (0, 0));
    }

    #[test]
    fn prefab_store_is_read() {
        let root = default_root();
        let loader = loader(&root);
        let world_json = loader.load_world_json(&root.join(FILE_WORLD_JSON));
        assert_eq!(
            loader.load_prefab_store(&world_json),
            PrefabStoreRoot::Assets
        );
        // Without the key the default is used.
        assert_eq!(
            loader.load_prefab_store(&serde_json::json!({})),
            PrefabStoreRoot::DEFAULT
        );
    }

    #[test]
    fn invalid_prefab_store_panics() {
        let root = default_root();
        let loader = loader(&root);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader.load_prefab_store(&serde_json::json!({ "PrefabStore": 42 }))
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert_eq!(message, "Expected 'PrefabStore' to be a string");
    }

    #[test]
    fn masks_pick_resolves_to_the_climate_branch() {
        let root = default_root();
        let loader = loader(&root);
        let world_json = loader.load_world_json(&root.join(FILE_WORLD_JSON));
        let provider = loader.load_mask_provider(&world_json, (1, 1), (0, 0));
        assert!(provider.as_climate().is_some());
        assert!(provider.as_image().is_none());
    }

    #[test]
    fn absent_masks_fall_back_to_the_png() {
        let root = default_root();
        let loader = loader(&root);
        // No "Masks" key -> "Mask.png" (which does not exist in the Default
        // world, so the image branch fails while loading the file).
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader.load_mask_provider(&serde_json::json!({}), (1, 1), (0, 0))
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert!(
            message.starts_with("Error while loading MaskProvider in "),
            "{message}"
        );
    }

    #[test]
    fn override_folder_must_exist_inside_the_parent() {
        let root = default_root();
        let loader = loader(&root);
        // The override folder does not exist -> the Java error.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader.load_override_data_folder_path(
                &serde_json::json!({ "OverrideDataFolder": "Missing" }),
                &root,
            )
        }));
        let message = match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        };
        assert!(
            message.starts_with("Override folder 'Missing' must exist within: '"),
            "{message}"
        );
    }

    #[test]
    fn normalize_path_resolves_dot_segments() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(
            normalize_path(Path::new("a/../../b")),
            PathBuf::from("../b")
        );
    }

    /// The message of the Java "the mask image could not be loaded" error.
    fn mask_image_error(world_json: &Value) -> String {
        let root = default_root();
        let loader = loader(&root);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loader.load_mask_provider(world_json, (1, 1), (0, 0))
        }));
        match result {
            Ok(_) => panic!("expected a panic"),
            Err(payload) => panic_message(payload),
        }
    }

    #[test]
    fn the_masks_string_form_resolves_to_the_png() {
        let message = mask_image_error(&serde_json::json!({ "Masks": "Mask.png" }));
        assert!(
            message.contains("Mask.png"),
            "the string form must resolve to Mask.png: {message}"
        );
    }

    #[test]
    fn an_empty_masks_array_falls_back_to_the_png() {
        let message = mask_image_error(&serde_json::json!({ "Masks": [] }));
        assert!(
            message.contains("Mask.png"),
            "an empty array must fall back to Mask.png: {message}"
        );
    }

    #[test]
    fn an_empty_masks_object_falls_back_to_the_png() {
        let message = mask_image_error(&serde_json::json!({ "Masks": {} }));
        assert!(
            message.contains("Mask.png"),
            "an empty object must fall back to Mask.png: {message}"
        );
    }

    #[test]
    fn a_masks_object_resolves_to_its_key() {
        let message = mask_image_error(&serde_json::json!({ "Masks": { "Mask.png": 0.5 } }));
        assert!(
            message.contains("Mask.png"),
            "the object form must resolve to its key: {message}"
        );
    }

    #[test]
    fn a_multi_entry_masks_array_picks_deterministically() {
        let root = default_root();
        let loader = loader(&root);
        let world_json = serde_json::json!({ "Masks": ["Mask.json", "Mask.png"] });

        // The pick consumes one `nextDouble()` from the FastRandom seeded
        // with the seed string hash — it is stable across calls.
        let first = loader.load_mask_provider(&world_json, (1, 1), (0, 0));
        let second = loader.load_mask_provider(&world_json, (1, 1), (0, 0));
        assert_eq!(
            first.as_climate().is_some(),
            second.as_climate().is_some(),
            "the weighted pick must be deterministic"
        );
    }
}
