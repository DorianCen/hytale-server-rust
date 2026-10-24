//! Typed server configuration with deterministic layering.
//!
//! Precedence, lowest to highest:
//!
//! 1. the defaults in this module;
//! 2. an optional JSON config file;
//! 3. `HYTALE_CORE_*` environment variables;
//! 4. typed CLI overrides supplied by the binary.
//!
//! Unknown JSON fields are **rejected** (`deny_unknown_fields`) so a typo does
//! not silently fall back to a default. Unknown `HYTALE_CORE_*` environment keys
//! are rejected for the same reason; environment variables outside that prefix
//! are ignored. `ConfigError::InvalidConfig` reports fields that are present but
//! invalid (for example a zero worker count).

use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Prefix reserved for core runtime environment overrides.
pub const ENV_PREFIX: &str = "HYTALE_CORE_";

/// Documented unknown-field policy.
pub const UNKNOWN_FIELD_POLICY: &str = "Unknown JSON fields are rejected; \
unknown HYTALE_CORE_* environment variables are rejected; other environment \
variables are ignored.";

/// Log verbosity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Trace.
    Trace,
    /// Debug.
    Debug,
    /// Info (default).
    #[default]
    Info,
    /// Warn.
    Warn,
    /// Error.
    Error,
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" | "warning" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            other => Err(format!(
                "unknown log level `{other}` (expected trace, debug, info, warn, error)"
            )),
        }
    }
}

impl LogLevel {
    /// Map to the `tracing` level.
    pub fn as_tracing(self) -> tracing::Level {
        match self {
            Self::Trace => tracing::Level::TRACE,
            Self::Debug => tracing::Level::DEBUG,
            Self::Info => tracing::Level::INFO,
            Self::Warn => tracing::Level::WARN,
            Self::Error => tracing::Level::ERROR,
        }
    }
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Human-readable text.
    #[default]
    Text,
    /// One JSON object per event.
    Json,
}

impl FromStr for LogFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" | "plain" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(format!(
                "unknown log format `{other}` (expected text or json)"
            )),
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    /// Maximum level emitted.
    pub level: LogLevel,
    /// Text or JSON output.
    pub format: LogFormat,
    /// Whether ANSI colour is enabled.
    pub ansi: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            format: LogFormat::Text,
            ansi: true,
        }
    }
}

/// Runtime sizing and shutdown settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    /// Worldgen/general worker count.
    pub workers: usize,
    /// Maximum accepted connections.
    pub max_connections: usize,
    /// Graceful shutdown deadline in milliseconds.
    pub shutdown_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            workers: 1,
            max_connections: 20,
            shutdown_timeout_ms: 10_000,
        }
    }
}

impl RuntimeConfig {
    /// The shutdown deadline as a `Duration`.
    pub fn shutdown_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.shutdown_timeout_ms)
    }
}

/// Typed server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// Displayed server name.
    pub server_name: String,
    /// Message of the day.
    pub motd: String,
    /// Logging settings.
    pub log: LoggingConfig,
    /// Runtime sizing and shutdown settings.
    pub runtime: RuntimeConfig,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server_name: "Rust CoreServer".to_string(),
            motd: "dev".to_string(),
            log: LoggingConfig::default(),
            runtime: RuntimeConfig::default(),
        }
    }
}

/// CLI overrides applied after the file and environment layers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerConfigOverrides {
    /// Override `server_name`.
    pub server_name: Option<String>,
    /// Override `motd`.
    pub motd: Option<String>,
    /// Override `log.level`.
    pub log_level: Option<LogLevel>,
    /// Override `log.format`.
    pub log_format: Option<LogFormat>,
    /// Override `log.ansi`.
    pub log_ansi: Option<bool>,
    /// Override `runtime.workers`.
    pub workers: Option<usize>,
    /// Override `runtime.max_connections`.
    pub max_connections: Option<usize>,
    /// Override `runtime.shutdown_timeout_ms`.
    pub shutdown_timeout_ms: Option<u64>,
}

impl ServerConfigOverrides {
    /// Apply these overrides to `config`.
    pub fn apply(&self, config: &mut ServerConfig) {
        if let Some(value) = &self.server_name {
            config.server_name.clone_from(value);
        }
        if let Some(value) = &self.motd {
            config.motd.clone_from(value);
        }
        if let Some(value) = self.log_level {
            config.log.level = value;
        }
        if let Some(value) = self.log_format {
            config.log.format = value;
        }
        if let Some(value) = self.log_ansi {
            config.log.ansi = value;
        }
        if let Some(value) = self.workers {
            config.runtime.workers = value;
        }
        if let Some(value) = self.max_connections {
            config.runtime.max_connections = value;
        }
        if let Some(value) = self.shutdown_timeout_ms {
            config.runtime.shutdown_timeout_ms = value;
        }
    }
}

/// Config load errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("read config `{path}`: {source}")]
    Io {
        /// Config path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not valid JSON or has an unknown/invalid field.
    #[error("parse config `{path}`: {source}")]
    Parse {
        /// Config path.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: serde_json::Error,
    },
    /// A `HYTALE_CORE_*` key is not recognised.
    #[error("unknown environment override `{key}` (prefix `{ENV_PREFIX}`)")]
    UnknownEnv {
        /// Environment key.
        key: String,
    },
    /// A recognised environment value could not be parsed.
    #[error("invalid value for `{key}`: {value:?} ({reason})")]
    InvalidEnv {
        /// Environment key.
        key: String,
        /// Raw value.
        value: String,
        /// Parse failure.
        reason: String,
    },
    /// A loaded value violates a field invariant.
    #[error("invalid config field `{field}`: {message}")]
    InvalidConfig {
        /// Field name.
        field: &'static str,
        /// What is wrong.
        message: String,
    },
}

impl ServerConfig {
    /// Load defaults, an optional JSON file, process environment and no CLI
    /// overrides.
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let env: Vec<(String, String)> = std::env::vars().collect();
        Self::from_sources(path, &env, &ServerConfigOverrides::default())
    }

    /// Load with explicit environment and CLI layers (used by tests and the
    /// binary).
    pub fn from_sources(
        path: Option<&Path>,
        env: &[(String, String)],
        overrides: &ServerConfigOverrides,
    ) -> Result<Self, ConfigError> {
        let mut config = match path {
            Some(path) => Self::from_file(path)?,
            None => Self::default(),
        };
        apply_environment(&mut config, env)?;
        overrides.apply(&mut config);
        config.validate()?;
        Ok(config)
    }

    /// Load one JSON file over the defaults, without env/CLI layers.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Validate field invariants.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.server_name.trim().is_empty() {
            return Err(ConfigError::InvalidConfig {
                field: "server_name",
                message: "must not be empty".to_string(),
            });
        }
        if self.runtime.workers == 0 {
            return Err(ConfigError::InvalidConfig {
                field: "runtime.workers",
                message: "must be positive".to_string(),
            });
        }
        if self.runtime.max_connections == 0 {
            return Err(ConfigError::InvalidConfig {
                field: "runtime.max_connections",
                message: "must be positive".to_string(),
            });
        }
        if self.runtime.shutdown_timeout_ms == 0 {
            return Err(ConfigError::InvalidConfig {
                field: "runtime.shutdown_timeout_ms",
                message: "must be positive".to_string(),
            });
        }
        Ok(())
    }
}

fn apply_environment(
    config: &mut ServerConfig,
    env: &[(String, String)],
) -> Result<(), ConfigError> {
    for (key, value) in env {
        match key.as_str() {
            "HYTALE_CORE_SERVER_NAME" => config.server_name.clone_from(value),
            "HYTALE_CORE_MOTD" => config.motd.clone_from(value),
            "HYTALE_CORE_LOG_LEVEL" => {
                config.log.level = parse_env(key, value)?;
            }
            "HYTALE_CORE_LOG_FORMAT" => {
                config.log.format = parse_env(key, value)?;
            }
            "HYTALE_CORE_LOG_ANSI" => {
                config.log.ansi = parse_env(key, value)?;
            }
            "HYTALE_CORE_WORKERS" => {
                config.runtime.workers = parse_env(key, value)?;
            }
            "HYTALE_CORE_MAX_CONNECTIONS" => {
                config.runtime.max_connections = parse_env(key, value)?;
            }
            "HYTALE_CORE_SHUTDOWN_TIMEOUT_MS" => {
                config.runtime.shutdown_timeout_ms = parse_env(key, value)?;
            }
            other if other.starts_with(ENV_PREFIX) => {
                return Err(ConfigError::UnknownEnv {
                    key: other.to_string(),
                });
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_env<T>(key: &str, value: &str) -> Result<T, ConfigError>
where
    T: FromStr,
    T::Err: Display,
{
    value
        .parse()
        .map_err(|error: T::Err| ConfigError::InvalidEnv {
            key: key.to_string(),
            value: value.to_string(),
            reason: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config(contents: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "hytale-core-config-{}-{nonce}.json",
            std::process::id()
        ));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn defaults_match_the_server_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.server_name, "Rust CoreServer");
        assert_eq!(config.motd, "dev");
        assert_eq!(config.log.level, LogLevel::Info);
        assert_eq!(config.log.format, LogFormat::Text);
        assert_eq!(config.runtime.workers, 1);
        assert_eq!(config.runtime.max_connections, 20);
        assert_eq!(
            config.runtime.shutdown_timeout(),
            std::time::Duration::from_secs(10)
        );
    }

    #[test]
    fn precedence_is_defaults_then_file_then_env_then_cli() {
        let path = temp_config(
            r#"{
                "server_name": "file-name",
                "motd": "file-motd",
                "log": {"level": "debug", "format": "json", "ansi": false},
                "runtime": {"workers": 2, "max_connections": 5, "shutdown_timeout_ms": 3000}
            }"#,
        );
        let env = vec![
            (
                "HYTALE_CORE_SERVER_NAME".to_string(),
                "env-name".to_string(),
            ),
            ("HYTALE_CORE_LOG_LEVEL".to_string(), "warn".to_string()),
            ("HYTALE_CORE_WORKERS".to_string(), "4".to_string()),
            ("UNRELATED".to_string(), "ignored".to_string()),
        ];
        let overrides = ServerConfigOverrides {
            server_name: Some("cli-name".to_string()),
            max_connections: Some(9),
            ..ServerConfigOverrides::default()
        };
        let config = ServerConfig::from_sources(Some(&path), &env, &overrides).unwrap();
        assert_eq!(config.server_name, "cli-name");
        assert_eq!(config.motd, "file-motd");
        assert_eq!(config.log.level, LogLevel::Warn);
        assert_eq!(config.log.format, LogFormat::Json);
        assert!(!config.log.ansi);
        assert_eq!(config.runtime.workers, 4);
        assert_eq!(config.runtime.max_connections, 9);
        assert_eq!(config.runtime.shutdown_timeout_ms, 3000);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn unknown_file_field_is_rejected() {
        let path = temp_config(r#"{"server_name": "x", "typo_field": 1}"#);
        let error = ServerConfig::from_file(&path).unwrap_err();
        assert!(matches!(error, ConfigError::Parse { .. }));
        assert!(error.to_string().contains("typo_field"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn unknown_core_env_key_and_bad_value_are_rejected() {
        let unknown = vec![("HYTALE_CORE_TYPO".to_string(), "1".to_string())];
        let error = ServerConfig::from_sources(None, &unknown, &ServerConfigOverrides::default())
            .unwrap_err();
        assert!(matches!(error, ConfigError::UnknownEnv { .. }));

        let bad = vec![("HYTALE_CORE_WORKERS".to_string(), "nope".to_string())];
        let error =
            ServerConfig::from_sources(None, &bad, &ServerConfigOverrides::default()).unwrap_err();
        assert!(matches!(error, ConfigError::InvalidEnv { .. }));
    }

    #[test]
    fn invalid_values_are_rejected_after_layering() {
        for overrides in [
            ServerConfigOverrides {
                workers: Some(0),
                ..ServerConfigOverrides::default()
            },
            ServerConfigOverrides {
                max_connections: Some(0),
                ..ServerConfigOverrides::default()
            },
            ServerConfigOverrides {
                shutdown_timeout_ms: Some(0),
                ..ServerConfigOverrides::default()
            },
            ServerConfigOverrides {
                server_name: Some("  ".to_string()),
                ..ServerConfigOverrides::default()
            },
        ] {
            let error =
                ServerConfig::from_sources(None, &[], &overrides).expect_err("invalid override");
            assert!(matches!(error, ConfigError::InvalidConfig { .. }));
        }
    }
}
