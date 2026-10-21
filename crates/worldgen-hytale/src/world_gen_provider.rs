// Ported from: `.hytale-source/.../worldgen/HytaleWorldGenProvider.java`
//! Java `HytaleWorldGenProvider` — the world-config semantics (`Name`,
//! `Version`, `Path`) and the config-to-generator load path.
//!
//! The Java provider is decoded by the CoreServer `Codec` framework:
//! `Name` defaults to `"Default"`, `Version` to `0.0.0` and `Path` is an
//! optional string. `getGenerator()` resolves the path (the default world-gen
//! folder, the configured `Path` or the `<Path>/<Name>` child), applies the
//! legacy "World.json in the configured folder" rule and boots the JSON
//! loader chain with the constant `"ChunkGenerator"` seed label.
//!
//! The Rust port keeps the same semantics without the CoreServer plugin
//! lifecycle (`REMAINING.md` §3): [`HytaleWorldGenProvider::load`] returns the
//! [`ChunkGenerator`] directly.

use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;

use crate::chunk::chunk_generator::ChunkGenerator;
use crate::loader::chunk_generator_json_loader::ChunkGeneratorJsonLoader;
use crate::world_gen_config::WorldGenConfig;

/// Java `HytaleWorldGenProvider.ID`.
pub const ID: &str = "Hytale";

/// Java `HytaleWorldGenProvider.DEFAULT_NAME`.
pub const DEFAULT_NAME: &str = "Default";

/// Java `HytaleWorldGenProvider.MIN_VERSION`.
pub const MIN_VERSION: &str = "0.0.0";

/// Java `ChunkGeneratorJsonLoader.Constants.FILE_WORLD_JSON`.
pub const FILE_WORLD_JSON: &str = "World.json";

/// The load/validation errors (Java `WorldGenLoadException`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldGenLoadError {
    /// The Java-style message.
    pub message: String,
}

impl WorldGenLoadError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WorldGenLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for WorldGenLoadError {}

/// Java `common.semver.Semver` — the subset the provider config needs
/// (`major.minor.patch`). The Java codec accepts the full semver grammar;
/// the Rust port only stores/validates the numeric triple (the value is only
/// carried through to `WorldGenConfig` and logged).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Semver {
    /// Major version.
    pub major: u64,
    /// Minor version.
    pub minor: u64,
    /// Patch version.
    pub patch: u64,
}

impl Semver {
    /// The Java `new Semver(0, 0, 0)`.
    pub const MIN: Semver = Semver {
        major: 0,
        minor: 0,
        patch: 0,
    };

    /// Parses `major.minor.patch` (the Java codec rejects anything else).
    pub fn parse(text: &str) -> Result<Self, WorldGenLoadError> {
        let mut parts = text.split('.');
        let parse = |part: &str| {
            part.parse::<u64>()
                .map_err(|_| WorldGenLoadError::new(format!("Invalid version: {text}")))
        };
        let (major, minor, patch) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(major), Some(minor), Some(patch), None) => {
                (parse(major)?, parse(minor)?, parse(patch)?)
            }
            _ => return Err(WorldGenLoadError::new(format!("Invalid version: {text}"))),
        };
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for Semver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Java `HytaleWorldGenProvider` — the decoded world-generator config.
#[derive(Debug, Clone)]
pub struct HytaleWorldGenProvider {
    /// Java `private String name`.
    pub name: String,
    /// Java `private Semver version`.
    pub version: Semver,
    /// Java `@Nullable private String path`.
    pub path: Option<String>,
}

impl Default for HytaleWorldGenProvider {
    fn default() -> Self {
        Self {
            name: DEFAULT_NAME.to_string(),
            version: Semver::MIN,
            path: None,
        }
    }
}

impl HytaleWorldGenProvider {
    /// Decodes the provider config from the CoreServer codec JSON shape
    /// (`{ "Name": ..., "Version": ..., "Path": ... }`; every field is
    /// optional).
    pub fn from_json(value: &Value) -> Result<Self, WorldGenLoadError> {
        let mut provider = Self::default();
        if let Some(name) = value.get("Name") {
            provider.name = name
                .as_str()
                .ok_or_else(|| WorldGenLoadError::new("The Name field must be a string"))?
                .to_string();
        }
        if let Some(version) = value.get("Version") {
            provider.version =
                Semver::parse(version.as_str().ok_or_else(|| {
                    WorldGenLoadError::new("The Version field must be a string")
                })?)?;
        }
        if let Some(path) = value.get("Path") {
            provider.path = Some(
                path.as_str()
                    .ok_or_else(|| WorldGenLoadError::new("The Path field must be a string"))?
                    .to_string(),
            );
        }
        Ok(provider)
    }

    /// Java `getGenerator()`'s path resolution:
    ///
    /// * `Path` wins over `default_world_gen_path`; a lexical traversal guard
    ///   replaces the CoreServer `PathUtil.isInTrustedRoot` check (the Rust
    ///   port has no trusted-root registry — document this deviation).
    /// * Legacy: when the name is `"Default"` and the resolved folder already
    ///   contains `World.json`, the folder itself is the world-gen folder;
    ///   otherwise `<folder>/<Name>` is used.
    pub fn resolve_path(
        &self,
        default_world_gen_path: &Path,
    ) -> Result<PathBuf, WorldGenLoadError> {
        let mut world_gen_path = match &self.path {
            Some(path) => guarded_path(Path::new(path)).ok_or_else(|| {
                WorldGenLoadError::new(format!(
                    "World gen path must be within a trusted directory: {path}"
                ))
            })?,
            None => default_world_gen_path.to_path_buf(),
        };

        if self.name != DEFAULT_NAME || !world_gen_path.join(FILE_WORLD_JSON).exists() {
            world_gen_path = resolve_within_dir(&world_gen_path, &self.name).ok_or_else(|| {
                WorldGenLoadError::new(format!("Invalid world gen name: {}", self.name))
            })?;
        }
        Ok(world_gen_path)
    }

    /// Java `getGenerator()` — resolves the path and loads the full
    /// `ChunkGenerator` chain with the provider's `"ChunkGenerator"` seed
    /// label.
    pub fn load(
        &self,
        default_world_gen_path: &Path,
    ) -> Result<Rc<ChunkGenerator>, WorldGenLoadError> {
        let path = self.resolve_path(default_world_gen_path)?;
        let config = WorldGenConfig::new(path, self.name.clone(), self.version.to_string());
        let loader = ChunkGeneratorJsonLoader::new(
            procedural::json::SeedString::new("ChunkGenerator"),
            config,
        );
        Ok(loader.load())
    }
}

/// Lexically normalizes a path and rejects `..` traversal. Absolute paths stay
/// absolute; relative paths stay relative (the caller decides the base).
fn guarded_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::CurDir => {}
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    Some(normalized)
}

/// Java `PathUtil.resolvePathWithinDir(dir, name)` — resolves `name` inside
/// `dir` and rejects a path that escapes it.
fn resolve_within_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let candidate = guarded_path(&dir.join(name))?;
    let base = guarded_path(dir)?;
    if candidate.starts_with(&base) {
        Some(candidate)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_match_the_java_codec() {
        let provider = HytaleWorldGenProvider::default();
        assert_eq!(provider.name, DEFAULT_NAME);
        assert_eq!(provider.version, Semver::MIN);
        assert_eq!(provider.path, None);
    }

    #[test]
    fn the_codec_reads_name_version_and_path() {
        let provider = HytaleWorldGenProvider::from_json(&serde_json::json!({
            "Name": "MyWorld",
            "Version": "1.2.3",
            "Path": "/tmp/worldgen"
        }))
        .expect("a valid provider config");
        assert_eq!(provider.name, "MyWorld");
        assert_eq!(provider.version.to_string(), "1.2.3");
        assert_eq!(provider.path.as_deref(), Some("/tmp/worldgen"));
    }

    #[test]
    fn an_invalid_version_is_rejected() {
        let error = HytaleWorldGenProvider::from_json(&serde_json::json!({
            "Version": "1.2"
        }))
        .expect_err("two components are not a semver");
        assert_eq!(error.message, "Invalid version: 1.2");
    }

    #[test]
    fn a_traversal_path_is_rejected() {
        let provider = HytaleWorldGenProvider {
            path: Some("../../etc".to_string()),
            ..Default::default()
        };
        let error = provider
            .resolve_path(Path::new("/tmp/default"))
            .expect_err("the traversal must be rejected");
        assert!(error
            .message
            .starts_with("World gen path must be within a trusted directory"));
    }

    #[test]
    fn a_name_traversal_is_rejected() {
        let provider = HytaleWorldGenProvider {
            name: "../outside".to_string(),
            ..Default::default()
        };
        let error = provider
            .resolve_path(Path::new("/tmp/default"))
            .expect_err("the name traversal must be rejected");
        assert_eq!(error.message, "Invalid world gen name: ../outside");
    }

    #[test]
    fn the_legacy_world_json_rule_keeps_the_folder() {
        let root =
            std::env::temp_dir().join(format!("worldgen-provider-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create the probe folder");
        std::fs::write(root.join(FILE_WORLD_JSON), "{}").expect("write World.json");
        let provider = HytaleWorldGenProvider::default();
        let resolved = provider.resolve_path(&root).expect("the legacy rule");
        assert_eq!(resolved, root);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_named_folder_is_appended_without_world_json() {
        let root =
            std::env::temp_dir().join(format!("worldgen-provider-named-{}", std::process::id()));
        let provider = HytaleWorldGenProvider {
            name: "MyWorld".to_string(),
            ..Default::default()
        };
        let resolved = provider.resolve_path(&root).expect("the named folder");
        assert_eq!(resolved, root.join("MyWorld"));
    }
}
