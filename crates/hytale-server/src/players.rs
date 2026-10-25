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
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use hytale_protocol::generated::common::{
    ComponentUpdate, Direction, EntityUpdate, EquipmentUpdate, Model, ModelTransform, ModelUpdate,
    PlayerSkin, PlayerSkinUpdate, Position as ProtocolPosition, ServerPlayerListPlayer,
    TransformUpdate,
};
use hytale_protocol::generated::entities::EntityUpdates;
use hytale_protocol::generated::interface_::{AddToServerPlayerList, RemoveFromServerPlayerList};
use hytale_protocol::generated::world::{ServerSetBlock, ServerSetFluid};
use hytale_protocol::io::Uuid;
use hytale_world::player_data::{
    PlayerAppearance, PlayerModelReference, PlayerSkinData, PlayerTransform, EMPTY_ITEM_ID,
};

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
}

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
    pub sender: Sender<PlayerMessage>,
}

/// What registration returned to the joining session.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerJoin {
    /// Assigned network/entity id (used by `SetClientId` and the self spawn).
    pub network_id: i32,
    /// The complete player list to send to the joining client (self included).
    pub player_list: Vec<ServerPlayerListPlayer>,
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
    sender: Sender<PlayerMessage>,
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
    pub fn register(self: &Arc<Self>, request: PlayerJoinRequest) -> (PlayerSession, PlayerJoin) {
        let mut inner = self.inner.lock().expect("player registry poisoned");
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

        let player_list = inner
            .players
            .values()
            .map(PlayerEntry::list_entry)
            .collect::<Vec<_>>();
        recompute_visibility(&mut inner);
        drop(inner);

        (
            PlayerSession {
                registry: Arc::clone(self),
                network_id,
            },
            PlayerJoin {
                network_id,
                player_list,
            },
        )
    }

    /// Remove a session, returning its UUID when it was registered.
    pub fn unregister(&self, network_id: i32) -> Option<Uuid> {
        let mut inner = self.inner.lock().expect("player registry poisoned");
        let entry = inner.players.remove(&network_id)?;
        let uuid = entry.uuid;
        let list = vec![uuid];
        for player in inner.players.values() {
            queue(player, PlayerMessage::RemoveFromPlayerList(list.clone()));
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

/// Create the queue pair used by one session.
pub fn channel() -> (Sender<PlayerMessage>, Receiver<PlayerMessage>) {
    mpsc::channel()
}

fn queue(player: &PlayerEntry, message: PlayerMessage) {
    let _ = player.sender.send(message);
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
    for (viewer_id, viewer) in &inner.players {
        if *viewer_id != network_id && viewer.visible.contains(&network_id) {
            queue(viewer, PlayerMessage::EntityUpdates(updates.clone()));
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
                    model: appearance.model.as_ref().map(model_packet),
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
        sender: Sender<PlayerMessage>,
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
    fn join_lists_and_spawns_are_visible_to_each_other() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, first_join) =
            registry.register(request(Uuid::new(1, 1), "First", (0, 0), first_tx));
        assert_eq!(first_join.network_id, 1);
        assert_eq!(first_join.player_list.len(), 1);
        assert_eq!(drain(&first_rx).len(), 0);

        let (second_tx, second_rx) = channel();
        let (second, second_join) =
            registry.register(request(Uuid::new(1, 2), "Second", (0, 0), second_tx));
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
        let (first, _) = registry.register(request(Uuid::new(1, 1), "First", (0, 0), first_tx));
        let (second_tx, second_rx) = channel();
        let (second, _) = registry.register(request(Uuid::new(1, 2), "Second", (0, 0), second_tx));
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
    fn visibility_filter_drops_far_entities() {
        let registry = Arc::new(PlayerRegistry::new());
        let (first_tx, first_rx) = channel();
        let (first, _) = registry.register(request(Uuid::new(1, 1), "First", (0, 0), first_tx));
        let (second_tx, _) = channel();
        let (second, _) = registry.register(PlayerJoinRequest {
            view_center: (10, 10),
            transform: transform(10.0 * 32.0, 10.0 * 32.0),
            ..request(Uuid::new(1, 2), "Second", (10, 10), second_tx)
        });
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
}
