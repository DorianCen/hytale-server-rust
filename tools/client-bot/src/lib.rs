//! Shared client-bot logic: Connect construction, framing and packet logging.
//!
//! The binary is intentionally small: it sends one `Connect`, then logs framed
//! packets until the server disconnects or the timeout expires. Packet handlers
//! and the full join flow land in later tasks.

use std::collections::BTreeMap;
use std::time::Duration;

use hytale_auth::{AuthService, SCOPE_CLIENT};
use hytale_net::{
    ChunkFrame, ChunkStreamError, ChunkStreamReader, ClientSession, FrameReader, FrameStreamError,
    FrameWriter,
};
use hytale_protocol::frame::{decode_frame, encode_frame, MAX_PACKET_SIZE};
use hytale_protocol::generated::assets::{
    UpdateBlockBreakingDecals, UpdateBlockHitboxes, UpdateBlockParticleSets, UpdateBlockSoundSets,
    UpdateBlockTypes, UpdateEntityEffects, UpdateEntityStatTypes, UpdateEnvironments,
    UpdateFluidFX, UpdateFluids, UpdateItemCategories, UpdateItemPlayerAnimations,
    UpdateItemQualities, UpdateItemReticles, UpdateItemSoundSets, UpdateItems,
    UpdatePhysicalMaterials, UpdateProjectileConfigs, UpdateSoundEvents, UpdateTagPatterns,
    UpdateTranslations, UpdateWeathers,
};
use hytale_protocol::generated::auth::{AuthGrant, AuthToken, ConnectAccept, ServerAuthToken};
use hytale_protocol::generated::common::{
    Asset, BlockDamageCmd, BlockPosition, ClientDisconnectReason, ComponentUpdate, DisconnectType,
    FormattedMessage, MapMarker, MouseButtonEvent, MouseButtonState, MouseButtonType,
    MovementSettings, Page, PlayerSkin, PongType, Position, TeleportAck, WindowType,
    WorldInteraction,
};
use hytale_protocol::generated::connection::{
    ClientDisconnect, ClientType, Connect, InsecurePlayerOptions, NetworkTick, Ping, Pong,
    RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::entities::{EntityUpdates, SetEntitySeed};
use hytale_protocol::generated::interface_::{
    AddToServerPlayerList, RemoveFromServerPlayerList, SetPage,
};
use hytale_protocol::generated::inventory::{SetActiveSlot, UpdatePlayerInventory};
use hytale_protocol::generated::player::{
    ClientMovement, ClientReady, ClientTeleport, JoinWorld, MouseInteraction, SetClientId,
    UpdateMovementSettings,
};
use hytale_protocol::generated::setup::{
    AssetFinalize, AssetInitialize, AssetPart, RequestAssets, RequestCommonAssetsRebuild,
    ServerTags, SetTimeDilation, SetUpdateRate, SetupFinalize, UpdateFeatures, ViewRadius,
    WorldLoadFinished, WorldSettings,
};
use hytale_protocol::generated::window::{CloseWindow, OpenWindow, UpdateWindow};
use hytale_protocol::generated::wire::{Packet, Vector2f, Wire};
use hytale_protocol::generated::world::{
    ServerSetBlock, SetChunk, SetChunkEnvironments, SetChunkTintmap, SetColumn, SetFluids,
    UnloadChunk, UpdateBlockDamage, UpdateBlockDamages, UpdateTime, UpdateTimeSettings,
    WorldGameplayConfig,
};
use hytale_protocol::generated::worldmap::{UpdateWorldMap, UpdateWorldMapSettings};
use hytale_protocol::io::{ProtocolError, Reader, Uuid, Writer};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Java `ProtocolSettings.PROTOCOL_CRC`.
pub const DEFAULT_PROTOCOL_CRC: i32 = -386_561_135;
/// Java `ProtocolSettings.PROTOCOL_BUILD_NUMBER`.
pub const DEFAULT_PROTOCOL_BUILD: i32 = 258;
/// Java `ProtocolSettings.PROTOCOL_VERSION`.
pub const DEFAULT_PROTOCOL_VERSION: i32 = 3;

/// Build the initial client `Connect` packet.
pub fn build_connect(
    protocol_crc: i32,
    protocol_build: i32,
    client_version: &str,
    language: &str,
    identity_token: Option<String>,
) -> Connect {
    Connect {
        protocol_crc,
        protocol_build_number: protocol_build,
        client_version: client_version.to_string(),
        client_type: ClientType::Game,
        identity_token,
        language: language.to_string(),
        referral_data: None,
        referral_source: None,
    }
}

/// Encode one packet payload.
pub fn encode_payload<T: Wire>(packet: &T) -> Result<Vec<u8>, ProtocolError> {
    let mut writer = Writer::new();
    packet.encode(&mut writer)?;
    Ok(writer.into_inner())
}

/// Encode any generated packet as a complete frame (`[len][id][payload]`).
pub fn build_frame<T: Packet>(packet: &T) -> Result<Vec<u8>, ProtocolError> {
    let payload = encode_payload(packet)?;
    let mut frame = Vec::new();
    encode_frame(T::ID, &payload, &mut frame)?;
    Ok(frame)
}

/// Encode `Connect` as a complete frame (`[len][id][payload]`).
pub fn build_connect_frame(connect: &Connect) -> Result<Vec<u8>, ProtocolError> {
    build_frame(connect)
}

/// A fully owned frame copied out of a stream buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRecord {
    /// Packet id.
    pub packet_id: u32,
    /// Payload bytes.
    pub payload: Vec<u8>,
}

impl FrameRecord {
    /// Name of a packet in the connection family, or `None` for other families.
    pub fn name(&self) -> Option<&'static str> {
        connection_packet_name(self.packet_id)
    }

    /// Decode a known connection-family packet into a readable line.
    pub fn describe(&self) -> Option<String> {
        decode_connection_packet(self.packet_id, &self.payload).map(|value| match value {
            Ok(description) => description,
            Err(error) => format!("decode error: {error}"),
        })
    }
}

/// Drain every complete frame from `buffer`, leaving an incomplete tail.
pub fn drain_frames(buffer: &mut Vec<u8>) -> Result<Vec<FrameRecord>, ProtocolError> {
    let mut frames = Vec::new();
    while let Some((frame, consumed)) = decode_frame(buffer, MAX_PACKET_SIZE)? {
        frames.push(FrameRecord {
            packet_id: frame.packet_id,
            payload: frame.payload.to_vec(),
        });
        buffer.drain(..consumed);
    }
    Ok(frames)
}

/// A decoded chunk packet, summarized for the bot's log/assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkPacketSummary {
    /// `SetColumn`: heightmap + tint.
    Column {
        /// Chunk x.
        x: i32,
        /// Chunk z.
        z: i32,
        /// Number of heightmap entries (Java sends 1024).
        heightmap_entries: usize,
        /// Tint palette bytes.
        tintmap_bytes: usize,
    },
    /// `SetChunkTintmap`.
    Tintmap {
        /// Chunk x.
        x: i32,
        /// Chunk z.
        z: i32,
        /// Tint palette bytes.
        tintmap_bytes: usize,
    },
    /// `SetChunk`.
    Chunk {
        /// Chunk x.
        x: i32,
        /// Section y.
        y: i32,
        /// Chunk z.
        z: i32,
        /// Block palette bytes (0 for an all-air section).
        data_bytes: usize,
        /// Local light bytes.
        local_light_bytes: usize,
        /// Global light bytes.
        global_light_bytes: usize,
    },
    /// `SetChunkEnvironments`.
    Environments {
        /// Chunk x.
        x: i32,
        /// Section y.
        y: i32,
        /// Chunk z.
        z: i32,
        /// Environment bytes.
        data_bytes: usize,
    },
    /// `SetFluids`.
    Fluids {
        /// Chunk x.
        x: i32,
        /// Section y.
        y: i32,
        /// Chunk z.
        z: i32,
        /// Fluid bytes.
        data_bytes: usize,
    },
    /// `UnloadChunk`.
    Unload {
        /// Chunk x.
        x: i32,
        /// Chunk z.
        z: i32,
    },
}

/// Summarize one chunk-stream frame, or `None` for a non-chunk packet.
pub fn summarize_chunk_packet(
    frame: &ChunkFrame,
) -> Option<Result<ChunkPacketSummary, ProtocolError>> {
    if frame.packet_id == SetColumn::ID {
        return Some(
            SetColumn::decode(&mut Reader::new(&frame.payload)).map(|packet| {
                ChunkPacketSummary::Column {
                    x: packet.x,
                    z: packet.z,
                    heightmap_entries: packet.heightmap.len(),
                    tintmap_bytes: packet.tintmap.len(),
                }
            }),
        );
    }
    if frame.packet_id == SetChunkTintmap::ID {
        return Some(
            SetChunkTintmap::decode(&mut Reader::new(&frame.payload)).map(|packet| {
                ChunkPacketSummary::Tintmap {
                    x: packet.x,
                    z: packet.z,
                    tintmap_bytes: packet.tintmap.len(),
                }
            }),
        );
    }
    if frame.packet_id == SetChunk::ID {
        return Some(
            SetChunk::decode(&mut Reader::new(&frame.payload)).map(|packet| {
                ChunkPacketSummary::Chunk {
                    x: packet.x,
                    y: packet.y,
                    z: packet.z,
                    data_bytes: packet.data.as_ref().map_or(0, Vec::len),
                    local_light_bytes: packet.local_light.as_ref().map_or(0, Vec::len),
                    global_light_bytes: packet.global_light.as_ref().map_or(0, Vec::len),
                }
            }),
        );
    }
    if frame.packet_id == SetChunkEnvironments::ID {
        return Some(
            SetChunkEnvironments::decode(&mut Reader::new(&frame.payload)).map(|packet| {
                ChunkPacketSummary::Environments {
                    x: packet.x,
                    y: packet.y,
                    z: packet.z,
                    data_bytes: packet.environments.len(),
                }
            }),
        );
    }
    if frame.packet_id == SetFluids::ID {
        return Some(
            SetFluids::decode(&mut Reader::new(&frame.payload)).map(|packet| {
                ChunkPacketSummary::Fluids {
                    x: packet.x,
                    y: packet.y,
                    z: packet.z,
                    data_bytes: packet.data.as_ref().map_or(0, Vec::len),
                }
            }),
        );
    }
    if frame.packet_id == UnloadChunk::ID {
        return Some(
            UnloadChunk::decode(&mut Reader::new(&frame.payload)).map(|packet| {
                ChunkPacketSummary::Unload {
                    x: packet.chunk_x,
                    z: packet.chunk_z,
                }
            }),
        );
    }
    None
}

/// Options for the dev/insecure join flow.
#[derive(Debug, Clone)]
pub struct DevJoinOptions {
    /// Player UUID sent in `InsecurePlayerOptions`.
    pub uuid: Uuid,
    /// Username sent in `InsecurePlayerOptions`.
    pub username: String,
    /// Requested view radius, in chunks.
    pub view_radius_chunks: u32,
    /// Optional skin advertised through `InsecurePlayerOptions`.
    pub skin: Option<PlayerSkin>,
    /// Move to this position after the initial chunk snapshot.
    pub movement_after_join: Option<Position>,
    /// Keep reading the setup stream for this long after the join and collect
    /// other-player list/entity broadcasts (`None` = stop immediately).
    pub observe_after_join: Option<Duration>,
    /// Close the connection this long after `dev_join` starts (for disconnect
    /// visibility tests while another bot is observing).
    pub disconnect_after: Option<Duration>,
    /// Keep the setup/chunk streams alive in a background task after the report
    /// is built, so the session stays registered for later joins.
    pub keep_alive: bool,
    /// Place this block with this item after the join (right click).
    pub place_block_after_join: Option<(BlockPosition, String)>,
    /// Break this block after the join (left click).
    pub break_block_after_join: Option<BlockPosition>,
    /// Extra right-click block uses after the snapshot (doors/containers).
    pub use_block_after_join: Vec<BlockPosition>,
    /// Send this absolute movement after the block edits, so a collision test can
    /// place a wall first and then walk into it.
    pub movement_after_edits: Option<Position>,
    /// Send `ClientReady(readyForGameplay)` after the snapshot so the server
    /// starts the teleport/time flow.
    pub send_ready_for_gameplay: bool,
}

/// World-join state observed by the bot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameJoinReport {
    /// `ViewRadius` arrived on the chunks stream.
    pub view_radius: Option<i32>,
    /// `JoinWorld` arrived on the setup stream.
    pub join_world: bool,
    /// `SetClientId` value.
    pub client_id: Option<i32>,
    /// `SetEntitySeed` value.
    pub entity_seed: Option<i32>,
    /// `SetTimeDilation` value.
    pub time_dilation: Option<f32>,
    /// `SetUpdateRate` value.
    pub update_rate: Option<i32>,
    /// `AddToServerPlayerList` arrived.
    pub player_list: bool,
    /// UUIDs in the initial `AddToServerPlayerList`.
    pub player_list_uuids: Vec<Uuid>,
    /// The self `EntityUpdates` spawn arrived.
    pub self_spawn: bool,
    /// Self-spawn model asset id (`ModelUpdate.model.assetId`).
    pub self_spawn_model: Option<String>,
    /// Self-spawn skin (`PlayerSkinUpdate.skin`).
    pub self_spawn_skin: Option<PlayerSkin>,
    /// Self-spawn armor ids (`EquipmentUpdate.armorIds`).
    pub self_spawn_armor: Option<Vec<String>>,
    /// Self-spawn right-hand item id.
    pub self_spawn_right_hand: Option<String>,
    /// Self-spawn left-hand item id.
    pub self_spawn_left_hand: Option<String>,
    /// Self-spawn transform position (saved position restored on reconnect).
    pub self_spawn_position: Option<Position>,
    /// `WorldGameplayConfig` payload.
    pub gameplay_config: Option<WorldGameplayConfig>,
    /// Latest `UpdateMovementSettings` payload.
    pub movement_settings: Option<MovementSettings>,
    /// Latest `UpdatePlayerInventory` payload.
    pub inventory: Option<UpdatePlayerInventory>,
    /// Latest `SetActiveSlot` selections keyed by inventory section id.
    pub active_slots: BTreeMap<i32, i32>,
    /// Initial `UpdateTimeSettings` payload.
    pub time_settings: Option<UpdateTimeSettings>,
    /// Initial `UpdateTime` value.
    pub initial_time: Option<(i64, i32)>,
    /// Initial world-map settings.
    pub world_map_settings: Option<UpdateWorldMapSettings>,
    /// The bot sent `ClientReady(readyForChunks)`.
    pub sent_ready_for_chunks: bool,
}

/// What the dev/insecure join produced.
#[derive(Debug, Clone, PartialEq)]
pub struct DevJoinReport {
    /// Server packets seen during the handshake, in order.
    pub handshake_packets: Vec<u32>,
    /// World-join state.
    pub game: GameJoinReport,
    /// Decoded chunk packets received after setup.
    pub chunks: Vec<ChunkPacketSummary>,
    /// Player-list and entity broadcasts collected after the join.
    pub observation: PlayerObservation,
}

/// One entity update seen during the post-join observation window.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedEntityUpdate {
    /// Entity network id.
    pub network_id: i32,
    /// The entity was removed (`EntityUpdates.removed`).
    pub removed: bool,
    /// The last transform position carried by the update.
    pub position: Option<Position>,
    /// The model asset id carried by a spawn.
    pub model_asset: Option<String>,
    /// Armor ids carried by a spawn.
    pub armor: Option<Vec<String>>,
}

/// One block edit seen on the chunks stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedBlockUpdate {
    /// Block x.
    pub x: i32,
    /// Block y.
    pub y: i32,
    /// Block z.
    pub z: i32,
    /// New block id.
    pub block_id: i32,
}

/// One block-health change seen on the chunks stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObservedBlockDamage {
    /// Block x.
    pub x: i32,
    /// Block y.
    pub y: i32,
    /// Block z.
    pub z: i32,
    /// Absolute remaining health.
    pub damage: f32,
    /// Change in health.
    pub delta: f32,
}

/// One world-map marker seen on the default stream.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedMapMarker {
    /// Marker id.
    pub id: String,
    /// Marker image.
    pub image: String,
    /// Display name (`messageId` or raw text).
    pub name: Option<String>,
    /// Block position.
    pub position: Option<Position>,
}

/// One world-map chunk image (or unload) from `UpdateWorldMap.chunks`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedMapChunk {
    /// Chunk x.
    pub chunk_x: i32,
    /// Chunk z.
    pub chunk_z: i32,
    /// False for an unload (`image: None`).
    pub loaded: bool,
    /// Image width when loaded.
    pub width: Option<i32>,
    /// Image height when loaded.
    pub height: Option<i32>,
    /// Palette entry count when loaded.
    pub palette_entries: Option<usize>,
}

/// One window packet seen during the observation window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWindow {
    /// Window id.
    pub id: i32,
    /// Packet that carried it.
    pub event: ObservedWindowEvent,
    /// `OpenWindow.windowType` when opened.
    pub window_type: Option<WindowType>,
    /// `OpenWindow.windowData` JSON string when opened.
    pub window_data: Option<String>,
    /// Container capacity when present.
    pub capacity: Option<i16>,
    /// Occupied container slots when present.
    pub occupied_slots: Option<usize>,
}

/// Window packet kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservedWindowEvent {
    /// `OpenWindow`.
    Open,
    /// `UpdateWindow`.
    Update,
    /// `CloseWindow`.
    Close,
}

/// Player-list, entity and block broadcasts observed after a join.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerObservation {
    /// Player UUIDs added to the server player list.
    pub added: Vec<Uuid>,
    /// Player UUIDs removed from the server player list.
    pub removed: Vec<Uuid>,
    /// Entity spawn/update/removal batches.
    pub entities: Vec<ObservedEntityUpdate>,
    /// Block edits from the chunks stream.
    pub block_updates: Vec<ObservedBlockUpdate>,
    /// Block-health changes from the chunks stream.
    pub block_damages: Vec<ObservedBlockDamage>,
    /// World-map markers from the default stream.
    pub map_markers: Vec<ObservedMapMarker>,
    /// World-map chunk images/unloads from the default stream.
    pub map_chunks: Vec<ObservedMapChunk>,
    /// Window open/update/close packets.
    pub windows: Vec<ObservedWindow>,
    /// `SetPage` values.
    pub pages: Vec<Page>,
    /// Authoritative `ClientTeleport` corrections received from the server.
    pub teleports: Vec<Position>,
}

/// Errors from the dev join.
#[derive(Debug, Error)]
pub enum DevJoinError {
    /// Opening the setup stream failed.
    #[error("opening the setup stream failed: {0}")]
    Connect(String),
    /// A framed packet failed.
    #[error(transparent)]
    Frame(#[from] FrameStreamError),
    /// The chunk stream failed.
    #[error(transparent)]
    Chunk(#[from] ChunkStreamError),
    /// The server disconnected or closed the stream.
    #[error("server closed the connection during setup")]
    Closed,
    /// The server sent a disconnect.
    #[error("server disconnected: {0}")]
    Disconnected(String),
    /// The post-setup game transition failed.
    #[error("game join failed: {0}")]
    Game(String),
}

/// Run the dev/insecure `Connect` → `SetupFinalize` handshake, then receive the
/// server's chunk stream. This mirrors the Java insecure-mode branch; it does
/// not authenticate.
pub async fn dev_join(
    session: &ClientSession,
    options: &DevJoinOptions,
) -> Result<DevJoinReport, DevJoinError> {
    let (send, recv) = session
        .connection()
        .open_bi()
        .await
        .map_err(|error| DevJoinError::Connect(error.to_string()))?;
    let mut writer = FrameWriter::new(send);
    let mut reader = FrameReader::new(recv);
    let connect = build_connect(
        DEFAULT_PROTOCOL_CRC,
        DEFAULT_PROTOCOL_BUILD,
        "2026.01.01",
        "en-US",
        None,
    );
    writer.write_packet(&connect, 3).await?;

    let mut handshake_packets = Vec::new();
    loop {
        let frame = reader.next_frame().await?.ok_or(DevJoinError::Closed)?;
        handshake_packets.push(frame.packet_id);
        match frame.packet_id {
            RequestInsecurePlayerOptions::ID => {
                writer
                    .write_packet(
                        &InsecurePlayerOptions {
                            uuid: options.uuid,
                            username: options.username.clone(),
                            skin: options.skin.clone(),
                        },
                        3,
                    )
                    .await?;
            }
            ConnectAccept::ID => {
                writer
                    .write_packet(
                        &ViewRadius {
                            value: (options.view_radius_chunks * 32) as i32,
                        },
                        3,
                    )
                    .await?;
            }
            WorldSettings::ID => {
                writer
                    .write_packet(
                        &RequestAssets {
                            assets: Some(Vec::new()),
                        },
                        3,
                    )
                    .await?;
            }
            WorldLoadFinished::ID => {
                writer.write_packet(&SetupFinalize {}, 3).await?;
                break;
            }
            ServerDisconnect::ID => {
                let disconnect: ServerDisconnect = frame.decode()?;
                let reason = disconnect
                    .reason
                    .and_then(|message| message.raw_text)
                    .unwrap_or_else(|| "disconnect".to_string());
                return Err(DevJoinError::Disconnected(reason));
            }
            Ping::ID => {
                let ping: Ping = frame.decode()?;
                writer
                    .write_packet(
                        &Pong {
                            id: ping.id,
                            time: ping.time,
                            r#type: PongType::Raw,
                            packet_queue_size: 0,
                        },
                        3,
                    )
                    .await?;
            }
            _ => {}
        }
    }

    let (mut game, mut chunks, mut pending) = enter_game_client(session, &mut reader, &mut writer)
        .await
        .map_err(|error| DevJoinError::Game(error.to_string()))?;
    let chunk_packets =
        receive_initial_snapshot(&mut chunks, &mut pending, options.view_radius_chunks)
            .await
            .map_err(|error| DevJoinError::Game(error.to_string()))?;

    if options.send_ready_for_gameplay {
        writer
            .write_packet(
                &ClientReady {
                    ready_for_chunks: false,
                    ready_for_gameplay: true,
                },
                3,
            )
            .await?;
    }
    if let Some(position) = options.movement_after_join.clone() {
        writer
            .write_packet(
                &ClientMovement {
                    absolute_position: Some(position),
                    ..ClientMovement::default()
                },
                3,
            )
            .await?;
    }
    if let Some((position, item)) = options.place_block_after_join.clone() {
        send_block_interaction(&mut writer, MouseButtonType::Right, position, Some(item)).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    if let Some(position) = options.break_block_after_join.clone() {
        send_block_interaction(&mut writer, MouseButtonType::Left, position, None).await?;
    }
    for position in &options.use_block_after_join {
        send_block_interaction(&mut writer, MouseButtonType::Right, position.clone(), None).await?;
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    if let Some(position) = options.movement_after_edits.clone() {
        writer
            .write_packet(
                &ClientMovement {
                    absolute_position: Some(position),
                    ..ClientMovement::default()
                },
                3,
            )
            .await?;
    }
    let disconnect_at = options
        .disconnect_after
        .map(|after| tokio::time::Instant::now() + after);
    let observation = match options.observe_after_join {
        Some(window) => {
            observe_players(
                &mut reader,
                &mut writer,
                &mut chunks,
                game.client_id,
                window,
                disconnect_at,
                &mut game,
            )
            .await?
        }
        None => PlayerObservation::default(),
    };
    if options.keep_alive {
        tokio::spawn(keep_alive(reader, writer, chunks));
    }

    Ok(DevJoinReport {
        handshake_packets,
        game,
        chunks: chunk_packets,
        observation,
    })
}

/// Hold the setup and chunk streams open for a connected bot.
///
/// The server ends a session when its control stream reaches EOF, so a bot that
/// wants to remain visible to later joins must keep reading the stream even
/// after its join report has been produced.
async fn keep_alive(
    mut reader: FrameReader,
    mut writer: FrameWriter,
    mut chunks: ChunkStreamReader,
) {
    let mut chunks_open = true;
    loop {
        tokio::select! {
            frame = reader.next_frame() => match frame {
                Ok(Some(frame)) if frame.packet_id == Ping::ID => {
                    let _ = answer_ping(&mut writer, &frame).await;
                }
                Ok(Some(frame)) if frame.packet_id == ServerDisconnect::ID => break,
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            },
            frame = chunks.next_frame(), if chunks_open => match frame {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => chunks_open = false,
            },
        }
    }
}

/// Read player-list/entity broadcasts for `window`, answering pings.
///
/// A closed connection ends the observation gracefully: the collected data is
/// still returned so a disconnect test can assert what the other client saw.
async fn observe_players(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    chunks: &mut ChunkStreamReader,
    self_id: Option<i32>,
    window: Duration,
    mut disconnect_at: Option<tokio::time::Instant>,
    game: &mut GameJoinReport,
) -> Result<PlayerObservation, FrameStreamError> {
    let mut observation = PlayerObservation::default();
    let deadline = tokio::time::Instant::now() + window;
    let mut chunks_open = true;
    loop {
        if let Some(at) = disconnect_at {
            if tokio::time::Instant::now() >= at {
                writer
                    .write_packet(
                        &ClientDisconnect {
                            reason: ClientDisconnectReason::PlayerLeave,
                            r#type: DisconnectType::Disconnect,
                        },
                        3,
                    )
                    .await?;
                disconnect_at = None;
            }
        }
        let until = disconnect_at.map_or(deadline, |at| at.min(deadline));
        tokio::select! {
            frame = tokio::time::timeout_at(until, reader.next_frame()) => {
                let frame = match frame {
                    Ok(Ok(Some(frame))) => frame,
                    Ok(Ok(None)) | Ok(Err(_)) => break,
                    Err(_) if !disconnect_at.is_some_and(|at| tokio::time::Instant::now() >= at) => break,
                    Err(_) => continue,
                };
                match frame.packet_id {
                    AddToServerPlayerList::ID => {
                        let packet: AddToServerPlayerList = frame.decode()?;
                        observation
                            .added
                            .extend(packet.players.into_iter().map(|player| player.uuid));
                    }
                    RemoveFromServerPlayerList::ID => {
                        let packet: RemoveFromServerPlayerList = frame.decode()?;
                        observation.removed.extend(packet.players);
                    }
                    EntityUpdates::ID => {
                        let packet: EntityUpdates = frame.decode()?;
                        record_observed_entities(&packet, &mut observation, self_id);
                    }
                    ClientTeleport::ID => {
                        let packet: ClientTeleport = frame.decode()?;
                        if let Some(position) = packet.model_transform.position {
                            observation.teleports.push(position);
                        }
                    }
                    UpdateMovementSettings::ID => {
                        let packet: UpdateMovementSettings = frame.decode()?;
                        game.movement_settings = Some(packet.movement_settings);
                    }
                    UpdateWorldMapSettings::ID => {
                        game.world_map_settings = Some(frame.decode()?);
                    }
                    UpdateWorldMap::ID => {
                        let packet: UpdateWorldMap = frame.decode()?;
                        if let Some(chunks) = packet.chunks {
                            observation.map_chunks.extend(chunks.into_iter().map(|chunk| {
                                let image = chunk.image;
                                ObservedMapChunk {
                                    chunk_x: chunk.chunk_x,
                                    chunk_z: chunk.chunk_z,
                                    loaded: image.is_some(),
                                    width: image.as_ref().map(|image| image.width),
                                    height: image.as_ref().map(|image| image.height),
                                    palette_entries: image
                                        .as_ref()
                                        .and_then(|image| image.palette.as_ref())
                                        .map(Vec::len),
                                }
                            }));
                        }
                        if let Some(markers) = packet.added_markers {
                            observation.map_markers.extend(
                                markers.into_iter().filter_map(observed_map_marker),
                            );
                        }
                    }
                    OpenWindow::ID => {
                        let packet: OpenWindow = frame.decode()?;
                        observation.windows.push(ObservedWindow {
                            id: packet.id,
                            event: ObservedWindowEvent::Open,
                            window_type: Some(packet.window_type),
                            window_data: packet.window_data,
                            capacity: packet.inventory.as_ref().map(|section| section.capacity),
                            occupied_slots: packet
                                .inventory
                                .as_ref()
                                .and_then(|section| section.items.as_ref())
                                .map(BTreeMap::len),
                        });
                    }
                    UpdateWindow::ID => {
                        let packet: UpdateWindow = frame.decode()?;
                        observation.windows.push(ObservedWindow {
                            id: packet.id,
                            event: ObservedWindowEvent::Update,
                            window_type: None,
                            window_data: packet.window_data,
                            capacity: packet.inventory.as_ref().map(|section| section.capacity),
                            occupied_slots: packet
                                .inventory
                                .as_ref()
                                .and_then(|section| section.items.as_ref())
                                .map(BTreeMap::len),
                        });
                    }
                    CloseWindow::ID => {
                        let packet: CloseWindow = frame.decode()?;
                        observation.windows.push(ObservedWindow {
                            id: packet.id,
                            event: ObservedWindowEvent::Close,
                            window_type: None,
                            window_data: None,
                            capacity: None,
                            occupied_slots: None,
                        });
                    }
                    SetPage::ID => {
                        let packet: SetPage = frame.decode()?;
                        observation.pages.push(packet.page);
                    }
                    UpdatePlayerInventory::ID => {
                        game.inventory = Some(frame.decode()?);
                    }
                    SetActiveSlot::ID => {
                        let packet: SetActiveSlot = frame.decode()?;
                        game.active_slots
                            .insert(packet.inventory_section_id, packet.active_slot);
                    }
                    Ping::ID => answer_ping(writer, &frame).await?,
                    ServerDisconnect::ID => return Ok(observation),
                    _ => {}
                }
            }
            frame = chunks.next_frame(), if chunks_open => {
                match frame {
                    Ok(Some(frame)) => match frame.packet_id {
                        ServerSetBlock::ID => {
                            let update = ServerSetBlock::decode(&mut Reader::new(&frame.payload))?;
                            observation.block_updates.push(ObservedBlockUpdate {
                                x: update.x,
                                y: update.y,
                                z: update.z,
                                block_id: update.block_id,
                            });
                        }
                        UpdateBlockDamage::ID => {
                            let update =
                                UpdateBlockDamage::decode(&mut Reader::new(&frame.payload))?;
                            if let Some(position) = update.block_position {
                                observation.block_damages.push(ObservedBlockDamage {
                                    x: position.x,
                                    y: position.y,
                                    z: position.z,
                                    damage: update.damage,
                                    delta: update.delta,
                                });
                            }
                        }
                        UpdateBlockDamages::ID => {
                            let update =
                                UpdateBlockDamages::decode(&mut Reader::new(&frame.payload))?;
                            for command in update.cmds {
                                observation.block_damages.push(section_block_damage(
                                    update.x,
                                    update.y,
                                    update.z,
                                    command,
                                ));
                            }
                        }
                        _ => {}
                    },
                    Ok(None) | Err(_) => chunks_open = false,
                }
            }
        }
    }
    Ok(observation)
}

async fn send_block_interaction(
    writer: &mut FrameWriter,
    button: MouseButtonType,
    position: BlockPosition,
    item_in_hand_id: Option<String>,
) -> Result<(), FrameStreamError> {
    writer
        .write_packet(
            &MouseInteraction {
                client_timestamp: 0,
                active_slot: 0,
                item_in_hand_id,
                screen_point: Vector2f::default(),
                mouse_button: Some(MouseButtonEvent {
                    mouse_button_type: button,
                    state: MouseButtonState::Pressed,
                    clicks: 1,
                }),
                mouse_motion: None,
                world_interaction: Some(WorldInteraction {
                    entity_id: -1,
                    block_position: Some(position),
                    block_rotation: None,
                }),
            },
            3,
        )
        .await
}

fn section_block_damage(
    section_x: i32,
    section_y: i32,
    section_z: i32,
    command: BlockDamageCmd,
) -> ObservedBlockDamage {
    let index = i32::from(command.index);
    ObservedBlockDamage {
        x: (section_x << 5) + (index & 31),
        y: (section_y << 5) + ((index >> 10) & 31),
        z: (section_z << 5) + ((index >> 5) & 31),
        damage: command.damage,
        delta: command.delta,
    }
}

fn observed_map_marker(marker: MapMarker) -> Option<ObservedMapMarker> {
    Some(ObservedMapMarker {
        id: marker.id,
        image: marker.marker_image,
        name: marker.name.and_then(format_message_name),
        position: marker.transform.position,
    })
}

fn format_message_name(message: FormattedMessage) -> Option<String> {
    message.message_id.or(message.raw_text)
}

fn record_observed_entities(
    packet: &EntityUpdates,
    observation: &mut PlayerObservation,
    self_id: Option<i32>,
) {
    if let Some(removed) = &packet.removed {
        for network_id in removed {
            if Some(*network_id) == self_id {
                continue;
            }
            observation.entities.push(ObservedEntityUpdate {
                network_id: *network_id,
                removed: true,
                position: None,
                model_asset: None,
                armor: None,
            });
        }
    }
    let Some(updates) = &packet.updates else {
        return;
    };
    for update in updates {
        if Some(update.network_id) == self_id {
            continue;
        }
        let mut observed = ObservedEntityUpdate {
            network_id: update.network_id,
            removed: false,
            position: None,
            model_asset: None,
            armor: None,
        };
        if let Some(components) = &update.updates {
            for component in components {
                match component {
                    ComponentUpdate::ModelUpdate(model) => {
                        observed.model_asset = model
                            .model
                            .as_ref()
                            .and_then(|model| model.asset_id.clone());
                    }
                    ComponentUpdate::EquipmentUpdate(equipment) => {
                        observed.armor = equipment.armor_ids.clone();
                    }
                    ComponentUpdate::TransformUpdate(transform) => {
                        observed.position = transform.transform.position.clone();
                    }
                    _ => {}
                }
            }
        }
        observation.entities.push(observed);
    }
}

/// One common asset the bot received and verified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSummary {
    /// Pack-relative name from `AssetInitialize`.
    pub name: String,
    /// SHA-256 from `AssetInitialize`.
    pub hash: String,
    /// Reassembled bytes.
    pub bytes: usize,
}

/// Options for the authenticated join.
#[derive(Debug, Clone)]
pub struct AuthJoinOptions {
    /// Player UUID (the JWT subject).
    pub uuid: Uuid,
    /// Player username.
    pub username: String,
    /// Requested view radius, in chunks.
    pub view_radius_chunks: u32,
    /// Local authority used to mint the identity/access tokens.
    pub auth: AuthService,
    /// Base64url fingerprint of the bot's mTLS certificate.
    pub certificate_fingerprint: String,
    /// How many advertised common assets to request (`None` = all).
    pub asset_limit: Option<usize>,
    /// Move to this chunk after the teleport ack to exercise a view update.
    pub movement_chunk: Option<(i32, i32)>,
    /// After the join, keep reading the setup stream for this long and collect
    /// live `UpdateTime` broadcasts (`None` = stop at the teleport).
    pub observe_time: Option<Duration>,
}

/// What the authenticated join produced.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthJoinReport {
    /// Server packets seen during the handshake, in order.
    pub handshake_packets: Vec<u32>,
    /// The server sent `AuthGrant`.
    pub auth_grant: bool,
    /// The server sent `ServerAuthToken`.
    pub server_auth_token: bool,
    /// Number of assets advertised in `WorldSettings`.
    pub advertised_assets: usize,
    /// Number of assets the bot requested.
    pub requested_assets: usize,
    /// Verified common assets.
    pub assets: Vec<AssetSummary>,
    /// `AssetPart` packets received.
    pub asset_parts: usize,
    /// Common-asset bytes received.
    pub asset_bytes: usize,
    /// Config asset packet ids received.
    pub config_packets: Vec<u32>,
    /// The server sent `UpdateTranslations`.
    pub translations: bool,
    /// The server sent `RequestCommonAssetsRebuild`.
    pub rebuild_requested: bool,
    /// World-join state.
    pub game: GameJoinReport,
    /// The server sent a `ClientTeleport` after `readyForGameplay`.
    pub teleport: bool,
    /// Chunk packets received after the movement-driven view change.
    pub view_update_chunks: Vec<ChunkPacketSummary>,
    /// Decoded chunk packets from the initial snapshot.
    pub chunks: Vec<ChunkPacketSummary>,
    /// Live `UpdateTime` values collected by `observe_time`.
    pub time_updates: Vec<(i64, i32)>,
}

/// Errors from the authenticated join.
#[derive(Debug, Error)]
pub enum AuthJoinError {
    /// Opening the setup stream failed.
    #[error("opening the setup stream failed: {0}")]
    Connect(String),
    /// A framed packet failed.
    #[error(transparent)]
    Frame(#[from] FrameStreamError),
    /// Opening a QUIC stream failed.
    #[error(transparent)]
    Transport(#[from] hytale_net::TransportError),
    /// The chunk stream failed.
    #[error(transparent)]
    Chunk(#[from] ChunkStreamError),
    /// The server disconnected or closed the stream.
    #[error("server closed the connection during setup")]
    Closed,
    /// The server sent a disconnect.
    #[error("server disconnected: {0}")]
    Disconnected(String),
    /// Token issuance/validation failed.
    #[error(transparent)]
    Auth(#[from] hytale_auth::AuthError),
    /// A packet payload was malformed.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// An `AssetPart` arrived without `AssetInitialize`.
    #[error("received an asset part without an initialize packet")]
    UnexpectedAssetPart,
    /// An asset ended without a finalize.
    #[error("asset stream closed before AssetFinalize")]
    MissingAssetFinalize,
    /// The declared size and the reassembled blob differ.
    #[error("asset `{name}` size mismatch: declared {declared}, got {actual}")]
    AssetSizeMismatch {
        /// Asset name.
        name: String,
        /// Declared size.
        declared: usize,
        /// Received bytes.
        actual: usize,
    },
    /// The reassembled blob hash does not match the advertised hash.
    #[error("asset `{name}` hash mismatch: expected {expected}, got {actual}")]
    AssetHashMismatch {
        /// Asset name.
        name: String,
        /// Advertised hash.
        expected: String,
        /// Actual hash.
        actual: String,
    },
    /// The expected handshake packet did not arrive.
    #[error("expected {expected} during the authenticated handshake")]
    MissingPacket {
        /// Packet name.
        expected: &'static str,
    },
}

/// Run the authenticated `Connect → AuthGrant → AuthToken → ServerAuthToken →
/// Setup` flow, request common assets, reach `Game` and receive chunks.
pub async fn auth_join(
    session: &ClientSession,
    options: &AuthJoinOptions,
) -> Result<AuthJoinReport, AuthJoinError> {
    auth_join_with_tokens(session, options, None, None).await
}

/// Like [`auth_join`], but with caller-supplied identity/access tokens.
///
/// This is the live-session shape: the tokens are issued by Hytale (or by the
/// test's session-service stub) rather than the local [`AuthService`].
pub async fn auth_join_with_tokens(
    session: &ClientSession,
    options: &AuthJoinOptions,
    identity_token: Option<String>,
    access_token: Option<String>,
) -> Result<AuthJoinReport, AuthJoinError> {
    let (send, recv) = session
        .connection()
        .open_bi()
        .await
        .map_err(|error| AuthJoinError::Connect(error.to_string()))?;
    let mut writer = FrameWriter::new(send);
    let mut reader = FrameReader::new(recv);

    let subject = options.uuid.to_string();
    let scopes = [SCOPE_CLIENT.to_string()];
    let identity = match identity_token {
        Some(token) => token,
        None => options.auth.issue_identity_token(
            &subject,
            &options.username,
            &scopes,
            Some(&options.certificate_fingerprint),
            300,
        )?,
    };
    let connect = build_connect(
        DEFAULT_PROTOCOL_CRC,
        DEFAULT_PROTOCOL_BUILD,
        "2026.01.01",
        "en-US",
        Some(identity),
    );
    writer.write_packet(&connect, 3).await?;

    let mut report = AuthJoinReport {
        handshake_packets: Vec::new(),
        auth_grant: false,
        server_auth_token: false,
        advertised_assets: 0,
        requested_assets: 0,
        assets: Vec::new(),
        asset_parts: 0,
        asset_bytes: 0,
        config_packets: Vec::new(),
        translations: false,
        rebuild_requested: false,
        game: GameJoinReport::default(),
        teleport: false,
        view_update_chunks: Vec::new(),
        chunks: Vec::new(),
        time_updates: Vec::new(),
    };
    let mut current: Option<(Asset, i32, Vec<u8>)> = None;
    let mut requested = false;

    loop {
        let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        report.handshake_packets.push(frame.packet_id);
        match frame.packet_id {
            AuthGrant::ID => {
                let grant: AuthGrant = frame.decode()?;
                report.auth_grant = true;
                let grant = grant
                    .authorization_grant
                    .ok_or(AuthJoinError::MissingPacket {
                        expected: "AuthGrant.authorizationGrant",
                    })?;
                let access = match access_token.clone() {
                    Some(token) => token,
                    None => options.auth.issue_access_token(
                        &subject,
                        &options.username,
                        &scopes,
                        Some(&options.certificate_fingerprint),
                        300,
                    )?,
                };
                writer
                    .write_packet(
                        &AuthToken {
                            access_token: Some(access),
                            server_authorization_grant: Some(grant),
                        },
                        3,
                    )
                    .await?;
            }
            ServerAuthToken::ID => {
                report.server_auth_token = true;
            }
            RequestInsecurePlayerOptions::ID => {
                return Err(AuthJoinError::MissingPacket {
                    expected: "authenticated flow (server requested insecure options)",
                });
            }
            ConnectAccept::ID => {
                return Err(AuthJoinError::MissingPacket {
                    expected: "authenticated flow (server sent ConnectAccept)",
                });
            }
            WorldSettings::ID => {
                let settings: WorldSettings = frame.decode()?;
                let required = settings.required_assets.unwrap_or_default();
                report.advertised_assets = required.len();
                let requested_assets: Vec<Asset> = match options.asset_limit {
                    Some(limit) => required.into_iter().take(limit).collect(),
                    None => required,
                };
                report.requested_assets = requested_assets.len();
                writer
                    .write_packet(
                        &RequestAssets {
                            assets: Some(requested_assets),
                        },
                        3,
                    )
                    .await?;
                requested = true;
            }
            AssetInitialize::ID => {
                let initialize: AssetInitialize = frame.decode()?;
                current = Some((initialize.asset, initialize.size, Vec::new()));
            }
            AssetPart::ID => {
                let part: AssetPart = frame.decode()?;
                let bytes = part.part.unwrap_or_default();
                let (_, _, buffer) = current.as_mut().ok_or(AuthJoinError::UnexpectedAssetPart)?;
                buffer.extend(bytes.into_iter().map(|byte| byte as u8));
                report.asset_parts += 1;
            }
            AssetFinalize::ID => {
                let (asset, size, bytes) =
                    current.take().ok_or(AuthJoinError::MissingAssetFinalize)?;
                if bytes.len() != size as usize {
                    return Err(AuthJoinError::AssetSizeMismatch {
                        name: asset.name,
                        declared: size as usize,
                        actual: bytes.len(),
                    });
                }
                let actual = hex(Sha256::digest(&bytes).as_slice());
                if actual != asset.hash.to_ascii_lowercase() {
                    return Err(AuthJoinError::AssetHashMismatch {
                        name: asset.name,
                        expected: asset.hash,
                        actual,
                    });
                }
                report.asset_bytes += bytes.len();
                report.assets.push(AssetSummary {
                    name: asset.name,
                    hash: asset.hash,
                    bytes: bytes.len(),
                });
            }
            RequestCommonAssetsRebuild::ID => {
                report.rebuild_requested = true;
            }
            UpdateTranslations::ID => {
                report.translations = true;
            }
            UpdatePhysicalMaterials::ID
            | UpdateBlockHitboxes::ID
            | UpdateSoundEvents::ID
            | UpdateBlockSoundSets::ID
            | UpdateFluidFX::ID
            | UpdateFluids::ID
            | UpdateEnvironments::ID
            | UpdateTagPatterns::ID
            | UpdateBlockTypes::ID
            | UpdateBlockParticleSets::ID
            | UpdateBlockBreakingDecals::ID
            | UpdateItemSoundSets::ID
            | UpdateItemQualities::ID
            | UpdateItemReticles::ID
            | UpdateItemCategories::ID
            | UpdateItemPlayerAnimations::ID
            | UpdateEntityStatTypes::ID
            | UpdateEntityEffects::ID
            | UpdateProjectileConfigs::ID
            | UpdateWeathers::ID
            | UpdateItems::ID => {
                report.config_packets.push(frame.packet_id);
            }
            WorldLoadFinished::ID => {
                if !requested {
                    return Err(AuthJoinError::MissingPacket {
                        expected: "WorldSettings",
                    });
                }
                if current.is_some() {
                    return Err(AuthJoinError::MissingAssetFinalize);
                }
                writer.write_packet(&SetupFinalize {}, 3).await?;
                break;
            }
            ServerDisconnect::ID => {
                let disconnect: ServerDisconnect = frame.decode()?;
                let reason = disconnect
                    .reason
                    .and_then(|message| message.raw_text)
                    .unwrap_or_else(|| "disconnect".to_string());
                return Err(AuthJoinError::Disconnected(reason));
            }
            Ping::ID => {
                answer_ping(&mut writer, &frame).await?;
            }
            _ => {}
        }
    }

    let (game, mut chunks, mut pending) =
        enter_game_client(session, &mut reader, &mut writer).await?;
    report.game = game;
    report.chunks =
        receive_initial_snapshot(&mut chunks, &mut pending, options.view_radius_chunks).await?;

    let (teleport, teleport_id) = finish_game(&mut reader, &mut writer, &mut report.game).await?;
    report.teleport = teleport;
    if let (Some(target), Some(teleport_id)) = (options.movement_chunk, teleport_id) {
        report.view_update_chunks =
            exercise_movement(&mut reader, &mut writer, &mut chunks, target, teleport_id).await?;
    }
    if let Some(window) = options.observe_time {
        report.time_updates =
            observe_time(&mut reader, &mut writer, window, &mut report.game).await?;
    }
    Ok(report)
}

async fn answer_ping(
    writer: &mut FrameWriter,
    frame: &hytale_net::Frame,
) -> Result<(), FrameStreamError> {
    let ping: Ping = frame.decode()?;
    writer
        .write_packet(
            &Pong {
                id: ping.id,
                time: ping.time,
                r#type: PongType::Raw,
                packet_queue_size: 0,
            },
            3,
        )
        .await
}

async fn enter_game_client(
    session: &ClientSession,
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
) -> Result<(GameJoinReport, ChunkStreamReader, Vec<ChunkFrame>), AuthJoinError> {
    let mut pending = Vec::new();
    let mut game = GameJoinReport::default();

    // The server writes JoinWorld on the setup stream and waits for our
    // ClientReady. The server-opened chunks stream is not observable until the
    // first frame is written (ViewRadius), so accept it only after ready.
    loop {
        let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        match frame.packet_id {
            AddToServerPlayerList::ID => {
                let packet: AddToServerPlayerList = frame.decode()?;
                game.player_list_uuids = packet.players.iter().map(|player| player.uuid).collect();
                game.player_list = true;
            }
            JoinWorld::ID => {
                game.join_world = true;
                break;
            }
            ServerTags::ID => {}
            Ping::ID => answer_ping(writer, &frame).await?,
            ServerDisconnect::ID => {
                let disconnect: ServerDisconnect = frame.decode()?;
                let reason = disconnect
                    .reason
                    .and_then(|message| message.raw_text)
                    .unwrap_or_else(|| "disconnect".to_string());
                return Err(AuthJoinError::Disconnected(reason));
            }
            _ => {}
        }
    }

    writer
        .write_packet(
            &ClientReady {
                ready_for_chunks: true,
                ready_for_gameplay: false,
            },
            3,
        )
        .await?;
    game.sent_ready_for_chunks = true;

    let mut chunks = ChunkStreamReader::accept(session.connection()).await?;
    // `ViewRadius` is written on the chunks stream after the client is ready.
    while game.view_radius.is_none() {
        let frame = chunks.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        match frame.packet_id {
            ViewRadius::ID => {
                let radius = ViewRadius::decode(&mut Reader::new(&frame.payload))?;
                game.view_radius = Some(radius.value);
            }
            _ => pending.push(frame),
        }
    }

    loop {
        let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        match frame.packet_id {
            SetEntitySeed::ID => {
                let packet: SetEntitySeed = frame.decode()?;
                game.entity_seed = Some(packet.entity_seed);
            }
            SetClientId::ID => {
                let packet: SetClientId = frame.decode()?;
                game.client_id = Some(packet.client_id);
            }
            SetTimeDilation::ID => {
                let packet: SetTimeDilation = frame.decode()?;
                game.time_dilation = Some(packet.time_dilation);
            }
            UpdateFeatures::ID => {}
            SetUpdateRate::ID => {
                let packet: SetUpdateRate = frame.decode()?;
                game.update_rate = Some(packet.updates_per_second);
                break;
            }
            Ping::ID => answer_ping(writer, &frame).await?,
            ServerDisconnect::ID => {
                let disconnect: ServerDisconnect = frame.decode()?;
                let reason = disconnect
                    .reason
                    .and_then(|message| message.raw_text)
                    .unwrap_or_else(|| "disconnect".to_string());
                return Err(AuthJoinError::Disconnected(reason));
            }
            _ => {}
        }
    }

    // Java sends the joining player's self entity update after the world
    // identity burst, before gameplay input starts. The gameplay/time packets
    // share this part of the stream.
    loop {
        let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        match frame.packet_id {
            EntityUpdates::ID => {
                let packet: EntityUpdates = frame.decode()?;
                record_self_spawn(&packet, &mut game);
                game.self_spawn = true;
                break;
            }
            WorldGameplayConfig::ID => {
                game.gameplay_config = Some(frame.decode()?);
            }
            UpdateMovementSettings::ID => {
                let packet: UpdateMovementSettings = frame.decode()?;
                game.movement_settings = Some(packet.movement_settings);
            }
            UpdatePlayerInventory::ID => {
                game.inventory = Some(frame.decode()?);
            }
            SetActiveSlot::ID => {
                let packet: SetActiveSlot = frame.decode()?;
                game.active_slots
                    .insert(packet.inventory_section_id, packet.active_slot);
            }
            UpdateTimeSettings::ID => {
                game.time_settings = Some(frame.decode()?);
            }
            UpdateTime::ID => {
                let packet: UpdateTime = frame.decode()?;
                game.initial_time = packet.game_time.map(|time| (time.seconds, time.nanos));
            }
            Ping::ID => answer_ping(writer, &frame).await?,
            ServerDisconnect::ID => {
                let disconnect: ServerDisconnect = frame.decode()?;
                let reason = disconnect
                    .reason
                    .and_then(|message| message.raw_text)
                    .unwrap_or_else(|| "disconnect".to_string());
                return Err(AuthJoinError::Disconnected(reason));
            }
            _ => {}
        }
    }

    Ok((game, chunks, pending))
}

/// Record the self-spawn components the server sent for the joining player.
fn record_self_spawn(packet: &EntityUpdates, game: &mut GameJoinReport) {
    let Some(updates) = &packet.updates else {
        return;
    };
    let Some(entity) = updates.first() else {
        return;
    };
    let Some(components) = &entity.updates else {
        return;
    };
    for component in components {
        match component {
            ComponentUpdate::ModelUpdate(update) => {
                game.self_spawn_model = update
                    .model
                    .as_ref()
                    .and_then(|model| model.asset_id.clone());
            }
            ComponentUpdate::PlayerSkinUpdate(update) => {
                game.self_spawn_skin = update.skin.clone();
            }
            ComponentUpdate::EquipmentUpdate(update) => {
                game.self_spawn_armor = update.armor_ids.clone();
                game.self_spawn_right_hand = update.right_hand_item_id.clone();
                game.self_spawn_left_hand = update.left_hand_item_id.clone();
            }
            ComponentUpdate::TransformUpdate(update) => {
                game.self_spawn_position = update.transform.position.clone();
            }
            _ => {}
        }
    }
}

/// Number of packets the initial snapshot contains for a view radius.
///
/// One `SetColumn` plus ten `SetChunk` and ten `SetChunkEnvironments` packets
/// per chunk.
fn expected_snapshot_packets(radius: u32) -> usize {
    let side = (radius as usize) * 2 + 1;
    side * side * 21
}

async fn receive_initial_snapshot(
    reader: &mut ChunkStreamReader,
    pending: &mut Vec<ChunkFrame>,
    radius: u32,
) -> Result<Vec<ChunkPacketSummary>, AuthJoinError> {
    let expected = expected_snapshot_packets(radius);
    let mut packets = Vec::with_capacity(expected);
    for frame in pending.drain(..) {
        if let Some(summary) = summarize_chunk_packet(&frame) {
            packets.push(summary?);
        }
    }
    while packets.len() < expected {
        let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        if let Some(summary) = summarize_chunk_packet(&frame) {
            packets.push(summary?);
        }
    }
    Ok(packets)
}

async fn finish_game(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    game: &mut GameJoinReport,
) -> Result<(bool, Option<i8>), AuthJoinError> {
    writer
        .write_packet(
            &ClientReady {
                ready_for_chunks: false,
                ready_for_gameplay: true,
            },
            3,
        )
        .await?;
    loop {
        let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        match frame.packet_id {
            ClientTeleport::ID => {
                let teleport: ClientTeleport = frame.decode()?;
                return Ok((true, Some(teleport.teleport_id)));
            }
            WorldGameplayConfig::ID => {
                game.gameplay_config = Some(frame.decode()?);
            }
            UpdateMovementSettings::ID => {
                let packet: UpdateMovementSettings = frame.decode()?;
                game.movement_settings = Some(packet.movement_settings);
            }
            UpdatePlayerInventory::ID => {
                game.inventory = Some(frame.decode()?);
            }
            SetActiveSlot::ID => {
                let packet: SetActiveSlot = frame.decode()?;
                game.active_slots
                    .insert(packet.inventory_section_id, packet.active_slot);
            }
            UpdateTimeSettings::ID => {
                game.time_settings = Some(frame.decode()?);
            }
            UpdateTime::ID => {
                let packet: UpdateTime = frame.decode()?;
                game.initial_time = packet.game_time.map(|time| (time.seconds, time.nanos));
            }
            Ping::ID => answer_ping(writer, &frame).await?,
            ServerDisconnect::ID => {
                let disconnect: ServerDisconnect = frame.decode()?;
                let reason = disconnect
                    .reason
                    .and_then(|message| message.raw_text)
                    .unwrap_or_else(|| "disconnect".to_string());
                return Err(AuthJoinError::Disconnected(reason));
            }
            _ => {}
        }
    }
}

/// Keep reading the setup stream for `window`, collecting live `UpdateTime`
/// broadcasts. Pings are answered; other packets are ignored.
async fn observe_time(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    window: Duration,
    game: &mut GameJoinReport,
) -> Result<Vec<(i64, i32)>, AuthJoinError> {
    let mut updates = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    loop {
        let frame = match tokio::time::timeout_at(deadline, reader.next_frame()).await {
            Ok(Ok(Some(frame))) => frame,
            Ok(Ok(None)) => break,
            Ok(Err(error)) => return Err(AuthJoinError::Frame(error)),
            Err(_) => break,
        };
        match frame.packet_id {
            UpdateTime::ID => {
                let packet: UpdateTime = frame.decode()?;
                if let Some(time) = packet.game_time {
                    updates.push((time.seconds, time.nanos));
                }
            }
            UpdateTimeSettings::ID => {
                game.time_settings = Some(frame.decode()?);
            }
            Ping::ID => answer_ping(writer, &frame).await?,
            ServerDisconnect::ID => {
                let disconnect: ServerDisconnect = frame.decode()?;
                let reason = disconnect
                    .reason
                    .and_then(|message| message.raw_text)
                    .unwrap_or_else(|| "disconnect".to_string());
                return Err(AuthJoinError::Disconnected(reason));
            }
            _ => {}
        }
    }
    Ok(updates)
}

async fn exercise_movement(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    chunks: &mut ChunkStreamReader,
    target: (i32, i32),
    teleport_id: i8,
) -> Result<Vec<ChunkPacketSummary>, AuthJoinError> {
    let position = Position {
        x: f64::from(target.0) * 32.0 + 16.0,
        y: 0.0,
        z: f64::from(target.1) * 32.0 + 16.0,
    };
    writer
        .write_packet(
            &ClientMovement {
                absolute_position: Some(position),
                teleport_ack: Some(TeleportAck { teleport_id }),
                ..ClientMovement::default()
            },
            3,
        )
        .await?;

    // The view change sends one UnloadChunk for the old chunk and the 21-packet
    // batch for the new one. Stop once the new column arrives, or after a
    // timeout so an empty target chunk cannot hang the bot.
    let while_reading = async {
        let mut frames = Vec::new();
        loop {
            let frame = chunks.next_frame().await?.ok_or(AuthJoinError::Closed)?;
            let is_column = frame.packet_id == hytale_protocol::generated::world::SetColumn::ID;
            if let Some(summary) = summarize_chunk_packet(&frame) {
                frames.push(summary?);
            }
            if is_column {
                return Ok::<Vec<ChunkPacketSummary>, AuthJoinError>(frames);
            }
            let _ = reader;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(30), while_reading).await {
        Ok(result) => result,
        Err(_) => Ok(Vec::new()),
    }
}

pub async fn receive_chunk_stream(
    session: &ClientSession,
) -> Result<Vec<ChunkPacketSummary>, ChunkStreamError> {
    let mut reader = ChunkStreamReader::accept(session.connection()).await?;
    let mut packets = Vec::new();
    while let Some(frame) = reader.next_frame().await? {
        if let Some(summary) = summarize_chunk_packet(&frame) {
            packets.push(summary?);
        }
    }
    Ok(packets)
}

/// Name of a connection-family packet.
pub fn connection_packet_name(packet_id: u32) -> Option<&'static str> {
    Some(match packet_id {
        0 => "Connect",
        1 => "ClientDisconnect",
        2 => "ServerDisconnect",
        3 => "Ping",
        4 => "Pong",
        5 => "NetworkTick",
        363 => "InsecurePlayerOptions",
        364 => "RequestInsecurePlayerOptions",
        _ => return None,
    })
}

/// Decode a connection-family packet payload.
pub fn decode_connection_packet(
    packet_id: u32,
    payload: &[u8],
) -> Option<Result<String, ProtocolError>> {
    let mut reader = Reader::new(payload);
    Some(match packet_id {
        0 => Connect::decode(&mut reader).map(|value| format!("{value:?}")),
        1 => ClientDisconnect::decode(&mut reader).map(|value| format!("{value:?}")),
        2 => ServerDisconnect::decode(&mut reader).map(|value| format!("{value:?}")),
        3 => Ping::decode(&mut reader).map(|value| format!("{value:?}")),
        4 => Pong::decode(&mut reader).map(|value| format!("{value:?}")),
        5 => NetworkTick::decode(&mut reader).map(|value| format!("{value:?}")),
        363 => InsecurePlayerOptions::decode(&mut reader).map(|value| format!("{value:?}")),
        364 => RequestInsecurePlayerOptions::decode(&mut reader).map(|_| "{}".to_string()),
        _ => return None,
    })
}

/// Hex helper for `--dry-run` output.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_frame_round_trips_through_the_framer() {
        let connect = build_connect(
            DEFAULT_PROTOCOL_CRC,
            DEFAULT_PROTOCOL_BUILD,
            "2026.01.01",
            "en-US",
            None,
        );
        let frame = build_connect_frame(&connect).expect("frame");
        let mut buffer = frame;
        let records = drain_frames(&mut buffer).expect("drain");
        assert_eq!(records.len(), 1);
        assert!(buffer.is_empty());
        assert_eq!(records[0].packet_id, 0);
        assert_eq!(records[0].name(), Some("Connect"));
        let mut reader = Reader::new(&records[0].payload);
        let decoded = Connect::decode(&mut reader).expect("decode");
        assert_eq!(decoded, connect);
    }

    #[test]
    fn drain_frames_keeps_an_incomplete_tail() {
        let connect = build_connect(1, 2, "v", "en-US", None);
        let first = build_connect_frame(&connect).expect("first frame");
        let mut second = build_connect_frame(&connect).expect("second frame");
        let tail = second.split_off(second.len() - 2);
        let mut buffer = first;
        buffer.extend_from_slice(&second);

        let records = drain_frames(&mut buffer).expect("partial drain");
        assert_eq!(records.len(), 1);
        assert!(!buffer.is_empty());

        buffer.extend_from_slice(&tail);
        let records = drain_frames(&mut buffer).expect("complete drain");
        assert_eq!(records.len(), 1);
        assert!(buffer.is_empty());
    }
}
