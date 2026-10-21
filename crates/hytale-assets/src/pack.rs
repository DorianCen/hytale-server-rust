//! Asset pack VFS: directory and zip packs with safe, case-insensitive lookup.
//!
//! Ported behavior from
//! `AssetStore/src/main/java/com/hypixel/hytale/assetstore/AssetPack.java`.
//! A pack exposes relative paths only; absolute paths and `..` components are
//! rejected before any filesystem access.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Errors produced by the asset layer.
#[derive(Debug, Error)]
pub enum AssetError {
    /// Filesystem I/O failed.
    #[error("{path}: {source}")]
    Io {
        /// Path being accessed.
        path: PathBuf,
        /// Source error.
        #[source]
        source: std::io::Error,
    },
    /// Zip handling failed.
    #[error("{path}: {message}")]
    Zip {
        /// Zip path.
        path: PathBuf,
        /// What went wrong.
        message: String,
    },
    /// A path tried to escape the pack root.
    #[error("unsafe asset path: {0}")]
    UnsafePath(String),
    /// The requested asset does not exist.
    #[error("asset not found: {0}")]
    NotFound(String),
    /// A `Parent` reference does not exist.
    #[error("missing parent `{parent}` while resolving {asset}; chain: {chain}")]
    MissingParent {
        /// Missing key.
        parent: String,
        /// Asset being resolved.
        asset: String,
        /// Visited chain.
        chain: String,
    },
    /// A `Parent` chain contains a cycle.
    #[error("parent cycle: {chain}")]
    ParentCycle {
        /// Chain including the repeated key.
        chain: String,
    },
    /// An asset JSON document is invalid.
    #[error("{key}: invalid JSON: {source}")]
    Json {
        /// Asset key.
        key: String,
        /// Serde error.
        #[source]
        source: serde_json::Error,
    },
    /// Store ordering failed.
    #[error("store order cycle: {cycle}")]
    StoreCycle {
        /// Nodes participating in the cycle.
        cycle: String,
    },
}

type Result<T> = std::result::Result<T, AssetError>;

/// One asset file discovered in a pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetKey {
    /// Lookup key (file stem by default).
    pub key: String,
    /// Pack-relative source path.
    pub source: String,
    /// Container key when the asset is embedded in another asset.
    pub container: Option<String>,
}

impl AssetKey {
    /// Build a key from a pack-relative path.
    pub fn from_source(source: &str) -> Self {
        let stem = source
            .rsplit('/')
            .next()
            .unwrap_or(source)
            .strip_suffix(".json")
            .unwrap_or_else(|| source.rsplit('/').next().unwrap_or(source));
        Self {
            key: stem.to_string(),
            source: source.to_string(),
            container: None,
        }
    }

    /// Mark this asset as embedded in a container asset with `container_key`.
    pub fn in_container(mut self, container_key: &str) -> Self {
        self.container = Some(container_key.to_string());
        self
    }

    /// Key used by the store. Embedded assets inherit the container key.
    pub fn effective_key(&self) -> &str {
        self.container.as_deref().unwrap_or(&self.key)
    }
}

/// A read-only asset pack backed by a directory or a `.zip`.
#[derive(Debug, Clone)]
pub struct AssetPack {
    root: PathBuf,
    is_zip: bool,
}

impl AssetPack {
    /// Open a directory or zip pack.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let root = path.into();
        let metadata = std::fs::metadata(&root).map_err(|source| AssetError::Io {
            path: root.clone(),
            source,
        })?;
        let is_zip = if metadata.is_dir() {
            false
        } else if root
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
        {
            true
        } else {
            return Err(AssetError::Zip {
                path: root,
                message: "pack is neither a directory nor a .zip".to_string(),
            });
        };
        Ok(Self { root, is_zip })
    }

    /// Pack root.
    pub fn path(&self) -> &Path {
        &self.root
    }

    /// True for a zip pack.
    pub fn is_zip(&self) -> bool {
        self.is_zip
    }

    /// List all files as sorted pack-relative `/` paths.
    pub fn list(&self) -> Result<Vec<String>> {
        if self.is_zip {
            self.list_zip()
        } else {
            self.list_directory()
        }
    }

    /// List `.json` assets as keys.
    pub fn list_keys(&self) -> Result<Vec<AssetKey>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|path| path.ends_with(".json"))
            .map(|path| AssetKey::from_source(&path))
            .collect())
    }

    /// Read a file by exact pack-relative path.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let path = validate_path(path)?;
        if self.is_zip {
            self.read_zip(&path)
        } else {
            let full = self
                .root
                .join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            std::fs::read(&full).map_err(|source| AssetError::Io { path: full, source })
        }
    }

    /// Find a file by case-insensitive path.
    pub fn find_case_insensitive(&self, path: &str) -> Result<Option<String>> {
        let needle = path.to_ascii_lowercase();
        Ok(self
            .list()?
            .into_iter()
            .find(|candidate| candidate.to_ascii_lowercase() == needle))
    }

    /// Read a file by exact path or, failing that, case-insensitively.
    pub fn read_case_insensitive(&self, path: &str) -> Result<Option<Vec<u8>>> {
        match self.read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(AssetError::Io { .. } | AssetError::NotFound(_)) => {
                match self.find_case_insensitive(path)? {
                    Some(actual) => Ok(Some(self.read(&actual)?)),
                    None => Ok(None),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn list_directory(&self) -> Result<Vec<String>> {
        fn walk(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<()> {
            let entries = std::fs::read_dir(directory).map_err(|source| AssetError::Io {
                path: directory.to_path_buf(),
                source,
            })?;
            for entry in entries {
                let entry = entry.map_err(|source| AssetError::Io {
                    path: directory.to_path_buf(),
                    source,
                })?;
                let path = entry.path();
                let file_type = entry.file_type().map_err(|source| AssetError::Io {
                    path: path.clone(),
                    source,
                })?;
                if file_type.is_dir() {
                    walk(root, &path, files)?;
                } else if file_type.is_file() {
                    let relative = path
                        .strip_prefix(root)
                        .map_err(|_| AssetError::UnsafePath(path.to_string_lossy().into_owned()))?;
                    files.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
            Ok(())
        }

        let mut files = Vec::new();
        walk(&self.root, &self.root, &mut files)?;
        files.sort();
        Ok(files)
    }

    fn list_zip(&self) -> Result<Vec<String>> {
        let file = File::open(&self.root).map_err(|source| AssetError::Io {
            path: self.root.clone(),
            source,
        })?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| AssetError::Zip {
            path: self.root.clone(),
            message: error.to_string(),
        })?;
        let mut files = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let entry = archive.by_index(index).map_err(|error| AssetError::Zip {
                path: self.root.clone(),
                message: error.to_string(),
            })?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            validate_path(&name)?;
            files.push(name);
        }
        files.sort();
        Ok(files)
    }

    fn read_zip(&self, path: &str) -> Result<Vec<u8>> {
        let file = File::open(&self.root).map_err(|source| AssetError::Io {
            path: self.root.clone(),
            source,
        })?;
        let mut archive = zip::ZipArchive::new(file).map_err(|error| AssetError::Zip {
            path: self.root.clone(),
            message: error.to_string(),
        })?;
        let mut entry = archive
            .by_name(path)
            .map_err(|_| AssetError::NotFound(path.to_string()))?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|source| AssetError::Io {
                path: self.root.join(path),
                source,
            })?;
        Ok(bytes)
    }
}

fn validate_path(path: &str) -> Result<String> {
    // Normalise separators *before* walking components so `..\foo` cannot sneak
    // past a Unix `Component` check.
    let normalised = path.replace('\\', "/");
    if normalised.is_empty()
        || normalised.starts_with('/')
        || normalised.chars().nth(1) == Some(':')
    {
        return Err(AssetError::UnsafePath(path.to_string()));
    }
    let candidate = Path::new(&normalised);
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AssetError::UnsafePath(path.to_string()));
    }
    Ok(normalised)
}

/// A JSON asset provider used by the parent resolver.
pub trait ParentProvider {
    /// Load an asset by key, or `None` when it does not exist.
    fn load(&self, key: &str) -> Option<serde_json::Value>;
}

impl ParentProvider for BTreeMap<String, serde_json::Value> {
    fn load(&self, key: &str) -> Option<serde_json::Value> {
        self.get(key).cloned()
    }
}
