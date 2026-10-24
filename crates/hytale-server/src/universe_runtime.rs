//! Universe-owned world threads.
//!
//! [`UniverseRuntime`] keeps the persistent world catalog from `hytale-world`
//! and one live [`WorldThread`] per started world. Stopping or shutting down a
//! world saves through the thread's normal save-on-shutdown path; the cached
//! asset registry is still per thread until T42 shares it.

use std::collections::BTreeMap;

use hytale_world::universe::{Universe, UniverseError, WorldDescriptor, WorldProviderKind};
use hytale_world::world_config::WorldUuid;
use thiserror::Error;

use crate::generation::GeneratorOptions;
use crate::world_thread::{WorldStartReport, WorldThread, WorldThreadError, WorldThreadOptions};

/// Errors from the universe runtime.
#[derive(Debug, Error)]
pub enum UniverseRuntimeError {
    /// The persistent catalog failed.
    #[error(transparent)]
    Universe(#[from] UniverseError),
    /// A world thread failed.
    #[error(transparent)]
    Thread(#[from] WorldThreadError),
    /// The world is already running.
    #[error("world `{0}` is already running")]
    AlreadyRunning(WorldUuid),
    /// The world is not running.
    #[error("world `{0}` is not running")]
    NotRunning(WorldUuid),
    /// The default thread only implements the Default provider.
    #[error(
        "provider `{provider}` is not available for world `{world}`; \
         only `default` is wired to the worldgen pipeline"
    )]
    UnsupportedProvider {
        /// World name.
        world: String,
        /// Requested provider.
        provider: WorldProviderKind,
    },
}

/// Result alias for the universe runtime.
pub type Result<T> = std::result::Result<T, UniverseRuntimeError>;

/// The persistent universe plus its live world threads.
pub struct UniverseRuntime {
    universe: Universe,
    generator: GeneratorOptions,
    view_radius: u32,
    chunks_per_tick: usize,
    running: BTreeMap<WorldUuid, WorldThread>,
}

impl UniverseRuntime {
    /// Open a universe root and bind the shared generator options.
    pub fn open(root: impl Into<std::path::PathBuf>, generator: GeneratorOptions) -> Result<Self> {
        Ok(Self {
            universe: Universe::open(root)?,
            generator,
            view_radius: 0,
            chunks_per_tick: 1,
            running: BTreeMap::new(),
        })
    }

    /// Configure the initial view for worlds started after this call.
    pub fn with_view(mut self, view_radius: u32, chunks_per_tick: usize) -> Self {
        self.view_radius = view_radius;
        self.chunks_per_tick = chunks_per_tick.max(1);
        self
    }

    /// The persistent catalog.
    pub fn universe(&self) -> &Universe {
        &self.universe
    }

    /// The persistent catalog, mutable for create/delete/default changes.
    pub fn universe_mut(&mut self) -> &mut Universe {
        &mut self.universe
    }

    /// UUIDs of the worlds with a live thread.
    pub fn running_worlds(&self) -> Vec<WorldUuid> {
        self.running.keys().copied().collect()
    }

    /// Whether a world currently has a live thread.
    pub fn is_running(&self, name_or_uuid: &str) -> bool {
        self.universe
            .resolve(name_or_uuid)
            .map(|descriptor| self.running.contains_key(&descriptor.uuid))
            .unwrap_or(false)
    }

    /// The live thread for one world.
    pub fn world_thread(&self, name_or_uuid: &str) -> Result<&WorldThread> {
        let uuid = self.universe.resolve(name_or_uuid)?.uuid;
        self.running
            .get(&uuid)
            .ok_or(UniverseRuntimeError::NotRunning(uuid))
    }

    /// Create a world in the catalog without starting it.
    pub fn create_world(
        &mut self,
        name: &str,
        seed: i64,
        provider: WorldProviderKind,
    ) -> Result<WorldDescriptor> {
        Ok(self.universe.create_world(name, seed, provider)?.clone())
    }

    /// Create a world and start its thread.
    pub fn create_and_start(
        &mut self,
        name: &str,
        seed: i64,
        provider: WorldProviderKind,
    ) -> Result<WorldStartReport> {
        let descriptor = self.create_world(name, seed, provider)?;
        self.start_world(&descriptor.uuid.to_string())
    }

    /// Start one world's thread. Only the Default provider is wired to the
    /// real worldgen pipeline; Flat/Void/Dummy are catalogued but cannot be
    /// started until M5.
    pub fn start_world(&mut self, name_or_uuid: &str) -> Result<WorldStartReport> {
        let descriptor = self.universe.resolve(name_or_uuid)?.clone();
        if self.running.contains_key(&descriptor.uuid) {
            return Err(UniverseRuntimeError::AlreadyRunning(descriptor.uuid));
        }
        if descriptor.provider != WorldProviderKind::Default {
            return Err(UniverseRuntimeError::UnsupportedProvider {
                world: descriptor.name,
                provider: descriptor.provider,
            });
        }
        let options = WorldThreadOptions {
            generator: self.generator.clone(),
            save_dir: descriptor.save_dir.clone(),
            seed: descriptor.seed,
            display_name: Some(descriptor.name.clone()),
            world_uuid: Some(descriptor.uuid.to_string()),
            player_uuid: None,
            view_radius: self.view_radius,
            chunks_per_tick: self.chunks_per_tick,
        };
        let (thread, start) = WorldThread::spawn(options)?;
        self.running.insert(descriptor.uuid, thread);
        Ok(start)
    }

    /// Save a running world immediately, or a stopped world's config.
    pub fn save_world(&self, name_or_uuid: &str) -> Result<usize> {
        let descriptor = self.universe.resolve(name_or_uuid)?;
        if let Some(thread) = self.running.get(&descriptor.uuid) {
            Ok(thread.save()?)
        } else {
            self.universe.save_world(name_or_uuid)?;
            Ok(0)
        }
    }

    /// Stop one world, flushing chunks/players exactly once through the
    /// thread's shutdown path.
    pub fn stop_world(&mut self, name_or_uuid: &str) -> Result<usize> {
        let uuid = self.universe.resolve(name_or_uuid)?.uuid;
        let thread = self
            .running
            .remove(&uuid)
            .ok_or(UniverseRuntimeError::NotRunning(uuid))?;
        Ok(thread.shutdown()?)
    }

    /// Stop every running world and return the number of saved chunks.
    pub fn shutdown(&mut self) -> Result<usize> {
        let running = std::mem::take(&mut self.running);
        let mut saved = 0;
        for (_uuid, thread) in running {
            saved += thread.shutdown()?;
        }
        Ok(saved)
    }
}

impl std::fmt::Debug for UniverseRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UniverseRuntime")
            .field("root", &self.universe.root())
            .field("catalogued", &self.universe.list_worlds().len())
            .field("running", &self.running.len())
            .finish()
    }
}
