// Ported from: `.hytale-source/.../CoreServer/.../prefab/PrefabStore.java`,
// `.hytale-source/.../CoreServer/.../prefab/selection/buffer/PrefabLoader.java`
//! and `.hytale-source/.../worldgen/prefab/PrefabLoadingCache.java`
//! Java `com.hypixel.hytale.server.core.prefab.PrefabStore` — the store that
//! resolves a prefab *name* into the prefab *file* and the prefab *buffer*.
//!
//! # What the Java does
//!
//! * [`PrefabStore`] owns the prefab folders (the asset packs and the
//!   world-gen folder) and resolves a prefab key into a path,
//! * [`PrefabLoader`] turns a prefab name into the file paths it matches
//!   (a name may match several files — the `".*"` wildcard names match every
//!   prefab of a folder),
//! * [`PrefabLoadingCache`] caches the loaded prefab buffers.
//!
//! # Deviations
//!
//! * **The `.lpf` binary format is not ported** (`REMAINING.md` § 5) — the
//!   JSON is the source of truth. The vanilla asset pack ships no `.lpf`
//!   file, so nothing is lost.
//! * **The store has a single root** — the Java store searches every asset
//!   pack; the Rust port searches the one root the world generator was
//!   configured with.
//! * **The caches are unbounded** — the Java name cache expires after 30
//!   seconds and the buffer cache is a plain map; the Rust port keeps the
//!   same maps (the prefab set of a world is finite).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use worldgen_model::registry::Registries;

use crate::prefab::buffer::prefab_buffer::PrefabBuffer;
use crate::prefab::prefab_json::parse_prefab_json;

/// Java `PrefabBufferUtil.JSON_FILE_SUFFIX` = `".prefab.json"`.
pub const JSON_FILE_SUFFIX: &str = ".prefab.json";
/// Java `PrefabBufferUtil.LPF_FILE_SUFFIX` = `".lpf"`.
pub const LPF_FILE_SUFFIX: &str = ".lpf";
/// Java `PrefabBufferUtil.JSON_LPF_FILE_SUFFIX` = `".prefab.json.lpf"`.
pub const JSON_LPF_FILE_SUFFIX: &str = ".prefab.json.lpf";

/// The prefab store of a world.
pub struct PrefabStore {
    /// Java `PrefabStore.getAssetPrefabsPath()` / the world-gen prefab
    /// folder — the root of the store.
    root: PathBuf,
    /// The block / fluid / environment registries the prefab names resolve
    /// against.
    registries: Rc<Registries>,
    /// Java `PrefabLoadingCache.cache` — the loaded prefab buffers, keyed by
    /// their path (the Java keys them by the supplier, whose `equals` is
    /// the path).
    buffers: RefCell<HashMap<PathBuf, Rc<PrefabBuffer>>>,
    /// The resolved prefab names (the Java `WorldGenPrefabLoader` cache).
    names: RefCell<HashMap<String, Rc<Vec<PathBuf>>>>,
}

impl PrefabStore {
    /// Java `new PrefabStore()` with the given root folder.
    pub fn new(root: PathBuf, registries: Rc<Registries>) -> Self {
        Self {
            root,
            registries,
            buffers: RefCell::new(HashMap::new()),
            names: RefCell::new(HashMap::new()),
        }
    }

    /// The root folder of the store.
    pub fn get_root_folder(&self) -> &Path {
        &self.root
    }

    /// The registries the prefab names resolve against.
    pub fn get_registries(&self) -> &Rc<Registries> {
        &self.registries
    }

    /// Java `PrefabLoader.resolvePrefabs(rootFolder, prefabName,
    /// pathConsumer)` — the paths the name matches.
    pub fn resolve(&self, prefab_name: &str) -> Rc<Vec<PathBuf>> {
        if let Some(cached) = self.names.borrow().get(prefab_name) {
            return Rc::clone(cached);
        }
        let mut paths = Vec::new();
        if prefab_name.ends_with(".*") {
            self.resolve_prefab_folder(prefab_name, &mut paths);
        } else {
            let relative_path = prefab_name.replace('.', "/");
            for suffix in [LPF_FILE_SUFFIX, JSON_FILE_SUFFIX] {
                let prefab_path = self.root.join(format!("{relative_path}{suffix}"));
                if !is_child_of(&self.root, &prefab_path) {
                    panic!("Invalid prefab name: {prefab_name}");
                }
                if prefab_path.exists() {
                    paths.push(prefab_path);
                    return Rc::new(paths);
                }
            }
        }
        let paths = Rc::new(paths);
        self.names
            .borrow_mut()
            .insert(prefab_name.to_string(), Rc::clone(&paths));
        paths
    }

    /// Java `PrefabLoader.resolvePrefabFolder(rootFolder, prefabName,
    /// pathConsumer)`.
    fn resolve_prefab_folder(&self, prefab_name: &str, paths: &mut Vec<PathBuf>) {
        let prefab_directory = &prefab_name[..prefab_name.len() - 2];
        let directory_path = self.root.join(prefab_directory.replace('.', "/"));
        if !is_child_of(&self.root, &directory_path) {
            panic!("Invalid prefab name: {prefab_name}");
        }
        if !directory_path.exists() {
            return;
        }
        if !directory_path.is_dir() {
            panic!("{}", directory_path.display());
        }
        let mut files = Vec::new();
        collect_prefab_files(&directory_path, &mut files);
        for file in files {
            let name = file
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let stripped = strip_prefab_suffix(&name);
            paths.push(
                file.parent()
                    .map(|parent| parent.join(&stripped))
                    .unwrap_or(file),
            );
        }
    }

    /// Java `PrefabBufferUtil.loadBuffer(path)` — the prefab buffer of a
    /// prefab file (cached).
    pub fn load(&self, path: &Path) -> Rc<PrefabBuffer> {
        if let Some(cached) = self.buffers.borrow().get(path) {
            return Rc::clone(cached);
        }
        let buffer = Rc::new(self.load_uncached(path));
        self.buffers
            .borrow_mut()
            .insert(path.to_path_buf(), Rc::clone(&buffer));
        buffer
    }

    /// Java `PrefabBufferUtil.loadBuffer(path)` — without the cache.
    fn load_uncached(&self, path: &Path) -> PrefabBuffer {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let stripped = strip_prefab_suffix(&file_name);

        // Java: a sibling `.lpf` wins over the `.prefab.json`; the binary
        // format is not ported, so the JSON is read.
        let json_path = path
            .parent()
            .map(|parent| parent.join(format!("{stripped}{JSON_FILE_SUFFIX}")))
            .unwrap_or_else(|| path.to_path_buf());
        if !json_path.exists() {
            panic!(
                "Error loading Prefab from {} (.lpf and .prefab.json) File NOT found!",
                json_path.display()
            );
        }
        let text = std::fs::read_to_string(&json_path).unwrap_or_else(|error| {
            panic!(
                "Error while loading Prefab from {}: {error}",
                json_path.display()
            )
        });
        let document: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|error| {
            panic!(
                "Error while loading Prefab from {}: {error}",
                json_path.display()
            )
        });
        let (buffer, _stats) = parse_prefab_json(&document, &self.registries);
        buffer
    }
}

/// Java `PrefabStore.stripPrefabSuffix(String fileName)`.
pub fn strip_prefab_suffix(file_name: &str) -> String {
    if let Some(stripped) = file_name.strip_suffix(JSON_FILE_SUFFIX) {
        return stripped.to_string();
    }
    if is_prefab_file_name(file_name) {
        return file_name
            .strip_suffix(LPF_FILE_SUFFIX)
            .unwrap_or(file_name)
            .to_string();
    }
    file_name.to_string()
}

/// Java `PrefabStore.isPrefabFileName(String fileName)`.
pub fn is_prefab_file_name(file_name: &str) -> bool {
    file_name.ends_with(JSON_FILE_SUFFIX)
        || (file_name.ends_with(LPF_FILE_SUFFIX) && !file_name.ends_with(JSON_LPF_FILE_SUFFIX))
}

/// Java `PathUtil.isChildOf(Path parent, Path child)`.
fn is_child_of(parent: &Path, child: &Path) -> bool {
    let parent = normalize_path(&absolute_path(parent));
    let child = normalize_path(&absolute_path(child));
    child.starts_with(parent)
}

/// Java `Path.toAbsolutePath()`.
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

/// Java `Path.normalize()`.
fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let pop = matches!(
                    out.components().next_back(),
                    Some(std::path::Component::Normal(_))
                );
                if pop {
                    out.pop();
                } else if !matches!(
                    out.components().next_back(),
                    Some(std::path::Component::RootDir)
                ) {
                    out.push(component.as_os_str());
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Java `PrefabLoader.resolvePrefabFolder` — the file tree walk that
/// collects the prefab files of a folder.
fn collect_prefab_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_prefab_files(&path, files);
        } else if let Some(name) = path.file_name() {
            let name = name.to_string_lossy().to_string();
            if is_prefab_file_name(&name) {
                files.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefab_suffix_is_stripped() {
        assert_eq!(strip_prefab_suffix("Foo.prefab.json"), "Foo");
        assert_eq!(strip_prefab_suffix("Foo.lpf"), "Foo");
        // The generated cache file is not a prefab file — the name is kept.
        assert_eq!(
            strip_prefab_suffix("Foo.prefab.json.lpf"),
            "Foo.prefab.json.lpf"
        );
        assert_eq!(strip_prefab_suffix("Foo"), "Foo");
    }

    #[test]
    fn the_prefab_file_names_are_recognised() {
        assert!(is_prefab_file_name("Foo.prefab.json"));
        assert!(is_prefab_file_name("Foo.lpf"));
        // The generated cache file is not a prefab.
        assert!(!is_prefab_file_name("Foo.prefab.json.lpf"));
        assert!(!is_prefab_file_name("Foo.json"));
    }
}
