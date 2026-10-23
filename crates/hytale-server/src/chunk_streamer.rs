//! Bridge from the world thread's demand queue to the client `Chunks` stream.
//!
//! The connection task owns one of these per player: it sets the view, ticks the
//! world thread until the queue is empty, then writes every loaded chunk in the
//! view as a Java-ordered packet batch. Incremental view changes can reuse the
//! same loop with `WorldEvent::ChunkLoaded`.

use std::collections::BTreeSet;

use hytale_net::{ChunkStreamWriter, TransportError};
use hytale_world::chunk::stream::{
    unload_chunk, ChunkPacketBatch, ChunkStreamError as PacketStreamError,
    DEFAULT_COMPRESSION_LEVEL,
};
use hytale_world::view::ChunkView;
use thiserror::Error;

use crate::world_thread::{WorldEvent, WorldThread, WorldThreadError};

/// Errors from streaming a view to a client.
#[derive(Debug, Error)]
pub enum ChunkSessionError {
    /// The world thread failed.
    #[error(transparent)]
    World(#[from] WorldThreadError),
    /// Packet encoding failed.
    #[error(transparent)]
    Stream(#[from] PacketStreamError),
    /// Writing to the QUIC stream failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
}

/// What a view send produced.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChunkStreamReport {
    /// Chunks sent.
    pub chunks: usize,
    /// Chunks unloaded on the client.
    pub unloaded: usize,
    /// Packets sent.
    pub packets: usize,
    /// QUIC frames written.
    pub frames: usize,
    /// Uncompressed payload bytes.
    pub payload_bytes: usize,
}

/// Load/generate every chunk in the view, then write the initial packet batches.
///
/// This is the join snapshot: stored chunks load from disk, missing chunks
/// generate, and the loaded set is sent nearest-first. It does not finish the
/// stream; the caller decides when the session ends.
pub async fn send_view(
    thread: &WorldThread,
    writer: &mut ChunkStreamWriter,
    center_x: i32,
    center_z: i32,
    radius: u32,
    compression_level: i32,
) -> Result<ChunkStreamReport, ChunkSessionError> {
    thread.set_view(center_x, center_z, radius)?;
    // Forward unloads caused by the view change before the snapshot.
    let mut report = send_updates(thread, writer, compression_level).await?;
    loop {
        let tick = thread.tick()?;
        if tick.remaining == 0 {
            break;
        }
    }
    // The initial snapshot is read from the loaded set, so `ChunkLoaded` events
    // from the demand ticks are not needed (and would double-send preloaded
    // chunks). Unloads were already forwarded above.
    while thread.try_event().is_some() {}

    let loaded: BTreeSet<(i32, i32)> = thread.loaded_chunks()?.into_iter().collect();
    let view = ChunkView::new(center_x, center_z, radius);
    let desired = view.plan([]).loads;
    for (x, z) in desired {
        if !loaded.contains(&(x, z)) {
            continue;
        }
        send_chunk(thread, writer, x, z, compression_level, &mut report).await?;
    }
    Ok(report)
}

/// Forward every queued world event to the client.
///
/// `ChunkLoaded` sends the chunk's initial packet batch; `ChunkUnloaded` sends
/// an `UnloadChunk` packet. The caller sets the view (or ticks the world thread)
/// before calling this.
pub async fn send_updates(
    thread: &WorldThread,
    writer: &mut ChunkStreamWriter,
    compression_level: i32,
) -> Result<ChunkStreamReport, ChunkSessionError> {
    let mut report = ChunkStreamReport::default();
    while let Some(event) = thread.try_event() {
        match event {
            WorldEvent::ChunkLoaded { x, z, .. } => {
                send_chunk(thread, writer, x, z, compression_level, &mut report).await?;
            }
            WorldEvent::ChunkUnloaded { x, z } => {
                let packet = unload_chunk(x, z)?;
                let payload_bytes = packet.payload.len();
                let frame = packet.frame(compression_level)?;
                writer.write_frame(&frame).await?;
                report.unloaded += 1;
                report.packets += 1;
                report.frames += 1;
                report.payload_bytes += payload_bytes;
            }
        }
    }
    Ok(report)
}

/// Write one chunk's initial packet batch.
pub async fn send_chunk(
    thread: &WorldThread,
    writer: &mut ChunkStreamWriter,
    x: i32,
    z: i32,
    compression_level: i32,
    report: &mut ChunkStreamReport,
) -> Result<(), ChunkSessionError> {
    let batch: ChunkPacketBatch = thread.chunk_packets(x, z, false)?;
    let payload_bytes = batch.payload_bytes();
    let frames = batch.frames(compression_level)?;
    writer
        .write_frames(frames.iter().map(Vec::as_slice))
        .await?;
    report.chunks += 1;
    report.packets += batch.packets.len();
    report.frames += frames.len();
    report.payload_bytes += payload_bytes;
    Ok(())
}

/// The default compression level used by Java `PacketIO`.
pub const fn default_compression_level() -> i32 {
    DEFAULT_COMPRESSION_LEVEL
}
