//! `hytale-builtin` — Rust-native builtin module runtime.
//!
//! The reference server ships 62 `builtin/` plugins plus the core `NPC` module.
//! T60 selects the modules whose native systems land in M5/M6: the block
//! simulation group (`BlockTick`, `RandomTick`, `FallingBlocks`,
//! `BlockPhysics`, `Fluid`), `Weather`, `NPC` and `EncounterManager`.
//!
//! Each module is feature-gated. A feature that is off contributes no
//! descriptor, trait object or runtime switch; the module host therefore never
//! visits it. Enabled modules register a manifest that mirrors the Java
//! `manifest.json` and start in dependency order.
//!
//! ```no_run
//! use hytale_builtin::{BuiltinHost, StartOptions};
//!
//! # fn main() -> Result<(), hytale_builtin::HostError> {
//! let mut host = BuiltinHost::compiled()?;
//! let report = host.start(&StartOptions::default())?;
//! assert!(report.disabled.is_empty());
//! let runtime = host.runtime();
//! host.shutdown();
//! # let _ = runtime;
//! # Ok(())
//! # }
//! ```

#![deny(clippy::all)]

pub mod lifecycle;
pub mod manifest;
pub mod modules;
pub mod runtime;

pub use lifecycle::{
    BuiltinHost, BuiltinModule, HostError, ModuleContext, ModuleState, ShutdownReport,
    StartOptions, StartReport, StartedModule,
};
pub use manifest::{AuthorInfo, ManifestError, PluginIdentifier, PluginManifest};
pub use modules::{
    block_physics_manifest, block_tick_manifest, compiled_manifests, compiled_modules,
    encounter_manager_manifest, falling_blocks_manifest, feature_compiled, fluid_manifest,
    npc_manifest, random_tick_manifest, weather_manifest, NativeModule,
};
pub use runtime::{BuiltinRuntime, ModuleKind};
