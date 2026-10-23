//! Chunk packet batches for the network stream.
//!
//! The world thread emits `ChunkLoaded` events; the connection task turns each
//! event into this batch and writes the frames to the client's chunk stream.
//! Packet order follows the Java `ChunkPacketDumper` manifest:
//!
//! ```text
//! SetColumn
//! SetChunkTintmap              (full updates only)
//! per section (ascending Y):
//!   SetChunk
//!   SetChunkEnvironments
//!   SetFluids                   (when the section holds fluid)
//! ```
//!
//! The initial join set matches CoreServer's load-query systems: `SetColumn`,
//! all section `SetChunk` and `SetChunkEnvironments`. Tint and fluid packets are
//! update-only, exactly as in the golden manifest.

use hytale_protocol::frame::{compress_payload, encode_frame};
use hytale_protocol::generated::wire::{NetworkChannel, Packet, Wire};
use hytale_protocol::generated::world::{
    SetChunk, SetChunkEnvironments, SetChunkTintmap, SetColumn, SetFluids, UnloadChunk,
};
use hytale_protocol::io::{ProtocolError, Writer};
use thiserror::Error;

use super::column::{ChunkColumn, SECTION_COUNT};
use super::section::SectionError;

/// Compression level used by Java `PacketIO` (zstd default).
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Errors from building or framing a chunk packet batch.
#[derive(Debug, Error)]
pub enum ChunkStreamError {
    /// A section could not be serialized.
    #[error(transparent)]
    Section(#[from] SectionError),
    /// The wire encoder or zstd failed.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
}

/// One encoded packet ready for the chunk stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPacket {
    /// Packet registry id.
    pub id: u32,
    /// Target channel (`Chunks` for all chunk packets).
    pub channel: NetworkChannel,
    /// Whether the frame payload must be zstd-compressed.
    pub compressed: bool,
    /// Protocol maximum payload size.
    pub max_size: u32,
    /// Uncompressed, unframed wire payload.
    pub payload: Vec<u8>,
}

impl StreamPacket {
    /// Raw payload bytes (before compression/framing).
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Frame the packet for the QUIC chunk stream (compressing when required).
    pub fn frame(&self, compression_level: i32) -> Result<Vec<u8>, ChunkStreamError> {
        let payload = if self.compressed {
            compress_payload(&self.payload, compression_level)?
        } else {
            self.payload.clone()
        };
        let mut frame = Vec::with_capacity(payload.len() + 8);
        encode_frame(self.id, &payload, &mut frame)?;
        Ok(frame)
    }
}

/// Every packet for one chunk, in Java send order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkPacketBatch {
    /// Chunk x.
    pub x: i32,
    /// Chunk z.
    pub z: i32,
    /// Packets in send order.
    pub packets: Vec<StreamPacket>,
}

impl ChunkPacketBatch {
    /// The initial join set: `SetColumn` + all `SetChunk`/`SetChunkEnvironments`.
    pub fn initial(column: &ChunkColumn) -> Result<Self, ChunkStreamError> {
        Self::build(column, false)
    }

    /// The full update set, including tint and fluid packets.
    pub fn full(column: &ChunkColumn) -> Result<Self, ChunkStreamError> {
        Self::build(column, true)
    }

    fn build(column: &ChunkColumn, include_updates: bool) -> Result<Self, ChunkStreamError> {
        let mut packets = Vec::with_capacity(SECTION_COUNT * 3 + 2);
        push(&column.set_column_packet()?, &mut packets)?;
        if include_updates {
            push(&column.tintmap_packet(), &mut packets)?;
        }
        for section_y in 0..SECTION_COUNT as i32 {
            push(&column.set_chunk_packet(section_y)?, &mut packets)?;
            push(
                &column.set_chunk_environments_packet(section_y)?,
                &mut packets,
            )?;
            if include_updates {
                if let Some(fluids) = column.set_fluids_packet(section_y) {
                    push(&fluids, &mut packets)?;
                }
            }
        }
        Ok(Self {
            x: column.x(),
            z: column.z(),
            packets,
        })
    }

    /// Encode every packet as a QUIC-ready frame.
    pub fn frames(&self, compression_level: i32) -> Result<Vec<Vec<u8>>, ChunkStreamError> {
        self.packets
            .iter()
            .map(|packet| packet.frame(compression_level))
            .collect()
    }

    /// Uncompressed payload bytes in the batch.
    pub fn payload_bytes(&self) -> usize {
        self.packets.iter().map(|packet| packet.payload.len()).sum()
    }
}

fn push<T: Packet + Wire>(
    packet: &T,
    packets: &mut Vec<StreamPacket>,
) -> Result<(), ChunkStreamError> {
    let mut writer = Writer::new();
    packet.encode(&mut writer)?;
    packets.push(StreamPacket {
        id: T::ID,
        channel: T::CHANNEL,
        compressed: T::COMPRESSED,
        max_size: T::MAX_SIZE,
        payload: writer.into_inner(),
    });
    Ok(())
}

/// The `UnloadChunk` packet for a chunk that left the view.
pub fn unload_chunk(x: i32, z: i32) -> Result<StreamPacket, ChunkStreamError> {
    let mut packets = Vec::with_capacity(1);
    push(
        &UnloadChunk {
            chunk_x: x,
            chunk_z: z,
        },
        &mut packets,
    )?;
    Ok(packets.remove(0))
}

/// Packet ids covered by a batch, for diagnostics.
pub fn chunk_packet_ids() -> [u32; 5] {
    [
        SetColumn::ID,
        SetChunkTintmap::ID,
        SetChunk::ID,
        SetChunkEnvironments::ID,
        SetFluids::ID,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use hytale_protocol::frame::{decode_frame, decompress_payload};
    use hytale_protocol::io::Reader;

    #[test]
    fn initial_and_full_batches_differ_only_by_update_packets() {
        let mut column = ChunkColumn::new(4, -5);
        column.set_block(1, 64, 1, 7, 0, 0).expect("set block");
        column.set_tint(1, 1, 0x1234_5678);

        let initial = ChunkPacketBatch::initial(&column).expect("initial");
        let full = ChunkPacketBatch::full(&column).expect("full");
        assert_eq!(initial.x, 4);
        assert_eq!(initial.z, -5);
        assert!(initial
            .packets
            .iter()
            .all(|packet| packet.id != SetChunkTintmap::ID));
        assert!(full
            .packets
            .iter()
            .any(|packet| packet.id == SetChunkTintmap::ID));
        assert!(full.packets.len() > initial.packets.len());
    }

    #[test]
    fn unload_chunk_frames_a_chunks_channel_packet() {
        let packet = unload_chunk(4, -5).expect("unload packet");
        assert_eq!(packet.id, UnloadChunk::ID);
        assert_eq!(packet.channel, NetworkChannel::Chunks);
        assert!(!packet.compressed);
        let frame = packet.frame(DEFAULT_COMPRESSION_LEVEL).expect("frame");
        let (decoded, consumed) = decode_frame(&frame, packet.max_size)
            .expect("decode frame")
            .expect("complete");
        assert_eq!(consumed, frame.len());
        let unloaded =
            UnloadChunk::decode(&mut Reader::new(decoded.payload)).expect("decode UnloadChunk");
        assert_eq!((unloaded.chunk_x, unloaded.chunk_z), (4, -5));
    }

    #[test]
    fn frames_round_trip_through_the_protocol_decoder() {
        let mut column = ChunkColumn::new(0, 0);
        column.set_block(2, 64, 3, 9, 0, 0).expect("set block");
        let batch = ChunkPacketBatch::full(&column).expect("batch");
        let frames = batch
            .frames(DEFAULT_COMPRESSION_LEVEL)
            .expect("encode frames");

        for (packet, frame) in batch.packets.iter().zip(frames) {
            let (decoded, consumed) = decode_frame(&frame, packet.max_size)
                .expect("decode frame")
                .expect("frame");
            assert_eq!(consumed, frame.len());
            assert_eq!(decoded.packet_id, packet.id);
            let payload = if packet.compressed {
                decompress_payload(decoded.payload, packet.max_size as usize).expect("decompress")
            } else {
                decoded.payload.to_vec()
            };
            assert_eq!(payload, packet.payload);
            assert_eq!(packet.channel, NetworkChannel::Chunks);

            // The generated decoder accepts the payload, which is the client-side
            // contract for every packet in the stream.
            let mut reader = Reader::new(&payload);
            match packet.id {
                SetColumn::ID => {
                    SetColumn::decode(&mut reader).expect("decode SetColumn");
                }
                SetChunk::ID => {
                    SetChunk::decode(&mut reader).expect("decode SetChunk");
                }
                SetChunkEnvironments::ID => {
                    SetChunkEnvironments::decode(&mut reader).expect("decode SetChunkEnvironments");
                }
                SetChunkTintmap::ID => {
                    SetChunkTintmap::decode(&mut reader).expect("decode SetChunkTintmap");
                }
                SetFluids::ID => {
                    SetFluids::decode(&mut reader).expect("decode SetFluids");
                }
                other => panic!("unexpected packet id {other}"),
            }
        }
    }
}
