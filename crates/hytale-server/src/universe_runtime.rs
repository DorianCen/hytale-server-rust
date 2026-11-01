//! Universe-owned world threads.
//!
//! [`UniverseRuntime`] keeps the persistent world catalog from `hytale-world`
//! and one live [`WorldThread`] per started world. Stopping or shutting down a
//! world saves through the thread's normal save-on-shutdown path; the cached
//! asset registry is still per thread until T42 shares it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use hytale_world::instance::{PortalCatalog, PortalTransportPlan};
use hytale_world::player_data::{PlayerData, PlayerTransform};
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
    /// A session still holds the world thread handle.
    #[error("world `{0}` still has live session handles")]
    StillInUse(WorldUuid),
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
    /// The portal catalog/plan failed.
    #[error("portal transfer failed: {0}")]
    Portal(String),
}

/// Result alias for the universe runtime.
pub type Result<T> = std::result::Result<T, UniverseRuntimeError>;

/// The persistent universe plus its live world threads.
pub struct UniverseRuntime {
    universe: Universe,
    generator: GeneratorOptions,
    view_radius: u32,
    chunks_per_tick: usize,
    builtin: hytale_builtin::BuiltinRuntime,
    running: BTreeMap<WorldUuid, Arc<WorldThread>>,
    portal_returns: Mutex<BTreeMap<WorldUuid, PortalReturnPoint>>,
    portal_entries: Mutex<BTreeMap<WorldUuid, i32>>,
}

impl UniverseRuntime {
    /// Open a universe root and bind the shared generator options.
    pub fn open(root: impl Into<std::path::PathBuf>, generator: GeneratorOptions) -> Result<Self> {
        Ok(Self {
            universe: Universe::open(root)?,
            generator,
            view_radius: 0,
            chunks_per_tick: 1,
            builtin: hytale_builtin::BuiltinRuntime::compiled(),
            running: BTreeMap::new(),
            portal_returns: Mutex::new(BTreeMap::new()),
            portal_entries: Mutex::new(BTreeMap::new()),
        })
    }

    /// Configure the initial view for worlds started after this call.
    pub fn with_view(mut self, view_radius: u32, chunks_per_tick: usize) -> Self {
        self.view_radius = view_radius;
        self.chunks_per_tick = chunks_per_tick.max(1);
        self
    }

    /// Configure the enabled builtin modules for worlds started after this call.
    pub fn with_builtin(mut self, builtin: hytale_builtin::BuiltinRuntime) -> Self {
        self.builtin = builtin;
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

    /// The live thread handle for one world.
    pub fn world_thread(&self, name_or_uuid: &str) -> Result<Arc<WorldThread>> {
        let uuid = self.universe.resolve(name_or_uuid)?.uuid;
        self.running
            .get(&uuid)
            .cloned()
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
            spawn_search: true,
            chunks_per_tick: self.chunks_per_tick,
            builtin: self.builtin,
        };
        let (thread, start) = WorldThread::spawn(options)?;
        self.running.insert(descriptor.uuid, Arc::new(thread));
        Ok(start)
    }
}

/// Where a portal sends a player.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalArrival {
    /// Resolved portal plan.
    pub plan: PortalTransportPlan,
    /// Target world UUID.
    pub world_uuid: WorldUuid,
    /// Target world catalog name.
    pub world_name: String,
    /// True when this call created/started the instance world.
    pub started: bool,
}

/// Result of moving one player's saved document through a portal.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalTransferReport {
    /// Portal and target instance.
    pub arrival: PortalArrival,
    /// Player UUID.
    pub player_uuid: WorldUuid,
    /// Source world UUID.
    pub source_world: WorldUuid,
    /// Saved document written to the target world.
    pub document: PlayerData,
}

/// Where a player returns after leaving an instance.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalReturnPoint {
    /// Source (return) world UUID.
    pub world_uuid: WorldUuid,
    /// Source transform captured before the handoff.
    pub transform: PlayerTransform,
    /// Items removed on return.
    pub ephemeral_items: Vec<String>,
}

/// Result of returning one player from an instance.
#[derive(Debug, Clone, PartialEq)]
pub struct PortalReturnReport {
    /// Player UUID.
    pub player_uuid: WorldUuid,
    /// Instance world the player left.
    pub from_world: WorldUuid,
    /// Return world UUID.
    pub to_world: WorldUuid,
    /// Saved document written to the return world.
    pub document: PlayerData,
}

impl UniverseRuntime {
    /// Resolve a portal and create/start the target instance world.
    ///
    /// Instance ids are catalogued as sanitized world names because the
    /// universe name rule rejects `/`. The returned thread is the same
    /// `Arc<WorldThread>` used by `UniverseRuntime::world_thread`.
    pub fn open_portal_instance(
        &mut self,
        portal_key: &str,
        fallback: PlayerTransform,
    ) -> Result<(PortalArrival, Arc<WorldThread>)> {
        let catalog = PortalCatalog::load_root(&self.generator.assets_root)
            .map_err(|error| UniverseRuntimeError::Portal(error.to_string()))?;
        let plan = catalog
            .plan(portal_key, fallback)
            .map_err(|error| UniverseRuntimeError::Portal(error.to_string()))?;
        let name = instance_world_name(&plan.instance_id);
        let descriptor = match self.universe.resolve(&name) {
            Ok(descriptor) => descriptor.clone(),
            Err(UniverseError::NotFound(_)) => self
                .universe
                .create_world(&name, 0, WorldProviderKind::Default)?
                .clone(),
            Err(error) => return Err(error.into()),
        };
        let started = !self.running.contains_key(&descriptor.uuid);
        if started {
            self.start_world(&descriptor.uuid.to_string())?;
        }
        let thread = self.world_thread(&descriptor.uuid.to_string())?;
        Ok((
            PortalArrival {
                plan,
                world_uuid: descriptor.uuid,
                world_name: descriptor.name,
                started,
            },
            thread,
        ))
    }

    /// Move a live session's in-memory player state through a portal.
    ///
    /// The source thread persists the live transform/inventory/health first,
    /// then [`Self::transfer_player_document`] moves the full holder and
    /// removes the source copy. The caller stops the source view/session after
    /// this returns.
    #[allow(clippy::too_many_arguments)]
    pub fn transfer_live_player(
        &self,
        source: &WorldThread,
        target: &WorldThread,
        player_uuid: WorldUuid,
        transform: PlayerTransform,
        inventory: hytale_inventory::PlayerInventory,
        health: Option<f32>,
        arrival: &PortalArrival,
    ) -> Result<PortalTransferReport> {
        source.save_player_state(player_uuid, transform, inventory)?;
        if let Some(health) = health {
            source.save_player_health(player_uuid, health)?;
        }
        self.transfer_player_document(source, target, player_uuid, arrival)
    }

    /// Move a player's complete saved document to the target world and remove
    /// the source copy. The live session must still stop its source view/session
    /// and reconnect to the target thread; this method owns the persistent
    /// handoff.
    pub fn transfer_player_document(
        &self,
        source: &WorldThread,
        target: &WorldThread,
        player_uuid: WorldUuid,
        arrival: &PortalArrival,
    ) -> Result<PortalTransferReport> {
        if arrival.plan.allowed_entry_count > 0 {
            let entries = self
                .portal_entries
                .lock()
                .ok()
                .and_then(|entries| entries.get(&arrival.world_uuid).copied())
                .unwrap_or(0);
            if entries >= arrival.plan.allowed_entry_count {
                return Err(UniverseRuntimeError::Portal(format!(
                    "instance `{}` entry limit of {} reached",
                    arrival.plan.instance_id, arrival.plan.allowed_entry_count
                )));
            }
        }
        let mut document = source.player_document(player_uuid)?;
        let source_world: WorldUuid =
            source.start_report().world_uuid.parse().map_err(|_| {
                UniverseRuntimeError::Portal("invalid source world UUID".to_string())
            })?;
        if let Some(transform) = document.transform() {
            if let Ok(mut returns) = self.portal_returns.lock() {
                returns.insert(
                    player_uuid,
                    PortalReturnPoint {
                        world_uuid: source_world,
                        transform,
                        ephemeral_items: arrival.plan.ephemeral_items.clone(),
                    },
                );
            }
        }
        document.set_transform(arrival.plan.spawn);
        target.save_player_document(player_uuid, document.clone())?;
        source.delete_player_document(player_uuid)?;
        if let Ok(mut entries) = self.portal_entries.lock() {
            *entries.entry(arrival.world_uuid).or_insert(0) += 1;
        }
        Ok(PortalTransferReport {
            arrival: arrival.clone(),
            player_uuid,
            source_world,
            document,
        })
    }

    /// The recorded return point for one player, if they entered an instance.
    pub fn portal_return_point(&self, player_uuid: WorldUuid) -> Option<PortalReturnPoint> {
        self.portal_returns
            .lock()
            .ok()
            .and_then(|returns| returns.get(&player_uuid).cloned())
    }

    /// Number of tracked portal entries for one instance world.
    pub fn portal_entry_count(&self, world_uuid: WorldUuid) -> i32 {
        self.portal_entries
            .lock()
            .ok()
            .and_then(|entries| entries.get(&world_uuid).copied())
            .unwrap_or(0)
    }

    /// Stop an instance world when no tracked portal entries remain.
    ///
    /// `close_when_empty` is the portal's `CloseWhenEmpty` policy; empty
    /// instances only tear down when the portal opts in.
    pub fn close_instance_if_empty(
        &mut self,
        world_uuid: WorldUuid,
        close_when_empty: bool,
    ) -> Result<bool> {
        if !close_when_empty || self.portal_entry_count(world_uuid) > 0 {
            return Ok(false);
        }
        let Some(thread) = self.running.remove(&world_uuid) else {
            return Ok(false);
        };
        let thread =
            Arc::try_unwrap(thread).map_err(|_| UniverseRuntimeError::StillInUse(world_uuid))?;
        thread.shutdown()?;
        Ok(true)
    }

    /// Move a player from an instance back to the recorded source world.
    ///
    /// Restores the pre-handoff transform and removes the portal's ephemeral
    /// items from the player's inventory.
    pub fn return_from_instance(
        &self,
        current: &WorldThread,
        player_uuid: WorldUuid,
    ) -> Result<PortalReturnReport> {
        let point = self
            .portal_returns
            .lock()
            .ok()
            .and_then(|mut returns| returns.remove(&player_uuid))
            .ok_or_else(|| {
                UniverseRuntimeError::Portal(format!("no return point for `{player_uuid}`"))
            })?;
        let target = self.running.get(&point.world_uuid).ok_or_else(|| {
            UniverseRuntimeError::Portal(format!(
                "return world `{}` is not running",
                point.world_uuid
            ))
        })?;
        let from_world: WorldUuid =
            current.start_report().world_uuid.parse().map_err(|_| {
                UniverseRuntimeError::Portal("invalid instance world UUID".to_string())
            })?;
        let mut document = current.player_document(player_uuid)?;
        if !point.ephemeral_items.is_empty() {
            let mut inventory = document.inventory().map_err(|error| {
                UniverseRuntimeError::Portal(format!("invalid player inventory: {error}"))
            })?;
            for item in &point.ephemeral_items {
                inventory.remove_item(item, i32::MAX);
            }
            document.set_inventory(&inventory);
        }
        document.set_transform(point.transform);
        target.save_player_document(player_uuid, document.clone())?;
        current.delete_player_document(player_uuid)?;
        if let Ok(mut entries) = self.portal_entries.lock() {
            let count = entries.entry(from_world).or_insert(0);
            *count = (*count - 1).max(0);
        }
        Ok(PortalReturnReport {
            player_uuid,
            from_world,
            to_world: point.world_uuid,
            document,
        })
    }
}

fn instance_world_name(instance_id: &str) -> String {
    let name: String = instance_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "Instance".to_string()
    } else {
        name
    }
}

impl UniverseRuntime {
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
        let thread = Arc::try_unwrap(thread).map_err(|_| UniverseRuntimeError::StillInUse(uuid))?;
        Ok(thread.shutdown()?)
    }

    /// Stop every running world and return the number of saved chunks.
    pub fn shutdown(&mut self) -> Result<usize> {
        let running = std::mem::take(&mut self.running);
        let mut saved = 0;
        for (uuid, thread) in running {
            let thread =
                Arc::try_unwrap(thread).map_err(|_| UniverseRuntimeError::StillInUse(uuid))?;
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
