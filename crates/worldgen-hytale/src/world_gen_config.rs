// Ported from: `.hytale-source/.../worldgen/WorldGenConfig.java`
//! Java `worldgen.WorldGenConfig` (record).
//!
//! Java `common.semver.Semver` is only used for asset-pack logging in the
//! load path, so the version is carried as its display string here.

use std::path::{Path, PathBuf};

/// Java `WorldGenConfig(Path path, String name, Semver version)`.
#[derive(Clone, Debug)]
pub struct WorldGenConfig {
    /// Java `@Nonnull Path path` — the world-gen data folder.
    pub path: PathBuf,
    /// Java `@Nonnull String name`.
    pub name: String,
    /// Java `@Nonnull Semver version` — display string (only logged).
    pub version: String,
}

impl WorldGenConfig {
    /// Java `WorldGenConfig(Path path, String name, Semver version)`.
    pub fn new(path: PathBuf, name: String, version: String) -> Self {
        Self {
            path,
            name,
            version,
        }
    }

    /// Java `WorldGenConfig withOverride(Path path)` — a new config with the
    /// override data folder (identity when unchanged, mirroring the Java
    /// `FileIO.equals` shortcut).
    pub fn with_override(&self, path: &Path) -> WorldGenConfig {
        if self.path == path {
            self.clone()
        } else {
            WorldGenConfig {
                path: path.to_path_buf(),
                name: self.name.clone(),
                version: self.version.clone(),
            }
        }
    }
}
