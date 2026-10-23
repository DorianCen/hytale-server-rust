//! Generic framed packet streams over QUIC.
//!
//! The chunk stream and the connection's bidirectional setup stream use the same
//! `[u32 length][u32 packet id][payload]` framing. This module owns the
//! incremental reader (including zstd decompression for compressed packet ids)
//! and the writer used by both.

use hytale_protocol::frame::{compress_payload, decompress_payload, encode_frame, MAX_PACKET_SIZE};
use hytale_protocol::generated::registry::packet_info;
use hytale_protocol::generated::wire::{Packet, Wire};
use hytale_protocol::io::{ProtocolError, Writer};
use quinn::{ClosedStream, ReadExactError, RecvStream, SendStream, WriteError};
use thiserror::Error;

/// Compression level used by Java `PacketIO` (zstd default).
pub const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Errors from a framed packet stream.
#[derive(Debug, Error)]
pub enum FrameStreamError {
    /// The frame header or payload is invalid.
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    /// Reading from the stream failed.
    #[error("packet stream read failed: {0}")]
    Read(#[from] ReadExactError),
    /// Writing to the stream failed.
    #[error("packet stream write failed: {0}")]
    Write(#[from] WriteError),
    /// Finishing the stream failed.
    #[error("packet stream is closed: {0}")]
    Closed(#[from] ClosedStream),
    /// The stream ended in the middle of a frame.
    #[error("packet stream ended in the middle of a frame")]
    Truncated,
    /// The packet id is not in the registry.
    #[error("unknown packet id {0}")]
    UnknownPacket(u32),
}

/// One received packet with its decompressed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Packet registry id.
    pub packet_id: u32,
    /// Uncompressed wire payload.
    pub payload: Vec<u8>,
}

impl Frame {
    /// Decode the payload as a known packet.
    pub fn decode<T: Packet + Wire>(&self) -> Result<T, FrameStreamError> {
        if self.packet_id != T::ID {
            return Err(FrameStreamError::Protocol(ProtocolError::UnknownPacket {
                packet_id: self.packet_id,
            }));
        }
        Ok(T::decode(&mut hytale_protocol::io::Reader::new(
            &self.payload,
        ))?)
    }
}

/// Writes frames to a QUIC send stream.
pub struct FrameWriter {
    send: SendStream,
}

impl FrameWriter {
    /// Wrap an open send stream.
    pub fn new(send: SendStream) -> Self {
        Self { send }
    }

    /// Encode, compress (when required) and frame a packet.
    pub async fn write_packet<T: Packet + Wire>(
        &mut self,
        packet: &T,
        compression_level: i32,
    ) -> Result<(), FrameStreamError> {
        let mut payload = Writer::new();
        packet.encode(&mut payload)?;
        let payload = if T::COMPRESSED {
            compress_payload(&payload.into_inner(), compression_level)?
        } else {
            payload.into_inner()
        };
        let mut frame = Vec::with_capacity(payload.len() + 8);
        encode_frame(T::ID, &payload, &mut frame)?;
        self.write_frame(&frame).await
    }

    /// Write an already-framed packet.
    pub async fn write_frame(&mut self, frame: &[u8]) -> Result<(), FrameStreamError> {
        self.send.write_all(frame).await?;
        Ok(())
    }

    /// Write frames in order.
    pub async fn write_frames<'a>(
        &mut self,
        frames: impl IntoIterator<Item = &'a [u8]>,
    ) -> Result<(), FrameStreamError> {
        for frame in frames {
            self.write_frame(frame).await?;
        }
        Ok(())
    }

    /// Finish the stream after the last frame.
    pub fn finish(self) -> Result<(), FrameStreamError> {
        let mut send = self.send;
        send.finish()?;
        Ok(())
    }
}

/// Reads frames from a QUIC receive stream.
pub struct FrameReader {
    recv: RecvStream,
}

impl FrameReader {
    /// Wrap an open receive stream.
    pub fn new(recv: RecvStream) -> Self {
        Self { recv }
    }

    /// Read the next frame, or `None` on a clean stream end.
    pub async fn next_frame(&mut self) -> Result<Option<Frame>, FrameStreamError> {
        let mut header = [0u8; 8];
        match self.recv.read_exact(&mut header).await {
            Ok(()) => {}
            Err(ReadExactError::FinishedEarly(0)) => return Ok(None),
            Err(ReadExactError::FinishedEarly(_)) => return Err(FrameStreamError::Truncated),
            Err(error) => return Err(FrameStreamError::Read(error)),
        }

        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let packet_id = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if length > MAX_PACKET_SIZE {
            return Err(FrameStreamError::Protocol(
                ProtocolError::InvalidFrameLength {
                    length,
                    max: MAX_PACKET_SIZE,
                },
            ));
        }
        let info = packet_info(packet_id).ok_or(FrameStreamError::UnknownPacket(packet_id))?;
        if length > info.max_size {
            return Err(FrameStreamError::Protocol(ProtocolError::PayloadTooLarge {
                packet_id,
                length,
                max: info.max_size,
            }));
        }

        let mut payload = vec![0u8; length as usize];
        match self.recv.read_exact(&mut payload).await {
            Ok(()) => {}
            Err(ReadExactError::FinishedEarly(_)) => return Err(FrameStreamError::Truncated),
            Err(error) => return Err(FrameStreamError::Read(error)),
        }
        if info.compressed {
            payload = decompress_payload(&payload, info.max_size as usize)?;
        }
        Ok(Some(Frame { packet_id, payload }))
    }

    /// Read the next frame and decode it as `T`.
    pub async fn next_packet<T: Packet + Wire>(&mut self) -> Result<Option<T>, FrameStreamError> {
        match self.next_frame().await? {
            Some(frame) => Ok(Some(frame.decode::<T>()?)),
            None => Ok(None),
        }
    }
}
