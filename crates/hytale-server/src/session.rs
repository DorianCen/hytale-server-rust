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
//! game:      JoinWorld (clearWorld=false, fadeInOut=false) → ClientReady(readyForChunks)
//!            → world/entity identity packets → chunk stream
//! ```
//!
//! Authentication uses the local EdDSA profile from [`hytale_auth`]: tokens are
//! Ed25519 JWTs with the Java issuer/audience/scope claims and an
//! `cnf.x5t#S256` binding to the client certificate. The packet flow and claim
//! rules are the same as the live session service; only the key authority is
//! local.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use hytale_assets::{
    load_translations, AssetPacket, CommonAssetInfo, CommonAssetRegistry, ConfigAssets,
};
use hytale_auth::live::LiveAuth;
use hytale_auth::{AuthService, SCOPE_CLIENT, SCOPE_SERVER};
use hytale_commands::{CommandContext, CommandEffect, CommandRegistry, CommandSender};
use hytale_entities::movement::{MovementBounds, MovementRejection, MovementState};
use hytale_interactions::{
    execute_root, state_from_block_key, BlockInteractions, ExecutionContext, InteractionCatalog,
    WorldAction,
};
use hytale_inventory::{ItemStack, PlayerInventory, Section};
use hytale_net::{
    ChunkStreamError, ChunkStreamWriter, FrameReader, FrameStreamError, FrameWriter, PeerIdentity,
    TraceRecorder, TraceSession, TransportError, WorldMapStreamWriter,
};
use hytale_protocol::generated::assets::UpdateTranslations;
use hytale_protocol::generated::auth::{AuthGrant, AuthToken, ConnectAccept, ServerAuthToken};
use hytale_protocol::generated::common::ServerPlayerListPlayer;
use hytale_protocol::generated::common::{
    BenchType, BlockPosition, ChatType, ComponentUpdate, DisconnectType, EntityEffectsUpdate,
    EntityStatUpdate, EntityStatsUpdate, FormattedMessage, GameMode, HudComponent, InstantData,
    InteractionState, InteractionType, MapChunk, MapMarker, MapMarkerIconSize, MouseButtonEvent,
    MouseButtonState, MouseButtonType, MovementSettings, MovementStates, MovementStatesUpdate,
    PlayerSkin, PongType, Position as ProtocolPosition, SyncInteractionChain,
    Transform as ProtocolTransform, UpdateType, WindowAction, WorldInteraction,
};
use hytale_protocol::generated::connection::{
    ClientDisconnect, ClientType, Connect, InsecurePlayerOptions, Ping, Pong,
    RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::entities::{EntityUpdates, SetEntitySeed};
use hytale_protocol::generated::interaction::{CancelInteractionChain, SyncInteractionChains};
use hytale_protocol::generated::interface_::Page;
use hytale_protocol::generated::interface_::UpdateKnownRecipes;
use hytale_protocol::generated::interface_::{
    AddToServerPlayerList, ChatMessage, CommandTreeSync, ServerInfo, ServerMessage, SetPage,
    UpdateBossBar, UpdateVisibleHudComponents,
};
use hytale_protocol::generated::inventory::{
    DropItemStack, MoveItemStack, SetActiveSlot, UpdatePlayerInventory,
};
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
use hytale_protocol::generated::window::{
    CloseWindow, OpenWindow, SendWindowAction, UpdateWindow, WindowType,
};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::generated::world::{ClearChunks, PlaySoundEvent3D, SpawnParticleSystem};
use hytale_protocol::generated::world::{
    ServerSetBlock, ServerSetFluid, SetAudioState, UpdateForcedMusic, UpdatePostFxSettings,
    UpdateSunSettings, UpdateTime, UpdateTimeSettings, UpdateWeather, WorldGameplayConfig,
};
use hytale_protocol::generated::worldmap::{UpdateWorldMap, UpdateWorldMapSettings};
use hytale_protocol::io::Uuid;
use hytale_world::chunk::stream::DEFAULT_COMPRESSION_LEVEL;
use hytale_world::collision::Vec3;
use hytale_world::crafting::{CraftingKnowledge, RecipeCatalog};
use hytale_world::player_data::{PlayerAppearance, PlayerTransform};
use hytale_world::world_config::{ClientEffects, WorldUuid};
use thiserror::Error;

use crate::chunk_streamer::{
    send_updates, send_view_registered, ChunkSessionError, ChunkStreamReport,
};
use crate::inventory_rules::AssetItemRules;
use crate::player_gameplay::PlayerGameplay;
use crate::players::{self, PlayerJoinError, PlayerJoinRequest, PlayerMessage, PlayerRegistry};
use crate::security::SessionLimits;
use crate::world_thread::{
    BlockDamagePacket, NpcPresentationKind, RuntimeEntityEvent, ViewId, WorldThread,
    WorldThreadError, WorldTimeState,
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
    /// Live Hytale OAuth/session authority (release clients).
    Authenticated(Arc<LiveAuth>),
}

impl AuthMode {
    /// A local authority using `service` and the default handshake token TTL.
    pub fn local(service: AuthService) -> Self {
        Self::Local {
            service,
            token_ttl_secs: HANDSHAKE_TOKEN_TTL_SECS,
        }
    }

    /// A live Hytale session authority.
    pub fn authenticated(live: Arc<LiveAuth>) -> Self {
        Self::Authenticated(live)
    }

    /// Short label for reports/logging.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Insecure => "insecure",
            Self::Local { .. } => "local",
            Self::Authenticated(_) => "authenticated",
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
    /// Abuse limits: inbound rate/payload, view radius and queue capacity.
    pub limits: SessionLimits,
    /// Health removed from a block per left-click hit (`1.0` = break a full
    /// health block in one hit, the pre-T57b behavior).
    pub block_break_damage: f32,
    /// Optional packet trace recorder shared by every connection.
    pub trace: Option<Arc<TraceRecorder>>,
    /// Optional live portal router (listener-provided).
    pub portal_router: Option<Arc<crate::serve::PortalRouter>>,
    /// Test/gameplay hook: transfer this portal immediately after the join.
    pub portal_on_join: Option<String>,
    /// Listener shutdown signal set by `/stop` (listener-provided).
    pub stop_signal: Option<Arc<AtomicBool>>,
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
            limits: SessionLimits::default(),
            block_break_damage: 1.0,
            trace: None,
            portal_router: None,
            portal_on_join: None,
            stop_signal: None,
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
    /// `ClearChunks` was written on the chunks stream (world switches only).
    pub clear_chunks: bool,
    /// `JoinWorld` was written.
    pub join_world: bool,
    /// `AddToServerPlayerList` contained the joining player.
    pub player_list: bool,
    /// The joining player's self `EntityUpdates` spawn was written.
    pub self_spawn: bool,
    /// Initial `EntityStatsUpdate` for the joining player was written.
    pub player_stats: bool,
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
    /// `UpdateRecipes`/`UpdateKnownRecipes` were written.
    pub recipes: bool,
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
    /// Damage instances applied to this player.
    pub player_damages: usize,
    /// Times this player's health reached zero.
    pub player_deaths: usize,
    /// `EntityStatsUpdate` packets broadcast for this player.
    pub player_stat_updates: usize,
    /// `EntityEffectsUpdate` packets broadcast for this player.
    pub player_effect_updates: usize,
    /// Automatic player respawns after death.
    pub player_respawns: usize,
    /// Encounter particle/sound actions written to this client.
    pub encounter_actions: usize,
    /// Recipe actions accepted by the crafting window.
    pub crafted_items: usize,
    /// Target world name after a live portal handoff.
    pub portal_world: Option<String>,
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
    /// Command lines handled from `ChatMessage`.
    pub commands: usize,
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
    /// The server disconnected the client for exceeding the inbound rate.
    pub rate_limited: bool,
    /// The server disconnected the client because its broadcast queue filled.
    pub queue_overflow: bool,
    /// Client view-radius requests clamped to `SessionLimits::max_view_radius`.
    pub view_radius_clamps: usize,
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
    /// Target world name after a live portal handoff, when one occurred.
    pub portal_world: Option<String>,
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
    /// A live portal transfer failed.
    #[error(transparent)]
    Portal(#[from] crate::universe_runtime::UniverseRuntimeError),
    /// A queued player/entity/block message failed to write.
    #[error(transparent)]
    PlayerMessage(#[from] players::PlayerMessageError),
    /// Token validation/issue failed.
    #[error(transparent)]
    Auth(#[from] hytale_auth::AuthError),
    /// Common-asset loading failed.
    #[error(transparent)]
    CommonAssets(#[from] hytale_assets::CommonAssetError),
    /// Pack asset loading failed (translations and other non-store files).
    #[error(transparent)]
    Assets(#[from] hytale_assets::AssetError),
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
    /// The same player UUID already owns a live session in this world.
    #[error("player {0} is already connected")]
    DuplicateIdentity(Uuid),
    /// The client exceeded the inbound frame budget.
    #[error("client exceeded the inbound packet rate during {phase}")]
    RateLimited {
        /// Handshake/game phase.
        phase: &'static str,
    },
    /// A token subject was valid JWT-wise but not a protocol UUID.
    #[error("invalid player UUID `{0}`")]
    InvalidUuid(String),
    /// The access token and identity token describe different players.
    #[error("authentication tokens describe different players")]
    TokenMismatch,
    /// A live session-service call failed.
    #[error("session-service authentication failed: {0}")]
    AuthService(String),
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
    client_effects: ClientEffects,
    gameplay_config: WorldGameplayConfig,
    movement_settings: MovementSettings,
    inventory: PlayerInventory,
    recipes: Arc<RecipeCatalog>,
    crafting: CraftingKnowledge,
    player_stats_init: Vec<(i32, EntityStatUpdate)>,
    trace: Option<Arc<TraceSession>>,
}

struct LiveGame<'a> {
    center: (i32, i32),
    radius: u32,
    ready_for_gameplay: bool,
    world_time: WorldTimeState,
    weather: Option<i32>,
    network_id: i32,
    player_uuid: WorldUuid,
    username: String,
    commands: Arc<CommandRegistry>,
    players: Arc<PlayerRegistry>,
    broadcasts: mpsc::Receiver<PlayerMessage>,
    view: ViewId,
    transform: &'a mut PlayerTransform,
    inventory: &'a mut PlayerInventory,
    interactions: Arc<InteractionCatalog>,
    recipes: Arc<RecipeCatalog>,
    crafting: &'a mut CraftingKnowledge,
    revealed_markers: &'a mut BTreeSet<String>,
    map_chunks: &'a mut BTreeSet<(i32, i32)>,
    world_map: &'a mut WorldMapStreamWriter,
    overflow: Arc<std::sync::atomic::AtomicBool>,
}

/// The one block-container or bench window this session owns (`OpenWindow.id`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenContainerWindow {
    id: i32,
    position: (i32, i32, i32),
    kind: OpenWindowKind,
    bench_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenWindowKind {
    Container,
    Bench,
}

/// Run authentication + setup on an accepted connection, transition to `Game`,
/// then stream the player view.
///
/// This creates a private registry, which is enough for a single-session demo
/// and the existing loopback tests. A listener that serves multiple sessions
/// must share one [`PlayerRegistry`] and call
/// [`serve_connection_with_registry`].
/// One session's setup/game-loop/stream outputs plus the target world when a
/// live portal handoff occurred.
type SessionGameResult = (
    GameReport,
    GameLoopReport,
    ChunkStreamReport,
    Option<Arc<WorldThread>>,
);

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
    let mut reader =
        FrameReader::new(recv).with_max_payload(options.limits.max_inbound_packet_bytes);
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
    let requested_radius = if handshake.view_radius == 0 {
        options.default_view_radius
    } else {
        handshake.view_radius
    };
    let radius = options.limits.clamp_view_radius(requested_radius);

    let player_uuid: WorldUuid = handshake
        .uuid
        .to_string()
        .parse()
        .map_err(|_| SessionError::InvalidUuid(handshake.uuid.to_string()))?;
    let player_state = thread.player_snapshot(player_uuid)?;
    let mut player_transform = player_state.transform;
    let appearance = player_state.appearance;
    let mut inventory = player_state.inventory;
    let recipes = thread.recipe_catalog()?;
    let mut revealed_markers = player_state.revealed_markers;
    let mut crafting = player_state.crafting;
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
    let client_effects = thread.client_effects()?;
    let movement_settings = thread.movement_settings()?;
    let skin = handshake.skin.clone();
    let username = handshake.username.clone();
    let commands = Arc::new(CommandRegistry::builtin());
    let world_uuid: Uuid = start
        .world_uuid
        .parse()
        .map_err(|_| SessionError::InvalidUuid(start.world_uuid.clone()))?;
    let interactions = thread.interaction_catalog()?;

    let (message_tx, message_rx) =
        players::channel_with_capacity(options.limits.max_queued_messages);
    let (player_session, join) = match players.register(PlayerJoinRequest {
        uuid: handshake.uuid,
        username: handshake.username.clone(),
        world_uuid,
        transform: player_transform,
        appearance: appearance.clone(),
        skin: skin.clone(),
        view_center: player_chunk,
        view_radius: radius,
        sender: message_tx,
    }) {
        Ok(value) => value,
        Err(PlayerJoinError::DuplicateIdentity(uuid)) => {
            send_disconnect(&mut writer, "player is already connected").await?;
            return Err(SessionError::DuplicateIdentity(uuid));
        }
    };
    let mut view_id = match thread.add_view(player_chunk.0, player_chunk.1, radius) {
        Ok(view_id) => view_id,
        Err(error) => {
            let _ = players.unregister(join.network_id);
            return Err(error.into());
        }
    };
    // Everything after registration runs under the teardown path below, so a
    // failed setup/game loop still persists the player and releases the chunk
    // view instead of leaking a registry entry or loaded chunks.
    let game_result: Result<SessionGameResult, SessionError> = async {
        send_player_list(
            &mut writer,
            &join.player_list,
            &config_assets.tags,
            options.chunk_compression_level,
        )
        .await?;
        let player_stats_init =
            thread.ensure_player_gameplay(join.network_id, player_state.health)?;
        let (mut game, mut chunks, mut world_map) = enter_game(
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
                client_effects,
                gameplay_config,
                movement_settings,
                inventory: inventory.clone(),
                recipes: Arc::clone(&recipes),
                crafting: crafting.clone(),
                player_stats_init,
                trace,
            },
            options,
        )
        .await?;
        game.player_list = true;
        // Java `PlayerHudManagerSystems.InitializeSystem` sends the default
        // visible HUD components when the player entity is added. Send them
        // after the stat map so the HUD can resolve every stat index.
        writer
            .write_packet(
                &UpdateVisibleHudComponents {
                    visible_components: Some(default_hud_components()),
                },
                options.chunk_compression_level,
            )
            .await?;

        // Java `GamePacketHandler.registered0` sends the command tree when the
        // playing handler is installed, before chunk tracking starts.
        // No permission provider is wired yet; the authenticated player is the
        // server operator for this first command-framework slice.
        let command_sender = CommandSender::player(username.clone()).operator();
        let command_tree: CommandTreeSync = commands.tree(&command_sender);
        writer
            .write_packet(&command_tree, options.chunk_compression_level)
            .await?;

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
        let (visible_markers, new_markers) =
            map_markers_for_player(thread, &revealed_markers, player_transform.position)?;
        if !visible_markers.is_empty() {
            world_map
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
            world_map
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
            &mut world_map,
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

        let (game_loop, portal_thread, active_view) = run_game_loop(
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
                player_uuid,
                username: username.clone(),
                commands: Arc::clone(&commands),
                players: Arc::clone(&players),
                broadcasts: message_rx,
                view: view_id,
                transform: &mut player_transform,
                inventory: &mut inventory,
                interactions,
                recipes: Arc::clone(&recipes),
                crafting: &mut crafting,
                revealed_markers: &mut revealed_markers,
                map_chunks: &mut map_chunks,
                world_map: &mut world_map,
                overflow: Arc::clone(&join.overflow),
            },
            options,
        )
        .await?;
        game.client_ready_for_gameplay |= game_loop.ready_for_gameplay;
        chunks.finish()?;
        world_map.finish()?;
        stream.chunks += game_loop.chunks;
        stream.packets += game_loop.packets;
        stream.frames += game_loop.frames;
        stream.payload_bytes += game_loop.payload_bytes;
        stream.unloaded += game_loop.unloaded;
        view_id = active_view;
        Ok((game, game_loop, stream, portal_thread))
    }
    .await;

    let (game, game_loop, mut stream, portal_thread) = game_result?;
    let active_thread = portal_thread.as_deref().unwrap_or(thread);
    let save_result = active_thread.save_player_state(player_uuid, player_transform, inventory);
    let save_markers_result =
        active_thread.save_player_revealed_markers(player_uuid, revealed_markers.clone());
    let save_health_result = match active_thread.player_health(join.network_id) {
        Ok(Some(health)) => active_thread.save_player_health(player_uuid, health),
        _ => Ok(()),
    };
    let _ = active_thread.remove_player_gameplay(join.network_id);
    let view_report = active_thread.remove_view(view_id);
    // Unregister after the save so a reconnect can never observe the old entity
    // removal after the new player has already loaded its saved state.
    drop(player_session);

    let portal_world = game_loop.portal_world.clone();
    save_result?;
    save_markers_result?;
    save_health_result?;
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
        portal_world,
    })
}

/// Decode the live identity token's `profile.skin` JSON into the protocol
/// `PlayerSkin`. The token carries the same camelCase fields as
/// `InsecurePlayerOptions.skin`; generated protocol types do not implement
/// `Deserialize`, so map the known fields explicitly.
fn live_skin(raw: &str) -> Option<PlayerSkin> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    // Java sometimes stores the skin as a JSON string inside the claim.
    let value = match value {
        serde_json::Value::String(text) => serde_json::from_str(&text).ok()?,
        value => value,
    };
    let field = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    Some(PlayerSkin {
        body_characteristic: field("bodyCharacteristic"),
        underwear: field("underwear"),
        face: field("face"),
        eyes: field("eyes"),
        ears: field("ears"),
        mouth: field("mouth"),
        facial_hair: field("facialHair"),
        haircut: field("haircut"),
        eyebrows: field("eyebrows"),
        pants: field("pants"),
        overpants: field("overpants"),
        undertop: field("undertop"),
        overtop: field("overtop"),
        shoes: field("shoes"),
        head_accessory: field("headAccessory"),
        face_accessory: field("faceAccessory"),
        ear_accessory: field("earAccessory"),
        skin_feature: field("skinFeature"),
        gloves: field("gloves"),
        cape: field("cape"),
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
    let language = connect.language.clone();
    if connect.protocol_crc != settings::CRC {
        send_disconnect(writer, "protocol mismatch").await?;
        return Err(SessionError::UnexpectedPacket {
            packet_id: Connect::ID,
            state: "protocol-crc",
        });
    }
    // T61: editor clients are unsupported in every auth mode. Refuse before
    // the auth branch so insecure mode cannot bypass the check.
    if connect.client_type == ClientType::Editor {
        send_disconnect(writer, "editor connections are not supported").await?;
        return Err(SessionError::UnexpectedPacket {
            packet_id: Connect::ID,
            state: "auth:editor",
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
        AuthMode::Authenticated(live) => {
            let identity_token = connect
                .identity_token
                .clone()
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    SessionError::Auth(hytale_auth::AuthError::InvalidClaim {
                        claim: "identityToken",
                        message: "server requires authentication".to_string(),
                    })
                })?;
            let identity_claims = match live.validate_identity_token(&identity_token, SCOPE_CLIENT)
            {
                Ok(claims) => claims,
                Err(error) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::Auth(error));
                }
            };
            let subject = identity_claims.subject.clone();
            let username = identity_claims.username.clone();
            let uuid: Uuid = subject
                .parse()
                .map_err(|_| SessionError::InvalidUuid(subject.clone()))?;
            let skin = identity_claims.skin.as_deref().and_then(live_skin);

            let grant_live = Arc::clone(live);
            let grant_identity = identity_token.clone();
            let grant = match tokio::task::spawn_blocking(move || {
                grant_live.request_player_auth_grant(&grant_identity)
            })
            .await
            {
                Ok(Ok(grant)) => grant,
                Ok(Err(error)) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::Auth(error));
                }
                Err(error) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::AuthService(format!(
                        "auth-grant task failed: {error}"
                    )));
                }
            };
            let server_identity = live.identity_token().ok_or_else(|| {
                SessionError::AuthService("server identity token is missing".to_string())
            })?;
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
            let access_token = auth_token
                .access_token
                .clone()
                .filter(|token| !token.is_empty())
                .ok_or_else(|| {
                    SessionError::Auth(hytale_auth::AuthError::InvalidClaim {
                        claim: "accessToken",
                        message: "missing access token".to_string(),
                    })
                })?;
            let client_fingerprint = client_fingerprint
                .ok_or_else(|| SessionError::Auth(hytale_auth::AuthError::CertificateBinding))?;
            let access_claims = match live.validate_access_token(&access_token, client_fingerprint)
            {
                Ok(claims) => claims,
                Err(error) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::Auth(error));
                }
            };
            if access_claims.subject != subject || access_claims.username != username {
                send_disconnect(writer, "authentication failed").await?;
                return Err(SessionError::TokenMismatch);
            }
            let grant_token = auth_token
                .server_authorization_grant
                .clone()
                .filter(|grant| !grant.is_empty())
                .ok_or_else(|| {
                    SessionError::Auth(hytale_auth::AuthError::InvalidClaim {
                        claim: "serverAuthorizationGrant",
                        message: "missing mutual-auth grant".to_string(),
                    })
                })?;
            let exchange_live = Arc::clone(live);
            let server_access = match tokio::task::spawn_blocking(move || {
                exchange_live.exchange_player_grant(&grant_token)
            })
            .await
            {
                Ok(Ok(token)) => token,
                Ok(Err(error)) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::Auth(error));
                }
                Err(error) => {
                    send_disconnect(writer, "authentication failed").await?;
                    return Err(SessionError::AuthService(format!(
                        "auth-token task failed: {error}"
                    )));
                }
            };
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
                    skin,
                },
                "authenticated",
            )
        }
    };

    let translations = load_translations(common_assets.pack(), &language)?;

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
    let mut inbound = options.limits.inbound_bucket();
    loop {
        let frame = reader
            .next_frame()
            .await?
            .ok_or(SessionError::UnexpectedPacket {
                packet_id: 0,
                state: "setup-eof",
            })?;
        if !inbound.allow(std::time::Instant::now()) {
            send_disconnect(writer, "packet rate limit exceeded").await?;
            return Err(SessionError::RateLimited { phase: "setup" });
        }
        match frame.packet_id {
            ViewRadius::ID => {
                let packet: ViewRadius = frame.decode()?;
                if packet.value > 0 {
                    view_radius = options
                        .limits
                        .clamp_view_radius(((packet.value as f32) / 32.0).ceil() as u32);
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
                    &translations,
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
    translations: &BTreeMap<String, String>,
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
                translations: Some(translations.clone()),
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
) -> Result<(GameReport, ChunkStreamWriter, WorldMapStreamWriter), SessionError> {
    let EnterGame {
        world_uuid,
        radius,
        network_id,
        player_transform,
        appearance,
        skin,
        world_time,
        weather,
        client_effects,
        gameplay_config,
        movement_settings,
        inventory,
        recipes,
        crafting,
        player_stats_init,
        trace,
    } = start;
    let mut chunks = ChunkStreamWriter::open(connection).await?;
    let mut world_map = WorldMapStreamWriter::open(connection).await?;
    if let Some(trace) = trace {
        chunks = chunks.with_trace(trace.clone() as Arc<dyn hytale_net::PacketTraceSink>);
        world_map = world_map.with_trace(trace as Arc<dyn hytale_net::PacketTraceSink>);
    }
    // Java `Universe` joins a fresh connection with
    // `world.addPlayer(playerRef, null, false, false)`: the first world add
    // neither clears the client's (empty) chunk view nor plays the world-switch
    // fade. Sending `ClearChunks`/`clearWorld`/`fadeInOut` here makes the client
    // treat the first join as a transition and discard the chunks that arrive
    // while it is still entering the world.
    writer
        .write_packet(
            &JoinWorld {
                clear_world: false,
                fade_in_out: false,
                world_uuid,
                min_section_y: Some(0),
                max_section_y: Some(9),
            },
            options.chunk_compression_level,
        )
        .await?;

    let mut game = GameReport {
        clear_chunks: false,
        join_world: true,
        ..GameReport::default()
    };
    let wait = async {
        let mut inbound = options.limits.inbound_bucket();
        loop {
            let frame = reader
                .next_frame()
                .await?
                .ok_or(SessionError::Disconnected)?;
            if !inbound.allow(std::time::Instant::now()) {
                send_disconnect(writer, "packet rate limit exceeded").await?;
                return Err(SessionError::RateLimited { phase: "join" });
            }
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
    // Java `World.onFinishPlayerJoining` writes the world's client effects
    // (sun and post-FX) before the chunk tracker starts.
    writer
        .write_packet(
            &UpdateSunSettings {
                height_percentage: client_effects.sun_height_percent as f32,
                angle_radians: client_effects.sun_angle_degrees.to_radians() as f32,
            },
            options.chunk_compression_level,
        )
        .await?;
    writer
        .write_packet(
            &UpdatePostFxSettings {
                global_intensity: client_effects.bloom_intensity as f32,
                power: client_effects.bloom_power as f32,
                sunshaft_scale: client_effects.sunshaft_scale_factor as f32,
                sun_intensity: client_effects.sun_intensity as f32,
                sunshaft_intensity: client_effects.sunshaft_intensity as f32,
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
            &self_spawn_packet(
                network_id,
                player_transform,
                &appearance,
                skin,
                &player_stats_init,
            ),
            options.chunk_compression_level,
        )
        .await?;
    game.self_spawn = true;
    game.player_stats = !player_stats_init.is_empty();

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

    if !recipes.is_empty() {
        // `UpdateRecipes` is a Setup-stage asset packet and was already sent in
        // the config asset burst; the client rejects it at the Playing stage.
        let known_map = recipes.known_for(&crafting.learned, crafting.memories_level);
        writer
            .write_packet(
                &UpdateKnownRecipes {
                    known: Some(known_map),
                },
                options.chunk_compression_level,
            )
            .await?;
        game.recipes = true;
    }

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
    // Java writes the world-map settings before chunk tracking begins; the
    // client expects them before the initial chunk batch.
    writer
        .write_packet(
            &world_map_settings_packet(),
            options.chunk_compression_level,
        )
        .await?;
    // This stream stays open for the chunk snapshot.
    Ok((game, chunks, world_map))
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

/// Java `HudManager.DEFAULT_HUD_COMPONENTS`.
fn default_hud_components() -> Vec<HudComponent> {
    vec![
        HudComponent::UtilitySlotSelector,
        HudComponent::BlockVariantSelector,
        HudComponent::StatusIcons,
        HudComponent::Hotbar,
        HudComponent::Chat,
        HudComponent::Notifications,
        HudComponent::KillFeed,
        HudComponent::InputBindings,
        HudComponent::Reticle,
        HudComponent::Compass,
        HudComponent::Speedometer,
        HudComponent::ObjectivePanel,
        HudComponent::PortalPanel,
        HudComponent::EventTitle,
        HudComponent::Stamina,
        HudComponent::AmmoIndicator,
        HudComponent::Health,
        HudComponent::Mana,
        HudComponent::Oxygen,
        HudComponent::BuilderToolsLegend,
        HudComponent::Sleep,
        HudComponent::Abilities,
        HudComponent::BossBar,
    ]
}

fn self_spawn_packet(
    network_id: i32,
    transform: PlayerTransform,
    appearance: &PlayerAppearance,
    auth_skin: Option<PlayerSkin>,
    player_stats_init: &[(i32, EntityStatUpdate)],
) -> EntityUpdates {
    let mut packet = players::spawn_entity_updates(network_id, transform, appearance, auth_skin);
    let components = packet
        .updates
        .as_mut()
        .and_then(|updates| updates.get_mut(0))
        .and_then(|update| update.updates.as_mut())
        .expect("self spawn always carries components");
    // Java `PlayerSystems.sendPlayerSelf` order: ... Transform, EntityEffects,
    // EntityStats. The local movement/animation code also needs MovementStates.
    components.push(ComponentUpdate::MovementStatesUpdate(
        MovementStatesUpdate {
            movement_states: MovementStates::default(),
        },
    ));
    components.push(ComponentUpdate::EntityEffectsUpdate(EntityEffectsUpdate {
        entity_effect_updates: Vec::new(),
    }));
    if !player_stats_init.is_empty() {
        let mut entity_stat_updates = BTreeMap::new();
        for (index, update) in player_stats_init {
            entity_stat_updates
                .entry(*index)
                .or_insert_with(Vec::new)
                .push(update.clone());
        }
        components.push(ComponentUpdate::EntityStatsUpdate(EntityStatsUpdate {
            entity_stat_updates,
        }));
    }
    packet
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

async fn send_server_message(
    writer: &mut FrameWriter,
    message: &str,
    options: &SessionOptions,
) -> Result<(), SessionError> {
    writer
        .write_packet(
            &ServerMessage {
                r#type: ChatType::Chat,
                message: Some(FormattedMessage {
                    raw_text: Some(message.to_string()),
                    ..FormattedMessage::default()
                }),
            },
            options.chunk_compression_level,
        )
        .await?;
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
    writer: &mut WorldMapStreamWriter,
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

/// Map an encounter target id (runtime NPC id or high-bit player id) to the
/// client-facing entity network id.
fn encounter_network_id(id: u64) -> i32 {
    if id & players::PLAYER_NPC_TARGET_OFFSET != 0 {
        (id & !players::PLAYER_NPC_TARGET_OFFSET) as i32
    } else {
        players::runtime_network_id(id)
    }
}

fn encounter_boss_bar_packet(
    action: &hytale_npc::encounter::EncounterAction,
    last_entity: &mut Option<i32>,
) -> Option<UpdateBossBar> {
    match action {
        hytale_npc::encounter::EncounterAction::BossBar {
            name,
            hard_mode,
            entity,
            ..
        } => {
            let entity_network_id = entity.map(encounter_network_id).unwrap_or(0);
            *last_entity = Some(entity_network_id);
            Some(UpdateBossBar {
                entity_network_id,
                name: Some(FormattedMessage {
                    raw_text: Some(name.clone()),
                    ..FormattedMessage::default()
                }),
                hide: false,
                hard_mode: *hard_mode,
            })
        }
        hytale_npc::encounter::EncounterAction::ClearBossBar => Some(UpdateBossBar {
            entity_network_id: last_entity.take().unwrap_or(0),
            name: None,
            hide: true,
            hard_mode: false,
        }),
        _ => None,
    }
}

fn encounter_audio_packet(
    action: &hytale_npc::encounter::EncounterAction,
    assets: &hytale_assets::AssetRegistry,
) -> Option<SetAudioState> {
    let hytale_npc::encounter::EncounterAction::AudioState {
        audio_state, value, ..
    } = action
    else {
        return None;
    };
    let audio_state_index = assets.audio_states.id_of(audio_state)?;
    let record = assets.audio_states.get(audio_state)?;
    let value_index = record
        .value
        .get("Values")?
        .as_array()?
        .iter()
        .position(|candidate| candidate.as_str() == Some(value.as_str()))?
        as i32;
    Some(SetAudioState {
        audio_state_index,
        value_index,
        transition_override: None,
    })
}

fn encounter_particle_packet(
    action: &hytale_npc::encounter::EncounterAction,
    position: ProtocolPosition,
) -> Option<SpawnParticleSystem> {
    let hytale_npc::encounter::EncounterAction::Particles {
        particle_system, ..
    } = action
    else {
        return None;
    };
    Some(SpawnParticleSystem {
        particle_system_id: Some(particle_system.clone()),
        position: Some(position),
        rotation: None,
        scale: 1.0,
        color: None,
        max_duration: 0.0,
    })
}

fn encounter_sound_packet(
    action: &hytale_npc::encounter::EncounterAction,
    sound_event_index: i32,
    position: ProtocolPosition,
) -> Option<PlaySoundEvent3D> {
    let hytale_npc::encounter::EncounterAction::PlaySound { .. } = action else {
        return None;
    };
    Some(PlaySoundEvent3D {
        sound_event_index,
        category: hytale_protocol::generated::common::SoundCategory::SFX,
        position: Some(position),
        volume_modifier: 1.0,
        pitch_modifier: 1.0,
    })
}

struct CraftPlan {
    outputs: Vec<ItemStack>,
    consumed: Vec<(String, i32)>,
}

fn plan_recipe(
    inventory: &mut PlayerInventory,
    recipe: &hytale_protocol::generated::common::CraftingRecipe,
    quantity: i32,
    rules: &dyn hytale_inventory::ItemRules,
    resource_types: &hytale_world::crafting::ResourceTypeIndex,
) -> Option<CraftPlan> {
    if quantity <= 0 {
        return None;
    }
    let mut required: Vec<(String, Vec<String>, i32)> = Vec::new();
    for input in recipe.inputs.iter().flatten() {
        let amount = input.quantity.saturating_mul(quantity);
        if amount <= 0 {
            continue;
        }
        if let Some(item_id) = &input.item_id {
            required.push((item_id.clone(), vec![item_id.clone()], amount));
            continue;
        }
        let Some(resource_type) = &input.resource_type_id else {
            return None;
        };
        let candidates = resource_types
            .items(resource_type)?
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return None;
        }
        required.push((resource_type.clone(), candidates, amount));
    }
    if required.is_empty() {
        return None;
    }
    for (_, candidates, amount) in &required {
        let available: i32 = candidates
            .iter()
            .map(|item_id| inventory.count_item(item_id))
            .sum();
        if available < *amount {
            return None;
        }
    }
    let outputs: Vec<_> = recipe
        .outputs
        .clone()
        .or_else(|| recipe.primary_output.clone().map(|output| vec![output]))
        .unwrap_or_default();
    if outputs.is_empty() {
        return None;
    }

    let mut consumed = Vec::new();
    for (_, candidates, amount) in required {
        let mut remaining = amount;
        for item_id in candidates {
            if remaining <= 0 {
                break;
            }
            let removed = inventory.remove_item(&item_id, remaining);
            if removed > 0 {
                consumed.push((item_id, removed));
                remaining -= removed;
            }
        }
        if remaining > 0 {
            // The availability check above should make this unreachable; fail
            // closed without producing a partial craft.
            return None;
        }
    }
    let mut stacks = Vec::new();
    for output in outputs {
        let Some(item_id) = output.item_id else {
            continue;
        };
        let amount = output.quantity.saturating_mul(quantity);
        if amount <= 0 {
            continue;
        }
        if let Ok(stack) = ItemStack::new(&item_id, amount, rules) {
            stacks.push(stack);
        }
    }
    Some(CraftPlan {
        outputs: stacks,
        consumed,
    })
}

#[cfg(test)]
fn apply_recipe(
    inventory: &mut PlayerInventory,
    recipe: &hytale_protocol::generated::common::CraftingRecipe,
    quantity: i32,
    rules: &dyn hytale_inventory::ItemRules,
    resource_types: &hytale_world::crafting::ResourceTypeIndex,
) -> Option<Vec<ItemStack>> {
    plan_recipe(inventory, recipe, quantity, rules, resource_types).map(|plan| plan.outputs)
}

#[derive(Debug)]
struct CraftJob {
    recipe_id: String,
    quantity: i32,
    remaining: f32,
    outputs: Vec<ItemStack>,
    consumed: Vec<(String, i32)>,
}

/// One session's bench crafting queue. Inputs are consumed when the job is
/// enqueued; outputs are granted when the recipe timer reaches zero.
#[derive(Debug, Default)]
struct CraftQueue {
    jobs: Vec<CraftJob>,
}

impl CraftQueue {
    fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    fn push(&mut self, job: CraftJob) {
        self.jobs.push(job);
    }

    fn cancel_last(&mut self) -> Option<CraftJob> {
        self.jobs.pop()
    }

    fn tick(&mut self, dt: f32) -> Vec<CraftJob> {
        let mut finished = Vec::new();
        let mut index = 0;
        while index < self.jobs.len() {
            self.jobs[index].remaining -= dt.max(0.0);
            if self.jobs[index].remaining > 0.0 {
                index += 1;
            } else {
                finished.push(self.jobs.remove(index));
            }
        }
        finished
    }
}

#[allow(clippy::too_many_arguments)]
fn begin_craft(
    thread: &WorldThread,
    inventory: &mut PlayerInventory,
    open: Option<&OpenContainerWindow>,
    recipes: &RecipeCatalog,
    knowledge: &mut CraftingKnowledge,
    recipe_id: &str,
    quantity: i32,
) -> Result<Option<CraftJob>, SessionError> {
    if quantity <= 0 {
        return Ok(None);
    }
    let Some(recipe) = recipes.get(recipe_id) else {
        return Ok(None);
    };
    if knowledge.can_craft(recipe).is_err() {
        return Ok(None);
    }
    if let Some(requirements) = recipe.bench_requirement.as_ref() {
        if !requirements.is_empty() {
            let Some(window) = open else {
                return Ok(None);
            };
            let Some(bench_id) = window.bench_id.as_deref() else {
                return Ok(None);
            };
            if !requirements
                .iter()
                .any(|requirement| requirement.id == bench_id)
            {
                return Ok(None);
            }
        }
    }
    let assets = thread.asset_registry()?;
    let rules = AssetItemRules::new(&assets);
    let Some(plan) = plan_recipe(
        inventory,
        recipe,
        quantity,
        &rules,
        recipes.resource_types(),
    ) else {
        return Ok(None);
    };
    knowledge.learn(recipe_id);
    Ok(Some(CraftJob {
        recipe_id: recipe_id.to_string(),
        quantity,
        remaining: recipe.time_seconds.max(0.0) * quantity as f32,
        outputs: plan.outputs,
        consumed: plan.consumed,
    }))
}

#[allow(clippy::too_many_arguments)]
async fn tick_crafts(
    queue: &mut CraftQueue,
    dt: f32,
    thread: &WorldThread,
    writer: &mut FrameWriter,
    players: &PlayerRegistry,
    inventory: &mut PlayerInventory,
    network_id: i32,
    position: hytale_world::player_data::Position,
    compression_level: i32,
) -> Result<usize, SessionError> {
    let assets = thread.asset_registry()?;
    let rules = AssetItemRules::new(&assets);
    let mut completed = 0;
    let mut changed = false;
    for job in queue.tick(dt) {
        for stack in job.outputs {
            if let Some(remainder) = inventory.pickup(stack, &rules) {
                let _ = thread.spawn_item_stack_entity(
                    remainder,
                    position.x,
                    position.y + 0.5,
                    position.z,
                );
            }
        }
        completed += job.quantity as usize;
        let _ = &job.recipe_id;
        changed = true;
    }
    if changed {
        send_inventory(writer, inventory, compression_level).await?;
        broadcast_held_items(players, network_id, inventory);
    }
    Ok(completed)
}

#[allow(clippy::too_many_arguments)]
async fn cancel_crafts(
    queue: &mut CraftQueue,
    thread: &WorldThread,
    writer: &mut FrameWriter,
    players: &PlayerRegistry,
    inventory: &mut PlayerInventory,
    network_id: i32,
    position: hytale_world::player_data::Position,
    compression_level: i32,
) -> Result<bool, SessionError> {
    let Some(job) = queue.cancel_last() else {
        return Ok(false);
    };
    let assets = thread.asset_registry()?;
    let rules = AssetItemRules::new(&assets);
    for (item_id, quantity) in job.consumed {
        if let Ok(stack) = ItemStack::new(&item_id, quantity, &rules) {
            if let Some(remainder) = inventory.pickup(stack, &rules) {
                let _ = thread.spawn_item_stack_entity(
                    remainder,
                    position.x,
                    position.y + 0.5,
                    position.z,
                );
            }
        }
    }
    send_inventory(writer, inventory, compression_level).await?;
    broadcast_held_items(players, network_id, inventory);
    Ok(true)
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
        kind: OpenWindowKind::Container,
        bench_id: None,
    }))
}

fn bench_window_type(bench: &serde_json::Value) -> WindowType {
    match bench
        .get("Type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Crafting")
    {
        "Processing" => WindowType::Processing,
        "DiagramCrafting" => WindowType::DiagramCrafting,
        "StructuralCrafting" => WindowType::StructuralCrafting,
        "AbilityBench" => WindowType::AbilityBench,
        _ => WindowType::BasicCrafting,
    }
}

/// Map a window type to the recipe `BenchType` it filters.
fn bench_type_for_window(window: WindowType) -> BenchType {
    match window {
        WindowType::Processing => BenchType::Processing,
        WindowType::DiagramCrafting => BenchType::DiagramCrafting,
        WindowType::StructuralCrafting => BenchType::StructuralCrafting,
        _ => BenchType::Crafting,
    }
}

/// Build the bench window payload: bench document, tier/category contract and
/// the recipe ids reachable with the player's knowledge.
fn bench_window_data(
    block_item_id: &str,
    bench: &serde_json::Value,
    window_type: WindowType,
    recipes: &RecipeCatalog,
    knowledge: &CraftingKnowledge,
) -> serde_json::Value {
    let bench_id = bench
        .get("Id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    serde_json::json!({
        "blockItemId": block_item_id,
        "bench": bench,
        "benchId": bench_id,
        "tierLevel": knowledge.bench_tier(bench_id),
        "categories": bench.get("Categories").cloned().unwrap_or(serde_json::Value::Null),
        "augmentTags": knowledge.augment_tags.iter().cloned().collect::<Vec<_>>(),
        "recipeIds": recipes
            .for_bench_with_tier(bench_type_for_window(window_type), knowledge)
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
    })
}

#[allow(clippy::too_many_arguments)]
async fn open_bench_window(
    thread: &WorldThread,
    writer: &mut FrameWriter,
    players: &PlayerRegistry,
    recipes: &RecipeCatalog,
    knowledge: &CraftingKnowledge,
    (x, y, z): (i32, i32, i32),
    (block_item_id, bench): (String, serde_json::Value),
    window_id: i32,
    compression_level: i32,
) -> Result<Option<OpenContainerWindow>, SessionError> {
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
    let window_type = bench_window_type(&bench);
    let bench_id = bench
        .get("Id")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned);
    let window_data = bench_window_data(&block_item_id, &bench, window_type, recipes, knowledge);
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
                window_type,
                window_data: Some(window_data.to_string()),
                inventory: None,
                extra_resources: None,
            },
            compression_level,
        )
        .await?;
    Ok(Some(OpenContainerWindow {
        id: window_id,
        position: (x, y, z),
        kind: OpenWindowKind::Bench,
        bench_id,
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
    if open.kind == OpenWindowKind::Container {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotSource {
    Player(Section),
    Window,
}

fn slot_source(id: i32, open: Option<&OpenContainerWindow>) -> Option<SlotSource> {
    if id >= 0 {
        return open
            .filter(|window| window.id == id)
            .map(|_| SlotSource::Window);
    }
    Section::from_section_id(id).map(SlotSource::Player)
}

#[allow(clippy::too_many_arguments)]
async fn move_item_stack(
    thread: &WorldThread,
    writer: &mut FrameWriter,
    players: &PlayerRegistry,
    inventory: &mut PlayerInventory,
    open: Option<&OpenContainerWindow>,
    network_id: i32,
    packet: MoveItemStack,
    compression_level: i32,
) -> Result<bool, SessionError> {
    let Some(from_source) = slot_source(packet.from_section_id, open) else {
        return Ok(false);
    };
    let Some(to_source) = slot_source(packet.to_section_id, open) else {
        return Ok(false);
    };
    let Ok(from_slot) = u16::try_from(packet.from_slot_id) else {
        return Ok(false);
    };
    let Ok(to_slot) = u16::try_from(packet.to_slot_id) else {
        return Ok(false);
    };
    if packet.quantity <= 0 {
        return Ok(false);
    }

    let window_position = open.map(|window| window.position);
    let mut window_container = match (from_source, to_source, window_position) {
        (SlotSource::Window, _, Some((x, y, z))) | (_, SlotSource::Window, Some((x, y, z))) => {
            let Some(container) = thread.block_container(x, y, z)? else {
                return Ok(false);
            };
            Some(container)
        }
        _ => None,
    };

    let mut from = match from_source {
        SlotSource::Player(section) => inventory.container(section).clone(),
        SlotSource::Window => window_container.clone().expect("loaded for window source"),
    };
    let mut to = match to_source {
        SlotSource::Player(section) => inventory.container(section).clone(),
        SlotSource::Window => window_container.clone().expect("loaded for window target"),
    };
    let assets = thread.asset_registry()?;
    let rules = AssetItemRules::new(&assets);
    let outcome = if from_source == to_source {
        match from.move_within(from_slot, packet.quantity, to_slot, &rules) {
            Ok(outcome) => outcome,
            Err(_) => return Ok(false),
        }
    } else {
        match from.move_to_slot(from_slot, packet.quantity, &mut to, to_slot, &rules) {
            Ok(outcome) => outcome,
            Err(_) => return Ok(false),
        }
    };
    if !outcome.changed() {
        return Ok(false);
    }

    let mut player_changed = false;
    let mut window_changed = false;
    if from_source == to_source {
        match from_source {
            SlotSource::Player(section) => {
                *inventory.container_mut(section) = from;
                player_changed = true;
            }
            SlotSource::Window => {
                window_container = Some(from);
                window_changed = true;
            }
        }
    } else {
        match from_source {
            SlotSource::Player(section) => {
                *inventory.container_mut(section) = from;
                player_changed = true;
            }
            SlotSource::Window => {
                window_container = Some(from);
                window_changed = true;
            }
        }
        match to_source {
            SlotSource::Player(section) => {
                *inventory.container_mut(section) = to;
                player_changed = true;
            }
            SlotSource::Window => {
                window_container = Some(to);
                window_changed = true;
            }
        }
    }

    if player_changed {
        send_inventory(writer, inventory, compression_level).await?;
        broadcast_held_items(players, network_id, inventory);
    }
    if window_changed {
        if let (Some(window), Some(container), Some((x, y, z))) =
            (open.cloned(), window_container, window_position)
        {
            thread.set_block_container(x, y, z, container.clone())?;
            writer
                .write_packet(
                    &UpdateWindow {
                        id: window.id,
                        window_data: None,
                        inventory: Some(container.to_packet()),
                        extra_resources: None,
                    },
                    compression_level,
                )
                .await?;
        }
    }
    Ok(true)
}

/// Result of switching one live session to another world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionWorldSwitch {
    /// Source view id.
    pub old_view: u64,
    /// Target view id.
    pub new_view: u64,
    /// Target world UUID.
    pub world_uuid: WorldUuid,
    /// Chunk stream counters for the new world snapshot.
    pub chunks: ChunkStreamReport,
}

#[allow(clippy::too_many_arguments)]
pub async fn switch_session_world(
    source: &WorldThread,
    target: &WorldThread,
    writer: &mut FrameWriter,
    chunks: &mut ChunkStreamWriter,
    players: &PlayerRegistry,
    network_id: i32,
    source_view: ViewId,
    target_transform: PlayerTransform,
    radius: u32,
    compression_level: i32,
) -> Result<SessionWorldSwitch, SessionError> {
    let target_start = target.start_report();
    let world_uuid: WorldUuid = target_start
        .world_uuid
        .parse()
        .map_err(|_| SessionError::InvalidUuid(target_start.world_uuid.clone()))?;
    let packet_uuid: Uuid = target_start
        .world_uuid
        .parse()
        .map_err(|_| SessionError::InvalidUuid(target_start.world_uuid.clone()))?;
    let center = (
        block_to_chunk(target_transform.position.x),
        block_to_chunk(target_transform.position.z),
    );

    source.remove_view(source_view)?;
    let new_view = target.add_view(center.0, center.1, radius)?;
    players.switch_world(network_id, packet_uuid, target_transform, center, radius);

    chunks
        .write_packet(&ClearChunks {}, compression_level)
        .await?;
    writer
        .write_packet(
            &JoinWorld {
                clear_world: true,
                fade_in_out: false,
                world_uuid: packet_uuid,
                min_section_y: Some(0),
                max_section_y: Some(9),
            },
            compression_level,
        )
        .await?;
    // Java resets the HUD on a world switch and the player-add system sends the
    // default component set again.
    writer
        .write_packet(
            &UpdateVisibleHudComponents {
                visible_components: Some(default_hud_components()),
            },
            compression_level,
        )
        .await?;
    let chunks_report = send_view_registered(
        target,
        chunks,
        center.0,
        center.1,
        radius,
        compression_level,
    )
    .await?;
    Ok(SessionWorldSwitch {
        old_view: source_view.raw(),
        new_view: new_view.raw(),
        world_uuid,
        chunks: chunks_report,
    })
}

async fn run_game_loop(
    connection: &quinn::Connection,
    thread: &WorldThread,
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    chunks: &mut ChunkStreamWriter,
    live: LiveGame<'_>,
    options: &SessionOptions,
) -> Result<(GameLoopReport, Option<Arc<WorldThread>>, ViewId), SessionError> {
    let LiveGame {
        mut center,
        mut radius,
        ready_for_gameplay,
        mut world_time,
        mut weather,
        network_id,
        player_uuid,
        username,
        commands,
        players,
        broadcasts,
        mut view,
        transform,
        inventory,
        interactions,
        recipes,
        crafting,
        revealed_markers,
        map_chunks,
        world_map,
        overflow,
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
    let mut last_movement_correction: Option<std::time::Instant> = None;
    let mut last_interaction_chain_id = 0i32;
    // Registry view center follows the player's chunk even before the client
    // acknowledges the teleport (the chunk view itself still waits for the ack).
    let mut registry_center = center;
    let mut pending_movement = false;
    let mut last_movement: Option<std::time::Instant> = None;
    let mut open_window: Option<OpenContainerWindow> = None;
    let mut craft_jobs = CraftQueue::default();
    let mut next_window_id = 1i32;
    let mut boss_bar_entity: Option<i32> = None;
    let mut last_map_view = (center, radius);
    let mut active_thread = thread;
    #[allow(unused_assignments)]
    let mut owned_target: Option<Arc<WorldThread>> = None;

    if let (Some(portal), Some(router)) = (&options.portal_on_join, &options.portal_router) {
        // Let the joining client finish its initial JoinWorld and chunk snapshot
        // before the live world switch.
        tokio::time::sleep(Duration::from_secs(2)).await;
        let (arrival, target) = router.open_portal_instance(portal, *transform)?;
        let health = active_thread.player_health(network_id)?;
        router.transfer_live_player(
            active_thread,
            &target,
            player_uuid,
            *transform,
            inventory.clone(),
            health,
            &arrival,
        )?;
        let _ = active_thread.remove_player_gameplay(network_id);
        let _ = target.ensure_player_gameplay(network_id, health);
        let switched = switch_session_world(
            active_thread,
            &target,
            writer,
            chunks,
            &players,
            network_id,
            view,
            arrival.plan.spawn,
            radius,
            options.chunk_compression_level,
        )
        .await?;
        report.portal_world = Some(arrival.world_name);
        *transform = arrival.plan.spawn;
        view = ViewId::from_raw(switched.new_view);
        center = (
            block_to_chunk(transform.position.x),
            block_to_chunk(transform.position.z),
        );
        registry_center = center;
        last_map_view = (center, radius);
        send_client_teleport(writer, *transform, options).await?;
        teleport_sent = true;
        report.teleport_sent = true;
        owned_target = Some(target);
        active_thread = owned_target.as_deref().expect("target installed");
    }

    if ready_for_gameplay {
        report.ready_for_gameplay = true;
        send_client_teleport(writer, *transform, options).await?;
        teleport_sent = true;
        report.teleport_sent = true;
    }

    let mut inbound = options.limits.inbound_bucket();
    loop {
        if connection.close_reason().is_some() {
            report.disconnected = true;
            break;
        }
        if overflow.load(std::sync::atomic::Ordering::Relaxed) {
            report.queue_overflow = true;
            let _ = send_disconnect(writer, "client is too slow").await;
            report.disconnected = true;
            break;
        }

        while let Ok(message) = broadcasts.try_recv() {
            players::write_message(
                writer,
                chunks,
                world_map,
                message,
                options.chunk_compression_level,
            )
            .await?;
            report.player_messages += 1;
        }

        active_thread.set_npc_targets(players.npc_targets())?;
        let spawn_report = active_thread.tick_npc_spawns()?;
        report.npc_spawns += spawn_report.spawned;
        let _ = active_thread.tick_spawn_markers(1.0 / 20.0, 1.0 / 20.0)?;
        let _ = active_thread.tick_beacons(1.0 / 20.0, 1.0 / 20.0)?;
        let npc_report = active_thread.tick_npcs()?;
        report.npc_ticks += npc_report.ticked;
        report.player_damages += npc_report.player_damages;
        report.player_deaths += npc_report.player_deaths;
        let crafted = if craft_jobs.is_empty() {
            0
        } else {
            tick_crafts(
                &mut craft_jobs,
                0.05,
                active_thread,
                writer,
                &players,
                inventory,
                network_id,
                transform.position,
                options.chunk_compression_level,
            )
            .await?
        };
        report.crafted_items += crafted;
        let stat_updates = active_thread.take_player_stat_updates(network_id)?;
        let effect_updates = active_thread.tick_player_gameplay(network_id, 0.05)?;
        if npc_report.player_deaths > 0 {
            let _ = active_thread.respawn_player(network_id);
            report.player_respawns += 1;
        }
        let respawn_effects = active_thread.take_player_effect_updates(network_id)?;
        let all_effects = if respawn_effects.is_empty() {
            effect_updates
        } else {
            let mut all = effect_updates;
            all.extend(respawn_effects);
            all
        };
        if !all_effects.is_empty() {
            let packet = PlayerGameplay::effect_entity_updates(network_id, all_effects);
            players.broadcast_entity_updates(network_id, packet);
            report.player_effect_updates += 1;
        }
        if !stat_updates.is_empty() {
            let packet = PlayerGameplay::entity_updates(network_id, stat_updates);
            players.broadcast_entity_updates(network_id, packet);
            report.player_stat_updates += 1;
        }
        let encounter_reports = active_thread.tick_encounters()?;
        if !encounter_reports.is_empty() {
            let assets = active_thread.asset_registry()?;
            let position = ProtocolPosition {
                x: transform.position.x,
                y: transform.position.y,
                z: transform.position.z,
            };
            for (_, encounter) in &encounter_reports {
                for action in &encounter.actions {
                    if let Some(packet) = encounter_particle_packet(action, position.clone()) {
                        writer
                            .write_packet(&packet, options.chunk_compression_level)
                            .await?;
                        report.encounter_actions += 1;
                        continue;
                    }
                    if let hytale_npc::encounter::EncounterAction::PlaySound { sound_event_id } =
                        action
                    {
                        if let Some(index) = assets.sound_event_index(sound_event_id) {
                            if let Some(packet) =
                                encounter_sound_packet(action, index, position.clone())
                            {
                                writer
                                    .write_packet(&packet, options.chunk_compression_level)
                                    .await?;
                                report.encounter_actions += 1;
                            }
                        }
                    }
                    match action {
                        hytale_npc::encounter::EncounterAction::Weather { weather, .. } => {
                            active_thread.force_weather(Some(weather))?;
                            report.encounter_actions += 1;
                        }
                        hytale_npc::encounter::EncounterAction::ClearWeather => {
                            active_thread.force_weather(None)?;
                            report.encounter_actions += 1;
                        }
                        hytale_npc::encounter::EncounterAction::BossBar { .. }
                        | hytale_npc::encounter::EncounterAction::ClearBossBar => {
                            if let Some(packet) =
                                encounter_boss_bar_packet(action, &mut boss_bar_entity)
                            {
                                writer
                                    .write_packet(&packet, options.chunk_compression_level)
                                    .await?;
                                report.encounter_actions += 1;
                            }
                        }
                        hytale_npc::encounter::EncounterAction::AudioState { .. } => {
                            if let Some(packet) = encounter_audio_packet(action, &assets) {
                                writer
                                    .write_packet(&packet, options.chunk_compression_level)
                                    .await?;
                                report.encounter_actions += 1;
                            }
                        }
                        hytale_npc::encounter::EncounterAction::StartMusic { music_container } => {
                            if let Some(container_index) =
                                assets.music_container_id(music_container)
                            {
                                let packet = UpdateForcedMusic { container_index };
                                writer
                                    .write_packet(&packet, options.chunk_compression_level)
                                    .await?;
                                report.encounter_actions += 1;
                            }
                        }
                        hytale_npc::encounter::EncounterAction::StopMusic => {
                            let packet = UpdateForcedMusic { container_index: 0 };
                            writer
                                .write_packet(&packet, options.chunk_compression_level)
                                .await?;
                            report.encounter_actions += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
        report.npc_moves += npc_report.moved;
        report.npc_attacks += npc_report.attacks;
        report.npc_deaths += npc_report.deaths;

        let presentation = active_thread.take_npc_presentation_events()?;
        if !presentation.is_empty() {
            let assets = active_thread.asset_registry()?;
            for event in presentation {
                let position = ProtocolPosition {
                    x: event.position.x,
                    y: event.position.y,
                    z: event.position.z,
                };
                match event.kind {
                    NpcPresentationKind::Particles => {
                        let packet = SpawnParticleSystem {
                            particle_system_id: Some(event.key),
                            position: Some(position),
                            rotation: None,
                            scale: 1.0,
                            color: None,
                            max_duration: 0.0,
                        };
                        writer
                            .write_packet(&packet, options.chunk_compression_level)
                            .await?;
                        report.encounter_actions += 1;
                    }
                    NpcPresentationKind::Sound => {
                        if let Some(index) = assets.sound_event_index(&event.key) {
                            let packet = PlaySoundEvent3D {
                                sound_event_index: index,
                                category: hytale_protocol::generated::common::SoundCategory::SFX,
                                position: Some(position),
                                volume_modifier: 1.0,
                                pitch_modifier: 1.0,
                            };
                            writer
                                .write_packet(&packet, options.chunk_compression_level)
                                .await?;
                            report.encounter_actions += 1;
                        }
                    }
                }
            }
        }
        for event in active_thread.take_block_change_events()? {
            if event.before != event.after {
                players.broadcast_block(ServerSetBlock {
                    x: event.x,
                    y: event.y,
                    z: event.z,
                    block_id: event.after,
                    filler: 0,
                    rotation: 0,
                });
            }
        }

        for event in active_thread.runtime_entity_events()? {
            match event {
                RuntimeEntityEvent::Spawned(id) | RuntimeEntityEvent::Updated(id) => {
                    if let Some(entity) = active_thread.entity(id)? {
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
                if !inbound.allow(last_activity) {
                    report.rate_limited = true;
                    let _ = send_disconnect(writer, "packet rate limit exceeded").await;
                    report.disconnected = true;
                    break;
                }
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
                                    let resolution = active_thread.resolve_movement(
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
                                        // Do not answer every movement packet with a
                                        // teleport: a client stuck against terrain can
                                        // send hundreds per second, and the resulting
                                        // correction flood trips the inbound rate limit.
                                        // At most ten corrections per second keep the
                                        // client in sync without the storm.
                                        let correction_due =
                                            last_movement_correction.is_none_or(|last| {
                                                last.elapsed() >= Duration::from_millis(100)
                                            });
                                        let moved = (resolution.position.x - target.x)
                                            .abs()
                                            .max((resolution.position.y - target.y).abs())
                                            .max((resolution.position.z - target.z).abs());
                                        if correction_due && moved > 0.05 {
                                            send_client_teleport_correction(
                                                writer,
                                                *transform,
                                                &mut movement_teleport_id,
                                                options,
                                            )
                                            .await?;
                                            last_movement_correction =
                                                Some(std::time::Instant::now());
                                        }
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
                                        active_thread.update_view(view, next.0, next.1, radius)?;
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
                                open_bench,
                            } => {
                                report.block_uses += 1;
                                report.block_state_changes += state_changes;
                                if open_container.is_some() || open_bench.is_some() {
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
                                }
                                if let Some(position) = open_container {
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
                                } else if let Some(bench) = open_bench {
                                    if let Some(opened) = open_bench_window(
                                        thread,
                                        writer,
                                        &players,
                                        &recipes,
                                        crafting,
                                        bench.position,
                                        (bench.block_item_id, bench.bench),
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
                    SyncInteractionChains::ID => {
                        let packet: SyncInteractionChains = frame.decode()?;
                        for update in &packet.updates {
                            if interaction_debug_enabled() {
                                eprintln!(
                                    "interaction chain id={} initial={} desync={} type={:?} state={:?} block={:?} item={:?} root={} ops={}",
                                    update.chain_id,
                                    update.initial,
                                    update.desync,
                                    update.interaction_type,
                                    update.state,
                                    update
                                        .data
                                        .as_ref()
                                        .and_then(|data| data.block_position.as_ref()),
                                    update.item_in_hand_id,
                                    update.override_root_interaction,
                                    update.interaction_data.as_ref().map_or(0, Vec::len),
                                );
                            }
                            if !update.initial {
                                continue;
                            }
                            // Java `InteractionManager.syncStart`: chain ids are
                            // strictly increasing, otherwise the packet is
                            // rejected and the client is told to cancel it.
                            if update.chain_id <= last_interaction_chain_id {
                                cancel_interaction_chain(
                                    writer,
                                    update,
                                    options.chunk_compression_level,
                                )
                                .await?;
                                continue;
                            }
                            last_interaction_chain_id = update.chain_id;
                            let root_id = interaction_chain_root(thread, update);

                            // F-key pickup: harvest the target block into the
                            // inventory, or pick up the nearest item entity.
                            if matches!(
                                update.interaction_type,
                                InteractionType::Pickup | InteractionType::Pick
                            ) {
                                let mut picked = Vec::new();
                                if let Some(block) = update
                                    .data
                                    .as_ref()
                                    .and_then(|data| data.block_position.as_ref())
                                {
                                    if let Ok(Some(block_id)) =
                                        thread.get_block(block.x, block.y, block.z)
                                    {
                                        let drops =
                                            thread.player_block_drops(block_id).unwrap_or_default();
                                        if !drops.is_empty() {
                                            if let Ok((before, after)) =
                                                thread.set_block(block.x, block.y, block.z, 0)
                                            {
                                                if before != 0 && before != after {
                                                    players.broadcast_block(ServerSetBlock {
                                                        x: block.x,
                                                        y: block.y,
                                                        z: block.z,
                                                        block_id: after,
                                                        filler: 0,
                                                        rotation: 0,
                                                    });
                                                    picked = drops;
                                                }
                                            }
                                        }
                                    }
                                }
                                if picked.is_empty() {
                                    if let Ok(Some(entity)) = thread.pickup_nearest_item(
                                        transform.position.x,
                                        transform.position.y,
                                        transform.position.z,
                                        3.0,
                                    ) {
                                        if let Some(item_id) = entity.item_id() {
                                            picked.push((
                                                item_id.to_string(),
                                                entity.item_quantity(),
                                            ));
                                        }
                                    }
                                }
                                if !picked.is_empty() {
                                    let assets = thread.asset_registry()?;
                                    let rules = AssetItemRules::new(&assets);
                                    for (item_id, quantity) in picked {
                                        if let Ok(stack) = ItemStack::new(item_id, quantity, &rules)
                                        {
                                            let _ = inventory.pickup(stack, &rules);
                                        }
                                    }
                                    send_inventory(
                                        writer,
                                        inventory,
                                        options.chunk_compression_level,
                                    )
                                    .await?;
                                    report.inventory_updates += 1;
                                    broadcast_held_items(&players, network_id, inventory);
                                    finish_interaction_chain(
                                        writer,
                                        update,
                                        root_id,
                                        options.chunk_compression_level,
                                    )
                                    .await?;
                                } else {
                                    cancel_interaction_chain(
                                        writer,
                                        update,
                                        options.chunk_compression_level,
                                    )
                                    .await?;
                                }
                                continue;
                            }

                            let Some(interaction) = block_interaction_from_chain(update) else {
                                cancel_interaction_chain(
                                    writer,
                                    update,
                                    options.chunk_compression_level,
                                )
                                .await?;
                                continue;
                            };
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
                                BlockInteractionOutcome::Use { state_changes, .. } => {
                                    // Block use still runs the server-side
                                    // transition; container/bench windows stay on
                                    // the `MouseInteraction` path for now.
                                    report.block_uses += 1;
                                    report.block_state_changes += state_changes;
                                }
                                BlockInteractionOutcome::Rejected => {
                                    report.block_edit_rejections += 1;
                                    cancel_interaction_chain(
                                        writer,
                                        update,
                                        options.chunk_compression_level,
                                    )
                                    .await?;
                                    continue;
                                }
                            }
                            finish_interaction_chain(
                                writer,
                                update,
                                root_id,
                                options.chunk_compression_level,
                            )
                            .await?;
                        }
                    }
                    ChatMessage::ID => {
                        let packet: ChatMessage = frame.decode()?;
                        let Some(message) = packet.message else {
                            continue;
                        };
                        let Some(line) = message.strip_prefix('/') else {
                            // Plain chat is not broadcast yet; commands own the
                            // first command-framework slice.
                            continue;
                        };
                        let sender = CommandSender::player(username.clone()).operator();
                        let context = CommandContext {
                            position: [
                                transform.position.x,
                                transform.position.y,
                                transform.position.z,
                            ],
                            players: players.usernames(),
                            game_time_seconds: Some(world_time.game_time_seconds),
                            time_paused: Some(world_time.time_paused),
                        };
                        report.commands += 1;
                        match commands.execute(line, &sender, &context) {
                            Ok(output) => {
                                for message in output.messages {
                                    send_server_message(writer, &message, options).await?;
                                }
                                for effect in output.effects {
                                    match effect {
                                        CommandEffect::Teleport { x, y, z } => {
                                            transform.position =
                                                hytale_world::player_data::Position { x, y, z };
                                            players.update_transform(network_id, *transform);
                                            send_client_teleport_correction(
                                                writer,
                                                *transform,
                                                &mut movement_teleport_id,
                                                options,
                                            )
                                            .await?;
                                            let next = (block_to_chunk(x), block_to_chunk(z));
                                            if next != registry_center {
                                                players.update_view(network_id, next, radius);
                                                registry_center = next;
                                            }
                                            if next != center && report.teleport_acked {
                                                active_thread
                                                    .update_view(view, next.0, next.1, radius)?;
                                                center = next;
                                                report.view_changes += 1;
                                            }
                                            send_server_message(
                                                writer,
                                                &format!("Teleported to {x:.1}, {y:.1}, {z:.1}"),
                                                options,
                                            )
                                            .await?;
                                        }
                                        CommandEffect::GiveItem { item_id, quantity } => {
                                            let assets = active_thread.asset_registry()?;
                                            let rules = AssetItemRules::new(&assets);
                                            match ItemStack::new(item_id.clone(), quantity, &rules)
                                            {
                                                Ok(stack) => {
                                                    if let Some(remainder) =
                                                        inventory.pickup(stack, &rules)
                                                    {
                                                        send_server_message(
                                                            writer,
                                                            &format!(
                                                                "Inventory full; {} x{} left over",
                                                                remainder.item_id,
                                                                remainder.quantity
                                                            ),
                                                            options,
                                                        )
                                                        .await?;
                                                    } else {
                                                        send_server_message(
                                                            writer,
                                                            &format!("Gave {item_id} x{quantity}"),
                                                            options,
                                                        )
                                                        .await?;
                                                    }
                                                    send_inventory(
                                                        writer,
                                                        inventory,
                                                        options.chunk_compression_level,
                                                    )
                                                    .await?;
                                                    report.inventory_updates += 1;
                                                    broadcast_held_items(
                                                        &players, network_id, inventory,
                                                    );
                                                }
                                                Err(error) => {
                                                    send_server_message(
                                                        writer,
                                                        &format!(
                                                            "Cannot give `{item_id}`: {error}"
                                                        ),
                                                        options,
                                                    )
                                                    .await?;
                                                }
                                            }
                                        }
                                        CommandEffect::SetGameMode { mode } => {
                                            let game_mode = if mode == "creative" {
                                                GameMode::Creative
                                            } else {
                                                GameMode::Adventure
                                            };
                                            writer
                                                .write_packet(
                                                    &SetGameMode { game_mode },
                                                    options.chunk_compression_level,
                                                )
                                                .await?;
                                            send_server_message(
                                                writer,
                                                &format!("Game mode set to {mode}"),
                                                options,
                                            )
                                            .await?;
                                        }
                                        CommandEffect::Kill => {
                                            active_thread.damage_player(
                                                network_id,
                                                1_000_000.0,
                                                "command",
                                            )?;
                                            send_server_message(writer, "Killed", options).await?;
                                        }
                                        CommandEffect::Respawn => {
                                            active_thread.respawn_player(network_id)?;
                                            send_server_message(writer, "Respawned", options)
                                                .await?;
                                        }
                                        CommandEffect::SaveWorld => {
                                            active_thread.save_player_state(
                                                player_uuid,
                                                *transform,
                                                inventory.clone(),
                                            )?;
                                            if let Some(health) =
                                                active_thread.player_health(network_id)?
                                            {
                                                active_thread
                                                    .save_player_health(player_uuid, health)?;
                                            }
                                            let dirty = active_thread.save()?;
                                            send_server_message(
                                                writer,
                                                &format!("Saved world ({dirty} dirty chunk(s))"),
                                                options,
                                            )
                                            .await?;
                                        }
                                        CommandEffect::StopServer => {
                                            if let Some(signal) = &options.stop_signal {
                                                signal.store(true, Ordering::Relaxed);
                                                send_server_message(
                                                    writer,
                                                    "Server shutdown requested",
                                                    options,
                                                )
                                                .await?;
                                            } else {
                                                send_server_message(
                                                    writer,
                                                    "Server shutdown is not available in this mode",
                                                    options,
                                                )
                                                .await?;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                send_server_message(writer, &error.to_string(), options).await?;
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
                            let _ = active_thread.spawn_item_stack_entity(
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
                    MoveItemStack::ID => {
                        let packet: MoveItemStack = frame.decode()?;
                        if move_item_stack(
                            thread,
                            writer,
                            &players,
                            inventory,
                            open_window.as_ref(),
                            network_id,
                            packet,
                            options.chunk_compression_level,
                        )
                        .await?
                        {
                            report.inventory_updates += 1;
                        }
                    }
                    CloseWindow::ID => {
                        let packet: CloseWindow = frame.decode()?;
                        if let Some(open) = open_window.take() {
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
                        let action: SendWindowAction = frame.decode()?;
                        if open_window
                            .as_ref()
                            .is_some_and(|window| window.id == action.id)
                        {
                            #[allow(clippy::collapsible_match)]
                            match action.action {
                                WindowAction::CraftRecipeAction(craft) => {
                                    if let (Some(recipe_id), true) =
                                        (craft.recipe_id.as_deref(), craft.quantity > 0)
                                    {
                                        if let Some(job) = begin_craft(
                                            thread,
                                            inventory,
                                            open_window.as_ref(),
                                            &recipes,
                                            crafting,
                                            recipe_id,
                                            craft.quantity,
                                        )? {
                                            craft_jobs.push(job);
                                            thread.save_player_crafting(player_uuid, crafting)?;
                                            send_inventory(
                                                writer,
                                                inventory,
                                                options.chunk_compression_level,
                                            )
                                            .await?;
                                            broadcast_held_items(&players, network_id, inventory);
                                            report.inventory_updates += 1;
                                        }
                                    }
                                }
                                WindowAction::CancelCraftingAction(_) => {
                                    if cancel_crafts(
                                        &mut craft_jobs,
                                        thread,
                                        writer,
                                        &players,
                                        inventory,
                                        network_id,
                                        transform.position,
                                        options.chunk_compression_level,
                                    )
                                    .await?
                                    {
                                        report.inventory_updates += 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    ViewRadius::ID => {
                        let packet: ViewRadius = frame.decode()?;
                        if packet.value > 0 {
                            let requested = ((packet.value as f32) / 32.0).ceil() as u32;
                            let next = options.limits.clamp_view_radius(requested);
                            if next != requested {
                                report.view_radius_clamps += 1;
                            }
                            if next != radius {
                                radius = next;
                                active_thread.update_view(view, center.0, center.1, radius)?;
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
            active_thread.tick()?;
            let simulation = active_thread.simulate(0.05)?;
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
            for packet in
                active_thread.block_damage_packets_for_deltas(&simulation.health_deltas)?
            {
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
                    world_map
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
                    world_map,
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
            let current_weather = active_thread.weather_at(
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
            world_time = active_thread.world_time()?;
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
            active_thread,
            writer,
            &players,
            open,
            options.chunk_compression_level,
        )
        .await;
        report.windows_closed += 1;
    }

    Ok((report, owned_target, view))
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

/// Resolve an inbound client interaction-chain start to the block edit it
/// represents.
///
/// A release client breaks and places blocks through `SyncInteractionChains`,
/// not `MouseInteraction`. The initial chain carries the target block in
/// `InteractionChainData`; later updates only carry operation state and must
/// not be replayed as a new edit. Only `Primary` (break/attack) and
/// `Secondary`/`Use` (place/use) block targets are handled here; entity
/// interactions and forked chains are rejected.
fn block_interaction_from_chain(update: &SyncInteractionChain) -> Option<MouseInteraction> {
    if !update.initial || update.forked_id.is_some() {
        return None;
    }
    let data = update.data.as_ref()?;
    let block_position = data.block_position.clone()?;
    let mouse_button_type = match update.interaction_type {
        InteractionType::Primary => MouseButtonType::Left,
        InteractionType::Secondary | InteractionType::Use => MouseButtonType::Right,
        _ => return None,
    };
    Some(MouseInteraction {
        client_timestamp: 0,
        active_slot: update.active_hotbar_slot,
        item_in_hand_id: update.item_in_hand_id.clone(),
        screen_point: hytale_protocol::generated::wire::Vector2f::default(),
        mouse_button: Some(MouseButtonEvent {
            mouse_button_type,
            state: MouseButtonState::Pressed,
            clicks: 1,
        }),
        mouse_motion: None,
        world_interaction: Some(WorldInteraction {
            // A block interaction is encoded with a negative entity id in
            // `WorldInteraction`; the chain's `entityId` defaults to 0, so
            // always use the block sentinel when a block position is present.
            entity_id: -1,
            block_position: Some(BlockPosition {
                x: block_position.x,
                y: block_position.y,
                z: block_position.z,
            }),
            block_rotation: None,
        }),
    })
}

/// Tell the client to abandon an interaction chain we do not simulate.
///
/// The release client otherwise keeps the chain alive and stops issuing new
/// interactions (observed as a frozen block break for simple `BreakBlock`
/// chains).
async fn cancel_interaction_chain(
    writer: &mut FrameWriter,
    update: &SyncInteractionChain,
    compression_level: i32,
) -> Result<(), SessionError> {
    writer
        .write_packet(
            &CancelInteractionChain {
                chain_id: update.chain_id,
                forked_id: update.forked_id.clone(),
            },
            compression_level,
        )
        .await?;
    Ok(())
}

/// Resolve the root interaction the client used for a block chain.
///
/// An empty-hand block primary uses the block type's `Primary` root when the
/// contained item defines one (`Block_Primary` for plants), otherwise the
/// unarmed primary root. The server must echo this id in its first sync packet;
/// `NOT_FOUND` leaves the client unable to resolve the chain and it desyncs.
fn interaction_chain_root(thread: &WorldThread, update: &SyncInteractionChain) -> i32 {
    const NOT_FOUND: i32 = hytale_assets::packets::INTERACTION_NOT_FOUND;
    let Some(block) = update
        .data
        .as_ref()
        .and_then(|data| data.block_position.as_ref())
    else {
        return NOT_FOUND;
    };
    let Ok(assets) = thread.asset_registry() else {
        return NOT_FOUND;
    };
    if let Ok(Some((key, json))) = thread.block_type_json(block.x, block.y, block.z) {
        let interactions = BlockInteractions::from_json(&key, &json);
        if let Some(root) = interactions
            .root(update.interaction_type)
            .and_then(|root| assets.root_interactions.id_of(root))
        {
            return root;
        }
    }
    assets
        .unarmed_interactions
        .get(&update.interaction_type)
        .and_then(|root| assets.root_interactions.id_of(root))
        .unwrap_or(NOT_FOUND)
}

fn interaction_chain_ack(
    update: &SyncInteractionChain,
    initial: bool,
    override_root_interaction: i32,
    state: InteractionState,
) -> SyncInteractionChain {
    SyncInteractionChain {
        active_hotbar_slot: update.active_hotbar_slot,
        active_utility_slot: update.active_utility_slot,
        active_tools_slot: update.active_tools_slot,
        item_in_hand_id: update.item_in_hand_id.clone(),
        utility_item_id: update.utility_item_id.clone(),
        tools_item_id: update.tools_item_id.clone(),
        initial,
        desync: false,
        override_root_interaction,
        interaction_type: update.interaction_type,
        equip_slot: update.equip_slot,
        chain_id: update.chain_id,
        forked_id: None,
        data: update.data.clone(),
        state,
        new_forks: None,
        operation_base_index: update.operation_base_index,
        interaction_data: None,
    }
}

/// Acknowledge a handled chain with the real root id and the client's own
/// operation payload, then finish it.
async fn finish_interaction_chain(
    writer: &mut FrameWriter,
    update: &SyncInteractionChain,
    root_id: i32,
    compression_level: i32,
) -> Result<(), SessionError> {
    const NOT_FOUND: i32 = hytale_assets::packets::INTERACTION_NOT_FOUND;
    let mut first = interaction_chain_ack(update, true, root_id, InteractionState::NotFinished);
    first.interaction_data = update.interaction_data.clone();

    let mut last = interaction_chain_ack(update, false, NOT_FOUND, InteractionState::Finished);
    last.interaction_data = update.interaction_data.clone().map(|data| {
        data.into_iter()
            .map(|entry| {
                entry.map(|mut entry| {
                    entry.state = InteractionState::Finished;
                    entry
                })
            })
            .collect()
    });

    writer
        .write_packet(
            &SyncInteractionChains {
                updates: vec![first, last],
            },
            compression_level,
        )
        .await?;
    Ok(())
}

fn interaction_debug_enabled() -> bool {
    std::env::var_os("HYTALE_DEBUG_INTERACTIONS").is_some()
}

#[derive(Debug, Clone, PartialEq)]
struct BenchOpen {
    position: (i32, i32, i32),
    block_item_id: String,
    bench: serde_json::Value,
}

enum BlockInteractionOutcome {
    Edit {
        consumed: bool,
    },
    Use {
        state_changes: usize,
        open_container: Option<(i32, i32, i32)>,
        open_bench: Option<BenchOpen>,
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
            if let Ok(drops) = thread.player_block_drops(before) {
                for (item_id, quantity) in drops {
                    let _ = thread.spawn_item_entity(
                        item_id,
                        quantity,
                        f64::from(x) + 0.5,
                        f64::from(y) + 0.5,
                        f64::from(z) + 0.5,
                    );
                }
            }
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
            open_bench: block_json.get("Bench").cloned().map(|bench| BenchOpen {
                position: (x, y, z),
                block_item_id: block_key.clone(),
                bench,
            }),
        });
    }
    if block.container {
        return Some(BlockInteractionOutcome::Use {
            state_changes: 0,
            open_container: Some((x, y, z)),
            open_bench: None,
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
        open_bench: None,
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
        BlockPosition, ComponentUpdate, InteractionChainData, MouseButtonEvent, WorldInteraction,
    };
    use hytale_protocol::generated::wire::Vector2f;
    use hytale_world::player_data::{
        PlayerModelReference, PlayerSkinData, Position, Rotation, EMPTY_ITEM_ID,
    };

    #[test]
    fn live_skin_maps_every_cosmetic_field() {
        let raw = serde_json::json!({
            "bodyCharacteristic": "BodyA",
            "underwear": "UnderwearA",
            "face": "FaceA",
            "eyes": "EyesA",
            "ears": "EarsA",
            "mouth": "MouthA",
            "facialHair": "FacialHairA",
            "haircut": "HaircutA",
            "eyebrows": "EyebrowsA",
            "pants": "PantsA",
            "overpants": "OverpantsA",
            "undertop": "UndertopA",
            "overtop": "OvertopA",
            "shoes": "ShoesA",
            "headAccessory": "HeadAccessoryA",
            "faceAccessory": "FaceAccessoryA",
            "earAccessory": "EarAccessoryA",
            "skinFeature": "SkinFeatureA",
            "gloves": "GlovesA",
            "cape": "CapeA"
        })
        .to_string();

        let skin = live_skin(&raw).expect("skin");
        assert_eq!(skin.body_characteristic.as_deref(), Some("BodyA"));
        assert_eq!(skin.underwear.as_deref(), Some("UnderwearA"));
        assert_eq!(skin.face.as_deref(), Some("FaceA"));
        assert_eq!(skin.eyes.as_deref(), Some("EyesA"));
        assert_eq!(skin.ears.as_deref(), Some("EarsA"));
        assert_eq!(skin.mouth.as_deref(), Some("MouthA"));
        assert_eq!(skin.facial_hair.as_deref(), Some("FacialHairA"));
        assert_eq!(skin.haircut.as_deref(), Some("HaircutA"));
        assert_eq!(skin.eyebrows.as_deref(), Some("EyebrowsA"));
        assert_eq!(skin.pants.as_deref(), Some("PantsA"));
        assert_eq!(skin.overpants.as_deref(), Some("OverpantsA"));
        assert_eq!(skin.undertop.as_deref(), Some("UndertopA"));
        assert_eq!(skin.overtop.as_deref(), Some("OvertopA"));
        assert_eq!(skin.shoes.as_deref(), Some("ShoesA"));
        assert_eq!(skin.head_accessory.as_deref(), Some("HeadAccessoryA"));
        assert_eq!(skin.face_accessory.as_deref(), Some("FaceAccessoryA"));
        assert_eq!(skin.ear_accessory.as_deref(), Some("EarAccessoryA"));
        assert_eq!(skin.skin_feature.as_deref(), Some("SkinFeatureA"));
        assert_eq!(skin.gloves.as_deref(), Some("GlovesA"));
        assert_eq!(skin.cape.as_deref(), Some("CapeA"));

        // Java sometimes nests the skin JSON as a string claim.
        let nested = serde_json::to_string(&raw).expect("quoted claim");
        assert_eq!(live_skin(&nested), Some(skin));
        assert!(live_skin("not json").is_none());
    }

    #[test]
    fn bench_types_map_to_java_window_types() {
        assert_eq!(
            bench_window_type(&serde_json::json!({"Type": "Crafting"})),
            WindowType::BasicCrafting
        );
        assert_eq!(
            bench_window_type(&serde_json::json!({"Type": "Processing"})),
            WindowType::Processing
        );
        assert_eq!(
            bench_window_type(&serde_json::json!({"Type": "DiagramCrafting"})),
            WindowType::DiagramCrafting
        );
        assert_eq!(
            bench_window_type(&serde_json::json!({})),
            WindowType::BasicCrafting
        );
    }

    #[test]
    fn window_types_map_to_recipe_bench_types() {
        assert_eq!(
            bench_type_for_window(WindowType::Processing),
            BenchType::Processing
        );
        assert_eq!(
            bench_type_for_window(WindowType::DiagramCrafting),
            BenchType::DiagramCrafting
        );
        assert_eq!(
            bench_type_for_window(WindowType::StructuralCrafting),
            BenchType::StructuralCrafting
        );
        assert_eq!(
            bench_type_for_window(WindowType::BasicCrafting),
            BenchType::Crafting
        );
    }

    #[test]
    fn bench_window_data_applies_tier_and_category_contract() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/Item/Items").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        let recipes = RecipeCatalog::load_root(&root).expect("recipes");
        let bench = serde_json::json!({
            "Id": "Armor_Bench",
            "Type": "Crafting",
            "Categories": ["Armor_Chest"]
        });
        let base = bench_window_data(
            "Bench_Armor",
            &bench,
            WindowType::BasicCrafting,
            &recipes,
            &CraftingKnowledge::default(),
        );
        assert_eq!(base["benchId"], "Armor_Bench");
        assert_eq!(base["tierLevel"], 0);
        assert_eq!(base["categories"][0], "Armor_Chest");
        let base_ids = base["recipeIds"].as_array().expect("recipe ids");
        assert!(!base_ids.iter().any(|id| id == "Armor_Adamantite_Chest"));

        let mut knowledge = CraftingKnowledge::default();
        knowledge.bench_tiers.insert("Armor_Bench".to_string(), 3);
        let upgraded = bench_window_data(
            "Bench_Armor",
            &bench,
            WindowType::BasicCrafting,
            &recipes,
            &knowledge,
        );
        assert_eq!(upgraded["tierLevel"], 3);
        assert!(upgraded["recipeIds"]
            .as_array()
            .expect("recipe ids")
            .iter()
            .any(|id| id == "Armor_Adamantite_Chest"));
    }

    #[test]
    fn encounter_particles_and_sounds_build_world_packets() {
        let position = ProtocolPosition {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let particles = hytale_npc::encounter::EncounterAction::Particles {
            particle_system: "Impact_Sword_Basic".to_string(),
            position_source: "Target".to_string(),
        };
        let packet = encounter_particle_packet(&particles, position.clone()).unwrap();
        assert_eq!(
            packet.particle_system_id.as_deref(),
            Some("Impact_Sword_Basic")
        );
        assert_eq!(packet.position.unwrap().y, 2.0);

        let sound = hytale_npc::encounter::EncounterAction::PlaySound {
            sound_event_id: "SFX_Unarmed_Impact".to_string(),
        };
        let packet = encounter_sound_packet(&sound, 42, position).unwrap();
        assert_eq!(packet.sound_event_index, 42);
        assert_eq!(
            packet.category,
            hytale_protocol::generated::common::SoundCategory::SFX
        );
        assert!(encounter_particle_packet(&sound, ProtocolPosition::default()).is_none());
    }

    #[test]
    fn encounter_boss_bar_packets_track_the_bound_entity() {
        let action = hytale_npc::encounter::EncounterAction::BossBar {
            name: "Test_Boss".to_string(),
            hard_mode: true,
            target_slot: Some("Boss".to_string()),
            entity: Some(42),
        };
        let mut last = None;
        let packet = encounter_boss_bar_packet(&action, &mut last).expect("boss bar packet");
        assert_eq!(packet.entity_network_id, players::runtime_network_id(42));
        assert!(packet.hard_mode);
        assert!(!packet.hide);
        assert_eq!(
            packet.name.and_then(|name| name.raw_text).as_deref(),
            Some("Test_Boss")
        );
        assert_eq!(last, Some(players::runtime_network_id(42)));

        let packet = encounter_boss_bar_packet(
            &hytale_npc::encounter::EncounterAction::ClearBossBar,
            &mut last,
        )
        .expect("clear packet");
        assert!(packet.hide);
        assert_eq!(packet.entity_network_id, players::runtime_network_id(42));
        assert_eq!(last, None);
    }

    #[test]
    fn apply_recipe_consumes_inputs_and_returns_outputs() {
        struct Rules;
        impl hytale_inventory::ItemRules for Rules {
            fn item_exists(&self, item_id: &str) -> bool {
                !item_id.is_empty()
            }
            fn max_stack(&self, _item_id: &str) -> i32 {
                100
            }
            fn max_durability(&self, _item_id: &str) -> f64 {
                0.0
            }
        }
        let mut inventory = PlayerInventory::new();
        inventory
            .storage
            .set_slot(
                0,
                Some(
                    hytale_inventory::ItemStack::from_fields(
                        "Ingredient_Bar_Bronze",
                        6,
                        0.0,
                        0.0,
                        None,
                        None,
                        false,
                    )
                    .expect("stack"),
                ),
            )
            .unwrap();
        let recipe = hytale_protocol::generated::common::CraftingRecipe {
            id: Some("Armor_Bronze_Chest".to_string()),
            inputs: Some(vec![hytale_protocol::generated::common::MaterialQuantity {
                item_id: Some("Ingredient_Bar_Bronze".to_string()),
                quantity: 6,
                ..Default::default()
            }]),
            primary_output: Some(hytale_protocol::generated::common::MaterialQuantity {
                item_id: Some("Armor_Bronze_Chest".to_string()),
                quantity: 1,
                ..Default::default()
            }),
            ..Default::default()
        };
        let resource_types = hytale_world::crafting::ResourceTypeIndex::default();
        let stacks =
            apply_recipe(&mut inventory, &recipe, 1, &Rules, &resource_types).expect("craft");
        assert_eq!(inventory.count_item("Ingredient_Bar_Bronze"), 0);
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].item_id, "Armor_Bronze_Chest");
        assert!(
            apply_recipe(&mut inventory, &recipe, 1, &Rules, &resource_types).is_none(),
            "a second craft fails without inputs"
        );
    }

    #[test]
    fn craft_queue_waits_for_the_bench_timer_and_cancels() {
        let job = CraftJob {
            recipe_id: "Test".to_string(),
            quantity: 1,
            remaining: 0.2,
            outputs: Vec::new(),
            consumed: Vec::new(),
        };
        let mut queue = CraftQueue::default();
        queue.push(job);
        assert!(queue.tick(0.1).is_empty());
        let finished = queue.tick(0.11);
        assert_eq!(finished.len(), 1);
        assert_eq!(finished[0].recipe_id, "Test");

        let mut queue = CraftQueue::default();
        queue.push(CraftJob {
            recipe_id: "Cancel".to_string(),
            quantity: 1,
            remaining: 5.0,
            outputs: Vec::new(),
            consumed: vec![("Rock_Stone".to_string(), 2)],
        });
        assert_eq!(queue.cancel_last().unwrap().consumed[0].1, 2);
        assert!(queue.cancel_last().is_none());
    }

    #[test]
    fn asset_backed_resource_recipe_plans_from_the_real_catalog() {
        let root =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Assets/extracted");
        if !root.join("Server/Item/Items").is_dir() {
            eprintln!("the extracted asset tree is not available — the test is skipped");
            return;
        }
        struct Rules;
        impl hytale_inventory::ItemRules for Rules {
            fn item_exists(&self, item_id: &str) -> bool {
                !item_id.is_empty()
            }
            fn max_stack(&self, _item_id: &str) -> i32 {
                100
            }
            fn max_durability(&self, _item_id: &str) -> f64 {
                0.0
            }
        }
        let catalog = RecipeCatalog::load_root(&root).expect("recipes");
        let recipe = catalog.get("Bench_Furnace").expect("Bench_Furnace recipe");
        assert!(recipe
            .inputs
            .as_ref()
            .unwrap()
            .iter()
            .any(|input| input.resource_type_id.as_deref() == Some("Rock")));
        let mut inventory = PlayerInventory::new();
        for (slot, (item_id, quantity)) in [("Rock_Stone", 6), ("Wood_Amber_Trunk", 6)]
            .into_iter()
            .enumerate()
        {
            inventory
                .storage
                .set_slot(
                    u16::try_from(slot).unwrap(),
                    Some(
                        hytale_inventory::ItemStack::from_fields(
                            item_id, quantity, 0.0, 0.0, None, None, false,
                        )
                        .expect("stack"),
                    ),
                )
                .unwrap();
        }
        let plan = plan_recipe(&mut inventory, recipe, 1, &Rules, catalog.resource_types())
            .expect("resource recipe plan");
        assert!(plan
            .outputs
            .iter()
            .any(|stack| stack.item_id == "Bench_Furnace"));
        assert_eq!(inventory.count_item("Rock_Stone"), 0);
        assert_eq!(inventory.count_item("Wood_Amber_Trunk"), 0);
        assert!(recipe.time_seconds > 0.0);
    }

    #[test]
    fn resource_type_recipe_consumes_matching_items() {
        struct Rules;
        impl hytale_inventory::ItemRules for Rules {
            fn item_exists(&self, item_id: &str) -> bool {
                !item_id.is_empty()
            }
            fn max_stack(&self, _item_id: &str) -> i32 {
                100
            }
            fn max_durability(&self, _item_id: &str) -> f64 {
                0.0
            }
        }
        let mut inventory = PlayerInventory::new();
        inventory
            .storage
            .set_slot(
                0,
                Some(
                    hytale_inventory::ItemStack::from_fields(
                        "Rock_Stone",
                        6,
                        0.0,
                        0.0,
                        None,
                        None,
                        false,
                    )
                    .expect("stack"),
                ),
            )
            .unwrap();
        let recipe = hytale_protocol::generated::common::CraftingRecipe {
            id: Some("Test_Resource".to_string()),
            inputs: Some(vec![hytale_protocol::generated::common::MaterialQuantity {
                resource_type_id: Some("Rock".to_string()),
                quantity: 6,
                ..Default::default()
            }]),
            primary_output: Some(hytale_protocol::generated::common::MaterialQuantity {
                item_id: Some("Rock_Stone_Brick".to_string()),
                quantity: 2,
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut index = hytale_world::crafting::ResourceTypeIndex::default();
        index.insert("Rock", "Rock_Stone");
        let stacks =
            apply_recipe(&mut inventory, &recipe, 1, &Rules, &index).expect("resource craft");
        assert_eq!(inventory.count_item("Rock_Stone"), 0);
        assert_eq!(stacks[0].item_id, "Rock_Stone_Brick");
        assert_eq!(stacks[0].quantity, 2);
    }

    #[test]
    fn window_section_ids_are_positive_and_player_sections_negative() {
        let window = OpenContainerWindow {
            id: 4,
            position: (1, 2, 3),
            kind: OpenWindowKind::Container,
            bench_id: None,
        };
        assert_eq!(slot_source(4, Some(&window)), Some(SlotSource::Window));
        assert_eq!(slot_source(5, Some(&window)), None);
        assert_eq!(
            slot_source(-1, Some(&window)),
            Some(SlotSource::Player(Section::Hotbar))
        );
        assert_eq!(slot_source(0, None), None);
    }

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
    fn self_spawn_carries_stats_effects_and_movement_states() {
        let stats = vec![(
            0,
            EntityStatUpdate {
                op: hytale_protocol::generated::common::EntityStatOp::Init,
                value: 100.0,
                ..EntityStatUpdate::default()
            },
        )];
        let packet = self_spawn_packet(1, transform(), &PlayerAppearance::default(), None, &stats);
        let updates = updates(&packet);
        assert!(matches!(updates[0], ComponentUpdate::ModelUpdate(_)));
        assert!(matches!(updates[1], ComponentUpdate::PlayerSkinUpdate(_)));
        assert!(matches!(updates[2], ComponentUpdate::EquipmentUpdate(_)));
        assert!(matches!(updates[3], ComponentUpdate::TransformUpdate(_)));
        assert!(updates
            .iter()
            .any(|update| matches!(update, ComponentUpdate::MovementStatesUpdate(_))));
        assert!(updates
            .iter()
            .any(|update| matches!(update, ComponentUpdate::EntityEffectsUpdate(_))));
        assert!(matches!(
            updates.last().expect("stats last"),
            ComponentUpdate::EntityStatsUpdate(_)
        ));
    }

    #[test]
    fn empty_save_uses_java_empty_equipment_defaults() {
        let packet = self_spawn_packet(1, transform(), &PlayerAppearance::default(), None, &[]);
        let updates = updates(&packet);
        match &updates[0] {
            ComponentUpdate::ModelUpdate(update) => {
                let model = update.model.as_ref().expect("default player model");
                assert_eq!(model.asset_id.as_deref(), Some("Player"));
            }
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
            model_packet: None,
        };
        let packet = self_spawn_packet(1, transform(), &appearance, None, &[]);
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
        let packet = self_spawn_packet(1, transform(), &appearance, Some(auth_skin), &[]);
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

    fn chain_update(
        interaction_type: InteractionType,
        block: Option<BlockPosition>,
    ) -> SyncInteractionChain {
        SyncInteractionChain {
            initial: true,
            interaction_type,
            chain_id: 7,
            item_in_hand_id: Some("Rock_Stone".to_string()),
            data: Some(InteractionChainData {
                entity_id: 0,
                block_position: block,
                ..Default::default()
            }),
            override_root_interaction: 42,
            ..Default::default()
        }
    }

    #[test]
    fn interaction_chains_map_to_block_edits() {
        let block = BlockPosition { x: 1, y: 2, z: 3 };

        let primary = block_interaction_from_chain(&chain_update(
            InteractionType::Primary,
            Some(block.clone()),
        ))
        .expect("primary block chain");
        let button = primary.mouse_button.as_ref().expect("button");
        assert_eq!(button.mouse_button_type, MouseButtonType::Left);
        assert_eq!(button.state, MouseButtonState::Pressed);
        assert_eq!(primary.item_in_hand_id.as_deref(), Some("Rock_Stone"));
        let world = primary.world_interaction.as_ref().expect("world");
        assert_eq!(world.entity_id, -1);
        assert_eq!(world.block_position, Some(block.clone()));
        assert_eq!(
            parse_block_edit(&transform(), &primary),
            Some(BlockEdit::Break { x: 1, y: 2, z: 3 })
        );

        let secondary = block_interaction_from_chain(&chain_update(
            InteractionType::Secondary,
            Some(block.clone()),
        ))
        .expect("secondary block chain");
        assert_eq!(
            secondary
                .mouse_button
                .as_ref()
                .expect("button")
                .mouse_button_type,
            MouseButtonType::Right
        );
        assert_eq!(
            parse_block_edit(&transform(), &secondary),
            Some(BlockEdit::Place {
                x: 1,
                y: 2,
                z: 3,
                item_id: "Rock_Stone".to_string()
            })
        );

        let use_chain =
            block_interaction_from_chain(&chain_update(InteractionType::Use, Some(block.clone())))
                .expect("use block chain");
        assert_eq!(
            use_chain
                .mouse_button
                .as_ref()
                .expect("button")
                .mouse_button_type,
            MouseButtonType::Right
        );
    }

    #[test]
    fn interaction_chains_reject_non_initial_missing_and_unsupported_targets() {
        let block = BlockPosition { x: 1, y: 2, z: 3 };

        let mut follow_up = chain_update(InteractionType::Primary, Some(block.clone()));
        follow_up.initial = false;
        assert!(block_interaction_from_chain(&follow_up).is_none());

        assert!(
            block_interaction_from_chain(&chain_update(InteractionType::Primary, None)).is_none()
        );
        assert!(block_interaction_from_chain(&chain_update(
            InteractionType::Pick,
            Some(block.clone())
        ))
        .is_none());

        let mut forked = chain_update(InteractionType::Primary, Some(block));
        forked.forked_id = Some(Default::default());
        assert!(block_interaction_from_chain(&forked).is_none());
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
