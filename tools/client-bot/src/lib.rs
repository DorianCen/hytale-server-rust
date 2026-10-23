//! Shared client-bot logic: Connect construction, framing and packet logging.
//!
//! The binary is intentionally small: it sends one `Connect`, then logs framed
//! packets until the server disconnects or the timeout expires. Packet handlers
//! and the full join flow land in later tasks.

use hytale_auth::{AuthService, SCOPE_CLIENT};
use hytale_net::{
    ChunkFrame, ChunkStreamError, ChunkStreamReader, ClientSession, FrameReader, FrameStreamError,
    FrameWriter,
};
use hytale_protocol::frame::{decode_frame, encode_frame, MAX_PACKET_SIZE};
use hytale_protocol::generated::assets::UpdateTranslations;
use hytale_protocol::generated::auth::{AuthGrant, AuthToken, ConnectAccept, ServerAuthToken};
use hytale_protocol::generated::common::{Asset, PongType};
use hytale_protocol::generated::connection::{
    ClientDisconnect, ClientType, Connect, InsecurePlayerOptions, NetworkTick, Ping, Pong,
    RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::entities::SetEntitySeed;
use hytale_protocol::generated::interface_::AddToServerPlayerList;
use hytale_protocol::generated::player::{ClientReady, JoinWorld, SetClientId};
use hytale_protocol::generated::setup::{
    AssetFinalize, AssetInitialize, AssetPart, RequestAssets, RequestCommonAssetsRebuild,
    ServerTags, SetTimeDilation, SetUpdateRate, SetupFinalize, UpdateFeatures, ViewRadius,
    WorldLoadFinished, WorldSettings,
};
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::generated::world::{
    ClearChunks, SetChunk, SetChunkEnvironments, SetChunkTintmap, SetColumn, SetFluids, UnloadChunk,
};
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
}

/// World-join state observed by the bot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameJoinReport {
    /// `ClearChunks` arrived on the chunks stream.
    pub clear_chunks: bool,
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
    /// The self `EntityUpdates` spawn arrived.
    pub self_spawn: bool,
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
                            skin: None,
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

    let (game, mut chunks, mut pending) = enter_game_client(session, &mut reader, &mut writer)
        .await
        .map_err(|error| DevJoinError::Game(error.to_string()))?;
    let chunks = receive_chunks_from(&mut chunks, &mut pending)
        .await
        .map_err(|error| DevJoinError::Game(error.to_string()))?;
    Ok(DevJoinReport {
        handshake_packets,
        game,
        chunks,
    })
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
    /// Decoded chunk packets.
    pub chunks: Vec<ChunkPacketSummary>,
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
    let (send, recv) = session
        .connection()
        .open_bi()
        .await
        .map_err(|error| AuthJoinError::Connect(error.to_string()))?;
    let mut writer = FrameWriter::new(send);
    let mut reader = FrameReader::new(recv);

    let subject = options.uuid.to_string();
    let scopes = [SCOPE_CLIENT.to_string()];
    let identity = options.auth.issue_identity_token(
        &subject,
        &options.username,
        &scopes,
        Some(&options.certificate_fingerprint),
        300,
    )?;
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
        chunks: Vec::new(),
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
                let access = options.auth.issue_access_token(
                    &subject,
                    &options.username,
                    &scopes,
                    Some(&options.certificate_fingerprint),
                    300,
                )?;
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
            hytale_protocol::generated::assets::UpdatePhysicalMaterials::ID
            | hytale_protocol::generated::assets::UpdateBlockHitboxes::ID => {
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
    report.chunks = receive_chunks_from(&mut chunks, &mut pending).await?;
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
    let mut chunks = ChunkStreamReader::accept(session.connection()).await?;
    let mut pending = Vec::new();
    let mut game = GameJoinReport::default();

    // The server opens the chunks stream and sends ClearChunks before
    // JoinWorld, then waits for our ClientReady. ViewRadius arrives only after
    // the server processes that ready packet.
    loop {
        let frame = chunks.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        match frame.packet_id {
            ClearChunks::ID => game.clear_chunks = true,
            _ => pending.push(frame),
        }
        if game.clear_chunks {
            break;
        }
    }

    loop {
        let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
        match frame.packet_id {
            AddToServerPlayerList::ID => {
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
    // identity burst, before gameplay input starts.
    let frame = reader.next_frame().await?.ok_or(AuthJoinError::Closed)?;
    if frame.packet_id == hytale_protocol::generated::entities::EntityUpdates::ID {
        game.self_spawn = true;
    }

    Ok((game, chunks, pending))
}

async fn receive_chunks_from(
    reader: &mut ChunkStreamReader,
    pending: &mut Vec<ChunkFrame>,
) -> Result<Vec<ChunkPacketSummary>, AuthJoinError> {
    let mut packets = Vec::new();
    for frame in pending.drain(..) {
        if let Some(summary) = summarize_chunk_packet(&frame) {
            packets.push(summary?);
        }
    }
    while let Some(frame) = reader.next_frame().await? {
        if let Some(summary) = summarize_chunk_packet(&frame) {
            packets.push(summary?);
        }
    }
    Ok(packets)
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
