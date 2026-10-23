//! Development/insecure connection handshake.
//!
//! This follows the Java `InitialPacketHandler` → `SetupPacketHandler` control
//! flow without authentication services:
//!
//! ```text
//! client → Connect
//! server → RequestInsecurePlayerOptions
//! client → InsecurePlayerOptions
//! server → ConnectAccept, WorldSettings, ServerInfo
//! client → ViewRadius, RequestAssets
//! server → WorldLoadProgress, WorldLoadFinished
//! client → SetupFinalize
//! server → (Chunks stream from WorldThread)
//! ```
//!
//! Authentication and real common/config asset streaming are out of scope. The
//! packet order and the insecure-mode branch are ported from
//! `CoreServer/.../io/handlers/{InitialPacketHandler,SetupPacketHandler}.java`.

use std::time::Duration;

use hytale_net::{ChunkStreamError, FrameReader, FrameStreamError, FrameWriter, TransportError};
use hytale_protocol::generated::auth::ConnectAccept;
use hytale_protocol::generated::common::{DisconnectType, FormattedMessage, PongType};
use hytale_protocol::generated::connection::{
    Connect, InsecurePlayerOptions, Ping, Pong, RequestInsecurePlayerOptions, ServerDisconnect,
};
use hytale_protocol::generated::interface_::ServerInfo;
use hytale_protocol::generated::registry::settings;
use hytale_protocol::generated::setup::{
    RequestAssets, SetupFinalize, ViewRadius, WorldLoadFinished, WorldLoadProgress, WorldSettings,
};
use hytale_protocol::generated::wire::Packet;
use hytale_protocol::io::Uuid;
use hytale_world::chunk::stream::DEFAULT_COMPRESSION_LEVEL;
use thiserror::Error;

use crate::chunk_serve::serve_connection as serve_chunk_connection;
use crate::chunk_streamer::{ChunkSessionError, ChunkStreamReport};
use crate::world_thread::{WorldThread, WorldThreadError};

/// Server identity / handshake settings for the dev flow.
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
        }
    }
}

/// What the dev handshake produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReport {
    /// Player UUID from `InsecurePlayerOptions`.
    pub uuid: Uuid,
    /// Username from `InsecurePlayerOptions`.
    pub username: String,
    /// View radius used for the chunk stream.
    pub view_radius: u32,
    /// Chunks sent after setup.
    pub stream: ChunkStreamReport,
}

/// Errors from the dev handshake.
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
    /// The handshake timed out.
    #[error("connection handshake timed out")]
    Timeout,
    /// The client sent a packet that is invalid in the current state.
    #[error("unexpected packet {packet_id} during {state}")]
    UnexpectedPacket {
        /// Packet id.
        packet_id: u32,
        /// State name.
        state: &'static str,
    },
}

struct Handshake {
    uuid: Uuid,
    username: String,
    view_radius: u32,
}

/// Run the dev handshake on an accepted connection, then stream the player view.
pub async fn serve_connection(
    connection: &quinn::Connection,
    thread: &WorldThread,
    options: &SessionOptions,
) -> Result<SessionReport, SessionError> {
    let (send, recv) = connection.accept_bi().await?;
    let mut writer = FrameWriter::new(send);
    let mut reader = FrameReader::new(recv);

    let handshake = tokio::time::timeout(
        options.handshake_timeout,
        perform_handshake(&mut reader, &mut writer, options),
    )
    .await
    .map_err(|_| SessionError::Timeout)??;

    let start = thread.start_report();
    let radius = if handshake.view_radius == 0 {
        options.default_view_radius
    } else {
        handshake.view_radius
    };
    let stream = serve_chunk_connection(
        thread,
        connection,
        start.player_chunk,
        radius,
        options.chunk_compression_level,
        false,
    )
    .await?;

    Ok(SessionReport {
        uuid: handshake.uuid,
        username: handshake.username,
        view_radius: radius,
        stream,
    })
}

async fn perform_handshake(
    reader: &mut FrameReader,
    writer: &mut FrameWriter,
    options: &SessionOptions,
) -> Result<Handshake, SessionError> {
    // Initial: Connect.
    let connect: Connect = expect(reader, Connect::ID).await?;
    if connect.protocol_crc != settings::CRC {
        let reason = FormattedMessage {
            raw_text: Some("protocol mismatch".to_string()),
            ..FormattedMessage::default()
        };
        writer
            .write_packet(
                &ServerDisconnect {
                    reason: Some(reason),
                    r#type: DisconnectType::Disconnect,
                },
                DEFAULT_COMPRESSION_LEVEL,
            )
            .await?;
        return Err(SessionError::UnexpectedPacket {
            packet_id: Connect::ID,
            state: "protocol-crc",
        });
    }
    writer
        .write_packet(&RequestInsecurePlayerOptions {}, DEFAULT_COMPRESSION_LEVEL)
        .await?;

    // Wait for the insecure options (the dev branch of InitialPacketHandler).
    let player: InsecurePlayerOptions = expect(reader, InsecurePlayerOptions::ID).await?;
    writer
        .write_packet(
            &ConnectAccept {
                password_challenge: None,
            },
            DEFAULT_COMPRESSION_LEVEL,
        )
        .await?;
    writer
        .write_packet(
            &WorldSettings {
                required_assets: None,
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

    // Setup: the client may send ViewRadius at any time, then RequestAssets,
    // then SetupFinalize.
    let mut view_radius = 0u32;
    let mut assets_requested = false;
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
                writer
                    .write_packet(
                        &WorldLoadProgress {
                            status: None,
                            percent_complete: 0,
                            percent_complete_subitem: 0,
                        },
                        DEFAULT_COMPRESSION_LEVEL,
                    )
                    .await?;
                writer
                    .write_packet(&WorldLoadFinished {}, DEFAULT_COMPRESSION_LEVEL)
                    .await?;
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
        uuid: player.uuid,
        username: player.username,
        view_radius,
    })
}

async fn expect<T: Packet + hytale_protocol::generated::wire::Wire>(
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
