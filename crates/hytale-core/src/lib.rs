//! `hytale-core` — shared server infrastructure.
//!
//! Configuration and command-line options, logging setup, the typed event bus,
//! the registry facade, the task scheduler, the plugin kernel, and module
//! manifests live here. It has no internal dependencies and is shared by the
//! asset, storage, world, network, auth, command, and server layers.

#![deny(clippy::all)]

pub mod config;
pub mod events;
pub mod logging;
pub mod modules;
pub mod tasks;

pub use config::{
    ConfigError, LogFormat, LogLevel, LoggingConfig, RuntimeConfig, ServerConfig,
    ServerConfigOverrides, ENV_PREFIX, UNKNOWN_FIELD_POLICY,
};
pub use events::{EventBus, PublishReport, Subscription};
pub use logging::{init_logging, LoggingError};
pub use modules::{ModuleError, ModuleManifest, ModuleRegistry};
pub use tasks::{ShutdownReport, ShutdownToken, TaskHandle, TaskScheduler};
