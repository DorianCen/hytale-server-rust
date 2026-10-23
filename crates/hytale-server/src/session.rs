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

use std::collections::BTreeMap;
use std::time::Duration;

use hytale_assets::{AssetPacket, CommonAssetInfo, CommonAssetRegistry};
use hytale_auth::{AuthService, SCOPE_CLIENT, SCOPE_SERVER};
use hytale_net::{
    ChunkStreamError, ChunkStreamWriter, FrameReader, FrameStreamError, FrameWriter, PeerIdentity,
    TransportError,
};
use hytale_protocol::generated::assets::UpdateTranslations;
use hytale_protocol::generated::auth::{AuthGrant, AuthToken, ConnectAccept, ServerAuthToken};
use hytale_protocol::generated::common::ServerPlayerListPlayer;
use hytale_protocol::generated::common::{
    CombatConfig, ComponentUpdate, Direction, DisconnectType, EntityUpdate, EquipmentUpdate,
    FormattedMessage, GameMode, InstantData, ModelTransform, ModelUpdate, PlayerConfig,
    PlayerSkinUpdate, PongType, Position, TransformUpdate, UpdateType,
};
use hytale_protocol::generated::connection::{
    ClientDisconnect, ClientType, Connect, InsecurePlayerOptions, Ping, Pong,
    RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::entities::{EntityUpdates, SetEntitySeed};
use hytale_protocol::generated::interface_::{AddToServerPlayerList, ServerInfo};
use hytale_protocol::generated::player::{
    ClientMovement, ClientReady, ClientTeleport, JoinWorld, SetClientId, SetGameMode,
};
use hytale_protocol::generated::registry::settings;
use hytale_protocol::generated::setup::{
    AssetFinalize, AssetInitialize, AssetPart, RequestAssets, RequestCommonAssetsRebuild,
    ServerTags, SetTimeDilation, SetUpdateRate, SetupFinalize, UpdateFeatures, ViewRadius,
    WorldLoadFinished, WorldLoadProgress, WorldSettings,
};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::generated::world::ClearChunks;
use hytale_protocol::generated::world::{UpdateTime, UpdateTimeSettings, WorldGameplayConfig};
use hytale_protocol::io::Uuid;
use hytale_world::chunk::stream::DEFAULT_COMPRESSION_LEVEL;
use hytale_world::player_data::PlayerTransform;
use thiserror::Error;

use crate::chunk_streamer::{send_updates, send_view, ChunkSessionError, ChunkStreamReport};
use crate::world_thread::{WorldThread, WorldThreadError, WorldTimeState};

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
    /// Authentication mode.
    pub auth: AuthMode,
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
            auth: AuthMode::Insecure,
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
    /// `SetGameMode` was written.
    pub game_mode: bool,
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
    /// The client disconnected (or the connection closed) cleanly.
    pub disconnected: bool,
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
    view_radius: u32,
    auth_mode: &'static str,
    assets: AssetStreamReport,
}

struct EnterGame {
    world_uuid: Uuid,
    radius: u32,
    player_transform: PlayerTransform,
    world_time: WorldTimeState,
}

struct LiveGame {
    center: (i32, i32),
    radius: u32,
    transform: PlayerTransform,
    ready_for_gameplay: bool,
}

/// Run authentication + setup on an accepted connection, transition to `Game`,
/// then stream the player view.
pub async fn serve_connection(
    connection: &quinn::Connection,
    thread: &WorldThread,
    options: &SessionOptions,
) -> Result<SessionReport, SessionError> {
    let (send, recv) = connection.accept_bi().await?;
    let mut writer = FrameWriter::new(send);
    let mut reader = FrameReader::new(recv);

    let config_packets = thread.config_asset_packets()?;
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
            &config_packets,
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

    let player_transform = thread
        .player_transform()?
        .unwrap_or_else(default_player_transform);
    let world_time = thread.world_time()?;
    let world_uuid: Uuid = start
        .world_uuid
        .parse()
        .map_err(|_| SessionError::InvalidUuid(start.world_uuid.clone()))?;
    send_player_list(
        &mut writer,
        handshake.uuid,
        &handshake.username,
        world_uuid,
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
            player_transform,
            world_time,
        },
        options,
    )
    .await?;
    game.player_list = true;

    let mut stream = send_view(
        thread,
        &mut chunks,
        start.player_chunk.0,
        start.player_chunk.1,
        radius,
        options.chunk_compression_level,
    )
    .await?;

    let game_loop = run_game_loop(
        connection,
        thread,
        &mut reader,
        &mut writer,
        &mut chunks,
        LiveGame {
            center: start.player_chunk,
            radius,
            transform: player_transform,
            ready_for_gameplay: game.client_ready_for_gameplay,
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
    config_packets: &[AssetPacket],
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
            (Identity { uuid, username }, "local")
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
                    config_packets,
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
        view_radius,
        auth_mode,
        assets,
    })
}

struct Identity {
    uuid: Uuid,
    username: String,
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
        player_transform,
        world_time,
    } = start;
    let mut chunks = ChunkStreamWriter::open(connection).await?;
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
            &SetClientId { client_id: 1 },
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
            &self_spawn_packet(player_transform),
            options.chunk_compression_level,
        )
        .await?;
    game.self_spawn = true;

    writer
        .write_packet(
            &WorldGameplayConfig {
                world_config: hytale_protocol::generated::common::WorldConfig {
                    allow_block_breaking: true,
                    allow_block_gathering: true,
                    allow_block_placement: true,
                    ..Default::default()
                },
                combat_config: CombatConfig::default(),
                player_config: PlayerConfig::default(),
            },
            options.chunk_compression_level,
        )
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
    writer
        .write_packet(
            &SetGameMode {
                game_mode: GameMode::Adventure,
            },
            options.chunk_compression_level,
        )
        .await?;
    game.time = true;
    game.game_mode = true;
    // This stream stays open for the chunk snapshot.
    Ok((game, chunks))
}

async fn send_player_list(
    writer: &mut FrameWriter,
    uuid: Uuid,
    username: &str,
    world_uuid: Uuid,
    compression_level: i32,
) -> Result<(), SessionError> {
    writer
        .write_packet(
            &ServerTags {
                tags: Some(BTreeMap::new()),
            },
            compression_level,
        )
        .await?;
    writer
        .write_packet(
            &AddToServerPlayerList {
                players: vec![ServerPlayerListPlayer {
                    uuid,
                    username: Some(username.to_string()),
                    world_uuid: Some(world_uuid),
                    ping: 0,
                    spectating: false,
                    lives_remaining: None,
                }],
            },
            compression_level,
        )
        .await?;
    Ok(())
}

fn default_player_transform() -> PlayerTransform {
    PlayerTransform::new(
        hytale_world::player_data::Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        hytale_world::player_data::Rotation {
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
        },
    )
}

fn model_transform(transform: PlayerTransform) -> ModelTransform {
    let position = Position {
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

fn self_spawn_packet(transform: PlayerTransform) -> EntityUpdates {
    EntityUpdates {
        removed: None,
        updates: Some(vec![EntityUpdate {
            network_id: 1,
            removed: None,
            updates: Some(vec![
                ComponentUpdate::ModelUpdate(ModelUpdate {
                    model: None,
                    entity_scale: 1.0,
                }),
                ComponentUpdate::PlayerSkinUpdate(PlayerSkinUpdate { skin: None }),
                ComponentUpdate::EquipmentUpdate(EquipmentUpdate {
                    armor_ids: Some(Vec::new()),
                    right_hand_item_id: None,
                    left_hand_item_id: None,
                }),
                ComponentUpdate::TransformUpdate(TransformUpdate {
                    transform: model_transform(transform),
                }),
            ]),
        }]),
    }
}

async fn run_game_loop(
    connection: &quinn::Connection,
    thread: &WorldThread,
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    chunks: &mut ChunkStreamWriter,
    live: LiveGame,
    options: &SessionOptions,
) -> Result<GameLoopReport, SessionError> {
    let LiveGame {
        mut center,
        mut radius,
        transform,
        ready_for_gameplay,
    } = live;
    let mut report = GameLoopReport::default();
    let mut ticker = std::time::Instant::now();
    let interval = Duration::from_millis(50);
    let mut teleport_sent = false;

    if ready_for_gameplay {
        report.ready_for_gameplay = true;
        send_client_teleport(writer, transform, options).await?;
        teleport_sent = true;
        report.teleport_sent = true;
    }

    loop {
        if connection.close_reason().is_some() {
            report.disconnected = true;
            break;
        }

        let wait = interval.saturating_sub(ticker.elapsed());
        match tokio::time::timeout(wait, reader.next_frame()).await {
            Ok(Ok(Some(frame))) => match frame.packet_id {
                ClientReady::ID => {
                    let ready: ClientReady = frame.decode()?;
                    if ready.ready_for_gameplay {
                        report.ready_for_gameplay = true;
                    }
                    if ready.ready_for_gameplay && !teleport_sent {
                        send_client_teleport(writer, transform, options).await?;
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
                    if let Some(position) = movement.absolute_position {
                        if position.x.is_finite() && position.z.is_finite() {
                            let next = (block_to_chunk(position.x), block_to_chunk(position.z));
                            if next != center && report.teleport_acked {
                                thread.set_view(next.0, next.1, radius)?;
                                center = next;
                                report.view_changes += 1;
                            }
                        }
                    }
                }
                ViewRadius::ID => {
                    let packet: ViewRadius = frame.decode()?;
                    if packet.value > 0 {
                        let next = ((packet.value as f32) / 32.0).ceil() as u32;
                        if next != radius {
                            radius = next;
                            thread.set_view(center.0, center.1, radius)?;
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
                    report.disconnected = true;
                    break;
                }
                _ => {}
            },
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

        if ticker.elapsed() >= interval {
            thread.tick()?;
            let updates = send_updates(thread, chunks, options.chunk_compression_level).await?;
            report.chunks += updates.chunks;
            report.packets += updates.packets;
            report.frames += updates.frames;
            report.payload_bytes += updates.payload_bytes;
            report.unloaded += updates.unloaded;
            ticker = std::time::Instant::now();
        }
    }

    Ok(report)
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
                model_transform: model_transform(transform),
                reset_velocity: true,
            },
            options.chunk_compression_level,
        )
        .await?;
    Ok(())
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
