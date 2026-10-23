//! Framed packet streams for the `Chunks` QUIC channel.
//!
//! The transport layer only moves `(packet id, payload)` frames. Chunk packet
//! construction lives in `hytale-world::chunk::stream` and stays out of this
//! crate; the connection task sends the frames produced there.
//!
//! A chunk stream is a dedicated unidirectional QUIC stream opened by the
//! server. Frames use the normal `[u32 length][u32 packet id][payload]` header;
//! payloads for compressed packet ids are zstd-decompressed by [`ChunkStreamReader`]
//! so callers see the same payload bytes the generated packet decoder expects.

use hytale_protocol::frame::{decompress_payload, MAX_PACKET_SIZE};
use hytale_protocol::generated::registry::packet_info;
use hytale_protocol::io::ProtocolError;
use quinn::{Connection, ReadExactError, RecvStream, SendStream};
use thiserror::Error;

use crate::TransportError;

/// Errors from reading or writing a chunk stream.
#[derive(Debug, Error)]
pub enum ChunkStreamError {
    /// The QUIC stream or connection failed.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The frame header or payload is invalid.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// Reading from the stream failed.
    #[error("chunk stream read failed: {0}")]
    Read(#[from] ReadExactError),
    /// The stream ended in the middle of a frame.
    #[error("chunk stream ended in the middle of a frame")]
    Truncated,
    /// The packet id is not in the protocol registry.
    #[error("unknown packet id {0} on the chunk stream")]
    UnknownPacket(u32),
}

/// One received packet with its decompressed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkFrame {
    /// Packet registry id.
    pub packet_id: u32,
    /// Uncompressed wire payload.
    pub payload: Vec<u8>,
}

/// Writes chunk frames to a server-opened unidirectional stream.
pub struct ChunkStreamWriter {
    send: SendStream,
}

impl ChunkStreamWriter {
    /// Open a new `Chunks` stream on an established connection.
    pub async fn open(connection: &Connection) -> Result<Self, TransportError> {
        Ok(Self {
            send: connection.open_uni().await?,
        })
    }

    /// Write one complete frame (`[length][packet id][payload]`).
    pub async fn write_frame(&mut self, frame: &[u8]) -> Result<(), TransportError> {
        self.send.write_all(frame).await?;
        Ok(())
    }

    /// Write frames in order.
    pub async fn write_frames<'a>(
        &mut self,
        frames: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<(), TransportError> {
        for frame in frames {
            self.write_frame(frame).await?;
        }
        Ok(())
    }

    /// Finish the stream after the last frame.
    pub fn finish(self) -> Result<(), TransportError> {
        let mut send = self.send;
        send.finish()?;
        Ok(())
    }
}

/// Reads chunk frames from a client-accepted unidirectional stream.
pub struct ChunkStreamReader {
    recv: RecvStream,
}

impl ChunkStreamReader {
    /// Accept the next server-opened unidirectional stream as the chunk stream.
    pub async fn accept(connection: &Connection) -> Result<Self, TransportError> {
        Ok(Self {
            recv: connection.accept_uni().await?,
        })
    }

    /// Read the next frame, or `None` on a clean stream end.
    pub async fn next_frame(&mut self) -> Result<Option<ChunkFrame>, ChunkStreamError> {
        let mut header = [0u8; 8];
        match self.recv.read_exact(&mut header).await {
            Ok(()) => {}
            Err(ReadExactError::FinishedEarly(0)) => return Ok(None),
            Err(ReadExactError::FinishedEarly(_)) => return Err(ChunkStreamError::Truncated),
            Err(error) => return Err(ChunkStreamError::Read(error)),
        }

        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let packet_id = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if length > MAX_PACKET_SIZE {
            return Err(ChunkStreamError::Protocol(
                ProtocolError::InvalidFrameLength {
                    length,
                    max: MAX_PACKET_SIZE,
                },
            ));
        }
        let info = packet_info(packet_id).ok_or(ChunkStreamError::UnknownPacket(packet_id))?;
        if length > info.max_size {
            return Err(ChunkStreamError::Protocol(ProtocolError::PayloadTooLarge {
                packet_id,
                length,
                max: info.max_size,
            }));
        }

        let mut payload = vec![0u8; length as usize];
        match self.recv.read_exact(&mut payload).await {
            Ok(()) => {}
            Err(ReadExactError::FinishedEarly(_)) => return Err(ChunkStreamError::Truncated),
            Err(error) => return Err(ChunkStreamError::Read(error)),
        }

        if info.compressed {
            payload = decompress_payload(&payload, info.max_size as usize)?;
        }
        Ok(Some(ChunkFrame { packet_id, payload }))
    }
}
