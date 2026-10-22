//! Load or create a world `config.json` and save it with the Java-compatible
//! codec.
//!
//! This is the metadata counterpart of `load-chunk`: it starts from an existing
//! Java config (or Java defaults), writes `config.json` into a save directory,
//! and lets a Java probe read the result.

use std::path::PathBuf;

use hytale_world::world_config::{WorldConfig, WorldConfigError, WorldUuid, WORLD_CONFIG_VERSION};
use thiserror::Error;

/// Paths and overrides for the world-config tool.
#[derive(Debug, Clone)]
pub struct WorldConfigToolOptions {
    /// Optional source `config.json` to load and re-save.
    pub input: Option<PathBuf>,
    /// World save root; the config is written to `<save>/config.json`.
    pub save_dir: PathBuf,
    /// Seed used only when a new config is created.
    pub seed: i64,
    /// Display name used only when a new config is created.
    pub display_name: Option<String>,
    /// UUID used only when a new config is created.
    pub uuid: Option<String>,
}

/// What the world-config tool produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldConfigToolReport {
    /// Written file.
    pub path: PathBuf,
    /// Codec version.
    pub version: i32,
    /// World UUID in canonical form.
    pub uuid: String,
    /// Seed.
    pub seed: i64,
    /// Player-facing name.
    pub display_name: Option<String>,
    /// True when an existing document was loaded instead of creating defaults.
    pub loaded_existing: bool,
}

/// Errors from the world-config tool.
#[derive(Debug, Error)]
pub enum WorldConfigToolError {
    /// The config codec failed.
    #[error(transparent)]
    Config(#[from] WorldConfigError),
    /// The requested UUID is malformed.
    #[error("invalid world UUID `{0}`")]
    InvalidUuid(String),
}

/// Load an existing config or create Java-compatible defaults, then save it.
pub fn run(
    options: &WorldConfigToolOptions,
) -> Result<WorldConfigToolReport, WorldConfigToolError> {
    let output_path = WorldConfig::path_in(&options.save_dir);

    let (mut config, loaded_existing) = if let Some(input) = &options.input {
        (WorldConfig::load(input)?, true)
    } else if output_path.is_file() {
        (WorldConfig::load(&output_path)?, true)
    } else {
        let uuid = match &options.uuid {
            Some(value) => value
                .parse()
                .map_err(|_| WorldConfigToolError::InvalidUuid(value.clone()))?,
            None => WorldUuid::derive(
                options.seed,
                options.display_name.as_deref().unwrap_or("Default"),
            ),
        };
        (
            WorldConfig::new(uuid, options.seed, options.display_name.clone()),
            false,
        )
    };

    // A caller-supplied UUID replaces the loaded one; this is how the tool
    // creates a specific world id without editing JSON.
    if let Some(value) = &options.uuid {
        if loaded_existing {
            config.uuid = value
                .parse()
                .map_err(|_| WorldConfigToolError::InvalidUuid(value.clone()))?;
        }
    }

    config.version = WORLD_CONFIG_VERSION;
    config.save(&output_path)?;

    Ok(WorldConfigToolReport {
        path: output_path,
        version: config.version,
        uuid: config.uuid.to_string(),
        seed: config.seed,
        display_name: config.display_name.clone(),
        loaded_existing,
    })
}
