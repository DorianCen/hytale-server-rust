//! Generic framed packet streams over QUIC.
//!
//! The chunk stream and the connection's bidirectional setup stream use the same
//! `[u32 length][u32 packet id][payload]` framing. This module owns the
//! incremental reader (including zstd decompression for compressed packet ids)
//! and the writer used by both.

use hytale_protocol::frame::{compress_payload, decompress_payload, encode_frame, MAX_PACKET_SIZE};
use hytale_protocol::generated::registry::packet_info;
use hytale_protocol::generated::wire::{NetworkChannel, Packet, Wire};
use hytale_protocol::io::{ProtocolError, Writer};
use quinn::{ClosedStream, ReadExactError, RecvStream, SendStream, WriteError};
use std::sync::Arc;
use thiserror::Error;

use crate::trace::{trace_wire_frame, PacketTraceSink, TraceDirection, TraceStream};

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
    /// The packet's registry channel does not belong on this stream.
    #[error("packet {packet_id} ({channel:?}) is not allowed on the {expected:?} stream")]
    WrongChannel {
        /// Packet registry id.
        packet_id: u32,
        /// The packet's registry channel.
        channel: NetworkChannel,
        /// The stream's channel.
        expected: NetworkChannel,
    },
}

/// Whether a packet channel may travel on `stream`.
///
/// Java `QuicheChannel.acceptsPacket` accepts every non-Voice packet on the
/// default game stream and requires an exact channel match on the dedicated
/// chunk/world-map/voice streams.
pub(crate) fn channel_allowed(stream: NetworkChannel, packet: NetworkChannel) -> bool {
    if stream == NetworkChannel::Default {
        packet != NetworkChannel::Voice
    } else {
        packet == stream
    }
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
    trace: Option<Arc<dyn PacketTraceSink>>,
}

impl FrameWriter {
    /// Wrap an open send stream.
    pub fn new(send: SendStream) -> Self {
        Self { send, trace: None }
    }

    /// Attach a packet trace sink.
    pub fn with_trace(mut self, trace: Arc<dyn PacketTraceSink>) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Encode, compress (when required) and frame a packet.
    pub async fn write_packet<T: Packet + Wire>(
        &mut self,
        packet: &T,
        compression_level: i32,
    ) -> Result<(), FrameStreamError> {
        if !channel_allowed(NetworkChannel::Default, T::CHANNEL) {
            return Err(FrameStreamError::WrongChannel {
                packet_id: T::ID,
                channel: T::CHANNEL,
                expected: NetworkChannel::Default,
            });
        }
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

    /// Write an already-encoded packet, applying the registry's compression
    /// policy.
    pub async fn write_raw_packet(
        &mut self,
        packet_id: u32,
        payload: &[u8],
        compression_level: i32,
    ) -> Result<(), FrameStreamError> {
        let info = packet_info(packet_id).ok_or(FrameStreamError::Protocol(
            ProtocolError::UnknownPacket { packet_id },
        ))?;
        if !channel_allowed(NetworkChannel::Default, info.channel) {
            return Err(FrameStreamError::WrongChannel {
                packet_id,
                channel: info.channel,
                expected: NetworkChannel::Default,
            });
        }
        if payload.len() as u32 > info.max_size {
            return Err(FrameStreamError::Protocol(ProtocolError::PayloadTooLarge {
                packet_id,
                length: payload.len() as u32,
                max: info.max_size,
            }));
        }
        let payload = if info.compressed {
            compress_payload(payload, compression_level)?
        } else {
            payload.to_vec()
        };
        let mut frame = Vec::with_capacity(payload.len() + 8);
        encode_frame(packet_id, &payload, &mut frame)?;
        self.write_frame(&frame).await
    }

    /// Write an already-framed packet.
    pub async fn write_frame(&mut self, frame: &[u8]) -> Result<(), FrameStreamError> {
        self.send.write_all(frame).await?;
        trace_wire_frame(
            &self.trace,
            TraceStream::Setup,
            TraceDirection::Outbound,
            frame,
        );
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
    trace: Option<Arc<dyn PacketTraceSink>>,
    max_payload: u32,
    expected_channel: NetworkChannel,
}

impl FrameReader {
    /// Wrap an open receive stream.
    pub fn new(recv: RecvStream) -> Self {
        Self {
            recv,
            trace: None,
            max_payload: MAX_PACKET_SIZE,
            expected_channel: NetworkChannel::Default,
        }
    }

    /// Set the channel this stream is allowed to carry (default `Default`).
    pub fn with_channel(mut self, channel: NetworkChannel) -> Self {
        self.expected_channel = channel;
        self
    }

    /// Attach a packet trace sink.
    pub fn with_trace(mut self, trace: Arc<dyn PacketTraceSink>) -> Self {
        self.trace = Some(trace);
        self
    }

    /// Cap inbound payloads below the registry's per-packet Java maximum.
    ///
    /// The cap is applied to the compressed frame length and to the declared
    /// decompressed size, so a small compressed bomb cannot bypass it.
    pub fn with_max_payload(mut self, max_payload: u32) -> Self {
        self.max_payload = max_payload.max(1);
        self
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
        if !channel_allowed(self.expected_channel, info.channel) {
            return Err(FrameStreamError::WrongChannel {
                packet_id,
                channel: info.channel,
                expected: self.expected_channel,
            });
        }
        let max_size = info.max_size.min(self.max_payload);
        if length > max_size {
            return Err(FrameStreamError::Protocol(ProtocolError::PayloadTooLarge {
                packet_id,
                length,
                max: max_size,
            }));
        }

        let mut payload = vec![0u8; length as usize];
        match self.recv.read_exact(&mut payload).await {
            Ok(()) => {}
            Err(ReadExactError::FinishedEarly(_)) => return Err(FrameStreamError::Truncated),
            Err(error) => return Err(FrameStreamError::Read(error)),
        }
        if let Some(trace) = &self.trace {
            trace.record(
                TraceStream::Setup,
                TraceDirection::Inbound,
                packet_id,
                &payload,
            );
        }
        if info.compressed {
            payload = decompress_payload(&payload, max_size as usize)?;
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

#[cfg(test)]
mod tests {
    use hytale_protocol::generated::wire::NetworkChannel;

    use super::channel_allowed;

    #[test]
    fn default_stream_rejects_only_voice_and_dedicated_streams_are_exact() {
        assert!(channel_allowed(
            NetworkChannel::Default,
            NetworkChannel::Default
        ));
        assert!(channel_allowed(
            NetworkChannel::Default,
            NetworkChannel::Chunks
        ));
        assert!(!channel_allowed(
            NetworkChannel::Default,
            NetworkChannel::Voice
        ));
        assert!(channel_allowed(
            NetworkChannel::Chunks,
            NetworkChannel::Chunks
        ));
        assert!(!channel_allowed(
            NetworkChannel::Chunks,
            NetworkChannel::Default
        ));
        assert!(!channel_allowed(
            NetworkChannel::WorldMap,
            NetworkChannel::Chunks
        ));
        assert!(channel_allowed(
            NetworkChannel::WorldMap,
            NetworkChannel::WorldMap
        ));
    }
}
