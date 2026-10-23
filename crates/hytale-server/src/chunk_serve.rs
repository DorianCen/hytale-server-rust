//! Minimal chunk-stream server for the transport demo.
//!
//! This binds a QUIC endpoint, accepts one client, opens the `Chunks` stream and
//! sends the player's view from [`WorldThread`]. It intentionally skips auth and
//! the `Setup`/`Game` packet state machine: the goal is to exercise the real
//! world thread → packet batch → QUIC stream path end to end.

use std::net::SocketAddr;
use std::time::Duration;

use hytale_net::{ServerEndpointBuilder, TlsIdentity, TransportError};
use thiserror::Error;
use tokio::runtime::Runtime;

use crate::chunk_streamer::{send_updates, send_view, ChunkSessionError, ChunkStreamReport};
use crate::session::{
    serve_connection as serve_session, SessionError, SessionOptions, SessionReport,
};
use crate::world_thread::{WorldThread, WorldThreadError, WorldThreadOptions};

/// Options for the demo chunk server.
#[derive(Debug, Clone)]
pub struct ChunkServeOptions {
    /// World thread configuration (save, generator, view radius, tick budget).
    pub world: WorldThreadOptions,
    /// QUIC bind address.
    pub bind_addr: SocketAddr,
    /// View center; defaults to the player chunk.
    pub center: Option<(i32, i32)>,
    /// zstd level for chunk frames.
    pub compression_level: i32,
    /// Send one view move after the initial snapshot (unload + new chunk).
    pub move_demo: bool,
    /// Run the dev/insecure Connect → Setup handshake before streaming.
    pub handshake: bool,
    /// Handshake settings when `handshake` is set.
    pub session: SessionOptions,
    /// How long to wait for the client to close after the stream finishes.
    pub close_timeout: Duration,
}

/// What the demo server produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkServeReport {
    /// Actual bound address.
    pub bound_addr: SocketAddr,
    /// World UUID.
    pub world_uuid: String,
    /// Player UUID.
    pub player_uuid: String,
    /// Player chunk used as the default center.
    pub player_chunk: (i32, i32),
    /// Stream counts.
    pub stream: ChunkStreamReport,
    /// Handshake result when the dev session flow ran.
    pub session: Option<SessionReport>,
}

/// Errors from the demo server.
#[derive(Debug, Error)]
pub enum ChunkServeError {
    /// The tokio runtime could not be created.
    #[error("could not create the tokio runtime: {0}")]
    Runtime(#[from] std::io::Error),
    /// The world thread failed.
    #[error(transparent)]
    World(#[from] WorldThreadError),
    /// QUIC setup/accept failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The chunk session failed.
    #[error(transparent)]
    Session(#[from] ChunkSessionError),
    /// The handshake failed.
    #[error(transparent)]
    Handshake(#[from] SessionError),
}

/// Run the demo server on a fresh tokio runtime.
pub fn run(options: &ChunkServeOptions) -> Result<ChunkServeReport, ChunkServeError> {
    let runtime = Runtime::new()?;
    runtime.block_on(serve_async(options))
}

async fn serve_async(options: &ChunkServeOptions) -> Result<ChunkServeReport, ChunkServeError> {
    let (thread, start) = WorldThread::spawn(options.world.clone())?;
    let identity = TlsIdentity::generate("localhost")?;
    let server = ServerEndpointBuilder::new(options.bind_addr, identity).bind()?;
    let bound_addr = server.local_addr();
    println!(
        "chunk-serve listening on {bound_addr} (world {}, player {})",
        start.world_uuid, start.player_uuid
    );

    let accepted = server.accept().await?;
    let center = options.center.unwrap_or(start.player_chunk);
    let (stream, session) = if options.handshake {
        let session = serve_session(accepted.connection(), &thread, &options.session).await?;
        (session.stream, Some(session))
    } else {
        let stream = serve_connection(
            &thread,
            accepted.connection(),
            center,
            options.world.view_radius,
            options.compression_level,
            options.move_demo,
        )
        .await?;
        (stream, None)
    };

    // Let the client read the FIN and close before the endpoint goes away.
    let _ = tokio::time::timeout(options.close_timeout, accepted.connection().closed()).await;
    thread.shutdown()?;
    server.close();

    Ok(ChunkServeReport {
        bound_addr,
        world_uuid: start.world_uuid,
        player_uuid: start.player_uuid,
        player_chunk: start.player_chunk,
        stream,
        session,
    })
}

/// Send one player's initial view on an accepted connection.
///
/// With `move_demo`, the view is moved one chunk afterwards and the resulting
/// unload/new-chunk events are streamed before the stream finishes.
pub async fn serve_connection(
    thread: &WorldThread,
    connection: &quinn::Connection,
    center: (i32, i32),
    radius: u32,
    compression_level: i32,
    move_demo: bool,
) -> Result<ChunkStreamReport, ChunkSessionError> {
    let mut writer = hytale_net::ChunkStreamWriter::open(connection).await?;
    let mut report = send_view(
        thread,
        &mut writer,
        center.0,
        center.1,
        radius,
        compression_level,
    )
    .await?;

    if move_demo {
        let moved = (center.0 + 1, center.1);
        thread.set_view(moved.0, moved.1, radius)?;
        loop {
            let tick = thread.tick()?;
            if tick.remaining == 0 {
                break;
            }
        }
        let updates = send_updates(thread, &mut writer, compression_level).await?;
        report.chunks += updates.chunks;
        report.unloaded += updates.unloaded;
        report.packets += updates.packets;
        report.frames += updates.frames;
        report.payload_bytes += updates.payload_bytes;
    }

    writer.finish()?;
    Ok(report)
}
