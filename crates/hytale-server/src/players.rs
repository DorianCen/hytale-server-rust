//! Per-world player registry, entity visibility and broadcast fan-out.
//!
//! The listener owns one [`PlayerRegistry`] per served world. Every session
//! registers its player (identity, transform, appearance and view), receives a
//! network id, and then drains [`PlayerMessage`]s from its own queue. The
//! registry tracks which entities each viewer currently sees, so join, leave,
//! movement and view changes only emit spawn/update/removal packets that are
//! relevant to that viewer.
//!
//! The registry is synchronous (`std::sync::Mutex` + `std::sync::mpsc`); the
//! session's async loop drains the queue and writes the packets, so no writer is
//! shared across tasks and a slow client cannot block another session's world
//! updates.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use hytale_npc::combat::NpcTarget;
use hytale_protocol::generated::common::{
    ComponentUpdate, Direction, EffectOp, EntityEffectUpdate, EntityEffectsUpdate, EntityStatOp,
    EntityStatUpdate, EntityStatsUpdate, EntityUpdate, EquipmentUpdate, FormattedMessage,
    ItemUpdate, ItemWithAllMetadata, MapMarker, MapMarkerComponent, MapMarkerIconSize, Model,
    ModelTransform, ModelUpdate, NameplateUpdate, PlayerMarkerComponent, PlayerSkin,
    PlayerSkinUpdate, Position as ProtocolPosition, ServerPlayerListPlayer,
    Transform as ProtocolTransform, TransformUpdate,
};
use hytale_protocol::generated::entities::EntityUpdates;
use hytale_protocol::generated::interface_::{
    AddToServerPlayerList, ChatType, RemoveFromServerPlayerList, ServerMessage,
};
use hytale_protocol::generated::world::{
    ServerSetBlock, ServerSetFluid, UpdateBlockDamage, UpdateBlockDamages,
};
use hytale_protocol::generated::worldmap::UpdateWorldMap;
use hytale_protocol::io::Uuid;
use hytale_world::collision::Vec3;
use hytale_world::player_data::{
    PlayerAppearance, PlayerModelReference, PlayerSkinData, PlayerTransform, EMPTY_ITEM_ID,
};
use hytale_world::runtime::{RuntimeEntity, RuntimeEntityKind};

/// A control action queued for another session's game loop.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlAction {
    /// Move the target to absolute coordinates.
    Teleport {
        /// X coordinate.
        x: f64,
        /// Y coordinate.
        y: f64,
        /// Z coordinate.
        z: f64,
    },
    /// Add an item stack to the target's inventory.
    GiveItem {
        /// Item asset id.
        item_id: String,
        /// Positive quantity.
        quantity: i32,
    },
    /// Change the target's game mode.
    SetGameMode {
        /// `adventure` or `creative`.
        mode: String,
    },
    /// Kill the target.
    Kill,
    /// Respawn the target.
    Respawn,
    /// Disconnect the target.
    Kick {
        /// Disconnect reason.
        reason: String,
    },
    /// Move the target to another world.
    SwitchWorld {
        /// World name or UUID.
        world: String,
    },
}

/// One packet batch queued for a session.
#[derive(Debug, Clone)]
pub enum PlayerMessage {
    /// Add one or more players to the client's server player list.
    AddToPlayerList(Vec<ServerPlayerListPlayer>),
    /// Remove players from the client's server player list.
    RemoveFromPlayerList(Vec<Uuid>),
    /// Spawn/update/remove one or more entities.
    EntityUpdates(EntityUpdates),
    /// A single block edit on the chunks stream.
    BlockUpdate(ServerSetBlock),
    /// A single fluid update on the chunks stream.
    FluidUpdate(ServerSetFluid),
    /// One block-health change on the chunks stream.
    BlockDamage(UpdateBlockDamage),
    /// A section's block-health changes on the chunks stream.
    BlockDamages(UpdateBlockDamages),
    /// A world-map marker add/remove batch.
    WorldMap(UpdateWorldMap),
    /// A chat line to display to this client.
    Chat(String),
    /// A command effect another session must apply to itself.
    Control(ControlAction),
}

/// Default per-session broadcast queue capacity.
pub const PLAYER_MESSAGE_QUEUE_CAPACITY: usize = 4096;

/// A queued player/entity/block message failed to write.
#[derive(Debug, thiserror::Error)]
pub enum PlayerMessageError {
    /// A control-stream packet failed.
    #[error(transparent)]
    Frame(#[from] hytale_net::FrameStreamError),
    /// A chunks-stream packet failed.
    #[error(transparent)]
    Chunk(#[from] hytale_net::ChunkStreamError),
}

/// A registration attempt that could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlayerJoinError {
    /// The same player UUID is already connected to this world.
    #[error("player {0} is already connected")]
    DuplicateIdentity(Uuid),
}

/// Everything needed to register a joining session.
#[derive(Debug, Clone)]
pub struct PlayerJoinRequest {
    /// Player UUID.
    pub uuid: Uuid,
    /// Player username.
    pub username: String,
    /// Served world UUID (for `ServerPlayerListPlayer`).
    pub world_uuid: Uuid,
    /// Current transform.
    pub transform: PlayerTransform,
    /// Saved appearance/equipment.
    pub appearance: PlayerAppearance,
    /// Auth-handshake skin, if any.
    pub skin: Option<PlayerSkin>,
    /// Current view center chunk.
    pub view_center: (i32, i32),
    /// Current view radius in chunks.
    pub view_radius: u32,
    /// Queue for this session's broadcasts.
    pub sender: SyncSender<PlayerMessage>,
}

/// What registration returned to the joining session.
#[derive(Debug, Clone)]
pub struct PlayerJoin {
    /// Assigned network/entity id (used by `SetClientId` and the self spawn).
    pub network_id: i32,
    /// The complete player list to send to the joining client (self included).
    pub player_list: Vec<ServerPlayerListPlayer>,
    /// Existing players' map markers for the joining client.
    pub player_markers: Vec<MapMarker>,
    /// Set when a broadcast cannot be queued because the session is too slow.
    pub overflow: Arc<AtomicBool>,
}

struct PlayerEntry {
    uuid: Uuid,
    username: String,
    world_uuid: Uuid,
    network_id: i32,
    transform: PlayerTransform,
    appearance: PlayerAppearance,
    skin: Option<PlayerSkin>,
    view_center: (i32, i32),
    view_radius: u32,
    sender: SyncSender<PlayerMessage>,
    overflow: Arc<AtomicBool>,
    visible: BTreeSet<i32>,
}

impl PlayerEntry {
    fn list_entry(&self) -> ServerPlayerListPlayer {
        ServerPlayerListPlayer {
            uuid: self.uuid,
            username: Some(self.username.clone()),
            world_uuid: Some(self.world_uuid),
            ping: 0,
            spectating: false,
            lives_remaining: None,
        }
    }
}

struct Inner {
    next_network_id: i32,
    players: BTreeMap<i32, PlayerEntry>,
}

/// One world's connected players.
pub struct PlayerRegistry {
    inner: Mutex<Inner>,
}

impl Default for PlayerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PlayerRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock().expect("player registry poisoned");
        formatter
            .debug_struct("PlayerRegistry")
            .field("players", &inner.players.len())
            .field("next_network_id", &inner.next_network_id)
            .finish()
    }
}

impl PlayerRegistry {
    /// An empty registry. Network ids start at 1, matching the first join path.
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                next_network_id: 1,
                players: BTreeMap::new(),
            }),
        }
    }

    /// Register a session and queue the join broadcasts.
    ///
    /// A second connection with an already-registered UUID is refused so a
    /// stolen/duplicated identity cannot take over or shadow a live session.
    pub fn register(
        self: &Arc<Self>,
        request: PlayerJoinRequest,
    ) -> Result<(PlayerSession, PlayerJoin), PlayerJoinError> {
        let mut inner = self.inner.lock().expect("player registry poisoned");
        if inner
            .players
            .values()
            .any(|player| player.uuid == request.uuid)
        {
            return Err(PlayerJoinError::DuplicateIdentity(request.uuid));
        }
        let network_id = inner.next_network_id;
        inner.next_network_id = if network_id == i32::MAX {
            1
        } else {
            network_id + 1
        };
        while inner.players.contains_key(&inner.next_network_id) {
            inner.next_network_id = if inner.next_network_id == i32::MAX {
                1
            } else {
                inner.next_network_id + 1
            };
        }

        let overflow = Arc::new(AtomicBool::new(false));
        let entry = PlayerEntry {
            uuid: request.uuid,
            username: request.username.clone(),
            world_uuid: request.world_uuid,
            network_id,
            transform: request.transform,
            appearance: request.appearance,
            skin: request.skin,
            view_center: request.view_center,
            view_radius: request.view_radius,
            sender: request.sender,
            overflow: Arc::clone(&overflow),
            visible: BTreeSet::new(),
        };
        let list_entry = entry.list_entry();
        inner.players.insert(network_id, entry);

        // Existing players hear about the new player; the new player's own
        // receiver gets spawns for entities already in view.
        for (id, player) in &inner.players {
            if *id == network_id {
                continue;
            }
            queue(
                player,
                PlayerMessage::AddToPlayerList(vec![list_entry.clone()]),
            );
        }
        // Map markers are global, not view-filtered.
        let new_marker = player_marker(&inner.players[&network_id]);
        for player in inner.players.values() {
            if player.network_id != network_id {
                queue(
                    player,
                    PlayerMessage::WorldMap(UpdateWorldMap {
                        chunks: None,
                        added_markers: Some(vec![new_marker.clone()]),
                        removed_markers: None,
                    }),
                );
            }
        }

        let player_list = inner
            .players
            .values()
            .map(PlayerEntry::list_entry)
            .collect::<Vec<_>>();
        let player_markers = inner
            .players
            .values()
            .filter(|player| player.network_id != network_id)
            .map(player_marker)
            .collect();
        recompute_visibility(&mut inner);
        drop(inner);

        Ok((
            PlayerSession {
                registry: Arc::clone(self),
                network_id,
            },
            PlayerJoin {
                network_id,
                player_list,
                player_markers,
                overflow,
            },
        ))
    }

    /// Remove a session, returning its UUID when it was registered.
    pub fn unregister(&self, network_id: i32) -> Option<Uuid> {
        let mut inner = self.inner.lock().expect("player registry poisoned");
        let entry = inner.players.remove(&network_id)?;
        let uuid = entry.uuid;
        let list = vec![uuid];
        let marker_id = format!("Player-{uuid}");
        for player in inner.players.values() {
            queue(player, PlayerMessage::RemoveFromPlayerList(list.clone()));
            queue(
                player,
                PlayerMessage::WorldMap(UpdateWorldMap {
                    chunks: None,
                    added_markers: None,
                    removed_markers: Some(vec![marker_id.clone()]),
                }),
            );
        }
        recompute_visibility(&mut inner);
        Some(uuid)
    }

    /// Update a player's transform and queue movement/visibility changes.
    pub fn update_transform(&self, network_id: i32, transform: PlayerTransform) -> bool {
        let mut inner = self.inner.lock().expect("player registry poisoned");
        let Some(entry) = inner.players.get(&network_id) else {
            return false;
        };
        if entry.transform == transform {
            return false;
        }
        if let Some(entry) = inner.players.get_mut(&network_id) {
            entry.transform = transform;
        }
        recompute_visibility(&mut inner);
        broadcast_transform(&inner, network_id);
        true
    }

    /// Update a player's view and queue entities entering/leaving it.
    pub fn update_view(&self, network_id: i32, center: (i32, i32), radius: u32) -> bool {
        let mut inner = self.inner.lock().expect("player registry poisoned");
        let Some(entry) = inner.players.get_mut(&network_id) else {
            return false;
        };
        if entry.view_center == center && entry.view_radius == radius {
            return false;
        }
        entry.view_center = center;
        entry.view_radius = radius;
        recompute_visibility(&mut inner);
        true
    }

    /// Move one registered player to another world, resetting its view and
    /// re-evaluating visibility against players in the target world.
    pub fn switch_world(
        &self,
        network_id: i32,
        world_uuid: Uuid,
        transform: PlayerTransform,
        center: (i32, i32),
        radius: u32,
    ) -> bool {
        let mut inner = self.inner.lock().expect("player registry poisoned");
        let Some(entry) = inner.players.get_mut(&network_id) else {
            return false;
        };
        entry.world_uuid = world_uuid;
        entry.transform = transform;
        entry.view_center = center;
        entry.view_radius = radius;
        entry.visible.clear();
        recompute_visibility(&mut inner);
        true
    }

    /// One player's current world UUID.
    pub fn world_uuid(&self, network_id: i32) -> Option<Uuid> {
        self.inner
            .lock()
            .expect("player registry poisoned")
            .players
            .get(&network_id)
            .map(|entry| entry.world_uuid)
    }

    /// One player's current transform.
    pub fn transform(&self, network_id: i32) -> Option<PlayerTransform> {
        self.inner
            .lock()
            .expect("player registry poisoned")
            .players
            .get(&network_id)
            .map(|entry| entry.transform)
    }

    /// One player's current view center/radius.
    pub fn view(&self, network_id: i32) -> Option<((i32, i32), u32)> {
        self.inner
            .lock()
            .expect("player registry poisoned")
            .players
            .get(&network_id)
            .map(|entry| (entry.view_center, entry.view_radius))
    }

    /// Queue a block edit for every viewer whose square chunk view contains it.
    ///
    /// The editing session is included: the client predicts the edit, and the
    /// server update is the authoritative correction.
    pub fn broadcast_block(&self, packet: ServerSetBlock) {
        let inner = self.inner.lock().expect("player registry poisoned");
        for viewer in inner.players.values() {
            if in_view(
                viewer.view_center,
                viewer.view_radius,
                hytale_world::player_data::Position {
                    x: f64::from(packet.x) + 0.5,
                    y: f64::from(packet.y) + 0.5,
                    z: f64::from(packet.z) + 0.5,
                },
            ) {
                queue(viewer, PlayerMessage::BlockUpdate(packet.clone()));
            }
        }
    }

    /// Broadcast one fluid update to every viewer of its chunk.
    pub fn broadcast_fluid(&self, packet: ServerSetFluid) {
        let inner = self.inner.lock().expect("player registry poisoned");
        for viewer in inner.players.values() {
            if in_view(
                viewer.view_center,
                viewer.view_radius,
                hytale_world::player_data::Position {
                    x: f64::from(packet.x) + 0.5,
                    y: f64::from(packet.y) + 0.5,
                    z: f64::from(packet.z) + 0.5,
                },
            ) {
                queue(viewer, PlayerMessage::FluidUpdate(packet.clone()));
            }
        }
    }

    /// Broadcast one block-health change to viewers of its block.
    pub fn broadcast_block_damage(&self, packet: UpdateBlockDamage) {
        let Some(position) = packet.block_position.as_ref() else {
            return;
        };
        let position = hytale_world::player_data::Position {
            x: f64::from(position.x) + 0.5,
            y: f64::from(position.y) + 0.5,
            z: f64::from(position.z) + 0.5,
        };
        let inner = self.inner.lock().expect("player registry poisoned");
        for viewer in inner.players.values() {
            if in_view(viewer.view_center, viewer.view_radius, position) {
                queue(viewer, PlayerMessage::BlockDamage(packet.clone()));
            }
        }
    }

    /// Broadcast a section's block-health changes to viewers of any entry.
    pub fn broadcast_block_damages(&self, packet: UpdateBlockDamages) {
        let positions: Vec<hytale_world::player_data::Position> = packet
            .cmds
            .iter()
            .map(|command| {
                let index = i32::from(command.index);
                hytale_world::player_data::Position {
                    x: f64::from((packet.x << 5) + (index & 31)) + 0.5,
                    y: f64::from((packet.y << 5) + ((index >> 10) & 31)) + 0.5,
                    z: f64::from((packet.z << 5) + ((index >> 5) & 31)) + 0.5,
                }
            })
            .collect();
        let inner = self.inner.lock().expect("player registry poisoned");
        for viewer in inner.players.values() {
            if positions
                .iter()
                .any(|position| in_view(viewer.view_center, viewer.view_radius, *position))
            {
                queue(viewer, PlayerMessage::BlockDamages(packet.clone()));
            }
        }
    }

    /// Broadcast an arbitrary `EntityUpdates` packet to viewers that can see
    /// the target player. The target itself is included so its client receives
    /// the authoritative stat/effect component update.
    pub fn broadcast_entity_updates(&self, network_id: i32, packet: EntityUpdates) {
        let inner = self.inner.lock().expect("player registry poisoned");
        let Some(target) = inner.players.get(&network_id) else {
            return;
        };
        let position = target.transform.position;
        for viewer in inner.players.values() {
            if in_view(viewer.view_center, viewer.view_radius, position) {
                queue(viewer, PlayerMessage::EntityUpdates(packet.clone()));
            }
        }
    }

    /// Broadcast a runtime entity spawn to viewers.
    pub fn broadcast_runtime_entity(&self, entity: &RuntimeEntity) {
        let position = entity.transform.position;
        let packet = runtime_entity_updates(entity);
        let inner = self.inner.lock().expect("player registry poisoned");
        for viewer in inner.players.values() {
            if in_view(viewer.view_center, viewer.view_radius, position) {
                queue(viewer, PlayerMessage::EntityUpdates(packet.clone()));
            }
        }
    }

    /// Broadcast a runtime entity removal to viewers.
    pub fn broadcast_runtime_entity_removed(
        &self,
        id: u64,
        position: hytale_world::player_data::Position,
    ) {
        let packet = removed_entity_updates(runtime_network_id(id));
        let inner = self.inner.lock().expect("player registry poisoned");
        for viewer in inner.players.values() {
            if in_view(viewer.view_center, viewer.view_radius, position) {
                queue(viewer, PlayerMessage::EntityUpdates(packet.clone()));
            }
        }
    }

    /// Update the authoritative active held items and notify visible viewers.
    pub fn update_equipment(
        &self,
        network_id: i32,
        right_hand: Option<String>,
        left_hand: Option<String>,
    ) -> bool {
        let mut inner = self.inner.lock().expect("player registry poisoned");
        let Some(target) = inner.players.get_mut(&network_id) else {
            return false;
        };
        target.appearance.right_hand = right_hand.clone();
        target.appearance.left_hand = left_hand.clone();
        let updates = equipment_entity_updates(network_id, right_hand, left_hand);
        for (viewer_id, viewer) in &inner.players {
            if *viewer_id != network_id && viewer.visible.contains(&network_id) {
                queue(viewer, PlayerMessage::EntityUpdates(updates.clone()));
            }
        }
        true
    }

    /// Player positions in the NPC target namespace. The high bit keeps player
    /// ids disjoint from runtime entity/NPC ids.
    pub fn npc_targets(&self) -> Vec<NpcTarget> {
        let inner = self.inner.lock().expect("player registry poisoned");
        inner
            .players
            .values()
            .map(|player| NpcTarget {
                id: PLAYER_NPC_TARGET_OFFSET | u64::from(player.network_id.max(0) as u32),
                position: Vec3::new(
                    player.transform.position.x,
                    player.transform.position.y,
                    player.transform.position.z,
                ),
            })
            .collect()
    }

    /// Every connected player's username, in network-id order.
    pub fn usernames(&self) -> Vec<String> {
        self.inner
            .lock()
            .expect("player registry poisoned")
            .players
            .values()
            .map(|player| player.username.clone())
            .collect()
    }

    /// Every connected player's username and position, in network-id order.
    pub fn player_positions(&self) -> Vec<(String, [f64; 3])> {
        self.inner
            .lock()
            .expect("player registry poisoned")
            .players
            .values()
            .map(|player| {
                (
                    player.username.clone(),
                    [
                        player.transform.position.x,
                        player.transform.position.y,
                        player.transform.position.z,
                    ],
                )
            })
            .collect()
    }

    /// Resolve a connected player's network id by username (case-insensitive).
    pub fn network_id_for(&self, username: &str) -> Option<i32> {
        self.inner
            .lock()
            .expect("player registry poisoned")
            .players
            .values()
            .find(|player| player.username.eq_ignore_ascii_case(username))
            .map(|player| player.network_id)
    }

    /// Queue a control action for one session.
    pub fn queue_control(&self, network_id: i32, action: ControlAction) -> bool {
        let inner = self.inner.lock().expect("player registry poisoned");
        let Some(player) = inner.players.get(&network_id) else {
            return false;
        };
        queue(player, PlayerMessage::Control(action));
        true
    }

    /// Send a chat line to every connected player.
    pub fn broadcast_chat(&self, message: impl Into<String>) {
        let message = message.into();
        let inner = self.inner.lock().expect("player registry poisoned");
        for player in inner.players.values() {
            queue(player, PlayerMessage::Chat(message.clone()));
        }
    }

    /// Number of registered players.
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("player registry poisoned")
            .players
            .len()
    }

    /// Whether no players are registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Keep a session registered until it is dropped.
#[derive(Debug)]
pub struct PlayerSession {
    registry: Arc<PlayerRegistry>,
    network_id: i32,
}

impl PlayerSession {
    /// The assigned network/entity id.
    pub fn network_id(&self) -> i32 {
        self.network_id
    }
}

impl Drop for PlayerSession {
    fn drop(&mut self) {
        self.registry.unregister(self.network_id);
    }
}

/// Create the default bounded queue pair used by one session.
pub fn channel() -> (SyncSender<PlayerMessage>, Receiver<PlayerMessage>) {
    channel_with_capacity(PLAYER_MESSAGE_QUEUE_CAPACITY)
}

/// Create a queue pair with an explicit capacity.
pub fn channel_with_capacity(
    capacity: usize,
) -> (SyncSender<PlayerMessage>, Receiver<PlayerMessage>) {
    mpsc::sync_channel(capacity.max(1))
}

fn queue(player: &PlayerEntry, message: PlayerMessage) {
    match player.sender.try_send(message) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => {
            // The session loop checks this flag and disconnects the slow
            // client before the world thread can block on it.
            player.overflow.store(true, Ordering::Relaxed);
        }
        Err(TrySendError::Disconnected(_)) => {}
    }
}

fn recompute_visibility(inner: &mut Inner) {
    let ids: Vec<i32> = inner.players.keys().copied().collect();
    for viewer_id in &ids {
        let mut next = BTreeSet::new();
        {
            let viewer = &inner.players[viewer_id];
            for target_id in &ids {
                if viewer_id == target_id {
                    continue;
                }
                let target = &inner.players[target_id];
                if viewer.world_uuid != target.world_uuid {
                    continue;
                }
                if in_view(
                    viewer.view_center,
                    viewer.view_radius,
                    target.transform.position,
                ) {
                    next.insert(*target_id);
                }
            }
        }

        let previous = inner.players[viewer_id].visible.clone();
        let newly_visible: Vec<i32> = next.difference(&previous).copied().collect();
        let lost: Vec<i32> = previous.difference(&next).copied().collect();
        if let Some(viewer) = inner.players.get(viewer_id) {
            for target_id in newly_visible {
                let target = &inner.players[&target_id];
                queue(
                    viewer,
                    PlayerMessage::EntityUpdates(spawn_entity_updates(
                        target.network_id,
                        target.transform,
                        &target.appearance,
                        target.skin.clone(),
                    )),
                );
            }
            for target_id in lost {
                queue(
                    viewer,
                    PlayerMessage::EntityUpdates(removed_entity_updates(target_id)),
                );
            }
        }
        if let Some(viewer) = inner.players.get_mut(viewer_id) {
            viewer.visible = next;
        }
    }
}

fn broadcast_transform(inner: &Inner, network_id: i32) {
    let Some(target) = inner.players.get(&network_id) else {
        return;
    };
    let updates = transform_entity_updates(network_id, target.transform);
    let map_update = UpdateWorldMap {
        chunks: None,
        added_markers: Some(vec![player_marker(target)]),
        removed_markers: None,
    };
    for (viewer_id, viewer) in &inner.players {
        if *viewer_id != network_id && viewer.visible.contains(&network_id) {
            queue(viewer, PlayerMessage::EntityUpdates(updates.clone()));
            queue(viewer, PlayerMessage::WorldMap(map_update.clone()));
        }
    }
}

fn chunk_of(position: hytale_world::player_data::Position) -> (i32, i32) {
    (
        (position.x.floor() as i32) >> 5,
        (position.z.floor() as i32) >> 5,
    )
}

fn in_view(center: (i32, i32), radius: u32, position: hytale_world::player_data::Position) -> bool {
    let radius = i32::try_from(radius).unwrap_or(i32::MAX);
    let (x, z) = chunk_of(position);
    (x - center.0).abs() <= radius && (z - center.1).abs() <= radius
}

/// Build a full self/remote spawn `EntityUpdates`.
pub fn spawn_entity_updates(
    network_id: i32,
    transform: PlayerTransform,
    appearance: &PlayerAppearance,
    auth_skin: Option<PlayerSkin>,
) -> EntityUpdates {
    let skin = auth_skin.or_else(|| appearance.skin.as_ref().map(skin_packet));
    // Java's player entity always carries a `ModelComponent`; the default model
    // is `TempAssetIdUtil.DEFAULT_PLAYER_MODEL_NAME` ("Player"). Sending a null
    // model and then an Equipment update makes the client throw
    // "Attempted to set character model without sending a model first".
    let model = appearance.model.clone().unwrap_or_default();
    let model_update = appearance
        .model_packet
        .clone()
        .unwrap_or_else(|| model_packet(&model));
    let armor_ids = appearance
        .armor
        .iter()
        .map(|item| item.clone().unwrap_or_default())
        .collect();
    EntityUpdates {
        removed: None,
        updates: Some(vec![EntityUpdate {
            network_id,
            removed: None,
            updates: Some(vec![
                ComponentUpdate::ModelUpdate(ModelUpdate {
                    model: Some(model_update),
                    entity_scale: 1.0,
                }),
                ComponentUpdate::PlayerSkinUpdate(PlayerSkinUpdate { skin }),
                ComponentUpdate::EquipmentUpdate(EquipmentUpdate {
                    armor_ids: Some(armor_ids),
                    right_hand_item_id: Some(
                        appearance
                            .right_hand
                            .clone()
                            .unwrap_or_else(|| EMPTY_ITEM_ID.to_string()),
                    ),
                    left_hand_item_id: Some(
                        appearance
                            .left_hand
                            .clone()
                            .unwrap_or_else(|| EMPTY_ITEM_ID.to_string()),
                    ),
                }),
                ComponentUpdate::TransformUpdate(TransformUpdate {
                    transform: model_transform(transform),
                }),
            ]),
        }]),
    }
}

/// Build a transform-only update for a moving player.
pub fn transform_entity_updates(network_id: i32, transform: PlayerTransform) -> EntityUpdates {
    EntityUpdates {
        removed: None,
        updates: Some(vec![EntityUpdate {
            network_id,
            removed: None,
            updates: Some(vec![ComponentUpdate::TransformUpdate(TransformUpdate {
                transform: model_transform(transform),
            })]),
        }]),
    }
}

/// Build an equipment-only update for the active held items.
pub fn equipment_entity_updates(
    network_id: i32,
    right_hand: Option<String>,
    left_hand: Option<String>,
) -> EntityUpdates {
    EntityUpdates {
        removed: None,
        updates: Some(vec![EntityUpdate {
            network_id,
            removed: None,
            updates: Some(vec![ComponentUpdate::EquipmentUpdate(EquipmentUpdate {
                armor_ids: None,
                right_hand_item_id: Some(right_hand.unwrap_or_else(|| EMPTY_ITEM_ID.to_string())),
                left_hand_item_id: Some(left_hand.unwrap_or_else(|| EMPTY_ITEM_ID.to_string())),
            })]),
        }]),
    }
}

/// Build an entity removal (`EntityUpdates.removed` is the network-id list).
pub fn removed_entity_updates(network_id: i32) -> EntityUpdates {
    EntityUpdates {
        removed: Some(vec![network_id]),
        updates: None,
    }
}

/// Network-id space reserved for runtime entities.
///
/// Player network ids start at 1; runtime entity ids start at 1 too, so the
/// presentation layer offsets them to keep the two id sets disjoint.
pub const RUNTIME_ENTITY_NETWORK_OFFSET: i32 = 1_000_000;

/// High bit used for player ids in the NPC target namespace, so a player can
/// never be mistaken for a runtime NPC when melee damage is resolved.
pub const PLAYER_NPC_TARGET_OFFSET: u64 = 1 << 63;

/// The client-facing network id of a runtime entity.
pub fn runtime_network_id(id: u64) -> i32 {
    RUNTIME_ENTITY_NETWORK_OFFSET.saturating_add(id.min(i32::MAX as u64) as i32)
}

/// Build a spawn `EntityUpdates` for a runtime entity.
///
/// Prefab/NPC entities carry their saved model and nameplate documents; item
/// drops carry the item stack. Projectiles use the same model path when their
/// component document provides one.
pub fn runtime_entity_updates(entity: &RuntimeEntity) -> EntityUpdates {
    let mut components = vec![ComponentUpdate::TransformUpdate(TransformUpdate {
        transform: model_transform(entity.transform),
    })];

    if entity.kind == RuntimeEntityKind::Item {
        components.push(ComponentUpdate::ItemUpdate(ItemUpdate {
            item: runtime_item(entity),
            // Java item entities have no `EntityScaleComponent`, so the packet
            // carries `0`. `1.0` renders dropped items at model scale and looks
            // oversized.
            entity_scale: 0.0,
        }));
    } else if let Some((asset_id, scale)) = runtime_entity_model(entity) {
        components.push(ComponentUpdate::ModelUpdate(ModelUpdate {
            model: Some(Model {
                asset_id: Some(asset_id),
                scale,
                ..Model::default()
            }),
            entity_scale: scale,
        }));
    }

    if entity.kind != RuntimeEntityKind::Item {
        if let Some(text) = runtime_entity_name(entity) {
            components.push(ComponentUpdate::NameplateUpdate(NameplateUpdate { text }));
        }
    }

    if let Some(stats) = runtime_entity_stats(entity) {
        components.push(ComponentUpdate::EntityStatsUpdate(stats));
    }
    if let Some(effects) = runtime_entity_effects(entity) {
        components.push(ComponentUpdate::EntityEffectsUpdate(effects));
    }

    EntityUpdates {
        removed: None,
        updates: Some(vec![EntityUpdate {
            network_id: runtime_network_id(entity.id),
            removed: None,
            updates: Some(components),
        }]),
    }
}

fn runtime_item(entity: &RuntimeEntity) -> ItemWithAllMetadata {
    let item = entity.components.get("Item");
    let number = |name: &str| -> f64 {
        item.and_then(|item| item.get(name))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
    };
    ItemWithAllMetadata {
        item_id: entity.item_id().unwrap_or_default().to_string(),
        quantity: entity.item_quantity(),
        durability: number("Durability"),
        max_durability: number("MaxDurability"),
        quality: item
            .and_then(|item| item.get("Quality"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32,
        override_dropped_item_animation: item
            .and_then(|item| item.get("OverrideDroppedItemAnimation"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        metadata: item
            .and_then(|item| item.get("Metadata"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
    }
}

fn runtime_entity_model(entity: &RuntimeEntity) -> Option<(String, f32)> {
    let model = entity.components.get("Model")?.get("Model")?;
    let asset_id = model.get("Id")?.as_str()?.to_string();
    let scale = model
        .get("Scale")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(1.0) as f32;
    Some((asset_id, scale))
}

fn runtime_entity_name(entity: &RuntimeEntity) -> Option<String> {
    entity
        .components
        .get("Nameplate")
        .and_then(|nameplate| nameplate.get("Text"))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

/// Build the per-entity stat init component from an NPC's `EntityStats`
/// document (`{ "<stat index>": { "Value": f32, "Max": f32 } }`).
fn runtime_entity_stats(entity: &RuntimeEntity) -> Option<EntityStatsUpdate> {
    let stats = entity.components.get("EntityStats")?.as_object()?;
    let mut updates = BTreeMap::new();
    for (index, stat) in stats {
        let Ok(index) = index.parse::<i32>() else {
            continue;
        };
        let value = stat
            .get("Value")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32;
        updates.insert(
            index,
            vec![EntityStatUpdate {
                op: EntityStatOp::Init,
                predictable: false,
                value,
                modifiers: None,
                modifier_key: None,
                modifier: None,
            }],
        );
    }
    (!updates.is_empty()).then_some(EntityStatsUpdate {
        entity_stat_updates: updates,
    })
}

/// Build the per-entity active-effect init component from an NPC's
/// `EntityEffects` document.
fn runtime_entity_effects(entity: &RuntimeEntity) -> Option<EntityEffectsUpdate> {
    let effects = entity.components.get("EntityEffects")?.as_object()?;
    let mut updates = Vec::new();
    for (index, effect) in effects {
        let Ok(id) = index.parse::<i32>() else {
            continue;
        };
        updates.push(EntityEffectUpdate {
            r#type: EffectOp::Add,
            id,
            remaining_time: effect
                .get("Remaining")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32,
            infinite: effect
                .get("Infinite")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            debuff: effect
                .get("Debuff")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            status_effect_icon: effect
                .get("Icon")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned),
        });
    }
    (!updates.is_empty()).then_some(EntityEffectsUpdate {
        entity_effect_updates: updates,
    })
}

fn player_marker(player: &PlayerEntry) -> MapMarker {
    MapMarker {
        id: format!("Player-{}", player.uuid),
        name: Some(FormattedMessage {
            raw_text: Some(player.username.clone()),
            ..FormattedMessage::default()
        }),
        marker_image: "Player.png".to_string(),
        transform: ProtocolTransform {
            position: Some(ProtocolPosition {
                x: player.transform.position.x,
                y: player.transform.position.y,
                z: player.transform.position.z,
            }),
            orientation: None,
        },
        context_menu_items: None,
        components: Some(vec![MapMarkerComponent::PlayerMarkerComponent(
            PlayerMarkerComponent {
                player_id: player.uuid,
            },
        )]),
        icon_size: MapMarkerIconSize::Default,
        compass_image: None,
    }
}

/// Convert a saved transform to the protocol model transform.
pub fn model_transform(transform: PlayerTransform) -> ModelTransform {
    let position = ProtocolPosition {
        x: transform.position.x,
        y: transform.position.y,
        z: transform.position.z,
    };
    let body = Direction {
        yaw: transform.rotation.yaw,
        pitch: 0.0,
        roll: 0.0,
    };
    let look = Direction {
        yaw: transform.rotation.yaw,
        pitch: transform.rotation.pitch,
        roll: transform.rotation.roll,
    };
    ModelTransform {
        position: Some(position),
        body_orientation: Some(body),
        look_orientation: Some(look),
    }
}

fn model_packet(reference: &PlayerModelReference) -> Model {
    Model {
        asset_id: Some(reference.id.clone()),
        texture: Some("textures/Unknown.png".to_string()),
        scale: reference.scale,
        ..Model::default()
    }
}

fn skin_packet(skin: &PlayerSkinData) -> PlayerSkin {
    PlayerSkin {
        body_characteristic: skin.body_characteristic.clone(),
        underwear: skin.underwear.clone(),
        face: skin.face.clone(),
        eyes: skin.eyes.clone(),
        ears: skin.ears.clone(),
        mouth: skin.mouth.clone(),
        facial_hair: skin.facial_hair.clone(),
        haircut: skin.haircut.clone(),
        eyebrows: skin.eyebrows.clone(),
        pants: skin.pants.clone(),
        overpants: skin.overpants.clone(),
        undertop: skin.undertop.clone(),
        overtop: skin.overtop.clone(),
        shoes: skin.shoes.clone(),
        head_accessory: skin.head_accessory.clone(),
        face_accessory: skin.face_accessory.clone(),
        ear_accessory: skin.ear_accessory.clone(),
        skin_feature: skin.skin_feature.clone(),
        gloves: skin.gloves.clone(),
        cape: skin.cape.clone(),
    }
}

/// Write one queued message to a session.
pub async fn write_message(
    writer: &mut hytale_net::FrameWriter,
    chunks: &mut hytale_net::ChunkStreamWriter,
    world_map: &mut hytale_net::WorldMapStreamWriter,
    message: PlayerMessage,
    compression_level: i32,
) -> Result<(), PlayerMessageError> {
    match message {
        PlayerMessage::AddToPlayerList(players) => {
            writer
                .write_packet(&AddToServerPlayerList { players }, compression_level)
                .await?;
        }
        PlayerMessage::RemoveFromPlayerList(players) => {
            writer
                .write_packet(&RemoveFromServerPlayerList { players }, compression_level)
                .await?;
        }
        PlayerMessage::EntityUpdates(updates) => {
            writer.write_packet(&updates, compression_level).await?;
        }
        PlayerMessage::BlockUpdate(update) => {
            chunks.write_packet(&update, compression_level).await?;
        }
        PlayerMessage::FluidUpdate(update) => {
            chunks.write_packet(&update, compression_level).await?;
        }
        PlayerMessage::BlockDamage(update) => {
            chunks.write_packet(&update, compression_level).await?;
        }
        PlayerMessage::BlockDamages(update) => {
            chunks.write_packet(&update, compression_level).await?;
        }
        PlayerMessage::WorldMap(update) => {
            world_map.write_packet(&update, compression_level).await?;
        }
        PlayerMessage::Chat(text) => {
            writer
                .write_packet(
                    &ServerMessage {
                        r#type: ChatType::Chat,
                        message: Some(FormattedMessage {
                            raw_text: Some(text),
                            ..FormattedMessage::default()
                        }),
                    },
                    compression_level,
                )
                .await?;
        }
        // Control actions mutably change the target session's state, so the
        // game loop handles them before calling this writer-only function.
        PlayerMessage::Control(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_world::player_data::{Position as WorldPosition, Rotation};

    fn transform(x: f64, z: f64) -> PlayerTransform {
        PlayerTransform::new(
            WorldPosition { x, y: 0.0, z },
            Rotation {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
            },
        )
    }

    fn request(
        uuid: Uuid,
        username: &str,
        center: (i32, i32),
        sender: SyncSender<PlayerMessage>,
    ) -> PlayerJoinRequest {
        PlayerJoinRequest {
            uuid,
            username: username.to_string(),
            world_uuid: Uuid::new(9, 9),
            transform: transform(8.0, 8.0),
            appearance: PlayerAppearance::default(),
            skin: None,
            view_center: center,
            view_radius: 0,
            sender,
        }
    }

    fn drain(receiver: &Receiver<PlayerMessage>) -> Vec<PlayerMessage> {
        receiver.try_iter().collect()
    }

    #[test]
    fn duplicate_identity_is_refused() {
        let registry = Arc::new(PlayerRegistry::new());
        let uuid = Uuid::new(3, 3);
        let (first_tx, _first_rx) = channel();
        let (first, _) = registry
            .register(request(uuid, "First", (0, 0), first_tx))
            .expect("first registration");
        let (second_tx, _second_rx) = channel();
        let error = registry
            .register(request(uuid, "Imposter", (0, 0), second_tx))
            .expect_err("a duplicate identity must be refused");
        assert_eq!(error, PlayerJoinError::DuplicateIdentity(uuid));

        drop(first);
        let (third_tx, _third_rx) = channel();
        assert!(registry
            .register(request(uuid, "First", (0, 0), third_tx))
            .is_ok());
    }

    #[test]
    fn a_full_queue_marks_the_session_for_disconnect() {
        let registry = Arc::new(PlayerRegistry::new());
        let (tx, rx) = channel_with_capacity(1);
        let (_session, join) = registry
            .register(request(Uuid::new(4, 4), "Slow", (0, 0), tx))
            .expect("register");
        let packet = ServerSetBlock {
            x: 0,
            y: 0,
            z: 0,
            block_id: 1,
            filler: 0,
            rotation: 0,
        };
        registry.broadcast_block(packet.clone());
        assert!(!join.overflow.load(Ordering::Relaxed));
        registry.broadcast_block(packet);
        assert!(join.overflow.load(Ordering::Relaxed));
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn saved_skin_preserves_every_cosmetic_slot() {
        let expected = PlayerSkin {
            body_characteristic: Some("BodyA".to_string()),
            underwear: Some("UnderwearA".to_string()),
            face: Some("FaceA".to_string()),
            eyes: Some("EyesA".to_string()),
            ears: Some("EarsA".to_string()),
            mouth: Some("MouthA".to_string()),
            facial_hair: Some("FacialHairA".to_string()),
            haircut: Some("HaircutA".to_string()),
            eyebrows: Some("EyebrowsA".to_string()),
            pants: Some("PantsA".to_string()),
            overpants: Some("OverpantsA".to_string()),
            undertop: Some("UndertopA".to_string()),
            overtop: Some("OvertopA".to_string()),
            shoes: Some("ShoesA".to_string()),
            head_accessory: Some("HeadAccessoryA".to_string()),
            face_accessory: Some("FaceAccessoryA".to_string()),
            ear_accessory: Some("EarAccessoryA".to_string()),
            skin_feature: Some("SkinFeatureA".to_string()),
            gloves: Some("GlovesA".to_string()),
            cape: Some("CapeA".to_string()),
        };
        let appearance = PlayerAppearance {
            skin: Some(PlayerSkinData {
                body_characteristic: expected.body_characteristic.clone(),
                underwear: expected.underwear.clone(),
                face: expected.face.clone(),
                eyes: expected.eyes.clone(),
                ears: expected.ears.clone(),
                mouth: expected.mouth.clone(),
                facial_hair: expected.facial_hair.clone(),
                haircut: expected.haircut.clone(),
                eyebrows: expected.eyebrows.clone(),
                pants: expected.pants.clone(),
                overpants: expected.overpants.clone(),
                undertop: expected.undertop.clone(),
                overtop: expected.overtop.clone(),
                shoes: expected.shoes.clone(),
                head_accessory: expected.head_accessory.clone(),
                face_accessory: expected.face_accessory.clone(),
                ear_accessory: expected.ear_accessory.clone(),
                skin_feature: expected.skin_feature.clone(),
                gloves: expected.gloves.clone(),
                cape: expected.cape.clone(),
            }),
            ..PlayerAppearance::default()
        };

        let updates = spawn_entity_updates(1, transform(0.0, 0.0), &appearance, None);
        let entity = &updates.updates.as_ref().expect("updates")[0];
        let skin = entity
            .updates
            .as_ref()
            .expect("components")
            .iter()
            .find_map(|component| match component {
                ComponentUpdate::PlayerSkinUpdate(update) => update.skin.clone(),
                _ => None,
            })
            .expect("saved skin");
        assert_eq!(skin, expected);
    }

    #[test]
    fn spawn_carries_a_default_model_for_a_new_player() {
        let updates =
            spawn_entity_updates(1, transform(0.0, 0.0), &PlayerAppearance::default(), None);
        let entity = &updates.updates.as_ref().expect("updates")[0];
        let model = entity
            .updates
            .as_ref()
            .expect("components")
            .iter()
            .find_map(|component| match component {
                ComponentUpdate::ModelUpdate(update) => update.model.clone(),
                _ => None,
            })
            .expect("model update");
        assert_eq!(model.asset_id.as_deref(), Some("Player"));
    }

    #[test]
    fn join_lists_and_spawns_are_visible_to_each_other() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, first_join) = registry
            .register(request(Uuid::new(1, 1), "First", (0, 0), first_tx))
            .expect("register");
        assert_eq!(first_join.network_id, 1);
        assert_eq!(first_join.player_list.len(), 1);
        assert_eq!(drain(&first_rx).len(), 0);

        let (second_tx, second_rx) = channel();
        let (second, second_join) = registry
            .register(request(Uuid::new(1, 2), "Second", (0, 0), second_tx))
            .expect("register");
        assert_eq!(second_join.network_id, 2);
        assert_eq!(second_join.player_list.len(), 2);

        let first_messages = drain(&first_rx);
        assert!(first_messages.iter().any(|message| matches!(
            message,
            PlayerMessage::AddToPlayerList(players) if players.len() == 1
        )));
        assert!(first_messages
            .iter()
            .any(|message| matches!(message, PlayerMessage::EntityUpdates(_))));
        let second_messages = drain(&second_rx);
        assert!(
            second_messages
                .iter()
                .any(|message| matches!(message, PlayerMessage::EntityUpdates(_))),
            "the joining player must receive existing spawns: {second_messages:?}"
        );

        drop(first);
        drop(second);
    }

    #[test]
    fn movement_updates_and_removal_reach_visible_viewers() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, _) = registry
            .register(request(Uuid::new(1, 1), "First", (0, 0), first_tx))
            .expect("register");
        let (second_tx, second_rx) = channel();
        let (second, _) = registry
            .register(request(Uuid::new(1, 2), "Second", (0, 0), second_tx))
            .expect("register");
        let _ = drain(&first_rx);
        let _ = drain(&second_rx);

        assert!(registry.update_transform(1, transform(12.0, 12.0)));
        let messages = drain(&second_rx);
        assert!(messages.iter().any(|message| matches!(
            message,
            PlayerMessage::EntityUpdates(updates)
                if updates.updates.as_ref().is_some_and(|updates| updates
                    .iter()
                    .any(|update| update.network_id == 1))
        )));

        drop(second);
        let messages = drain(&first_rx);
        assert!(messages.iter().any(|message| matches!(
            message,
            PlayerMessage::RemoveFromPlayerList(players) if players.len() == 1
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            PlayerMessage::EntityUpdates(updates)
                if updates.removed.as_deref() == Some([2].as_slice())
        )));
        drop(first);
    }

    #[test]
    fn runtime_entities_spawn_only_for_viewers() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, _) = registry
            .register(request(Uuid::new(1, 1), "First", (0, 0), first_tx))
            .expect("register");
        let (second_tx, _) = channel();
        let (second, _) = registry
            .register(PlayerJoinRequest {
                view_center: (10, 10),
                transform: transform(10.0 * 32.0, 10.0 * 32.0),
                ..request(Uuid::new(1, 2), "Second", (10, 10), second_tx)
            })
            .expect("register");
        let _ = drain(&first_rx);

        let entity = RuntimeEntity::new(
            7,
            RuntimeEntityKind::Prefab,
            Some("Hunt.Generic.Site".to_string()),
            hytale_world::runtime::transform([4.0, 64.0, 4.0], [0.0, 0.0, 0.0]),
            serde_json::json!({
                "Model": {"Model": {"Id": "Encounter_Marker", "Scale": 2.0}},
                "Nameplate": {"Text": "Hunt_Site"}
            }),
        );
        registry.broadcast_runtime_entity(&entity);
        let messages = drain(&first_rx);
        let updates = messages
            .iter()
            .find_map(|message| match message {
                PlayerMessage::EntityUpdates(updates) => Some(updates),
                _ => None,
            })
            .expect("the near viewer gets the spawn");
        let update = &updates.updates.as_ref().expect("updates")[0];
        assert_eq!(update.network_id, runtime_network_id(7));
        let components = update.updates.as_ref().expect("components");
        assert!(components.iter().any(|component| matches!(
            component,
            ComponentUpdate::ModelUpdate(model) if model
                .model
                .as_ref()
                .and_then(|model| model.asset_id.as_deref()) == Some("Encounter_Marker")
        )));
        assert!(components.iter().any(|component| matches!(
            component,
            ComponentUpdate::NameplateUpdate(nameplate) if nameplate.text == "Hunt_Site"
        )));

        drop(first);
        drop(second);
    }

    #[test]
    fn npc_spawn_updates_carry_entity_stats_keyed_by_stat_index() {
        let entity = RuntimeEntity::new(
            11,
            RuntimeEntityKind::Npc,
            None,
            hytale_world::runtime::transform([2.0, 64.0, 3.0], [0.0, 0.0, 0.0]),
            serde_json::json!({
                "Npc": {"Role": "Rat", "Health": 8.0, "MaxHealth": 10.0},
                "EntityStats": {
                    "1": {"Value": 8.0, "Max": 10.0}
                }
            }),
        );
        let updates = runtime_entity_updates(&entity);
        let update = &updates.updates.as_ref().expect("updates")[0];
        assert_eq!(update.network_id, runtime_network_id(11));
        let components = update.updates.as_ref().expect("components");
        let stats = components
            .iter()
            .find_map(|component| match component {
                ComponentUpdate::EntityStatsUpdate(stats) => Some(stats),
                _ => None,
            })
            .expect("the NPC spawn carries EntityStatsUpdate");
        assert_eq!(stats.entity_stat_updates[&1][0].value, 8.0);
        assert_eq!(stats.entity_stat_updates[&1][0].op, EntityStatOp::Init);
    }

    #[test]
    fn npc_spawn_updates_carry_active_entity_effects() {
        let entity = RuntimeEntity::new(
            12,
            RuntimeEntityKind::Npc,
            None,
            hytale_world::runtime::transform([2.0, 64.0, 3.0], [0.0, 0.0, 0.0]),
            serde_json::json!({
                "Npc": {"Role": "Rat", "Health": 8.0, "MaxHealth": 10.0},
                "EntityEffects": {
                    "5": {"Remaining": 3.5, "Infinite": false, "Debuff": true, "Icon": "Burn"}
                }
            }),
        );
        let updates = runtime_entity_updates(&entity);
        let components = updates.updates.as_ref().expect("updates")[0]
            .updates
            .as_ref()
            .expect("components");
        let effects = components
            .iter()
            .find_map(|component| match component {
                ComponentUpdate::EntityEffectsUpdate(effects) => Some(effects),
                _ => None,
            })
            .expect("the NPC spawn carries EntityEffectsUpdate");
        assert_eq!(effects.entity_effect_updates[0].id, 5);
        assert_eq!(effects.entity_effect_updates[0].r#type, EffectOp::Add);
        assert!(effects.entity_effect_updates[0].debuff);
        assert_eq!(
            effects.entity_effect_updates[0]
                .status_effect_icon
                .as_deref(),
            Some("Burn")
        );
    }

    #[test]
    fn item_entity_updates_carry_the_full_stack_metadata() {
        let entity = RuntimeEntity::new(
            9,
            RuntimeEntityKind::Item,
            None,
            hytale_world::runtime::transform([1.0, 64.0, 2.0], [0.0, 0.0, 0.0]),
            serde_json::json!({
                "Item": {
                    "Id": "Rock_Stone",
                    "Quantity": 3,
                    "Durability": 0.5,
                    "MaxDurability": 2.0,
                    "Quality": 4,
                    "OverrideDroppedItemAnimation": true,
                    "Metadata": "{\"Test\":true}"
                }
            }),
        );
        let updates = runtime_entity_updates(&entity);
        let update = &updates.updates.expect("updates")[0];
        let Some(ComponentUpdate::ItemUpdate(item)) = update
            .updates
            .as_ref()
            .expect("components")
            .iter()
            .find(|component| matches!(component, ComponentUpdate::ItemUpdate(_)))
        else {
            panic!("the item component");
        };
        assert_eq!(item.item.item_id, "Rock_Stone");
        assert_eq!(item.entity_scale, 0.0);
        assert_eq!(item.item.quantity, 3);
        assert_eq!(item.item.durability, 0.5);
        assert_eq!(item.item.max_durability, 2.0);
        assert_eq!(item.item.quality, 4);
        assert!(item.item.override_dropped_item_animation);
        assert_eq!(item.item.metadata.as_deref(), Some("{\"Test\":true}"));
    }

    #[test]
    fn player_markers_follow_joins_moves_and_leaves() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, _) = registry
            .register(request(Uuid::new(1, 1), "First", (0, 0), first_tx))
            .expect("register");
        let (second_tx, _second_rx) = channel();
        let (second, second_join) = registry
            .register(request(Uuid::new(1, 2), "Second", (0, 0), second_tx))
            .expect("register");
        assert!(second_join
            .player_markers
            .iter()
            .any(|marker| marker.id == format!("Player-{}", Uuid::new(1, 1))));

        let first_messages = drain(&first_rx);
        assert!(first_messages.iter().any(|message| matches!(
            message,
            PlayerMessage::WorldMap(update)
                if update.added_markers.as_ref().is_some_and(|markers| markers
                    .iter()
                    .any(|marker| marker.id == format!("Player-{}", Uuid::new(1, 2))))
        )));

        assert!(registry.update_transform(2, transform(12.0, 12.0)));
        let first_messages = drain(&first_rx);
        assert!(first_messages.iter().any(|message| matches!(
            message,
            PlayerMessage::WorldMap(update)
                if update.added_markers.as_ref().is_some_and(|markers| markers
                    .iter()
                    .any(|marker| marker.id == format!("Player-{}", Uuid::new(1, 2))))
        )));

        drop(second);
        let first_messages = drain(&first_rx);
        assert!(first_messages.iter().any(|message| matches!(
            message,
            PlayerMessage::WorldMap(update)
                if update.removed_markers.as_deref() == Some([format!("Player-{}", Uuid::new(1, 2))].as_slice())
        )));
        drop(first);
    }

    #[test]
    fn visibility_filter_drops_far_entities() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, _) = registry
            .register(request(Uuid::new(1, 1), "First", (0, 0), first_tx))
            .expect("register");
        let (second_tx, _) = channel();
        let (second, _) = registry
            .register(PlayerJoinRequest {
                view_center: (10, 10),
                transform: transform(10.0 * 32.0, 10.0 * 32.0),
                ..request(Uuid::new(1, 2), "Second", (10, 10), second_tx)
            })
            .expect("register");
        let messages = drain(&first_rx);
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, PlayerMessage::EntityUpdates(_))),
            "far entity must not spawn: {messages:?}"
        );
        drop(first);
        drop(second);
    }

    #[test]
    fn chat_broadcasts_and_control_actions_reach_every_player() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, _) = registry
            .register(request(Uuid::new(2, 1), "First", (0, 0), first_tx))
            .expect("register");
        let (second_tx, second_rx) = channel();
        let (second, _) = registry
            .register(request(Uuid::new(2, 2), "Second", (0, 0), second_tx))
            .expect("register");
        let _ = drain(&first_rx);
        let _ = drain(&second_rx);

        assert_eq!(registry.network_id_for("sEcOnD"), Some(2));
        assert_eq!(
            registry.player_positions(),
            [
                ("First".to_string(), [8.0, 0.0, 8.0]),
                ("Second".to_string(), [8.0, 0.0, 8.0])
            ]
        );
        registry.broadcast_chat("hello");
        assert!(drain(&first_rx)
            .iter()
            .any(|message| matches!(message, PlayerMessage::Chat(text) if text == "hello")));
        assert!(drain(&second_rx)
            .iter()
            .any(|message| matches!(message, PlayerMessage::Chat(text) if text == "hello")));

        assert!(registry.queue_control(2, ControlAction::Kill));
        assert!(drain(&second_rx)
            .iter()
            .any(|message| matches!(message, PlayerMessage::Control(ControlAction::Kill))));
        assert!(!registry.queue_control(99, ControlAction::Respawn));

        drop(first);
        drop(second);
    }
}
