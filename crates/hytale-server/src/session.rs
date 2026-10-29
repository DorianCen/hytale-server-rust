//! Connection authentication, setup and the transition into `Game`.
//!
//! The control flow follows the Java handlers:
//!
//! ```text
//! insecure:  Connect → RequestInsecurePlayerOptions → InsecurePlayerOptions
//!            → ConnectAccept → WorldSettings/ServerInfo
//! local:     Connect(identityToken) → AuthGrant → AuthToken
//!            → ServerAuthToken → WorldSettings/ServerInfo
//! both:      RequestAssets → common asset blobs + config asset packets
//!            → WorldLoadProgress/WorldLoadFinished → SetupFinalize
//! game:      ClearChunks + JoinWorld → ClientReady(readyForChunks)
//!            → world/entity identity packets → chunk stream
//! ```
//!
//! Authentication uses the local EdDSA profile from [`hytale_auth`]: tokens are
//! Ed25519 JWTs with the Java issuer/audience/scope claims and an
//! `cnf.x5t#S256` binding to the client certificate. The packet flow and claim
//! rules are the same as the live session service; only the key authority is
//! local.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use hytale_assets::{AssetPacket, CommonAssetInfo, CommonAssetRegistry, ConfigAssets};
use hytale_auth::{AuthService, SCOPE_CLIENT, SCOPE_SERVER};
use hytale_entities::movement::{MovementBounds, MovementRejection, MovementState};
use hytale_interactions::{
    execute_root, state_from_block_key, BlockInteractions, ExecutionContext, InteractionCatalog,
    WorldAction,
};
use hytale_inventory::{PlayerInventory, Section};
use hytale_net::{
    ChunkStreamError, ChunkStreamWriter, FrameReader, FrameStreamError, FrameWriter, PeerIdentity,
    TraceRecorder, TraceSession, TransportError,
};
use hytale_protocol::generated::assets::UpdateTranslations;
use hytale_protocol::generated::auth::{AuthGrant, AuthToken, ConnectAccept, ServerAuthToken};
use hytale_protocol::generated::common::ServerPlayerListPlayer;
use hytale_protocol::generated::common::{
    DisconnectType, FormattedMessage, GameMode, InstantData, MapChunk, MapMarker,
    MapMarkerIconSize, MouseButtonState, MouseButtonType, MovementSettings, MovementStates,
    PlayerSkin, PongType, Position as ProtocolPosition, Transform as ProtocolTransform, UpdateType,
};
use hytale_protocol::generated::connection::{
    ClientDisconnect, ClientType, Connect, InsecurePlayerOptions, Ping, Pong,
    RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::entities::{EntityUpdates, SetEntitySeed};
use hytale_protocol::generated::interface_::Page;
use hytale_protocol::generated::interface_::{AddToServerPlayerList, ServerInfo, SetPage};
use hytale_protocol::generated::inventory::{DropItemStack, SetActiveSlot, UpdatePlayerInventory};
use hytale_protocol::generated::player::{
    ClientMovement, ClientReady, ClientTeleport, JoinWorld, MouseInteraction, SetClientId,
    SetGameMode, UpdateMovementSettings,
};
use hytale_protocol::generated::registry::settings;
use hytale_protocol::generated::setup::{
    AssetFinalize, AssetInitialize, AssetPart, RequestAssets, RequestCommonAssetsRebuild,
    ServerTags, SetTimeDilation, SetUpdateRate, SetupFinalize, UpdateFeatures, ViewRadius,
    WorldLoadFinished, WorldLoadProgress, WorldSettings,
};
use hytale_protocol::generated::window::{CloseWindow, OpenWindow, SendWindowAction, WindowType};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::generated::world::ClearChunks;
use hytale_protocol::generated::world::{
    ServerSetBlock, ServerSetFluid, UpdateTime, UpdateTimeSettings, UpdateWeather,
    WorldGameplayConfig,
};
use hytale_protocol::generated::worldmap::{UpdateWorldMap, UpdateWorldMapSettings};
use hytale_protocol::io::Uuid;
use hytale_world::chunk::stream::DEFAULT_COMPRESSION_LEVEL;
use hytale_world::collision::Vec3;
use hytale_world::player_data::{PlayerAppearance, PlayerTransform};
use hytale_world::world_config::WorldUuid;
use thiserror::Error;

use crate::chunk_streamer::{
    send_updates, send_view_registered, ChunkSessionError, ChunkStreamReport,
};
use crate::players::{self, PlayerJoinRequest, PlayerMessage, PlayerRegistry};
use crate::world_thread::{
    BlockDamagePacket, RuntimeEntityEvent, ViewId, WorldThread, WorldThreadError, WorldTimeState,
};

/// Java `CommonAssetModule.MAX_FRAME`: maximum bytes in one `AssetPart`.
pub const MAX_COMMON_ASSET_FRAME: usize = 2_621_440;
/// Local server identity subject used for the local authority's server tokens.
const LOCAL_SERVER_SUBJECT: &str = "00000000-0000-0000-0000-000000000000";
/// Token lifetime for the short-lived handshake tokens.
const HANDSHAKE_TOKEN_TTL_SECS: i64 = 300;

/// Authentication mode for a connection.
#[derive(Debug, Clone, Default)]
pub enum AuthMode {
    /// Java's development branch: `RequestInsecurePlayerOptions` and
    /// `InsecurePlayerOptions`, no token validation.
    #[default]
    Insecure,
    /// Local EdDSA JWT authority with mTLS certificate binding.
    Local {
        /// Issuer/audience and signing key.
        service: AuthService,
        /// Lifetime for tokens the server issues during the exchange.
        token_ttl_secs: i64,
    },
}

impl AuthMode {
    /// A local authority using `service` and the default handshake token TTL.
    pub fn local(service: AuthService) -> Self {
        Self::Local {
            service,
            token_ttl_secs: HANDSHAKE_TOKEN_TTL_SECS,
        }
    }

    /// Short label for reports/logging.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Insecure => "insecure",
            Self::Local { .. } => "local",
        }
    }
}

/// Server identity / handshake settings.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// `ServerInfo.serverName`.
    pub server_name: String,
    /// `ServerInfo.motd`.
    pub motd: String,
    /// `ServerInfo.maxPlayers`.
    pub max_players: i32,
    /// View radius used when the client does not send `ViewRadius`.
    pub default_view_radius: u32,
    /// zstd level for the chunk stream.
    pub chunk_compression_level: i32,
    /// Timeout for the whole handshake.
    pub handshake_timeout: Duration,
    /// Timeout for the client's `ClientReady(readyForChunks)`.
    pub world_join_timeout: Duration,
    /// Disconnect a game session after this long without a client frame.
    pub idle_timeout: Duration,
    /// Authentication mode.
    pub auth: AuthMode,
    /// Health removed from a block per left-click hit (`1.0` = break a full
    /// health block in one hit, the pre-T57b behavior).
    pub block_break_damage: f32,
    /// Optional packet trace recorder shared by every connection.
    pub trace: Option<Arc<TraceRecorder>>,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            server_name: "Rust CoreServer".to_string(),
            motd: "dev".to_string(),
            max_players: 20,
            default_view_radius: 1,
            chunk_compression_level: DEFAULT_COMPRESSION_LEVEL,
            handshake_timeout: Duration::from_secs(30),
            world_join_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            auth: AuthMode::Insecure,
            block_break_damage: 1.0,
            trace: None,
        }
    }
}

/// Common-asset streaming counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssetStreamReport {
    /// Assets advertised in `WorldSettings.requiredAssets`.
    pub required: usize,
    /// Assets the client asked for.
    pub requested: usize,
    /// Assets actually streamed.
    pub sent: usize,
    /// Blob bytes streamed.
    pub bytes: usize,
    /// `AssetPart` packets written.
    pub parts: usize,
    /// Config asset packets written after the common assets.
    pub config_packets: usize,
}

/// What the post-setup world join produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameReport {
    /// The client sent `ClientReady(readyForChunks)`.
    pub client_ready_for_chunks: bool,
    /// The client sent `ClientReady(readyForGameplay)` before the chunk stream.
    pub client_ready_for_gameplay: bool,
    /// `ClearChunks` was written on the chunks stream.
    pub clear_chunks: bool,
    /// `JoinWorld` was written.
    pub join_world: bool,
    /// `AddToServerPlayerList` contained the joining player.
    pub player_list: bool,
    /// The joining player's self `EntityUpdates` spawn was written.
    pub self_spawn: bool,
    /// `WorldGameplayConfig` was written.
    pub gameplay_config: bool,
    /// `UpdateTimeSettings`/`UpdateTime` were written.
    pub time: bool,
    /// `UpdateWeather` was written.
    pub weather: bool,
    /// `SetGameMode` was written.
    pub game_mode: bool,
    /// The player's `UpdatePlayerInventory` was written.
    pub inventory: bool,
    /// The initial hotbar/utility/tools `SetActiveSlot` packets were written.
    pub active_slots: bool,
    /// `UpdateMovementSettings` was written.
    pub movement_settings: bool,
    /// World-map settings/markers were written.
    pub world_map: bool,
    /// World-map chunk images written on join.
    pub map_chunks: usize,
    /// World-map chunk images unloaded on join.
    pub map_chunks_unloaded: usize,
    /// Runtime entities spawned after the chunk snapshot.
    pub runtime_entities: usize,
}

/// What the live game loop produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GameLoopReport {
    /// The client sent `ClientReady(readyForGameplay)`.
    pub ready_for_gameplay: bool,
    /// `ClientTeleport` was written after `ClientReady(readyForGameplay)`.
    pub teleport_sent: bool,
    /// The client acknowledged the teleport.
    pub teleport_acked: bool,
    /// `ClientMovement` packets processed.
    pub movement_packets: usize,
    /// Movement packets refused by the finite/world bounds.
    pub movement_rejections: usize,
    /// Movement targets corrected by block collision (and teleported back).
    pub movement_corrections: usize,
    /// Movement auto-stepped onto a one-block obstacle.
    pub movement_steps: usize,
    /// Block-simulation passes run.
    pub simulation_ticks: usize,
    /// Block changes emitted by the simulation.
    pub simulation_block_changes: usize,
    /// Fluid changes emitted by the simulation.
    pub simulation_fluid_changes: usize,
    /// Block-health entries broadcast by the simulation.
    pub simulation_health_changes: usize,
    /// Runtime entity spawn/update events broadcast.
    pub runtime_entity_updates: usize,
    /// Runtime entity despawn events broadcast.
    pub runtime_entity_despawns: usize,
    /// NPCs spawned by the automatic population tick.
    pub npc_spawns: usize,
    /// NPC actors advanced by the world tick.
    pub npc_ticks: usize,
    /// NPC actors that moved.
    pub npc_moves: usize,
    /// NPC melee attacks produced.
    pub npc_attacks: usize,
    /// NPC actors that died.
    pub npc_deaths: usize,
    /// Movement caused a view change.
    pub view_changes: usize,
    /// Chunks streamed during the loop.
    pub chunks: usize,
    /// Chunk packets streamed during the loop.
    pub packets: usize,
    /// Frames streamed during the loop.
    pub frames: usize,
    /// Uncompressed chunk payload bytes during the loop.
    pub payload_bytes: usize,
    /// Chunks unloaded during the loop.
    pub unloaded: usize,
    /// Periodic `UpdateTime` packets written.
    pub time_updates: usize,
    /// `UpdateWeather` change packets written.
    pub weather_updates: usize,
    /// `UpdateTimeSettings` packets written after a pause-state change.
    pub time_settings_updates: usize,
    /// Queued player list/entity packets written during the loop.
    pub player_messages: usize,
    /// Block edits applied to the world.
    pub block_edits: usize,
    /// Block interactions rejected (reach, unknown item, occupied target).
    pub block_edit_rejections: usize,
    /// Inventory updates written after an active-slot or held-item change.
    pub inventory_updates: usize,
    /// Block-use interactions handled (doors/state/container/bench).
    pub block_uses: usize,
    /// Block state changes applied by interactions.
    pub block_state_changes: usize,
    /// Block-container windows opened.
    pub windows_opened: usize,
    /// Block-container windows closed by the client or teardown.
    pub windows_closed: usize,
    /// World-map chunk images written while the player moved.
    pub map_chunks: usize,
    /// World-map chunk images unloaded while the player moved.
    pub map_chunks_unloaded: usize,
    /// The client disconnected (or the connection closed) cleanly.
    pub disconnected: bool,
    /// The server disconnected the client for idling.
    pub idle_timeout: bool,
}

/// What the session produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReport {
    /// Player UUID (from `InsecurePlayerOptions` or the access token).
    pub uuid: Uuid,
    /// Player username.
    pub username: String,
    /// View radius used for the chunk stream.
    pub view_radius: u32,
    /// `insecure` or `local`.
    pub auth_mode: &'static str,
    /// Common/config asset counters.
    pub assets: AssetStreamReport,
    /// World-join state.
    pub game: GameReport,
    /// Live game-loop counters.
    pub game_loop: GameLoopReport,
    /// Chunks sent after setup.
    pub stream: ChunkStreamReport,
}

/// Errors from the session.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The QUIC stream/connection failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// Opening the setup stream failed.
    #[error("QUIC connection failed: {0}")]
    Connection(#[from] quinn::ConnectionError),
    /// A frame failed to read/write/decode.
    #[error(transparent)]
    Frame(#[from] FrameStreamError),
    /// The chunk stream failed.
    #[error(transparent)]
    Chunk(#[from] ChunkStreamError),
    /// The chunk session failed.
    #[error(transparent)]
    Session(#[from] ChunkSessionError),
    /// The world thread failed.
    #[error(transparent)]
    World(#[from] WorldThreadError),
    /// A queued player/entity/block message failed to write.
    #[error(transparent)]
    PlayerMessage(#[from] players::PlayerMessageError),
    /// Token validation/issue failed.
    #[error(transparent)]
    Auth(#[from] hytale_auth::AuthError),
    /// Common-asset loading failed.
    #[error(transparent)]
    CommonAssets(#[from] hytale_assets::CommonAssetError),
    /// The blocking asset load task failed.
    #[error("common-asset load task failed: {0}")]
    AssetTask(String),
    /// The handshake timed out.
    #[error("connection handshake timed out")]
    Timeout,
    /// Waiting for `ClientReady` timed out.
    #[error("client did not become ready for chunks")]
    WorldJoinTimeout,
    /// The client sent a packet that is invalid in the current state.
    #[error("unexpected packet {packet_id} during {state}")]
    UnexpectedPacket {
        /// Packet id.
        packet_id: u32,
        /// State name.
        state: &'static str,
    },
    /// A token subject was valid JWT-wise but not a protocol UUID.
    #[error("invalid player UUID `{0}`")]
    InvalidUuid(String),
    /// The access token and identity token describe different players.
    #[error("authentication tokens describe different players")]
    TokenMismatch,
    /// The client disconnected.
    #[error("client disconnected during setup")]
    Disconnected,
}

struct Handshake {
    uuid: Uuid,
    username: String,
    skin: Option<PlayerSkin>,
    view_radius: u32,
    auth_mode: &'static str,
    assets: AssetStreamReport,
}

struct EnterGame {
    world_uuid: Uuid,
    radius: u32,
    network_id: i32,
    player_transform: PlayerTransform,
    appearance: PlayerAppearance,
    skin: Option<PlayerSkin>,
    world_time: WorldTimeState,
    weather: Option<i32>,
    gameplay_config: WorldGameplayConfig,
    movement_settings: MovementSettings,
    inventory: PlayerInventory,
    trace: Option<Arc<TraceSession>>,
}

struct LiveGame<'a> {
    center: (i32, i32),
    radius: u32,
    ready_for_gameplay: bool,
    world_time: WorldTimeState,
    weather: Option<i32>,
    network_id: i32,
    players: Arc<PlayerRegistry>,
    broadcasts: mpsc::Receiver<PlayerMessage>,
    view: ViewId,
    transform: &'a mut PlayerTransform,
    inventory: &'a mut PlayerInventory,
    interactions: Arc<InteractionCatalog>,
    revealed_markers: &'a mut BTreeSet<String>,
    map_chunks: &'a mut BTreeSet<(i32, i32)>,
}

/// The one block-container window this session owns (`OpenWindow.id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenContainerWindow {
    id: i32,
    position: (i32, i32, i32),
}

/// Run authentication + setup on an accepted connection, transition to `Game`,
/// then stream the player view.
///
/// This creates a private registry, which is enough for a single-session demo
/// and the existing loopback tests. A listener that serves multiple sessions
/// must share one [`PlayerRegistry`] and call
/// [`serve_connection_with_registry`].
pub async fn serve_connection(
    connection: &quinn::Connection,
    thread: &WorldThread,
    options: &SessionOptions,
) -> Result<SessionReport, SessionError> {
    serve_connection_with_registry(connection, thread, options, Arc::new(PlayerRegistry::new()))
        .await
}

/// Run one session against a listener-owned player registry.
pub async fn serve_connection_with_registry(
    connection: &quinn::Connection,
    thread: &WorldThread,
    options: &SessionOptions,
    players: Arc<PlayerRegistry>,
) -> Result<SessionReport, SessionError> {
    let (send, recv) = connection.accept_bi().await?;
    let trace = options
        .trace
        .as_ref()
        .map(|recorder| recorder.begin_session());
    let mut writer = FrameWriter::new(send);
    let mut reader = FrameReader::new(recv);
    if let Some(trace) = &trace {
        writer = writer.with_trace(Arc::clone(trace) as Arc<dyn hytale_net::PacketTraceSink>);
        reader = reader.with_trace(Arc::clone(trace) as Arc<dyn hytale_net::PacketTraceSink>);
    }

    let config_assets = thread.config_asset_packets()?;
    let common_assets = thread.common_assets()?;
    let client_fingerprint =
        PeerIdentity::from_connection(connection).map(|identity| identity.fingerprint_base64url());

    let handshake = tokio::time::timeout(
        options.handshake_timeout,
        perform_handshake(
            &mut reader,
            &mut writer,
            options,
            client_fingerprint.as_deref(),
            &config_assets,
            &common_assets,
        ),
    )
    .await
    .map_err(|_| SessionError::Timeout)??;

    let start = thread.start_report();
    let radius = if handshake.view_radius == 0 {
        options.default_view_radius
    } else {
        handshake.view_radius
    };

    let player_uuid: WorldUuid = handshake
        .uuid
        .to_string()
        .parse()
        .map_err(|_| SessionError::InvalidUuid(handshake.uuid.to_string()))?;
    let player_state = thread.player_snapshot(player_uuid)?;
    let mut player_transform = player_state.transform;
    let appearance = player_state.appearance;
    let mut inventory = player_state.inventory;
    let mut revealed_markers = player_state.revealed_markers;
    let player_chunk = (
        block_to_chunk(player_transform.position.x),
        block_to_chunk(player_transform.position.z),
    );
    let world_time = thread.world_time()?;
    let weather = thread.weather_at(
        player_transform.position.x.floor() as i32,
        player_transform.position.y.floor() as i32,
        player_transform.position.z.floor() as i32,
    )?;
    let gameplay_config = thread.gameplay_config()?;
    let movement_settings = thread.movement_settings()?;
    let skin = handshake.skin.clone();
    let world_uuid: Uuid = start
        .world_uuid
        .parse()
        .map_err(|_| SessionError::InvalidUuid(start.world_uuid.clone()))?;
    let view_id = thread.add_view(player_chunk.0, player_chunk.1, radius)?;
    let interactions = thread.interaction_catalog()?;

    let (message_tx, message_rx) = players::channel();
    let (player_session, join) = players.register(PlayerJoinRequest {
        uuid: handshake.uuid,
        username: handshake.username.clone(),
        world_uuid,
        transform: player_transform,
        appearance: appearance.clone(),
        skin: skin.clone(),
        view_center: player_chunk,
        view_radius: radius,
        sender: message_tx,
    });
    // Everything after registration runs under the teardown path below, so a
    // failed setup/game loop still persists the player and releases the chunk
    // view instead of leaking a registry entry or loaded chunks.
    let game_result: Result<(GameReport, GameLoopReport, ChunkStreamReport), SessionError> =
        async {
            send_player_list(
                &mut writer,
                &join.player_list,
                &config_assets.tags,
                options.chunk_compression_level,
            )
            .await?;
            let (mut game, mut chunks) = enter_game(
                connection,
                &mut reader,
                &mut writer,
                EnterGame {
                    world_uuid,
                    radius,
                    network_id: join.network_id,
                    player_transform,
                    appearance,
                    skin,
                    world_time,
                    weather,
                    gameplay_config,
                    movement_settings,
                    inventory: inventory.clone(),
                    trace,
                },
                options,
            )
            .await?;
            game.player_list = true;

            let mut stream = send_view_registered(
                thread,
                &mut chunks,
                player_chunk.0,
                player_chunk.1,
                radius,
                options.chunk_compression_level,
            )
            .await?;

            // Runtime entities are streamed after the chunk snapshot, like
            // Java's entity adds that follow chunk tracking.
            let mut runtime_entities = 0;
            for entity in thread.entities()? {
                if chunk_view_contains(player_chunk, radius, entity.transform.position) {
                    writer
                        .write_packet(
                            &players::runtime_entity_updates(&entity),
                            options.chunk_compression_level,
                        )
                        .await?;
                    runtime_entities += 1;
                }
            }
            game.runtime_entities = runtime_entities;
            writer
                .write_packet(
                    &world_map_settings_packet(),
                    options.chunk_compression_level,
                )
                .await?;
            let (visible_markers, new_markers) =
                map_markers_for_player(thread, &revealed_markers, player_transform.position)?;
            if !visible_markers.is_empty() {
                writer
                    .write_packet(
                        &UpdateWorldMap {
                            chunks: None,
                            added_markers: Some(visible_markers),
                            removed_markers: None,
                        },
                        options.chunk_compression_level,
                    )
                    .await?;
            }
            revealed_markers.extend(new_markers);
            if !join.player_markers.is_empty() {
                writer
                    .write_packet(
                        &UpdateWorldMap {
                            chunks: None,
                            added_markers: Some(join.player_markers.clone()),
                            removed_markers: None,
                        },
                        options.chunk_compression_level,
                    )
                    .await?;
            }
            let mut map_chunks = BTreeSet::new();
            let (map_added, map_removed) = send_world_map_images(
                thread,
                &mut writer,
                player_chunk,
                radius,
                &mut map_chunks,
                options.chunk_compression_level,
            )
            .await?;
            game.map_chunks = map_added;
            game.map_chunks_unloaded = map_removed;
            game.world_map = true;
            // Entities already present in the snapshot do not need their
            // queued spawn events replayed.
            let _ = thread.runtime_entity_events()?;

            let game_loop = run_game_loop(
                connection,
                thread,
                &mut reader,
                &mut writer,
                &mut chunks,
                LiveGame {
                    center: player_chunk,
                    radius,
                    ready_for_gameplay: game.client_ready_for_gameplay,
                    world_time,
                    weather,
                    network_id: join.network_id,
                    players: Arc::clone(&players),
                    broadcasts: message_rx,
                    view: view_id,
                    transform: &mut player_transform,
                    inventory: &mut inventory,
                    interactions,
                    revealed_markers: &mut revealed_markers,
                    map_chunks: &mut map_chunks,
                },
                options,
            )
            .await?;
            game.client_ready_for_gameplay |= game_loop.ready_for_gameplay;
            chunks.finish()?;
            stream.chunks += game_loop.chunks;
            stream.packets += game_loop.packets;
            stream.frames += game_loop.frames;
            stream.payload_bytes += game_loop.payload_bytes;
            stream.unloaded += game_loop.unloaded;
            Ok((game, game_loop, stream))
        }
        .await;

    let save_result = thread.save_player_state(player_uuid, player_transform, inventory);
    let save_markers_result =
        thread.save_player_revealed_markers(player_uuid, revealed_markers.clone());
    let view_report = thread.remove_view(view_id);
    // Unregister after the save so a reconnect can never observe the old entity
    // removal after the new player has already loaded its saved state.
    drop(player_session);

    let (game, game_loop, mut stream) = game_result?;
    save_result?;
    save_markers_result?;
    stream.unloaded += view_report?.unloaded;

    Ok(SessionReport {
        uuid: handshake.uuid,
        username: handshake.username,
        view_radius: radius,
        auth_mode: handshake.auth_mode,
        assets: handshake.assets,
        game,
        game_loop,
        stream,
    })
}

async fn perform_handshake(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    options: &SessionOptions,
    client_fingerprint: Option<&str>,
    config_assets: &ConfigAssets,
    common_assets: &CommonAssetRegistry,
) -> Result<Handshake, SessionError> {
    let connect: Connect = expect(reader, Connect::ID).await?;
    if connect.protocol_crc != settings::CRC {
        send_disconnect(writer, "protocol mismatch").await?;
        return Err(SessionError::UnexpectedPacket {
            packet_id: Connect::ID,
            state: "protocol-crc",
        });
    }

    let (identity, auth_mode) = match &options.auth {
        AuthMode::Insecure => {
            writer
                .write_packet(&RequestInsecurePlayerOptions {}, DEFAULT_COMPRESSION_LEVEL)
                .await?;
            let player: InsecurePlayerOptions = expect(reader, InsecurePlayerOptions::ID).await?;
            writer
                .write_packet(
                    &ConnectAccept {
                        password_challenge: None,
                    },
                    DEFAULT_COMPRESSION_LEVEL,
                )
                .await?;
            (
                Identity {
                    uuid: player.uuid,
                    username: player.username,
                    skin: player.skin,
                },
                "insecure",
            )
        }
        AuthMode::Local {
            service,
            token_ttl_secs,
        } => {
            if connect.client_type == ClientType::Editor {
                send_disconnect(writer, "editor connections are not supported").await?;
                return Err(SessionError::UnexpectedPacket {
                    packet_id: Connect::ID,
                    state: "auth:editor",
                });
            }
            let identity_token = match connect.identity_token.clone() {
                Some(token) => token,
                None => {
                    send_disconnect(writer, "server requires authentication").await?;
                    return Err(SessionError::Auth(hytale_auth::AuthError::InvalidClaim {
                        claim: "identityToken",
                        message: "server requires authentication".to_string(),
                    }));
                }
            };
            let claims = match service.validate(
                &identity_token,
                "identity",
                Some(SCOPE_CLIENT),
                client_fingerprint,
            ) {
                Ok(claims) => claims,
                Err(error) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::Auth(error));
                }
            };
            let subject = claims.subject.clone().unwrap_or_default();
            let username = claims.username.clone().unwrap_or_default();
            let uuid: Uuid = subject
                .parse()
                .map_err(|_| SessionError::InvalidUuid(subject.clone()))?;

            let grant =
                service.issue_grant(&subject, &username, client_fingerprint, *token_ttl_secs)?;
            let server_identity = service.issue_server_token(
                LOCAL_SERVER_SUBJECT,
                "server",
                client_fingerprint,
                *token_ttl_secs,
            )?;
            writer
                .write_packet(
                    &AuthGrant {
                        authorization_grant: Some(grant),
                        server_identity_token: Some(server_identity),
                    },
                    DEFAULT_COMPRESSION_LEVEL,
                )
                .await?;

            let auth_token: AuthToken = expect(reader, AuthToken::ID).await?;
            let access_token = auth_token.access_token.clone().ok_or_else(|| {
                SessionError::Auth(hytale_auth::AuthError::InvalidClaim {
                    claim: "accessToken",
                    message: "missing access token".to_string(),
                })
            })?;
            let access_claims = match service.validate(
                &access_token,
                "access",
                Some(SCOPE_CLIENT),
                client_fingerprint,
            ) {
                Ok(claims) => claims,
                Err(error) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::Auth(error));
                }
            };
            if access_claims.subject.as_deref() != Some(subject.as_str())
                || access_claims.username.as_deref() != Some(username.as_str())
            {
                send_disconnect(writer, "authentication failed").await?;
                return Err(SessionError::TokenMismatch);
            }
            let grant_token = auth_token
                .server_authorization_grant
                .clone()
                .ok_or_else(|| {
                    SessionError::Auth(hytale_auth::AuthError::InvalidClaim {
                        claim: "serverAuthorizationGrant",
                        message: "missing mutual-auth grant".to_string(),
                    })
                })?;
            let grant_claims = match service.validate(
                &grant_token,
                "grant",
                Some(SCOPE_SERVER),
                client_fingerprint,
            ) {
                Ok(claims) => claims,
                Err(error) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::Auth(error));
                }
            };
            if grant_claims.subject.as_deref() != Some(subject.as_str()) {
                send_disconnect(writer, "authentication failed").await?;
                return Err(SessionError::TokenMismatch);
            }

            let server_access = service.issue_server_token(
                LOCAL_SERVER_SUBJECT,
                "server",
                client_fingerprint,
                *token_ttl_secs,
            )?;
            writer
                .write_packet(
                    &ServerAuthToken {
                        server_access_token: Some(server_access),
                        password_challenge: None,
                    },
                    DEFAULT_COMPRESSION_LEVEL,
                )
                .await?;
            (
                Identity {
                    uuid,
                    username,
                    skin: None,
                },
                "local",
            )
        }
    };

    let required_assets = common_assets.required_assets();
    writer
        .write_packet(
            &WorldSettings {
                required_assets: Some(required_assets.clone()),
            },
            DEFAULT_COMPRESSION_LEVEL,
        )
        .await?;
    writer
        .write_packet(
            &ServerInfo {
                server_name: Some(options.server_name.clone()),
                motd: Some(options.motd.clone()),
                max_players: options.max_players,
                fallback_server: None,
            },
            DEFAULT_COMPRESSION_LEVEL,
        )
        .await?;

    let mut view_radius = 0u32;
    let mut assets_requested = false;
    let mut assets = AssetStreamReport {
        required: required_assets.len(),
        ..AssetStreamReport::default()
    };
    loop {
        let frame = reader
            .next_frame()
            .await?
            .ok_or(SessionError::UnexpectedPacket {
                packet_id: 0,
                state: "setup-eof",
            })?;
        match frame.packet_id {
            ViewRadius::ID => {
                let packet: ViewRadius = frame.decode()?;
                if packet.value > 0 {
                    view_radius = ((packet.value as f32) / 32.0).ceil() as u32;
                }
            }
            RequestAssets::ID => {
                if assets_requested {
                    return Err(SessionError::UnexpectedPacket {
                        packet_id: frame.packet_id,
                        state: "setup:duplicate-assets",
                    });
                }
                assets_requested = true;
                let packet: RequestAssets = frame.decode()?;
                assets = send_common_assets(
                    writer,
                    common_assets,
                    packet.assets.as_deref(),
                    &config_assets.packets,
                    options.chunk_compression_level,
                )
                .await?;
                assets.required = required_assets.len();
            }
            SetupFinalize::ID => {
                if !assets_requested {
                    return Err(SessionError::UnexpectedPacket {
                        packet_id: frame.packet_id,
                        state: "setup:finalize-without-assets",
                    });
                }
                break;
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
                        DEFAULT_COMPRESSION_LEVEL,
                    )
                    .await?;
            }
            _ => {
                // NetworkTick and other client control packets are harmless here.
            }
        }
    }

    Ok(Handshake {
        uuid: identity.uuid,
        username: identity.username,
        skin: identity.skin,
        view_radius,
        auth_mode,
        assets,
    })
}

struct Identity {
    uuid: Uuid,
    username: String,
    skin: Option<PlayerSkin>,
}

async fn send_common_assets(
    writer: &mut FrameWriter,
    common_assets: &CommonAssetRegistry,
    requested: Option<&[hytale_protocol::generated::common::Asset]>,
    config_packets: &[AssetPacket],
    compression_level: i32,
) -> Result<AssetStreamReport, SessionError> {
    let to_send: Vec<CommonAssetInfo> = common_assets
        .requested(requested)?
        .into_iter()
        .cloned()
        .collect();
    let requested_count = to_send.len();
    let total = to_send.len().max(1);

    let registry = common_assets.clone();
    let load = to_send.clone();
    let blobs = tokio::task::spawn_blocking(move || {
        let refs: Vec<&CommonAssetInfo> = load.iter().collect();
        registry.load_blobs(&refs)
    })
    .await
    .map_err(|error| SessionError::AssetTask(error.to_string()))??;

    let mut report = AssetStreamReport {
        requested: requested_count,
        ..AssetStreamReport::default()
    };
    for (index, (asset, blob)) in to_send.iter().zip(blobs).enumerate() {
        let percent = (index * 100 / total) as i32;
        writer
            .write_packet(
                &AssetInitialize {
                    asset: asset.to_packet(),
                    size: blob.len() as i32,
                },
                compression_level,
            )
            .await?;

        let parts = blob.len().div_ceil(MAX_COMMON_ASSET_FRAME).max(1);
        for part_index in 0..parts {
            let start = part_index * MAX_COMMON_ASSET_FRAME;
            let end = (start + MAX_COMMON_ASSET_FRAME).min(blob.len());
            let bytes = &blob[start..end];
            writer
                .write_packet(
                    &WorldLoadProgress {
                        status: Some(loading_message(&asset.name)),
                        percent_complete: percent,
                        percent_complete_subitem: (part_index * 100 / parts) as i32,
                    },
                    compression_level,
                )
                .await?;
            writer
                .write_packet(
                    &AssetPart {
                        part: Some(bytes.iter().map(|byte| *byte as i8).collect()),
                    },
                    compression_level,
                )
                .await?;
            report.parts += 1;
        }
        writer
            .write_packet(&AssetFinalize {}, compression_level)
            .await?;
        report.sent += 1;
        report.bytes += blob.len();
    }

    if !to_send.is_empty() {
        writer
            .write_packet(&RequestCommonAssetsRebuild {}, compression_level)
            .await?;
    }

    for packet in config_packets {
        writer
            .write_raw_packet(packet.id, &packet.payload, compression_level)
            .await?;
        report.config_packets += 1;
    }

    writer
        .write_packet(
            &UpdateTranslations {
                r#type: UpdateType::Init,
                translations: Some(BTreeMap::new()),
            },
            compression_level,
        )
        .await?;

    writer
        .write_packet(
            &WorldLoadProgress {
                status: Some(FormattedMessage {
                    raw_text: Some("loading world".to_string()),
                    ..FormattedMessage::default()
                }),
                percent_complete: 0,
                percent_complete_subitem: 0,
            },
            compression_level,
        )
        .await?;
    writer
        .write_packet(&WorldLoadFinished {}, compression_level)
        .await?;
    Ok(report)
}

fn loading_message(name: &str) -> FormattedMessage {
    FormattedMessage {
        raw_text: Some(format!("loading {name}")),
        ..FormattedMessage::default()
    }
}

async fn enter_game(
    connection: &quinn::Connection,
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    start: EnterGame,
    options: &SessionOptions,
) -> Result<(GameReport, ChunkStreamWriter), SessionError> {
    let EnterGame {
        world_uuid,
        radius,
        network_id,
        player_transform,
        appearance,
        skin,
        world_time,
        weather,
        gameplay_config,
        movement_settings,
        inventory,
        trace,
    } = start;
    let mut chunks = ChunkStreamWriter::open(connection).await?;
    if let Some(trace) = trace {
        chunks = chunks.with_trace(trace as Arc<dyn hytale_net::PacketTraceSink>);
    }
    chunks
        .write_packet(&ClearChunks {}, options.chunk_compression_level)
        .await?;
    writer
        .write_packet(
            &JoinWorld {
                clear_world: true,
                fade_in_out: true,
                world_uuid,
                min_section_y: Some(0),
                max_section_y: Some(9),
            },
            options.chunk_compression_level,
        )
        .await?;

    let mut game = GameReport {
        clear_chunks: true,
        join_world: true,
        ..GameReport::default()
    };
    let wait = async {
        loop {
            let frame = reader
                .next_frame()
                .await?
                .ok_or(SessionError::Disconnected)?;
            match frame.packet_id {
                ClientReady::ID => {
                    let ready: ClientReady = frame.decode()?;
                    game.client_ready_for_chunks |= ready.ready_for_chunks;
                    game.client_ready_for_gameplay |= ready.ready_for_gameplay;
                    if ready.ready_for_chunks {
                        return Ok::<(), SessionError>(());
                    }
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
                            DEFAULT_COMPRESSION_LEVEL,
                        )
                        .await?;
                }
                _ => {}
            }
        }
    };
    tokio::time::timeout(options.world_join_timeout, wait)
        .await
        .map_err(|_| SessionError::WorldJoinTimeout)??;

    // Java `World.onFinishPlayerJoining`: world radius and entity identity
    // first, then the chunk tracker starts streaming.
    chunks
        .write_packet(
            &ViewRadius {
                value: (radius * 32) as i32,
            },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &SetEntitySeed { entity_seed: 0 },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &SetClientId {
                client_id: network_id,
            },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &SetTimeDilation { time_dilation: 1.0 },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &UpdateFeatures {
                features: Some(BTreeMap::new()),
            },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &SetUpdateRate {
                updates_per_second: 30,
            },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &self_spawn_packet(network_id, player_transform, &appearance, skin),
            options.chunk_compression_level,
        )
        .await?;
    game.self_spawn = true;

    // Java `PlayerSystems` sends the full inventory and the three active-slot
    // selections after the self spawn.
    send_inventory(writer, &inventory, options.chunk_compression_level).await?;
    game.inventory = true;
    for section in [Section::Hotbar, Section::Utility, Section::Tools] {
        let active_slot = inventory
            .active_container(section)
            .and_then(|container| container.active_slot)
            .map_or(-1, i32::from);
        writer
            .write_packet(
                &SetActiveSlot {
                    inventory_section_id: section.section_id(),
                    active_slot,
                },
                options.chunk_compression_level,
            )
            .await?;
    }
    game.active_slots = true;

    writer
        .write_packet(&gameplay_config, options.chunk_compression_level)
        .await?;
    game.gameplay_config = true;
    writer
        .write_packet(
            &UpdateTimeSettings {
                daytime_duration_seconds: world_time.daytime_duration_seconds,
                nighttime_duration_seconds: world_time.nighttime_duration_seconds,
                total_moon_phases: world_time.total_moon_phases,
                time_paused: world_time.time_paused,
            },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &UpdateTime {
                game_time: Some(InstantData {
                    seconds: world_time.game_time_seconds,
                    nanos: world_time.game_time_nanos,
                }),
            },
            options.chunk_compression_level,
        )
        .await?;
    if let Some(weather_index) = weather {
        writer
            .write_packet(
                &UpdateWeather {
                    weather_index,
                    transition_seconds: 0.5,
                },
                options.chunk_compression_level,
            )
            .await?;
        game.weather = true;
    }
    writer
        .write_packet(
            &SetGameMode {
                game_mode: GameMode::Adventure,
            },
            options.chunk_compression_level,
        )
        .await?;
    // Java `MovementManager.update` sends the selected movement config to the
    // client; it is the same data the server validates/clamps against.
    writer
        .write_packet(
            &UpdateMovementSettings { movement_settings },
            options.chunk_compression_level,
        )
        .await?;
    game.time = true;
    game.game_mode = true;
    game.movement_settings = true;
    // This stream stays open for the chunk snapshot.
    Ok((game, chunks))
}

async fn send_player_list(
    writer: &mut FrameWriter,
    players: &[ServerPlayerListPlayer],
    tags: &BTreeMap<String, i32>,
    compression_level: i32,
) -> Result<(), SessionError> {
    writer
        .write_packet(
            &ServerTags {
                tags: (!tags.is_empty()).then(|| tags.clone()),
            },
            compression_level,
        )
        .await?;
    writer
        .write_packet(
            &AddToServerPlayerList {
                players: players.to_vec(),
            },
            compression_level,
        )
        .await?;
    Ok(())
}

fn self_spawn_packet(
    network_id: i32,
    transform: PlayerTransform,
    appearance: &PlayerAppearance,
    auth_skin: Option<PlayerSkin>,
) -> EntityUpdates {
    players::spawn_entity_updates(network_id, transform, appearance, auth_skin)
}

async fn send_inventory(
    writer: &mut FrameWriter,
    inventory: &PlayerInventory,
    compression_level: i32,
) -> Result<(), SessionError> {
    let packet: UpdatePlayerInventory = inventory.to_update_packet();
    writer.write_packet(&packet, compression_level).await?;
    Ok(())
}

fn chunk_view_contains(
    center: (i32, i32),
    radius: u32,
    position: hytale_world::player_data::Position,
) -> bool {
    let radius = i32::try_from(radius).unwrap_or(i32::MAX);
    let x = block_to_chunk(position.x);
    let z = block_to_chunk(position.z);
    (x - center.0).abs() <= radius && (z - center.1).abs() <= radius
}

/// The Java chunk-world-map defaults (`WorldWorldMapConfig`).
fn world_map_settings_packet() -> UpdateWorldMapSettings {
    UpdateWorldMapSettings {
        enabled: true,
        allow_teleport_to_coordinates: false,
        allow_teleport_to_markers: false,
        allow_show_on_map_toggle: true,
        allow_compass_tracking_toggle: true,
        allow_creating_map_markers: false,
        allow_removing_other_players_markers: false,
        default_scale: 128.0,
        min_scale: 32.0,
        max_scale: 175.0,
    }
}

fn map_markers_for_player(
    thread: &WorldThread,
    revealed: &BTreeSet<String>,
    position: hytale_world::player_data::Position,
) -> Result<(Vec<MapMarker>, Vec<String>), WorldThreadError> {
    const DISCOVERY_RADIUS: f64 = 32.0;
    let mut visible = Vec::new();
    let mut discovered = Vec::new();
    for ((x, y, z), component) in thread.block_components()? {
        let Some(marker) = map_marker_from_component(x, y, z, &component.components) else {
            continue;
        };
        let discoverable = marker_discoverable(&component.components);
        let distance_squared =
            (f64::from(x) + 0.5 - position.x).powi(2) + (f64::from(z) + 0.5 - position.z).powi(2);
        let already_revealed = revealed.contains(&marker.id);
        if already_revealed
            || !discoverable
            || distance_squared <= DISCOVERY_RADIUS * DISCOVERY_RADIUS
        {
            if !already_revealed {
                discovered.push(marker.id.clone());
            }
            visible.push(marker);
        }
    }
    Ok((visible, discovered))
}

fn marker_discoverable(components: &serde_json::Value) -> bool {
    components
        .get("Components")
        .and_then(|value| value.get("BlockMapMarker"))
        .and_then(|value| value.get("Discoverable"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

fn map_marker_from_component(
    x: i32,
    y: i32,
    z: i32,
    components: &serde_json::Value,
) -> Option<MapMarker> {
    let marker = components.get("Components")?.get("BlockMapMarker")?;
    let name = marker.get("Name")?.as_str()?;
    let icon = marker.get("Icon")?.as_str()?;
    let icon_size = match marker.get("IconSize").and_then(serde_json::Value::as_str) {
        Some("Major") => MapMarkerIconSize::Major,
        _ => MapMarkerIconSize::Default,
    };
    Some(MapMarker {
        id: format!("block:{x}:{y}:{z}"),
        name: Some(FormattedMessage {
            message_id: Some(name.to_string()),
            ..FormattedMessage::default()
        }),
        marker_image: icon.to_string(),
        transform: ProtocolTransform {
            position: Some(ProtocolPosition {
                x: f64::from(x) + 0.5,
                y: f64::from(y) + 0.5,
                z: f64::from(z) + 0.5,
            }),
            orientation: None,
        },
        context_menu_items: None,
        components: None,
        icon_size,
        compass_image: marker
            .get("CompassIcon")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
    })
}

async fn send_world_map_images(
    thread: &WorldThread,
    writer: &mut FrameWriter,
    center: (i32, i32),
    radius: u32,
    map_chunks: &mut BTreeSet<(i32, i32)>,
    compression_level: i32,
) -> Result<(usize, usize), SessionError> {
    const MAX_CHUNKS_PER_PACKET: usize = 32;
    let radius = i32::try_from(radius).unwrap_or(i32::MAX);
    let desired: BTreeSet<(i32, i32)> = thread
        .loaded_chunks()?
        .into_iter()
        .filter(|(x, z)| (x - center.0).abs() <= radius && (z - center.1).abs() <= radius)
        .collect();
    let removed: Vec<(i32, i32)> = map_chunks.difference(&desired).copied().collect();
    let added: Vec<(i32, i32)> = desired.difference(map_chunks).copied().collect();

    let mut updates = Vec::new();
    let mut added_count = 0;
    for (x, z) in added {
        if let Some(image) = thread.map_chunk_image(x, z)? {
            updates.push(MapChunk {
                chunk_x: x,
                chunk_z: z,
                image: Some(image),
            });
            map_chunks.insert((x, z));
            added_count += 1;
        }
    }
    let removed_count = removed.len();
    for (x, z) in removed {
        updates.push(MapChunk {
            chunk_x: x,
            chunk_z: z,
            image: None,
        });
        map_chunks.remove(&(x, z));
    }

    for batch in updates.chunks(MAX_CHUNKS_PER_PACKET) {
        writer
            .write_packet(
                &UpdateWorldMap {
                    chunks: Some(batch.to_vec()),
                    added_markers: None,
                    removed_markers: None,
                },
                compression_level,
            )
            .await?;
    }

    Ok((added_count, removed_count))
}

async fn open_container_window(
    thread: &WorldThread,
    writer: &mut FrameWriter,
    players: &PlayerRegistry,
    (x, y, z): (i32, i32, i32),
    window_id: i32,
    compression_level: i32,
) -> Result<Option<OpenContainerWindow>, SessionError> {
    let Some(open) = thread.open_block_container(x, y, z)? else {
        return Ok(None);
    };
    if let Ok(Some((before, after))) = thread.set_block_state(x, y, z, "OpenWindow") {
        if before != after {
            players.broadcast_block(ServerSetBlock {
                x,
                y,
                z,
                block_id: after,
                filler: 0,
                rotation: 0,
            });
        }
    }
    writer
        .write_packet(
            &SetPage {
                page: Page::Bench,
                can_close_through_interaction: true,
            },
            compression_level,
        )
        .await?;
    writer
        .write_packet(
            &OpenWindow {
                id: window_id,
                window_type: WindowType::Container,
                window_data: Some(
                    serde_json::json!({ "blockItemId": open.block_item_id }).to_string(),
                ),
                inventory: Some(open.section),
                extra_resources: None,
            },
            compression_level,
        )
        .await?;
    Ok(Some(OpenContainerWindow {
        id: window_id,
        position: (x, y, z),
    }))
}

async fn close_container_window(
    thread: &WorldThread,
    writer: &mut FrameWriter,
    players: &PlayerRegistry,
    open: OpenContainerWindow,
    compression_level: i32,
) -> Result<(), SessionError> {
    let (x, y, z) = open.position;
    if let Ok(Some((before, after))) = thread.set_block_state(x, y, z, "CloseWindow") {
        if before != after {
            players.broadcast_block(ServerSetBlock {
                x,
                y,
                z,
                block_id: after,
                filler: 0,
                rotation: 0,
            });
        }
    }
    writer
        .write_packet(&CloseWindow { id: open.id }, compression_level)
        .await?;
    writer
        .write_packet(
            &SetPage {
                page: Page::None,
                can_close_through_interaction: false,
            },
            compression_level,
        )
        .await?;
    Ok(())
}

async fn run_game_loop(
    connection: &quinn::Connection,
    thread: &WorldThread,
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    chunks: &mut ChunkStreamWriter,
    live: LiveGame<'_>,
    options: &SessionOptions,
) -> Result<GameLoopReport, SessionError> {
    let LiveGame {
        mut center,
        mut radius,
        ready_for_gameplay,
        mut world_time,
        mut weather,
        network_id,
        players,
        broadcasts,
        view,
        transform,
        inventory,
        interactions,
        revealed_markers,
        map_chunks,
    } = live;
    let mut report = GameLoopReport::default();
    let mut ticker = std::time::Instant::now();
    let mut last_activity = std::time::Instant::now();
    let interval = Duration::from_millis(50);
    let mut time_ticker = std::time::Instant::now();
    // The join burst already sent the settings/time; only re-send settings
    // after an actual pause change, and time once per real second while
    // running (Java `TimePacketSystem.BROADCAST_INTERVAL`).
    let mut last_time_paused = world_time.time_paused;
    let mut teleport_sent = false;
    let mut movement_teleport_id = 1i8;
    // Registry view center follows the player's chunk even before the client
    // acknowledges the teleport (the chunk view itself still waits for the ack).
    let mut registry_center = center;
    let mut pending_movement = false;
    let mut last_movement: Option<std::time::Instant> = None;
    let mut open_window: Option<OpenContainerWindow> = None;
    let mut next_window_id = 1i32;
    let mut last_map_view = (center, radius);

    if ready_for_gameplay {
        report.ready_for_gameplay = true;
        send_client_teleport(writer, *transform, options).await?;
        teleport_sent = true;
        report.teleport_sent = true;
    }

    loop {
        if connection.close_reason().is_some() {
            report.disconnected = true;
            break;
        }

        while let Ok(message) = broadcasts.try_recv() {
            players::write_message(writer, chunks, message, options.chunk_compression_level)
                .await?;
            report.player_messages += 1;
        }

        thread.set_npc_targets(players.npc_targets())?;
        let spawn_report = thread.tick_npc_spawns()?;
        report.npc_spawns += spawn_report.spawned;
        let npc_report = thread.tick_npcs()?;
        report.npc_ticks += npc_report.ticked;
        let _ = thread.tick_encounters()?;
        report.npc_moves += npc_report.moved;
        report.npc_attacks += npc_report.attacks;
        report.npc_deaths += npc_report.deaths;

        for event in thread.runtime_entity_events()? {
            match event {
                RuntimeEntityEvent::Spawned(id) | RuntimeEntityEvent::Updated(id) => {
                    if let Some(entity) = thread.entity(id)? {
                        players.broadcast_runtime_entity(&entity);
                        report.runtime_entity_updates += 1;
                    }
                }
                RuntimeEntityEvent::Despawned { id, position } => {
                    players.broadcast_runtime_entity_removed(id, position);
                    report.runtime_entity_despawns += 1;
                }
            }
        }

        let wait = interval.saturating_sub(ticker.elapsed());
        match tokio::time::timeout(wait, reader.next_frame()).await {
            Ok(Ok(Some(frame))) => {
                last_activity = std::time::Instant::now();
                match frame.packet_id {
                    ClientReady::ID => {
                        let ready: ClientReady = frame.decode()?;
                        if ready.ready_for_gameplay {
                            report.ready_for_gameplay = true;
                        }
                        if ready.ready_for_gameplay && !teleport_sent {
                            send_client_teleport(writer, *transform, options).await?;
                            teleport_sent = true;
                            report.teleport_sent = true;
                        }
                    }
                    ClientMovement::ID => {
                        let movement: ClientMovement = frame.decode()?;
                        report.movement_packets += 1;
                        if movement.teleport_ack.is_some() {
                            report.teleport_acked = true;
                        }

                        // Java `ValidateUtil.isSafePosition`/`isSafeDirection`:
                        // non-finite input disconnects before it reaches the world.
                        let unsafe_position = movement
                            .absolute_position
                            .as_ref()
                            .is_some_and(|position| !safe_position(position));
                        let unsafe_orientation = movement
                            .body_orientation
                            .as_ref()
                            .is_some_and(|direction| !safe_direction(direction))
                            || movement
                                .look_orientation
                                .as_ref()
                                .is_some_and(|direction| !safe_direction(direction));
                        if unsafe_position || unsafe_orientation {
                            report.movement_rejections += 1;
                            let _ = send_disconnect(writer, "impossible position").await;
                            report.disconnected = true;
                            break;
                        }

                        let from = Vec3::new(
                            transform.position.x,
                            transform.position.y,
                            transform.position.z,
                        );
                        let target = movement
                            .absolute_position
                            .as_ref()
                            .map(|position| Vec3::new(position.x, position.y, position.z))
                            .or_else(|| {
                                // Java `RELATIVE_POSITION_DELTA_SCALE = 10000`.
                                movement.relative_position.as_ref().map(|relative| {
                                    from.plus(Vec3::new(
                                        f64::from(relative.x) / 10_000.0,
                                        f64::from(relative.y) / 10_000.0,
                                        f64::from(relative.z) / 10_000.0,
                                    ))
                                })
                            });

                        if let Some(target) = target {
                            let bounds = MovementBounds::default();
                            match bounds.validate(from, target) {
                                Err(MovementRejection::NonFinite) => {
                                    report.movement_rejections += 1;
                                    let _ = send_disconnect(writer, "impossible position").await;
                                    report.disconnected = true;
                                    break;
                                }
                                Err(MovementRejection::OutOfBounds)
                                | Err(MovementRejection::StepTooLarge) => {
                                    report.movement_rejections += 1;
                                }
                                Ok(()) => {
                                    let states =
                                        movement.movement_states.clone().unwrap_or_default();
                                    let state = movement_state(&states);
                                    let resolution = thread.resolve_movement(
                                        from,
                                        target,
                                        state.crouching,
                                        states.on_ground && !state.flying && !state.climbing,
                                    )?;
                                    if resolution.corrected {
                                        report.movement_corrections += 1;
                                        report.movement_steps += usize::from(resolution.stepped);
                                        transform.position = hytale_world::player_data::Position {
                                            x: resolution.position.x,
                                            y: resolution.position.y,
                                            z: resolution.position.z,
                                        };
                                        send_client_teleport_correction(
                                            writer,
                                            *transform,
                                            &mut movement_teleport_id,
                                            options,
                                        )
                                        .await?;
                                    } else {
                                        transform.position = hytale_world::player_data::Position {
                                            x: target.x,
                                            y: target.y,
                                            z: target.z,
                                        };
                                    }
                                    pending_movement = true;

                                    let (x, z) = (transform.position.x, transform.position.z);
                                    let next = (block_to_chunk(x), block_to_chunk(z));
                                    if next != registry_center {
                                        players.update_view(network_id, next, radius);
                                        registry_center = next;
                                    }
                                    if next != center && report.teleport_acked {
                                        thread.update_view(view, next.0, next.1, radius)?;
                                        center = next;
                                        report.view_changes += 1;
                                    }
                                }
                            }
                        }

                        if let Some(orientation) =
                            movement.body_orientation.or(movement.look_orientation)
                        {
                            transform.rotation = hytale_world::player_data::Rotation {
                                pitch: orientation.pitch,
                                yaw: orientation.yaw,
                                roll: orientation.roll,
                            };
                            pending_movement = true;
                        }
                    }
                    MouseInteraction::ID => {
                        let interaction: MouseInteraction = frame.decode()?;
                        match handle_block_interaction(
                            thread,
                            &players,
                            transform,
                            inventory,
                            &interactions,
                            &interaction,
                            options.block_break_damage,
                        ) {
                            BlockInteractionOutcome::Edit { consumed } => {
                                report.block_edits += 1;
                                if consumed {
                                    send_inventory(
                                        writer,
                                        inventory,
                                        options.chunk_compression_level,
                                    )
                                    .await?;
                                    report.inventory_updates += 1;
                                    broadcast_held_items(&players, network_id, inventory);
                                }
                            }
                            BlockInteractionOutcome::Use {
                                state_changes,
                                open_container,
                            } => {
                                report.block_uses += 1;
                                report.block_state_changes += state_changes;
                                if let Some(position) = open_container {
                                    if let Some(previous) = open_window.take() {
                                        close_container_window(
                                            thread,
                                            writer,
                                            &players,
                                            previous,
                                            options.chunk_compression_level,
                                        )
                                        .await?;
                                    }
                                    if let Some(opened) = open_container_window(
                                        thread,
                                        writer,
                                        &players,
                                        position,
                                        next_window_id,
                                        options.chunk_compression_level,
                                    )
                                    .await?
                                    {
                                        next_window_id = next_window_id.saturating_add(1).max(1);
                                        open_window = Some(opened);
                                        report.windows_opened += 1;
                                    }
                                }
                            }
                            BlockInteractionOutcome::Rejected => {
                                report.block_edit_rejections += 1;
                            }
                        }
                    }
                    SetActiveSlot::ID => {
                        let packet: SetActiveSlot = frame.decode()?;
                        let Some(section) = Section::from_section_id(packet.inventory_section_id)
                        else {
                            continue;
                        };
                        if inventory.set_active_slot(section, packet.active_slot) {
                            broadcast_held_items(&players, network_id, inventory);
                        }
                    }
                    DropItemStack::ID => {
                        let packet: DropItemStack = frame.decode()?;
                        let Some(section) = Section::from_section_id(packet.inventory_section_id)
                        else {
                            continue;
                        };
                        let Ok(slot) = u16::try_from(packet.slot_id) else {
                            continue;
                        };
                        if packet.quantity <= 0 {
                            continue;
                        }
                        if let Ok(Some(stack)) =
                            inventory.drop_from_slot(section, slot, packet.quantity)
                        {
                            let position = transform.position;
                            let _ = thread.spawn_item_stack_entity(
                                stack,
                                position.x,
                                position.y + 0.5,
                                position.z,
                            );
                            send_inventory(writer, inventory, options.chunk_compression_level)
                                .await?;
                            report.inventory_updates += 1;
                            broadcast_held_items(&players, network_id, inventory);
                        }
                    }
                    CloseWindow::ID => {
                        let packet: CloseWindow = frame.decode()?;
                        if let Some(open) = open_window {
                            if open.id == packet.id {
                                close_container_window(
                                    thread,
                                    writer,
                                    &players,
                                    open,
                                    options.chunk_compression_level,
                                )
                                .await?;
                                open_window = None;
                                report.windows_closed += 1;
                            }
                        }
                    }
                    SendWindowAction::ID => {
                        // T57b exposes the window/page contract only; recipe
                        // and container transaction actions remain gameplay
                        // work (T59/T60). Decoding keeps the packet framed.
                        let _action: SendWindowAction = frame.decode()?;
                    }
                    ViewRadius::ID => {
                        let packet: ViewRadius = frame.decode()?;
                        if packet.value > 0 {
                            let next = ((packet.value as f32) / 32.0).ceil() as u32;
                            if next != radius {
                                radius = next;
                                thread.update_view(view, center.0, center.1, radius)?;
                                players.update_view(network_id, registry_center, radius);
                            }
                        }
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
                                DEFAULT_COMPRESSION_LEVEL,
                            )
                            .await?;
                    }
                    ClientDisconnect::ID => {
                        let _disconnect: ClientDisconnect = frame.decode()?;
                        report.disconnected = true;
                        break;
                    }
                    _ => {}
                }
            }
            Ok(Ok(None)) => {
                report.disconnected = true;
                break;
            }
            Ok(Err(error)) => {
                if connection.close_reason().is_some() {
                    report.disconnected = true;
                    break;
                }
                return Err(SessionError::Frame(error));
            }
            Err(_) => {}
        }

        if last_activity.elapsed() >= options.idle_timeout {
            let _ = send_disconnect(writer, "idle timeout").await;
            report.idle_timeout = true;
            report.disconnected = true;
            break;
        }

        if ticker.elapsed() >= interval {
            thread.tick()?;
            let simulation = thread.simulate(0.05)?;
            report.simulation_ticks += 1;
            for change in simulation.block_changes {
                report.simulation_block_changes += 1;
                players.broadcast_block(ServerSetBlock {
                    x: change.x,
                    y: change.y,
                    z: change.z,
                    block_id: change.block_id,
                    filler: change.filler as i16,
                    rotation: change.rotation as i8,
                });
            }
            for change in simulation.fluid_changes {
                report.simulation_fluid_changes += 1;
                players.broadcast_fluid(ServerSetFluid {
                    x: change.x,
                    y: change.y,
                    z: change.z,
                    fluid_id: change.fluid_id,
                    fluid_level: change.level,
                });
            }
            for packet in thread.block_damage_packets_for_deltas(&simulation.health_deltas)? {
                match packet {
                    BlockDamagePacket::Single(packet) => {
                        players.broadcast_block_damage(packet);
                        report.simulation_health_changes += 1;
                    }
                    BlockDamagePacket::Section(packet) => {
                        report.simulation_health_changes += packet.cmds.len();
                        players.broadcast_block_damages(packet);
                    }
                }
            }
            let updates = send_updates(thread, chunks, options.chunk_compression_level).await?;
            report.chunks += updates.chunks;
            report.packets += updates.packets;
            report.frames += updates.frames;
            report.payload_bytes += updates.payload_bytes;
            report.unloaded += updates.unloaded;
            ticker = std::time::Instant::now();
        }

        if pending_movement
            && last_movement.is_none_or(|last| last.elapsed() >= Duration::from_millis(100))
        {
            players.update_transform(network_id, *transform);
            last_movement = Some(std::time::Instant::now());
            pending_movement = false;
        }

        if time_ticker.elapsed() >= Duration::from_secs(1) {
            let (visible, discovered) =
                map_markers_for_player(thread, revealed_markers, transform.position)?;
            if !discovered.is_empty() {
                let discovered: BTreeSet<String> = discovered.into_iter().collect();
                let added: Vec<MapMarker> = visible
                    .into_iter()
                    .filter(|marker| discovered.contains(&marker.id))
                    .collect();
                if !added.is_empty() {
                    writer
                        .write_packet(
                            &UpdateWorldMap {
                                chunks: None,
                                added_markers: Some(added),
                                removed_markers: None,
                            },
                            options.chunk_compression_level,
                        )
                        .await?;
                }
                revealed_markers.extend(discovered);
            }
            if last_map_view != (center, radius) {
                let (added, removed) = send_world_map_images(
                    thread,
                    writer,
                    center,
                    radius,
                    map_chunks,
                    options.chunk_compression_level,
                )
                .await?;
                report.map_chunks += added;
                report.map_chunks_unloaded += removed;
                last_map_view = (center, radius);
            }
            let current_weather = thread.weather_at(
                transform.position.x.floor() as i32,
                transform.position.y.floor() as i32,
                transform.position.z.floor() as i32,
            )?;
            if weather != current_weather {
                if let Some(weather_index) = current_weather {
                    writer
                        .write_packet(
                            &UpdateWeather {
                                weather_index,
                                transition_seconds: 10.0,
                            },
                            options.chunk_compression_level,
                        )
                        .await?;
                    report.weather_updates += 1;
                }
                weather = current_weather;
            }
            world_time = thread.world_time()?;
            if world_time.time_paused != last_time_paused {
                writer
                    .write_packet(
                        &UpdateTimeSettings {
                            daytime_duration_seconds: world_time.daytime_duration_seconds,
                            nighttime_duration_seconds: world_time.nighttime_duration_seconds,
                            total_moon_phases: world_time.total_moon_phases,
                            time_paused: world_time.time_paused,
                        },
                        options.chunk_compression_level,
                    )
                    .await?;
                report.time_settings_updates += 1;
                last_time_paused = world_time.time_paused;
            }
            if !world_time.time_paused {
                writer
                    .write_packet(
                        &UpdateTime {
                            game_time: Some(InstantData {
                                seconds: world_time.game_time_seconds,
                                nanos: world_time.game_time_nanos,
                            }),
                        },
                        options.chunk_compression_level,
                    )
                    .await?;
                report.time_updates += 1;
            }
            time_ticker = std::time::Instant::now();
        }
    }

    if let Some(open) = open_window.take() {
        let _ = close_container_window(
            thread,
            writer,
            &players,
            open,
            options.chunk_compression_level,
        )
        .await;
        report.windows_closed += 1;
    }

    Ok(report)
}

/// Apply a left/right mouse interaction as a block break/place.
///
/// Invalid targets, out-of-reach positions and non-block items are rejected
/// without propagating an error, so a bad interaction cannot kill the session.
/// A validated left/right mouse interaction for a block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum BlockEdit {
    /// Break the block at `(x, y, z)`.
    Break { x: i32, y: i32, z: i32 },
    /// Place `item_id` at `(x, y, z)`.
    Place {
        x: i32,
        y: i32,
        z: i32,
        item_id: String,
    },
}

fn block_target(
    transform: &PlayerTransform,
    interaction: &MouseInteraction,
) -> Option<(i32, i32, i32)> {
    let world = interaction.world_interaction.as_ref()?;
    if world.entity_id >= 0 {
        return None;
    }
    let target = world.block_position.as_ref()?;
    // Adventure reach (5) plus Java's interaction buffer (2).
    const MAX_REACH_SQUARED: f64 = 7.0 * 7.0;
    let dx = transform.position.x - (f64::from(target.x) + 0.5);
    let dy = transform.position.y - (f64::from(target.y) + 0.5);
    let dz = transform.position.z - (f64::from(target.z) + 0.5);
    if dx * dx + dy * dy + dz * dz > MAX_REACH_SQUARED {
        return None;
    }
    Some((target.x, target.y, target.z))
}

/// Parse the state-independent part of a block interaction.
///
/// This is pure so reach, button-state and held-item validation can be tested
/// without a world thread.
fn parse_block_edit(
    transform: &PlayerTransform,
    interaction: &MouseInteraction,
) -> Option<BlockEdit> {
    let button = interaction.mouse_button.as_ref()?;
    if button.state != MouseButtonState::Pressed {
        return None;
    }
    let (x, y, z) = block_target(transform, interaction)?;

    match button.mouse_button_type {
        MouseButtonType::Left => Some(BlockEdit::Break { x, y, z }),
        MouseButtonType::Right => {
            let item_id = interaction
                .item_in_hand_id
                .as_deref()
                .filter(|id| !id.is_empty() && *id != hytale_world::player_data::EMPTY_ITEM_ID)?;
            Some(BlockEdit::Place {
                x,
                y,
                z,
                item_id: item_id.to_string(),
            })
        }
        MouseButtonType::Middle | MouseButtonType::X1 | MouseButtonType::X2 => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockInteractionOutcome {
    Edit {
        consumed: bool,
    },
    Use {
        state_changes: usize,
        open_container: Option<(i32, i32, i32)>,
    },
    Rejected,
}

fn handle_block_interaction(
    thread: &WorldThread,
    players: &PlayerRegistry,
    transform: &PlayerTransform,
    inventory: &mut PlayerInventory,
    interactions: &InteractionCatalog,
    interaction: &MouseInteraction,
    block_break_damage: f32,
) -> BlockInteractionOutcome {
    if let Some(outcome) = try_block_use(thread, players, transform, interactions, interaction) {
        return outcome;
    }

    match parse_block_edit(transform, interaction) {
        Some(BlockEdit::Break { x, y, z }) => {
            // Block health is authoritative. The default damage breaks a full
            // health block in one hit (pre-T57b behavior); a lower configured
            // value sends partial damage and keeps the block until health hits
            // zero.
            let damage = block_break_damage.clamp(f32::EPSILON, 1.0);
            let remaining = match thread.damage_block(x, y, z, damage) {
                Ok(remaining) => remaining,
                Err(_) => return BlockInteractionOutcome::Rejected,
            };
            if remaining > 0.0 {
                let delta = match thread.take_block_health_delta(x, y, z) {
                    Ok(delta) => delta.unwrap_or(-damage),
                    Err(_) => -damage,
                };
                if let Ok(packets) = thread.block_damage_packets_for_deltas(&[
                    hytale_world::simulation::BlockHealthDelta { x, y, z, delta },
                ]) {
                    for packet in packets {
                        match packet {
                            BlockDamagePacket::Single(packet) => {
                                players.broadcast_block_damage(packet)
                            }
                            BlockDamagePacket::Section(packet) => {
                                players.broadcast_block_damages(packet)
                            }
                        }
                    }
                }
                return BlockInteractionOutcome::Edit { consumed: false };
            }
            let Ok((before, after)) = thread.set_block(x, y, z, 0) else {
                return BlockInteractionOutcome::Rejected;
            };
            if before == 0 || before == after {
                return BlockInteractionOutcome::Rejected;
            }
            players.broadcast_block(ServerSetBlock {
                x,
                y,
                z,
                block_id: after,
                filler: 0,
                rotation: 0,
            });
            BlockInteractionOutcome::Edit { consumed: false }
        }
        Some(BlockEdit::Place { x, y, z, item_id }) => {
            // The inventory is authoritative. The packet's item id remains a
            // compatibility fallback for sessions/tests without a saved
            // inventory (creative/debug joins).
            let (item_id, from_inventory) = inventory
                .right_hand()
                .map(|stack| (stack.item_id.clone(), true))
                .unwrap_or((item_id, false));
            let Ok(Some(block_id)) = thread.block_id_for_item(&item_id) else {
                return BlockInteractionOutcome::Rejected;
            };
            let Ok(Some((_before, after))) = thread.place_block(x, y, z, block_id) else {
                return BlockInteractionOutcome::Rejected;
            };
            if from_inventory && !matches!(inventory.consume_right_hand(1), Ok(Some(_))) {
                return BlockInteractionOutcome::Rejected;
            }
            players.broadcast_block(ServerSetBlock {
                x,
                y,
                z,
                block_id: after,
                filler: 0,
                rotation: 0,
            });
            BlockInteractionOutcome::Edit {
                consumed: from_inventory,
            }
        }
        None => BlockInteractionOutcome::Rejected,
    }
}

fn try_block_use(
    thread: &WorldThread,
    players: &PlayerRegistry,
    transform: &PlayerTransform,
    interactions: &InteractionCatalog,
    interaction: &MouseInteraction,
) -> Option<BlockInteractionOutcome> {
    let button = interaction.mouse_button.as_ref()?;
    if button.state != MouseButtonState::Pressed
        || button.mouse_button_type != MouseButtonType::Right
    {
        return None;
    }
    let (x, y, z) = block_target(transform, interaction)?;
    let (block_key, block_json) = thread.block_type_json(x, y, z).ok()??;
    let block = BlockInteractions::from_json(&block_key, &block_json);

    if block.bench {
        return Some(BlockInteractionOutcome::Use {
            state_changes: 0,
            open_container: None,
        });
    }
    if block.container {
        return Some(BlockInteractionOutcome::Use {
            state_changes: 0,
            open_container: Some((x, y, z)),
        });
    }

    let (interaction_type, root_id) = block.use_root()?;
    let context = ExecutionContext {
        interaction_type,
        vars: interaction_vars(&block_json),
        ..ExecutionContext::default()
    };
    let outcome = execute_root(interactions, root_id, &context);
    if !outcome.success || outcome.actions.is_empty() {
        return None;
    }

    let mut handled = false;
    let mut state_changes = 0;
    let mut open_container = None;
    for action in outcome.actions {
        match action {
            WorldAction::Door { .. } => {
                if let Some((before, after)) = toggle_door(thread, transform, &block_key, x, y, z) {
                    players.broadcast_block(ServerSetBlock {
                        x,
                        y,
                        z,
                        block_id: after,
                        filler: 0,
                        rotation: 0,
                    });
                    if before != after {
                        state_changes += 1;
                    }
                    handled = true;
                }
            }
            WorldAction::ChangeState { changes } => {
                let Some(state) = block.mapped_state(&changes) else {
                    continue;
                };
                if let Ok(Some((before, after))) = thread.set_block_state(x, y, z, state) {
                    players.broadcast_block(ServerSetBlock {
                        x,
                        y,
                        z,
                        block_id: after,
                        filler: 0,
                        rotation: 0,
                    });
                    if before != after {
                        state_changes += 1;
                    }
                    handled = true;
                }
            }
            WorldAction::OpenContainer => {
                open_container = Some((x, y, z));
                handled = true;
            }
            WorldAction::OpenBench => handled = true,
            WorldAction::BreakBlock => {
                if let Ok((before, after)) = thread.set_block(x, y, z, 0) {
                    if before != 0 && before != after {
                        players.broadcast_block(ServerSetBlock {
                            x,
                            y,
                            z,
                            block_id: after,
                            filler: 0,
                            rotation: 0,
                        });
                        state_changes += 1;
                        handled = true;
                    }
                }
            }
            WorldAction::ChangeBlock { .. }
            | WorldAction::UseBlock { .. }
            | WorldAction::PlaceBlock => {}
        }
    }

    handled.then_some(BlockInteractionOutcome::Use {
        state_changes,
        open_container,
    })
}

fn toggle_door(
    thread: &WorldThread,
    transform: &PlayerTransform,
    block_key: &str,
    x: i32,
    y: i32,
    z: i32,
) -> Option<(i32, i32)> {
    let current = state_from_block_key(block_key);
    let state = match current.as_deref() {
        Some("OpenDoorIn") => "CloseDoorOut",
        Some("OpenDoorOut") => "CloseDoorIn",
        _ => {
            // Java `DoorBlockUtils.isInFrontOfDoor` with a zero yaw: the door
            // faces +Z, so a viewer north of the block is in front.
            let in_front = transform.position.z < f64::from(z) + 0.5;
            if in_front {
                "OpenDoorIn"
            } else {
                "OpenDoorOut"
            }
        }
    };
    thread.set_block_state(x, y, z, state).ok().flatten()
}

fn interaction_vars(block_json: &serde_json::Value) -> BTreeMap<String, String> {
    block_json
        .get("InteractionVars")
        .and_then(serde_json::Value::as_object)
        .map(|vars| {
            vars.iter()
                .filter_map(|(key, value)| {
                    value.as_str().map(|value| (key.clone(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn broadcast_held_items(players: &PlayerRegistry, network_id: i32, inventory: &PlayerInventory) {
    players.update_equipment(
        network_id,
        inventory.right_hand().map(|stack| stack.item_id.clone()),
        inventory.left_hand().map(|stack| stack.item_id.clone()),
    );
}

async fn send_client_teleport(
    writer: &mut FrameWriter,
    transform: PlayerTransform,
    options: &SessionOptions,
) -> Result<(), SessionError> {
    writer
        .write_packet(
            &ClientTeleport {
                teleport_id: 0,
                ignored_transform_fields: 0,
                relative_transform_fields: 0,
                model_transform: players::model_transform(transform),
                reset_velocity: true,
            },
            options.chunk_compression_level,
        )
        .await?;
    Ok(())
}

/// Send an authoritative correction after collision resolution.
async fn send_client_teleport_correction(
    writer: &mut FrameWriter,
    transform: PlayerTransform,
    next_id: &mut i8,
    options: &SessionOptions,
) -> Result<(), SessionError> {
    let teleport_id = *next_id;
    *next_id = next_id.wrapping_add(1);
    if *next_id == 0 {
        *next_id = 1;
    }
    writer
        .write_packet(
            &ClientTeleport {
                teleport_id,
                ignored_transform_fields: 0,
                relative_transform_fields: 0,
                model_transform: players::model_transform(transform),
                reset_velocity: true,
            },
            options.chunk_compression_level,
        )
        .await?;
    Ok(())
}

fn safe_position(position: &hytale_protocol::generated::common::Position) -> bool {
    position.x.is_finite() && position.y.is_finite() && position.z.is_finite()
}

fn safe_direction(direction: &hytale_protocol::generated::common::Direction) -> bool {
    direction.yaw.is_finite() && direction.pitch.is_finite() && direction.roll.is_finite()
}

fn movement_state(states: &MovementStates) -> MovementState {
    MovementState {
        walking: states.walking,
        running: states.running,
        sprinting: states.sprinting,
        crouching: states.crouching || states.forced_crouching,
        flying: states.flying,
        climbing: states.climbing,
        in_fluid: states.in_fluid,
        swimming: states.swimming,
        sliding: states.sliding,
        rolling: states.rolling,
        mantling: states.mantling,
    }
}

fn block_to_chunk(value: f64) -> i32 {
    (value.floor() as i32) >> 5
}

async fn send_disconnect(writer: &mut FrameWriter, text: &str) -> Result<(), SessionError> {
    writer
        .write_packet(
            &ServerDisconnect {
                reason: Some(FormattedMessage {
                    raw_text: Some(text.to_string()),
                    ..FormattedMessage::default()
                }),
                r#type: DisconnectType::Disconnect,
            },
            DEFAULT_COMPRESSION_LEVEL,
        )
        .await?;
    Ok(())
}

async fn expect<T: Packet + Wire>(
    reader: &mut FrameReader,
    expected_id: u32,
) -> Result<T, SessionError> {
    let frame = reader
        .next_frame()
        .await?
        .ok_or(SessionError::UnexpectedPacket {
            packet_id: 0,
            state: "handshake-eof",
        })?;
    if frame.packet_id != expected_id {
        return Err(SessionError::UnexpectedPacket {
            packet_id: frame.packet_id,
            state: "handshake",
        });
    }
    Ok(frame.decode()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_protocol::generated::common::{
        BlockPosition, ComponentUpdate, MouseButtonEvent, WorldInteraction,
    };
    use hytale_protocol::generated::wire::Vector2f;
    use hytale_world::player_data::{
        PlayerModelReference, PlayerSkinData, Position, Rotation, EMPTY_ITEM_ID,
    };

    fn transform() -> PlayerTransform {
        PlayerTransform::new(
            Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            Rotation {
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
            },
        )
    }

    fn updates(packet: &EntityUpdates) -> &[ComponentUpdate] {
        packet.updates.as_ref().expect("updates")[0]
            .updates
            .as_deref()
            .expect("component updates")
    }

    #[test]
    fn empty_save_uses_java_empty_equipment_defaults() {
        let packet = self_spawn_packet(1, transform(), &PlayerAppearance::default(), None);
        let updates = updates(&packet);
        match &updates[0] {
            ComponentUpdate::ModelUpdate(update) => assert!(update.model.is_none()),
            other => panic!("expected ModelUpdate, got {other:?}"),
        }
        match &updates[1] {
            ComponentUpdate::PlayerSkinUpdate(update) => assert!(update.skin.is_none()),
            other => panic!("expected PlayerSkinUpdate, got {other:?}"),
        }
        match &updates[2] {
            ComponentUpdate::EquipmentUpdate(update) => {
                assert_eq!(
                    update.armor_ids,
                    Some(vec![
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new()
                    ])
                );
                assert_eq!(update.right_hand_item_id.as_deref(), Some(EMPTY_ITEM_ID));
                assert_eq!(update.left_hand_item_id.as_deref(), Some(EMPTY_ITEM_ID));
            }
            other => panic!("expected EquipmentUpdate, got {other:?}"),
        }
    }

    #[test]
    fn saved_appearance_populates_model_equipment_and_skin() {
        let appearance = PlayerAppearance {
            model: Some(PlayerModelReference {
                id: "Player".to_string(),
                scale: 1.0,
                ..PlayerModelReference::default()
            }),
            skin: Some(PlayerSkinData {
                face: Some("FaceA".to_string()),
                haircut: Some("HairA".to_string()),
                ..PlayerSkinData::default()
            }),
            armor: [
                Some("Armor_Adamantite_Head".to_string()),
                Some("Armor_Adamantite_Chest".to_string()),
                Some("Armor_Adamantite_Hands".to_string()),
                Some("Armor_Adamantite_Legs".to_string()),
            ],
            right_hand: Some("Weapon_Longsword_Adamantite".to_string()),
            left_hand: Some("Potion_Health".to_string()),
        };
        let packet = self_spawn_packet(1, transform(), &appearance, None);
        let updates = updates(&packet);

        match &updates[0] {
            ComponentUpdate::ModelUpdate(update) => {
                let model = update.model.as_ref().expect("model");
                assert_eq!(model.asset_id.as_deref(), Some("Player"));
                assert_eq!(model.scale, 1.0);
            }
            other => panic!("expected ModelUpdate, got {other:?}"),
        }
        match &updates[1] {
            ComponentUpdate::PlayerSkinUpdate(update) => {
                let skin = update.skin.as_ref().expect("saved skin");
                assert_eq!(skin.face.as_deref(), Some("FaceA"));
                assert_eq!(skin.haircut.as_deref(), Some("HairA"));
            }
            other => panic!("expected PlayerSkinUpdate, got {other:?}"),
        }
        match &updates[2] {
            ComponentUpdate::EquipmentUpdate(update) => {
                assert_eq!(
                    update.armor_ids,
                    Some(vec![
                        "Armor_Adamantite_Head".to_string(),
                        "Armor_Adamantite_Chest".to_string(),
                        "Armor_Adamantite_Hands".to_string(),
                        "Armor_Adamantite_Legs".to_string(),
                    ])
                );
                assert_eq!(
                    update.right_hand_item_id.as_deref(),
                    Some("Weapon_Longsword_Adamantite")
                );
                assert_eq!(update.left_hand_item_id.as_deref(), Some("Potion_Health"));
            }
            other => panic!("expected EquipmentUpdate, got {other:?}"),
        }
    }

    #[test]
    fn auth_skin_wins_over_the_saved_skin() {
        let appearance = PlayerAppearance {
            skin: Some(PlayerSkinData {
                face: Some("SavedFace".to_string()),
                ..PlayerSkinData::default()
            }),
            ..PlayerAppearance::default()
        };
        let auth_skin = PlayerSkin {
            face: Some("AuthFace".to_string()),
            ..PlayerSkin::default()
        };
        let packet = self_spawn_packet(1, transform(), &appearance, Some(auth_skin));
        match &updates(&packet)[1] {
            ComponentUpdate::PlayerSkinUpdate(update) => {
                assert_eq!(
                    update.skin.as_ref().unwrap().face.as_deref(),
                    Some("AuthFace")
                );
            }
            other => panic!("expected PlayerSkinUpdate, got {other:?}"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn interaction(
        button: MouseButtonType,
        state: MouseButtonState,
        x: i32,
        y: i32,
        z: i32,
        item: Option<&str>,
    ) -> MouseInteraction {
        MouseInteraction {
            client_timestamp: 0,
            active_slot: 0,
            item_in_hand_id: item.map(ToOwned::to_owned),
            screen_point: Vector2f::default(),
            mouse_button: Some(MouseButtonEvent {
                mouse_button_type: button,
                state,
                clicks: 1,
            }),
            mouse_motion: None,
            world_interaction: Some(WorldInteraction {
                entity_id: -1,
                block_position: Some(BlockPosition { x, y, z }),
                block_rotation: None,
            }),
        }
    }

    #[test]
    fn block_edit_parses_valid_break_and_place() {
        let transform = transform();
        assert_eq!(
            parse_block_edit(
                &transform,
                &interaction(
                    MouseButtonType::Left,
                    MouseButtonState::Pressed,
                    1,
                    2,
                    3,
                    None
                )
            ),
            Some(BlockEdit::Break { x: 1, y: 2, z: 3 })
        );
        assert_eq!(
            parse_block_edit(
                &transform,
                &interaction(
                    MouseButtonType::Right,
                    MouseButtonState::Pressed,
                    1,
                    2,
                    3,
                    Some("Rock_Stone")
                )
            ),
            Some(BlockEdit::Place {
                x: 1,
                y: 2,
                z: 3,
                item_id: "Rock_Stone".to_string()
            })
        );
    }

    #[test]
    fn block_edit_rejects_out_of_reach_released_and_missing_items() {
        let transform = transform();
        assert_eq!(
            parse_block_edit(
                &transform,
                &interaction(
                    MouseButtonType::Left,
                    MouseButtonState::Pressed,
                    100,
                    2,
                    3,
                    None
                )
            ),
            None
        );
        assert_eq!(
            parse_block_edit(
                &transform,
                &interaction(
                    MouseButtonType::Left,
                    MouseButtonState::Released,
                    1,
                    2,
                    3,
                    None
                )
            ),
            None
        );
        assert_eq!(
            parse_block_edit(
                &transform,
                &interaction(
                    MouseButtonType::Right,
                    MouseButtonState::Pressed,
                    1,
                    2,
                    3,
                    None
                )
            ),
            None
        );
        assert_eq!(
            parse_block_edit(
                &transform,
                &interaction(
                    MouseButtonType::Right,
                    MouseButtonState::Pressed,
                    1,
                    2,
                    3,
                    Some(EMPTY_ITEM_ID)
                )
            ),
            None
        );
        assert_eq!(
            parse_block_edit(
                &transform,
                &interaction(
                    MouseButtonType::Middle,
                    MouseButtonState::Pressed,
                    1,
                    2,
                    3,
                    None
                )
            ),
            None
        );
    }

    #[test]
    fn block_map_markers_become_world_map_packets() {
        let marker = map_marker_from_component(
            10,
            64,
            -3,
            &serde_json::json!({
                "Components": {
                    "BlockMapMarker": {
                        "Name": "server.worldevent.hunts.site.title",
                        "Icon": "HuntSite.png",
                        "IconSize": "Major",
                        "CompassIcon": "CompassHunt.png"
                    }
                }
            }),
        )
        .expect("the block marker");
        assert_eq!(marker.id, "block:10:64:-3");
        assert_eq!(marker.marker_image, "HuntSite.png");
        assert_eq!(marker.icon_size, MapMarkerIconSize::Major);
        assert_eq!(marker.compass_image.as_deref(), Some("CompassHunt.png"));
        assert_eq!(
            marker
                .name
                .as_ref()
                .and_then(|name| name.message_id.as_deref()),
            Some("server.worldevent.hunts.site.title")
        );
        assert_eq!(
            marker
                .transform
                .position
                .as_ref()
                .map(|position| (position.x, position.y, position.z)),
            Some((10.5, 64.5, -2.5))
        );
        assert!(marker_discoverable(&serde_json::json!({
            "Components": {"BlockMapMarker": {"Discoverable": true}}
        })));
        assert!(!marker_discoverable(&serde_json::json!({
            "Components": {"BlockMapMarker": {}}
        })));
        let settings = world_map_settings_packet();
        assert!(settings.enabled);
        assert_eq!(settings.default_scale, 128.0);
        assert_eq!(settings.min_scale, 32.0);
        assert_eq!(settings.max_scale, 175.0);
    }
}
