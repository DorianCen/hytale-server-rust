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

use crate::chunk_streamer::{send_view, ChunkSessionError, ChunkStreamReport};
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
    let stream = serve_connection(
        &thread,
        accepted.connection(),
        center,
        options.world.view_radius,
        options.compression_level,
    )
    .await?;

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
    })
}

/// Send one player's initial view on an accepted connection.
pub async fn serve_connection(
    thread: &WorldThread,
    connection: &quinn::Connection,
    center: (i32, i32),
    radius: u32,
    compression_level: i32,
) -> Result<ChunkStreamReport, ChunkSessionError> {
    let mut writer = hytale_net::ChunkStreamWriter::open(connection).await?;
    let report = send_view(
        thread,
        &mut writer,
        center.0,
        center.1,
        radius,
        compression_level,
    )
    .await?;
    writer.finish()?;
    Ok(report)
}
